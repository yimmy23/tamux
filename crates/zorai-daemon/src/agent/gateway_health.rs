#![allow(dead_code)]

//! Gateway platform health tracking, backoff, and rate limiting.
//!
//! Pure-function module — no I/O, no async, fully testable.
//! Used by `gateway_loop.rs` to decide when to retry failed platforms
//! and to enforce per-platform send rate limits.

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Connection status for a gateway platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayConnectionStatus {
    Connected,
    Disconnected,
    Error,
}

/// Per-platform health state tracking failures, backoff, and last success/error.
///
/// Used by `gateway_loop` to decide whether a platform is healthy enough to
/// attempt message delivery. Backoff schedule per D-04:
/// 1 failure -> 5s, 2 -> 10s, 3 -> 30s, 4+ -> 60s (cap).
#[derive(Debug, Clone)]
pub struct PlatformHealthState {
    pub status: GatewayConnectionStatus,
    pub last_success_at: Option<u64>,
    pub last_error_at: Option<u64>,
    pub consecutive_failure_count: u32,
    pub last_error: Option<String>,
    pub current_backoff_secs: u64,
}

impl PlatformHealthState {
    /// Create a new health state — starts Disconnected with zero failures.
    pub fn new() -> Self {
        Self {
            status: GatewayConnectionStatus::Disconnected,
            last_success_at: None,
            last_error_at: None,
            consecutive_failure_count: 0,
            last_error: None,
            current_backoff_secs: 0,
        }
    }

    /// Record a successful operation. Transitions to Connected, resets all
    /// failure tracking. Per D-04: reset on first successful poll.
    pub fn on_success(&mut self, now_ms: u64) {
        self.status = GatewayConnectionStatus::Connected;
        self.last_success_at = Some(now_ms);
        self.consecutive_failure_count = 0;
        self.current_backoff_secs = 0;
        self.last_error = None;
    }

    /// Record a failure. Increments failure count, sets Error status, and
    /// computes exponential backoff per D-04:
    /// 1 failure -> 5s, 2 -> 10s, 3 -> 30s, 4+ -> 60s (cap).
    pub fn on_failure(&mut self, now_ms: u64, error: String) {
        self.consecutive_failure_count += 1;
        self.status = GatewayConnectionStatus::Error;
        self.last_error_at = Some(now_ms);
        self.last_error = Some(error);
        self.current_backoff_secs = match self.consecutive_failure_count {
            1 => 5,
            2 => 10,
            3 => 30,
            _ => 60,
        };
    }

    /// Returns true if enough time has elapsed since the last error to retry,
    /// or if there is no active backoff (backoff_secs == 0).
    pub fn should_retry(&self, now_ms: u64) -> bool {
        if self.current_backoff_secs == 0 {
            return true;
        }
        if let Some(last_err) = self.last_error_at {
            let elapsed_ms = now_ms.saturating_sub(last_err);
            elapsed_ms >= self.current_backoff_secs * 1000
        } else {
            true
        }
    }

    /// Check if the status changed from a previous value (for event emission).
    pub fn status_changed(&self, old: GatewayConnectionStatus) -> bool {
        self.status != old
    }

    /// Returns true only when the platform transitions from a non-connected
    /// state into `Connected`.
    pub fn is_reconnect_transition(&self, old: GatewayConnectionStatus) -> bool {
        self.status == GatewayConnectionStatus::Connected
            && old != GatewayConnectionStatus::Connected
    }
}

/// Token-bucket rate limiter for gateway message sends.
///
/// Each platform has different rate limits (per D-10):
/// - Slack: 1 msg/sec
/// - Discord: 5 msg/sec
/// - Telegram: 30 msg/min (~0.5 msg/sec)
#[derive(Debug, Clone)]
pub struct TokenBucket {
    pub capacity: u32,
    tokens: f64,
    refill_rate: f64,
    last_refill: Instant,
}

impl TokenBucket {
    /// Create a new token bucket. Starts full.
    ///
    /// * `capacity` — maximum tokens in the bucket.
    /// * `refill_rate` — tokens added per second.
    pub fn new(capacity: u32, refill_rate: f64) -> Self {
        Self {
            capacity,
            tokens: capacity as f64,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    /// Slack rate limiter: 1 msg/sec per D-10.
    pub fn slack() -> Self {
        Self::new(1, 1.0)
    }

    /// Discord rate limiter: 5 msg/sec per D-10.
    pub fn discord() -> Self {
        Self::new(5, 5.0)
    }

    /// Telegram rate limiter: 30 msg/min (~0.5 msg/sec) per D-10.
    pub fn telegram() -> Self {
        Self::new(1, 0.5)
    }

    /// Try to acquire one token. Returns `None` if a token was available
    /// (request can proceed), or `Some(Duration)` indicating how long to
    /// wait before a token will be available.
    pub fn try_acquire(&mut self) -> Option<Duration> {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            None
        } else {
            let deficit = 1.0 - self.tokens;
            let wait_secs = deficit / self.refill_rate;
            Some(Duration::from_secs_f64(wait_secs))
        }
    }

    /// Refill tokens based on elapsed time since last refill.
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity as f64);
        self.last_refill = now;
    }

    /// Create a token bucket with a specific instant for testing.
    #[cfg(test)]
    fn new_with_instant(capacity: u32, refill_rate: f64, instant: Instant) -> Self {
        Self {
            capacity,
            tokens: capacity as f64,
            refill_rate,
            last_refill: instant,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // PlatformHealthState tests
    // -----------------------------------------------------------------------

    #[test]
    fn new_starts_disconnected_with_zero_failures() {
        let state = PlatformHealthState::new();
        assert_eq!(state.status, GatewayConnectionStatus::Disconnected);
        assert_eq!(state.consecutive_failure_count, 0);
        assert_eq!(state.current_backoff_secs, 0);
        assert!(state.last_success_at.is_none());
        assert!(state.last_error_at.is_none());
        assert!(state.last_error.is_none());
    }

    #[test]
    fn on_success_transitions_to_connected() {
        let mut state = PlatformHealthState::new();
        state.on_success(1000);
        assert_eq!(state.status, GatewayConnectionStatus::Connected);
        assert_eq!(state.last_success_at, Some(1000));
        assert_eq!(state.consecutive_failure_count, 0);
        assert_eq!(state.current_backoff_secs, 0);
    }

    #[test]
    fn on_failure_sets_error_and_computes_backoff() {
        let mut state = PlatformHealthState::new();

        state.on_failure(1000, "timeout".to_string());
        assert_eq!(state.status, GatewayConnectionStatus::Error);
        assert_eq!(state.consecutive_failure_count, 1);
        assert_eq!(state.current_backoff_secs, 5);

        state.on_failure(2000, "timeout".to_string());
        assert_eq!(state.consecutive_failure_count, 2);
        assert_eq!(state.current_backoff_secs, 10);

        state.on_failure(3000, "timeout".to_string());
        assert_eq!(state.consecutive_failure_count, 3);
        assert_eq!(state.current_backoff_secs, 30);

        state.on_failure(4000, "timeout".to_string());
        assert_eq!(state.consecutive_failure_count, 4);
        assert_eq!(state.current_backoff_secs, 60);

        // Further failures stay capped at 60s.
        state.on_failure(5000, "timeout".to_string());
        assert_eq!(state.current_backoff_secs, 60);
    }

    #[test]
    fn should_retry_returns_true_when_no_backoff() {
        let state = PlatformHealthState::new();
        assert!(state.should_retry(0));
    }

    #[test]
    fn should_retry_returns_false_before_backoff_elapsed() {
        let mut state = PlatformHealthState::new();
        state.on_failure(1000, "error".to_string());
        // Backoff is 5s = 5000ms. At 1000 + 4999 = 5999ms it should still be false.
        assert!(!state.should_retry(5999));
    }

    #[test]
    fn should_retry_returns_true_after_backoff_elapsed() {
        let mut state = PlatformHealthState::new();
        state.on_failure(1000, "error".to_string());
        // Backoff is 5s = 5000ms. At 1000 + 5000 = 6000ms it should be true.
        assert!(state.should_retry(6000));
    }

    #[test]
    fn on_success_after_failures_resets_all_fields() {
        let mut state = PlatformHealthState::new();
        state.on_failure(1000, "err1".to_string());
        state.on_failure(2000, "err2".to_string());
        state.on_failure(3000, "err3".to_string());
        assert_eq!(state.consecutive_failure_count, 3);
        assert_eq!(state.current_backoff_secs, 30);

        state.on_success(10000);
        assert_eq!(state.status, GatewayConnectionStatus::Connected);
        assert_eq!(state.consecutive_failure_count, 0);
        assert_eq!(state.current_backoff_secs, 0);
        assert!(state.last_error.is_none());
        assert_eq!(state.last_success_at, Some(10000));
    }

    #[test]
    fn status_changed_detects_transitions() {
        let mut state = PlatformHealthState::new();
        let old = state.status;
        state.on_failure(1000, "err".to_string());
        assert!(state.status_changed(old)); // Disconnected -> Error

        let old = state.status;
        state.on_success(2000);
        assert!(state.status_changed(old)); // Error -> Connected

        let old = state.status;
        state.on_success(3000);
        assert!(!state.status_changed(old)); // Connected -> Connected (no change)
    }

    #[test]
    fn reconnect_transition_from_disconnected_or_error_to_connected_returns_true() {
        let mut state = PlatformHealthState::new();

        let old = state.status;
        state.on_success(1000);
        assert!(state.is_reconnect_transition(old));

        state.on_failure(2000, "err".to_string());
        let old = state.status;
        state.on_success(3000);
        assert!(state.is_reconnect_transition(old));
    }

    #[test]
    fn reconnect_transition_connected_to_connected_returns_false() {
        let mut state = PlatformHealthState::new();
        state.on_success(1000);

        let old = state.status;
        state.on_success(2000);
        assert!(!state.is_reconnect_transition(old));
    }

    // -----------------------------------------------------------------------
    // TokenBucket tests
    // -----------------------------------------------------------------------

    #[test]
    fn new_starts_with_full_tokens() {
        let bucket = TokenBucket::new(5, 5.0);
        assert_eq!(bucket.capacity, 5);
        // tokens should be capacity (5.0)
        assert!((bucket.tokens - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn try_acquire_returns_none_when_tokens_available() {
        let mut bucket = TokenBucket::new(3, 1.0);
        assert!(bucket.try_acquire().is_none());
        assert!(bucket.try_acquire().is_none());
        assert!(bucket.try_acquire().is_none());
    }

    #[test]
    fn try_acquire_returns_some_duration_when_empty() {
        let mut bucket = TokenBucket::new(1, 1.0);
        assert!(bucket.try_acquire().is_none()); // consume the one token
        let wait = bucket.try_acquire();
        assert!(wait.is_some());
        // Wait should be approximately 1s (one token at 1 token/sec)
        let wait_ms = wait.unwrap().as_millis();
        assert!(wait_ms > 0 && wait_ms <= 1100, "wait_ms was {wait_ms}");
    }

    #[test]
    fn tokens_refill_over_elapsed_time() {
        let past = Instant::now() - Duration::from_secs(2);
        let mut bucket = TokenBucket::new_with_instant(3, 1.0, past);
        // Manually set tokens to 0 to simulate empty bucket
        bucket.tokens = 0.0;

        // After 2 seconds at 1 token/sec, refill should add 2 tokens
        let result = bucket.try_acquire();
        assert!(result.is_none(), "should have refilled enough tokens");
    }

    #[test]
    fn tokens_do_not_exceed_capacity() {
        let past = Instant::now() - Duration::from_secs(100);
        let mut bucket = TokenBucket::new_with_instant(3, 1.0, past);
        bucket.refill();
        assert!((bucket.tokens - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn slack_rate_limiter_capacity_and_rate() {
        let bucket = TokenBucket::slack();
        assert_eq!(bucket.capacity, 1);
        assert!((bucket.refill_rate - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn discord_rate_limiter_capacity_and_rate() {
        let bucket = TokenBucket::discord();
        assert_eq!(bucket.capacity, 5);
        assert!((bucket.refill_rate - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn telegram_rate_limiter_capacity_and_rate() {
        let bucket = TokenBucket::telegram();
        assert_eq!(bucket.capacity, 1);
        assert!((bucket.refill_rate - 0.5).abs() < f64::EPSILON);
    }
}
