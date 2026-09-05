use std::{sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}}, time::{Duration, Instant}};

use axum::http::Uri;
use url::Url;

use crate::{breaker::CircuitBreaker, config::UpstreamConfig};

#[derive(Debug)]
pub struct Upstream {
    pub name: String,
    pub base_url: Url,
    pub weight: u32,
    pub health_path: String,
    healthy: AtomicBool,
    pub breaker: CircuitBreaker,
}

impl Upstream {
    fn new(config: &UpstreamConfig) -> Self {
        Self {
            name: config.name.clone(),
            base_url: Url::parse(&config.url).expect("validated upstream URL"),
            weight: config.weight,
            health_path: config.health_path.clone(),
            healthy: AtomicBool::new(true),
            breaker: CircuitBreaker::new(config.failure_threshold, Duration::from_millis(config.cooldown_ms)),
        }
    }

    pub fn is_healthy(&self) -> bool { self.healthy.load(Ordering::Acquire) }
    pub fn set_healthy(&self, value: bool) { self.healthy.store(value, Ordering::Release); }

    pub fn target_url(&self, uri: &Uri) -> Result<Url, url::ParseError> {
        let path = uri.path_and_query().map_or("/", |value| value.as_str());
        self.base_url.join(path)
    }

    pub fn health_url(&self) -> Result<Url, url::ParseError> { self.base_url.join(&self.health_path) }
}

#[derive(Debug)]
pub struct UpstreamPool {
    upstreams: Vec<Arc<Upstream>>,
    cursor: AtomicU64,
}

impl UpstreamPool {
    pub fn new(configs: &[UpstreamConfig]) -> Self {
        Self { upstreams: configs.iter().map(|config| Arc::new(Upstream::new(config))).collect(), cursor: AtomicU64::new(0) }
    }

    pub fn all(&self) -> &[Arc<Upstream>] { &self.upstreams }

    pub fn any_ready(&self) -> bool {
        let now = Instant::now();
        self.upstreams.iter().any(|upstream| upstream.is_healthy() && upstream.breaker.is_available(now))
    }

    pub fn select(&self) -> Option<Arc<Upstream>> {
        let now = Instant::now();
        let candidates: Vec<_> = self.upstreams.iter()
            .filter(|upstream| upstream.is_healthy() && upstream.breaker.is_available(now))
            .cloned()
            .collect();
        let total_weight: u64 = candidates.iter().map(|upstream| u64::from(upstream.weight)).sum();
        if total_weight == 0 { return None; }

        for offset in 0..candidates.len() {
            let slot = self.cursor.fetch_add(1, Ordering::Relaxed) % total_weight;
            let mut cumulative = 0_u64;
            let mut selected = None;
            for upstream in &candidates {
                cumulative += u64::from(upstream.weight);
                if slot < cumulative {
                    selected = Some(upstream.clone());
                    break;
                }
            }
            let upstream = selected.or_else(|| candidates.last().cloned())?;
            if upstream.breaker.begin_request(now) { return Some(upstream); }
            if offset + 1 >= candidates.len() { break; }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upstream(name: &str, url: &str, weight: u32) -> UpstreamConfig {
        UpstreamConfig { name: name.into(), url: url.into(), weight, health_path: "/healthz".into(), failure_threshold: 3, cooldown_ms: 100 }
    }

    #[test]
    fn excludes_unhealthy_upstreams() {
        let pool = UpstreamPool::new(&[upstream("a", "http://127.0.0.1:1", 1), upstream("b", "http://127.0.0.1:2", 1)]);
        pool.all()[0].set_healthy(false);
        for _ in 0..10 { assert_eq!(pool.select().unwrap().name, "b"); }
    }
}
