use std::{collections::HashSet, fs, net::SocketAddr, path::Path};

use serde::Deserialize;
use thiserror::Error;
use url::Url;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_listen")]
    pub listen: String,
    #[serde(default = "default_max_body")]
    pub max_body_bytes: usize,
    #[serde(default = "default_max_in_flight")]
    pub max_in_flight: usize,
    #[serde(default = "default_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_retries")]
    pub retry_attempts: usize,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub health: HealthConfig,
    pub hmac: Option<HmacConfig>,
    pub upstreams: Vec<UpstreamConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpstreamConfig {
    pub name: String,
    pub url: String,
    #[serde(default = "default_weight")]
    pub weight: u32,
    #[serde(default = "default_health_path")]
    pub health_path: String,
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,
    #[serde(default = "default_cooldown_ms")]
    pub cooldown_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_bucket_capacity")]
    pub capacity: u32,
    #[serde(default = "default_refill_per_second")]
    pub refill_per_second: f64,
    #[serde(default = "default_rate_limit_entries")]
    pub max_entries: usize,
    #[serde(default = "default_rate_limit_idle_seconds")]
    pub idle_seconds: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            capacity: default_bucket_capacity(),
            refill_per_second: default_refill_per_second(),
            max_entries: default_rate_limit_entries(),
            idle_seconds: default_rate_limit_idle_seconds(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct HealthConfig {
    #[serde(default = "default_health_interval_ms")]
    pub interval_ms: u64,
    #[serde(default = "default_health_timeout_ms")]
    pub timeout_ms: u64,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_health_interval_ms(),
            timeout_ms: default_health_timeout_ms(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct HmacConfig {
    #[serde(default = "default_hmac_secret_env")]
    pub secret_env: String,
    #[serde(default = "default_signature_header")]
    pub signature_header: String,
    #[serde(default = "default_timestamp_header")]
    pub timestamp_header: String,
    #[serde(default = "default_clock_skew_seconds")]
    pub max_clock_skew_seconds: u64,
    #[serde(default)]
    pub protected_prefixes: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read configuration: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid TOML configuration: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("invalid configuration: {0}")]
    Invalid(String),
}

impl Config {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let raw = fs::read_to_string(path)?;
        let config: Self = toml::from_str(&raw)?;
        config.validate()?;
        Ok(config)
    }

    pub fn socket_addr(&self) -> Result<SocketAddr, ConfigError> {
        self.listen.parse().map_err(|_| {
            ConfigError::Invalid(format!("listen must be a socket address: {}", self.listen))
        })
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        self.socket_addr()?;
        if self.upstreams.is_empty() {
            return Err(ConfigError::Invalid(
                "at least one upstream is required".into(),
            ));
        }
        if self.max_body_bytes == 0 || self.max_in_flight == 0 || self.request_timeout_ms == 0 {
            return Err(ConfigError::Invalid(
                "body, concurrency and timeout limits must be non-zero".into(),
            ));
        }
        if self.retry_attempts > 5 {
            return Err(ConfigError::Invalid("retry_attempts must be <= 5".into()));
        }
        if self.rate_limit.capacity == 0
            || !self.rate_limit.refill_per_second.is_finite()
            || self.rate_limit.refill_per_second <= 0.0
            || self.rate_limit.max_entries == 0
            || self.rate_limit.idle_seconds == 0
        {
            return Err(ConfigError::Invalid(
                "rate_limit values must be positive and finite".into(),
            ));
        }
        if self.health.interval_ms == 0 || self.health.timeout_ms == 0 {
            return Err(ConfigError::Invalid(
                "health check intervals must be non-zero".into(),
            ));
        }

        let mut names = HashSet::new();
        for upstream in &self.upstreams {
            if upstream.name.trim().is_empty() || !names.insert(upstream.name.clone()) {
                return Err(ConfigError::Invalid(format!(
                    "upstream names must be non-empty and unique: {}",
                    upstream.name
                )));
            }
            if upstream.weight == 0 || upstream.failure_threshold == 0 || upstream.cooldown_ms == 0
            {
                return Err(ConfigError::Invalid(format!(
                    "upstream {} has invalid weight/breaker settings",
                    upstream.name
                )));
            }
            let parsed = Url::parse(&upstream.url).map_err(|_| {
                ConfigError::Invalid(format!("upstream {} has invalid URL", upstream.name))
            })?;
            if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
                return Err(ConfigError::Invalid(format!(
                    "upstream {} must use http or https",
                    upstream.name
                )));
            }
            if !upstream.health_path.starts_with('/') {
                return Err(ConfigError::Invalid(format!(
                    "upstream {} health_path must start with /",
                    upstream.name
                )));
            }
        }

        if let Some(hmac) = &self.hmac {
            if hmac.secret_env.trim().is_empty() || hmac.max_clock_skew_seconds == 0 {
                return Err(ConfigError::Invalid(
                    "HMAC secret_env and max clock skew must be configured".into(),
                ));
            }
            if hmac
                .protected_prefixes
                .iter()
                .any(|prefix| !prefix.starts_with('/'))
            {
                return Err(ConfigError::Invalid(
                    "HMAC protected prefixes must start with /".into(),
                ));
            }
        }
        Ok(())
    }
}

fn default_listen() -> String {
    "0.0.0.0:8080".into()
}
fn default_max_body() -> usize {
    1_048_576
}
fn default_max_in_flight() -> usize {
    512
}
fn default_timeout_ms() -> u64 {
    5_000
}
fn default_retries() -> usize {
    1
}
fn default_weight() -> u32 {
    1
}
fn default_health_path() -> String {
    "/healthz".into()
}
fn default_failure_threshold() -> u32 {
    5
}
fn default_cooldown_ms() -> u64 {
    5_000
}
fn default_bucket_capacity() -> u32 {
    100
}
fn default_refill_per_second() -> f64 {
    50.0
}
fn default_rate_limit_entries() -> usize {
    10_000
}
fn default_rate_limit_idle_seconds() -> u64 {
    300
}
fn default_health_interval_ms() -> u64 {
    5_000
}
fn default_health_timeout_ms() -> u64 {
    1_500
}
fn default_hmac_secret_env() -> String {
    "IRONROUTE_HMAC_SECRET".into()
}
fn default_signature_header() -> String {
    "x-ironroute-signature".into()
}
fn default_timestamp_header() -> String {
    "x-ironroute-timestamp".into()
}
fn default_clock_skew_seconds() -> u64 {
    60
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_upstreams() {
        let config = Config {
            listen: "127.0.0.1:8080".into(),
            max_body_bytes: 10,
            max_in_flight: 1,
            request_timeout_ms: 100,
            retry_attempts: 0,
            rate_limit: RateLimitConfig::default(),
            health: HealthConfig::default(),
            hmac: None,
            upstreams: vec![
                UpstreamConfig {
                    name: "a".into(),
                    url: "http://127.0.0.1:1".into(),
                    weight: 1,
                    health_path: "/healthz".into(),
                    failure_threshold: 2,
                    cooldown_ms: 10,
                },
                UpstreamConfig {
                    name: "a".into(),
                    url: "http://127.0.0.1:2".into(),
                    weight: 1,
                    health_path: "/healthz".into(),
                    failure_threshold: 2,
                    cooldown_ms: 10,
                },
            ],
        };
        assert!(config.validate().is_err());
    }
}
