use std::net::SocketAddr;

use axum::{
    Router,
    body::Bytes,
    http::{HeaderMap, StatusCode},
    routing::{any, get},
};
use karzoun_ironroute::{
    config::{Config, HealthConfig, RateLimitConfig, UpstreamConfig},
    proxy::AppState,
};
use tokio::{net::TcpListener, task::JoinHandle};

async fn upstream_echo(headers: HeaderMap, body: Bytes) -> String {
    let forwarded_for = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("missing");
    let real_ip = headers
        .get("x-real-ip")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("missing");
    let forwarded_present = headers.contains_key("forwarded");
    format!(
        "xff={forwarded_for};real={real_ip};forwarded={forwarded_present};body={}",
        String::from_utf8_lossy(&body)
    )
}

async fn start_gateway(max_body_bytes: usize) -> (SocketAddr, JoinHandle<()>, JoinHandle<()>) {
    let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();
    let upstream_app = Router::new()
        .route("/healthz", get(|| async { StatusCode::OK }))
        .route("/echo", any(upstream_echo));
    let upstream_task = tokio::spawn(async move {
        axum::serve(upstream_listener, upstream_app).await.unwrap();
    });

    let config = Config {
        listen: "127.0.0.1:0".into(),
        max_body_bytes,
        max_in_flight: 32,
        request_timeout_ms: 2_000,
        retry_attempts: 1,
        rate_limit: RateLimitConfig {
            capacity: 100,
            refill_per_second: 100.0,
            max_entries: 100,
            idle_seconds: 60,
        },
        health: HealthConfig {
            interval_ms: 60_000,
            timeout_ms: 500,
        },
        hmac: None,
        upstreams: vec![UpstreamConfig {
            name: "integration-upstream".into(),
            url: format!("http://{upstream_addr}"),
            weight: 1,
            health_path: "/healthz".into(),
            failure_threshold: 3,
            cooldown_ms: 100,
        }],
    };

    let gateway_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway_addr = gateway_listener.local_addr().unwrap();
    let app = AppState::new(config).unwrap().router();
    let gateway_task = tokio::spawn(async move {
        axum::serve(
            gateway_listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    (gateway_addr, gateway_task, upstream_task)
}

#[tokio::test]
async fn forwards_real_peer_identity_and_removes_spoofed_forwarding_headers() {
    let (gateway_addr, gateway_task, upstream_task) = start_gateway(1_024).await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("http://{gateway_addr}/echo?request=1"))
        .header("forwarded", "for=203.0.113.99;proto=https")
        .header("x-forwarded-for", "203.0.113.99")
        .header("x-forwarded-proto", "https")
        .header("x-real-ip", "203.0.113.99")
        .body("hello")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.unwrap();
    assert!(body.contains("xff=127.0.0.1"), "{body}");
    assert!(body.contains("real=127.0.0.1"), "{body}");
    assert!(body.contains("forwarded=false"), "{body}");
    assert!(body.contains("body=hello"), "{body}");
    assert!(!body.contains("203.0.113.99"), "{body}");

    gateway_task.abort();
    upstream_task.abort();
}

#[tokio::test]
async fn rejects_oversized_request_before_forwarding_it() {
    let (gateway_addr, gateway_task, upstream_task) = start_gateway(4).await;
    let response = reqwest::Client::new()
        .post(format!("http://{gateway_addr}/echo"))
        .body("12345")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        response.text().await.unwrap(),
        "request body exceeds configured limit"
    );

    gateway_task.abort();
    upstream_task.abort();
}
