//! Token bucket rate limiter for plugin API proxy.
//!
//! Each plugin gets its own `TokenBucket` tracked in a `RateLimiterMap`.
//! Tokens refill continuously based on elapsed time at the configured RPM.

use std::collections::HashMap;
use std::time::Instant;

/// Default requests per minute when not declared in plugin manifest.
pub const DEFAULT_REQUESTS_PER_MINUTE: u32 = 60;

/// A token bucket rate limiter with time-based refill.
///
/// Tokens refill continuously at `capacity / 60.0` tokens per second.
/// Each `try_acquire()` call consumes one token if available.
#[derive(Debug)]
pub struct TokenBucket {
    capacity: u32,
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucket {
    /// Create a new token bucket with the given RPM capacity.
    pub fn new(requests_per_minute: u32) -> Self {
        Self {
            capacity: requests_per_minute,
            tokens: requests_per_minute as f64,
            last_refill: Instant::now(),
        }
    }

    /// Try to acquire one token. Returns true if a token was available.
    pub fn try_acquire(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Refill tokens based on elapsed time since last refill.
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed_secs = now.duration_since(self.last_refill).as_secs_f64();
        let refill_rate = self.capacity as f64 / 60.0; // tokens per second
        self.tokens = (self.tokens + elapsed_secs * refill_rate).min(self.capacity as f64);
        self.last_refill = now;
    }

    /// Expose last_refill for testing (allows simulating time passage).
    #[cfg(test)]
    fn set_last_refill(&mut self, instant: Instant) {
        self.last_refill = instant;
    }
}

/// A map of plugin names to their rate limiter buckets.
///
/// Creates buckets lazily on first check per plugin name (Pitfall 5 from research).
#[derive(Debug)]
pub struct RateLimiterMap {
    buckets: HashMap<String, TokenBucket>,
}

impl RateLimiterMap {
    /// Create a new empty rate limiter map.
    pub fn new() -> Self {
        Self {
            buckets: HashMap::new(),
        }
    }

    /// Check if a plugin can make a request. Creates a bucket if none exists.
    /// Returns true if the request is allowed, false if rate limited.
    pub fn check(&mut self, plugin_name: &str, rpm: u32) -> bool {
        self.buckets
            .entry(plugin_name.to_string())
            .or_insert_with(|| TokenBucket::new(rpm))
            .try_acquire()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn new_bucket_allows_first_request() {
        let mut bucket = TokenBucket::new(60);
        assert!(bucket.try_acquire());
    }

    #[test]
    fn exhausting_tokens_blocks_next_request() {
        let mut bucket = TokenBucket::new(2);
        assert!(bucket.try_acquire()); // token 1
        assert!(bucket.try_acquire()); // token 2
        assert!(!bucket.try_acquire()); // depleted
    }

    #[test]
    fn tokens_refill_after_time() {
        let mut bucket = TokenBucket::new(60);
        // Exhaust all tokens
        for _ in 0..60 {
            bucket.try_acquire();
        }
        assert!(!bucket.try_acquire()); // depleted

        // Simulate 2 seconds passing (should refill 2 tokens at 60 RPM = 1/sec)
        bucket.set_last_refill(Instant::now() - Duration::from_secs(2));
        assert!(bucket.try_acquire());
    }

    #[test]
    fn rate_limiter_map_creates_bucket_on_first_check() {
        let mut map = RateLimiterMap::new();
        // First check should succeed (creates new bucket with full capacity)
        assert!(map.check("my-plugin", 60));
    }

    #[test]
    fn rate_limiter_map_default_rpm() {
        let mut map = RateLimiterMap::new();
        // Use default RPM constant
        assert!(map.check("test-plugin", DEFAULT_REQUESTS_PER_MINUTE));
    }

    #[test]
    fn rate_limiter_map_tracks_per_plugin() {
        let mut map = RateLimiterMap::new();
        // Exhaust plugin A with capacity 1
        assert!(map.check("plugin-a", 1));
        assert!(!map.check("plugin-a", 1)); // depleted

        // Plugin B should still work (separate bucket)
        assert!(map.check("plugin-b", 1));
    }
}
