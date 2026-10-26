//! In-process token-bucket rate limiting for unauthenticated routes that
//! allocate or reveal shared state, notably `POST /create`.
//!
//! The app is a single small process, so a per-process limiter is enough: it
//! only needs to make room flooding expensive, not to be a globally consistent
//! quota.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Hard cap on tracked clients. A flood of distinct source addresses must not
/// grow the table without bound; once full, new clients are refused (fail
/// closed) until the periodic sweep frees space.
const MAX_BUCKETS: usize = 65_536;
/// A bucket untouched for this long is already full again, so its state can be
/// dropped.
const IDLE_TTL: Duration = Duration::from_secs(600);
const SWEEP_EVERY: Duration = Duration::from_secs(60);

pub struct RateLimiter {
    capacity: f64,
    refill_per_sec: f64,
    buckets: Mutex<Buckets>,
}

struct Buckets {
    map: HashMap<IpAddr, Bucket>,
    last_sweep: Instant,
}

struct Bucket {
    tokens: f64,
    updated: Instant,
}

impl RateLimiter {
    /// Allow `burst` requests at once, then `per_minute` sustained.
    pub fn new(burst: u32, per_minute: u32) -> Self {
        Self {
            capacity: burst.max(1) as f64,
            refill_per_sec: per_minute as f64 / 60.0,
            buckets: Mutex::new(Buckets {
                map: HashMap::new(),
                last_sweep: Instant::now(),
            }),
        }
    }

    /// Consume from one process-wide bucket, for callers that cannot tell
    /// clients apart.
    pub fn allow_shared(&self) -> bool {
        self.allow(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
    }

    /// Take one token for `ip`; `false` means the request should be throttled.
    pub fn allow(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut buckets = self.buckets.lock().unwrap();

        if now.duration_since(buckets.last_sweep) >= SWEEP_EVERY {
            buckets
                .map
                .retain(|_, b| now.duration_since(b.updated) < IDLE_TTL);
            buckets.last_sweep = now;
        }
        if buckets.map.len() >= MAX_BUCKETS && !buckets.map.contains_key(&ip) {
            return false;
        }

        let bucket = buckets.map.entry(ip).or_insert_with(|| Bucket {
            tokens: self.capacity,
            updated: now,
        });
        let elapsed = now.duration_since(bucket.updated).as_secs_f64();
        let tokens = (bucket.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        bucket.updated = now;
        if tokens < 1.0 {
            bucket.tokens = tokens;
            false
        } else {
            bucket.tokens = tokens - 1.0;
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip(last: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, last))
    }

    #[test]
    fn a_burst_is_allowed_then_throttled() {
        let limiter = RateLimiter::new(3, 3);
        assert!(limiter.allow(ip(1)));
        assert!(limiter.allow(ip(1)));
        assert!(limiter.allow(ip(1)));
        assert!(!limiter.allow(ip(1)), "the burst is spent");
    }

    #[test]
    fn budgets_are_per_client() {
        let limiter = RateLimiter::new(1, 1);
        assert!(limiter.allow(ip(1)));
        assert!(!limiter.allow(ip(1)));
        assert!(
            limiter.allow(ip(2)),
            "a throttled client must not starve another address"
        );
    }

    #[test]
    fn tokens_refill_over_time() {
        let limiter = RateLimiter::new(1, 60_000);
        assert!(limiter.allow(ip(1)));
        assert!(!limiter.allow(ip(1)));
        std::thread::sleep(Duration::from_millis(10));
        assert!(limiter.allow(ip(1)), "a refilled bucket must allow again");
    }
}
