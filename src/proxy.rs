use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{ConnectInfo, State},
    http::{HeaderMap, HeaderName, Method, Request, Response, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use reqwest::Client;
use tokio::sync::Semaphore;
use tracing::{info, warn};

use crate::{
    config::Config,
    health::spawn_health_checks,
    metrics::Metrics,
    rate_limit::{Decision, RateLimiter},
    signature::HmacVerifier,
    upstream::UpstreamPool,
};

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
    pool: Arc<UpstreamPool>,
    limiter: Arc<RateLimiter>,
    verifier: Option<Arc<HmacVerifier>>,
    client: Client,
    semaphore: Arc<Semaphore>,
    metrics: Arc<Metrics>,
}

impl AppState {
    pub fn new(config: Config) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_millis(config.request_timeout_ms))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let pool = Arc::new(UpstreamPool::new(&config.upstreams));
        let verifier = if let Some(hmac) = &config.hmac {
            let secret = std::env::var(&hmac.secret_env)
                .map_err(|_| format!("HMAC is enabled but {} is not set", hmac.secret_env))?;
            if secret.len() < 32 {
                return Err("HMAC secret must contain at least 32 bytes".into());
            }
            Some(Arc::new(HmacVerifier::from_config(
                hmac,
                secret.into_bytes(),
            )?))
        } else {
            None
        };
        let limiter = Arc::new(RateLimiter::new(config.rate_limit.clone()));
        let semaphore = Arc::new(Semaphore::new(config.max_in_flight));
        let state = Self {
            config: Arc::new(config),
            pool,
            limiter,
            verifier,
            client,
            semaphore,
            metrics: Arc::new(Metrics::default()),
        };
        spawn_health_checks(
            state.pool.clone(),
            state.client.clone(),
            state.config.health.clone(),
        );
        Ok(state)
    }

    pub fn router(self) -> Router {
        Router::new()
            .route("/healthz", get(healthz))
            .route("/readyz", get(readyz))
            .route("/metrics", get(metrics))
            .fallback(proxy)
            .with_state(self)
    }
}

async fn healthz() -> StatusCode {
    StatusCode::OK
}

async fn readyz(State(state): State<AppState>) -> StatusCode {
    if state.pool.any_ready() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        state.metrics.render(),
    )
}

async fn proxy(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Request<Body>,
) -> Response<Body> {
    state.metrics.request();

    let _permit = match state.semaphore.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            state.metrics.load_shed();
            return error_response(StatusCode::SERVICE_UNAVAILABLE, "gateway overloaded");
        }
    };

    match state.limiter.check(&peer.ip().to_string()).await {
        Decision::Allow => {}
        Decision::Limited => {
            state.metrics.rate_limited();
            return error_response(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded");
        }
        Decision::Saturated => {
            state.metrics.rate_limited();
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "rate limiter capacity exhausted",
            );
        }
    }

    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, state.config.max_body_bytes).await {
        Ok(body) => body,
        Err(_) => {
            return error_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "request body exceeds configured limit",
            );
        }
    };

    if let Some(verifier) = &state.verifier
        && verifier.protects(parts.uri.path())
        && let Err(error) = verifier.verify(&parts.method, &parts.uri, &parts.headers, &body)
    {
        state.metrics.signature_rejection();
        warn!(%error, path = %parts.uri.path(), "request signature rejected");
        return error_response(StatusCode::UNAUTHORIZED, "request signature rejected");
    }

    let retryable = is_idempotent(&parts.method);
    let retries = if retryable {
        state.config.retry_attempts
    } else {
        0
    };
    let outbound_headers = sanitize_headers(&parts.headers, true);

    for attempt in 0..=retries {
        let Some(upstream) = state.pool.select() else {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "no healthy upstream available",
            );
        };
        let Ok(target) = upstream.target_url(&parts.uri) else {
            upstream.breaker.on_failure(Instant::now());
            return error_response(StatusCode::BAD_GATEWAY, "failed to construct upstream URL");
        };

        let mut request_builder = state
            .client
            .request(parts.method.clone(), target)
            .headers(outbound_headers.clone());
        let trusted_peer = peer.ip().to_string();
        request_builder = request_builder
            .header("x-forwarded-for", &trusted_peer)
            .header("x-real-ip", &trusted_peer)
            .header("x-forwarded-proto", "http")
            .header("x-ironroute-upstream", &upstream.name);

        match request_builder.body(body.clone()).send().await {
            Ok(response) => {
                let status = response.status();
                if status.is_server_error() {
                    upstream.breaker.on_failure(Instant::now());
                    state.metrics.upstream_failure();
                    if attempt < retries {
                        state.metrics.retry();
                        backoff(attempt).await;
                        continue;
                    }
                } else {
                    upstream.breaker.on_success();
                }

                state.metrics.response(status.as_u16());
                let headers = sanitize_headers(response.headers(), false);
                let stream = response.bytes_stream();
                let mut downstream = Response::new(Body::from_stream(stream));
                *downstream.status_mut() = status;
                *downstream.headers_mut() = headers;
                return downstream;
            }
            Err(error) => {
                upstream.breaker.on_failure(Instant::now());
                state.metrics.upstream_failure();
                warn!(upstream = %upstream.name, %error, "upstream request failed");
                if attempt < retries {
                    state.metrics.retry();
                    backoff(attempt).await;
                    continue;
                }
            }
        }
    }

    error_response(StatusCode::BAD_GATEWAY, "upstream request failed")
}

fn sanitize_headers(input: &HeaderMap, outbound_request: bool) -> HeaderMap {
    let mut output = input.clone();
    if let Some(value) = input.get(header::CONNECTION)
        && let Ok(value) = value.to_str()
    {
        for token in value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if let Ok(name) = HeaderName::from_bytes(token.as_bytes()) {
                output.remove(name);
            }
        }
    }
    for name in [
        header::CONNECTION,
        HeaderName::from_static("keep-alive"),
        header::PROXY_AUTHENTICATE,
        header::PROXY_AUTHORIZATION,
        header::TE,
        header::TRAILER,
        header::TRANSFER_ENCODING,
        header::UPGRADE,
    ] {
        output.remove(name);
    }
    if outbound_request {
        for name in [
            header::HOST,
            HeaderName::from_static("forwarded"),
            HeaderName::from_static("x-forwarded-for"),
            HeaderName::from_static("x-forwarded-host"),
            HeaderName::from_static("x-forwarded-proto"),
            HeaderName::from_static("x-forwarded-port"),
            HeaderName::from_static("x-real-ip"),
        ] {
            output.remove(name);
        }
    }
    output
}

fn is_idempotent(method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::HEAD | Method::OPTIONS | Method::PUT | Method::DELETE
    )
}

async fn backoff(attempt: usize) {
    let exponent = u32::try_from(attempt.min(3)).unwrap_or(3);
    let millis = 25_u64.saturating_mul(2_u64.pow(exponent));
    tokio::time::sleep(Duration::from_millis(millis)).await;
}

fn error_response(status: StatusCode, message: &'static str) -> Response<Body> {
    let mut response = Response::new(Body::from(message));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

pub fn describe_startup(config: &Config) {
    info!(listen = %config.listen, upstreams = config.upstreams.len(), max_in_flight = config.max_in_flight, "IronRoute configured");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn strips_hop_by_hop_and_connection_named_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONNECTION, HeaderValue::from_static("x-remove"));
        headers.insert("x-remove", HeaderValue::from_static("secret"));
        headers.insert("x-keep", HeaderValue::from_static("yes"));
        let clean = sanitize_headers(&headers, false);
        assert!(!clean.contains_key(header::CONNECTION));
        assert!(!clean.contains_key("x-remove"));
        assert_eq!(clean.get("x-keep").unwrap(), "yes");
    }

    #[test]
    fn strips_client_forwarding_identity_on_upstream_requests() {
        let mut headers = HeaderMap::new();
        headers.insert("forwarded", HeaderValue::from_static("for=203.0.113.10"));
        headers.insert("x-forwarded-for", HeaderValue::from_static("203.0.113.10"));
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));
        headers.insert("x-real-ip", HeaderValue::from_static("203.0.113.10"));
        headers.insert("x-keep", HeaderValue::from_static("yes"));
        let clean = sanitize_headers(&headers, true);
        assert!(!clean.contains_key("forwarded"));
        assert!(!clean.contains_key("x-forwarded-for"));
        assert!(!clean.contains_key("x-forwarded-proto"));
        assert!(!clean.contains_key("x-real-ip"));
        assert_eq!(clean.get("x-keep").unwrap(), "yes");
    }

    #[test]
    fn retries_only_semantically_idempotent_methods() {
        assert!(is_idempotent(&Method::GET));
        assert!(is_idempotent(&Method::PUT));
        assert!(!is_idempotent(&Method::POST));
        assert!(!is_idempotent(&Method::PATCH));
    }
}
