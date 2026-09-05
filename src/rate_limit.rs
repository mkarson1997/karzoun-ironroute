use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

use crate::config::RateLimitConfig;

#[derive(Debug)]
pub struct RateLimiter {
    config: RateLimitConfig,
    buckets: Mutex<HashMap<String, Bucket>>,
}

#[derive(Debug)]
struct Bucket {
    tokens: f64,
    last_refill: Instant,
    last_seen: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Limited,
    Saturated,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    pub async fn check(&self, key: &str) -> Decision {
        let now = Instant::now();
        let mut buckets = self.buckets.lock().await;
        let idle = Duration::from_secs(self.config.idle_seconds);

        if !buckets.contains_key(key) && buckets.len() >= self.config.max_entries {
            buckets.retain(|_, bucket| now.duration_since(bucket.last_seen) < idle);
            if buckets.len() >= self.config.max_entries {
                return Decision::Saturated;
            }
        }

        let bucket = buckets.entry(key.to_owned()).or_insert(Bucket {
            tokens: self.config.capacity as f64,
            last_refill: now,
            last_seen: now,
        });

        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.config.refill_per_second)
            .min(self.config.capacity as f64);
        bucket.last_refill = now;
        bucket.last_seen = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Decision::Allow
        } else {
            Decision::Limited
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn enforces_capacity() {
        let limiter = RateLimiter::new(RateLimitConfig {
            capacity: 2,
            refill_per_second: 0.01,
            max_entries: 10,
            idle_seconds: 60,
        });
        assert_eq!(limiter.check("client").await, Decision::Allow);
        assert_eq!(limiter.check("client").await, Decision::Allow);
        assert_eq!(limiter.check("client").await, Decision::Limited);
    }

    #[tokio::test]
    async fn bounds_identity_cardinality() {
        let limiter = RateLimiter::new(RateLimitConfig {
            capacity: 1,
            refill_per_second: 1.0,
            max_entries: 1,
            idle_seconds: 60,
        });
        assert_eq!(limiter.check("a").await, Decision::Allow);
        assert_eq!(limiter.check("b").await, Decision::Saturated);
    }
}
