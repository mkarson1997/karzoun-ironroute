use std::{sync::Mutex, time::{Duration, Instant}};

#[derive(Debug)]
pub struct CircuitBreaker {
    failure_threshold: u32,
    cooldown: Duration,
    inner: Mutex<Inner>,
}

#[derive(Debug)]
struct Inner {
    failures: u32,
    open_until: Option<Instant>,
    half_open_in_flight: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    Closed,
    Open,
    HalfOpen,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, cooldown: Duration) -> Self {
        Self {
            failure_threshold,
            cooldown,
            inner: Mutex::new(Inner { failures: 0, open_until: None, half_open_in_flight: false }),
        }
    }

    pub fn is_available(&self, now: Instant) -> bool {
        let inner = self.inner.lock().expect("circuit breaker mutex poisoned");
        match inner.open_until {
            None => true,
            Some(until) if now >= until => !inner.half_open_in_flight,
            Some(_) => false,
        }
    }

    pub fn begin_request(&self, now: Instant) -> bool {
        let mut inner = self.inner.lock().expect("circuit breaker mutex poisoned");
        match inner.open_until {
            None => true,
            Some(until) if now >= until && !inner.half_open_in_flight => {
                inner.half_open_in_flight = true;
                true
            }
            Some(_) => false,
        }
    }

    pub fn on_success(&self) {
        let mut inner = self.inner.lock().expect("circuit breaker mutex poisoned");
        inner.failures = 0;
        inner.open_until = None;
        inner.half_open_in_flight = false;
    }

    pub fn on_failure(&self, now: Instant) {
        let mut inner = self.inner.lock().expect("circuit breaker mutex poisoned");
        inner.half_open_in_flight = false;
        inner.failures = inner.failures.saturating_add(1);
        if inner.failures >= self.failure_threshold {
            inner.open_until = Some(now + self.cooldown);
        }
    }

    pub fn state(&self, now: Instant) -> BreakerState {
        let inner = self.inner.lock().expect("circuit breaker mutex poisoned");
        match inner.open_until {
            None => BreakerState::Closed,
            Some(until) if now < until => BreakerState::Open,
            Some(_) => BreakerState::HalfOpen,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_after_threshold_and_recovers_to_half_open() {
        let breaker = CircuitBreaker::new(2, Duration::from_millis(2));
        let now = Instant::now();
        assert!(breaker.begin_request(now));
        breaker.on_failure(now);
        breaker.on_failure(now);
        assert_eq!(breaker.state(now), BreakerState::Open);
        std::thread::sleep(Duration::from_millis(3));
        let later = Instant::now();
        assert_eq!(breaker.state(later), BreakerState::HalfOpen);
        assert!(breaker.begin_request(later));
        assert!(!breaker.begin_request(later));
        breaker.on_success();
        assert_eq!(breaker.state(Instant::now()), BreakerState::Closed);
    }
}
