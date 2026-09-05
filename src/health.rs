use std::{sync::Arc, time::Duration};

use reqwest::Client;
use tracing::{debug, warn};

use crate::{config::HealthConfig, upstream::UpstreamPool};

pub fn spawn_health_checks(pool: Arc<UpstreamPool>, client: Client, config: HealthConfig) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(config.interval_ms));
        loop {
            ticker.tick().await;
            for upstream in pool.all() {
                let Ok(url) = upstream.health_url() else {
                    upstream.set_healthy(false);
                    continue;
                };
                let healthy = match client.get(url).timeout(Duration::from_millis(config.timeout_ms)).send().await {
                    Ok(response) => response.status().is_success(),
                    Err(error) => {
                        debug!(upstream = %upstream.name, %error, "health check failed");
                        false
                    }
                };
                let previous = upstream.is_healthy();
                upstream.set_healthy(healthy);
                if previous && !healthy { warn!(upstream = %upstream.name, "upstream became unhealthy"); }
            }
        }
    });
}
