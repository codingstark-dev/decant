//! Per-host rate limiter backed by `governor`.
//!
//! One `RateLimiter` per host is stored in a `DashMap`-like structure.
//! Callers `await` [`HostRateLimiter::acquire`] before each fetch.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

use governor::{DefaultDirectRateLimiter, Quota, RateLimiter as GovernorLimiter};

use crate::CoreError;

/// Manager that holds one governor rate-limiter per host.
#[derive(Clone, Default)]
pub struct HostRateLimiter {
    limiters: Arc<Mutex<HashMap<String, Arc<DefaultDirectRateLimiter>>>>,
    /// Requests per second (applies to every host).
    rps: u32,
}

impl HostRateLimiter {
    /// Create a new manager with the given requests-per-second limit.
    pub fn new(rps: u32) -> Self {
        Self {
            limiters: Arc::default(),
            rps: rps.max(1),
        }
    }

    /// Block (async) until a request slot is available for `host`.
    pub async fn acquire(&self, host: &str) -> Result<(), CoreError> {
        let limiter = self.get_or_create(host);
        // governor's `until_ready()` is sync; spin-yield to be async-friendly.
        loop {
            match limiter.check() {
                Ok(_) => return Ok(()),
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
            }
        }
    }

    fn get_or_create(&self, host: &str) -> Arc<DefaultDirectRateLimiter> {
        let mut map = self.limiters.lock().expect("rate limiter lock poisoned");
        map.entry(host.to_string())
            .or_insert_with(|| {
                let quota = Quota::per_second(NonZeroU32::new(self.rps).expect("rps > 0"));
                Arc::new(GovernorLimiter::direct(quota))
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_allows_immediate_requests() {
        let limiter = HostRateLimiter::new(100);
        // Warm up
        let _ = limiter.get_or_create("example.com");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let start = std::time::Instant::now();
        for _ in 0..5 {
            limiter.acquire("example.com").await.unwrap();
        }
        assert!(start.elapsed() < std::time::Duration::from_millis(100));
    }

    #[tokio::test]
    async fn test_rate_limiter_host_isolation() {
        // Setup a strict rate limiter (1 request per second)
        let limiter = HostRateLimiter::new(1);

        // First request to host A succeeds immediately
        limiter.acquire("host-a.com").await.unwrap();

        // A second request to host A would normally block/sleep.
        // But a request to host B should succeed immediately because of host isolation!
        let start = std::time::Instant::now();
        limiter.acquire("host-b.com").await.unwrap();
        assert!(start.elapsed() < std::time::Duration::from_millis(50));
    }
}
