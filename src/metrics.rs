use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct Metrics {
    requests: AtomicU64,
    responses_2xx: AtomicU64,
    responses_4xx: AtomicU64,
    responses_5xx: AtomicU64,
    rate_limited: AtomicU64,
    load_shed: AtomicU64,
    upstream_failures: AtomicU64,
    retries: AtomicU64,
    signature_rejections: AtomicU64,
}

impl Metrics {
    pub fn request(&self) { self.requests.fetch_add(1, Ordering::Relaxed); }
    pub fn response(&self, status: u16) {
        match status {
            200..=299 => { self.responses_2xx.fetch_add(1, Ordering::Relaxed); }
            400..=499 => { self.responses_4xx.fetch_add(1, Ordering::Relaxed); }
            500..=599 => { self.responses_5xx.fetch_add(1, Ordering::Relaxed); }
            _ => {}
        }
    }
    pub fn rate_limited(&self) { self.rate_limited.fetch_add(1, Ordering::Relaxed); }
    pub fn load_shed(&self) { self.load_shed.fetch_add(1, Ordering::Relaxed); }
    pub fn upstream_failure(&self) { self.upstream_failures.fetch_add(1, Ordering::Relaxed); }
    pub fn retry(&self) { self.retries.fetch_add(1, Ordering::Relaxed); }
    pub fn signature_rejection(&self) { self.signature_rejections.fetch_add(1, Ordering::Relaxed); }

    pub fn render(&self) -> String {
        let value = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        format!(
            "# TYPE ironroute_requests_total counter\nironroute_requests_total {}\n\
# TYPE ironroute_responses_total counter\nironroute_responses_total{{class=\"2xx\"}} {}\nironroute_responses_total{{class=\"4xx\"}} {}\nironroute_responses_total{{class=\"5xx\"}} {}\n\
# TYPE ironroute_rate_limited_total counter\nironroute_rate_limited_total {}\n\
# TYPE ironroute_load_shed_total counter\nironroute_load_shed_total {}\n\
# TYPE ironroute_upstream_failures_total counter\nironroute_upstream_failures_total {}\n\
# TYPE ironroute_retries_total counter\nironroute_retries_total {}\n\
# TYPE ironroute_signature_rejections_total counter\nironroute_signature_rejections_total {}\n",
            value(&self.requests), value(&self.responses_2xx), value(&self.responses_4xx), value(&self.responses_5xx),
            value(&self.rate_limited), value(&self.load_shed), value(&self.upstream_failures), value(&self.retries), value(&self.signature_rejections)
        )
    }
}
