//! Circuit breaker — protect against cascading failures from LLM API outages.
//!
//! Each LLM provider gets its own [`CircuitBreaker`] via [`CircuitBreakerRegistry`].
//! When a provider accumulates too many consecutive failures the breaker trips to
//! **Open** and rejects requests immediately, giving the downstream service time to
//! recover without flooding it with doomed requests.

/// The three states of a circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation — requests flow through.
    Closed,
    /// Too many failures — requests are rejected.
    Open,
    /// Tentatively allowing requests to test recovery.
    HalfOpen,
}

/// A circuit breaker that tracks LLM API call health and short-circuits
/// requests when the downstream service is unhealthy.
#[derive(Debug)]
pub struct CircuitBreaker {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    failure_threshold: u32,
    success_threshold: u32,
    open_duration_ms: u64,
    last_failure_at: Option<u64>,
    last_state_change_at: u64,
    total_trips: u32,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given thresholds.
    ///
    /// * `failure_threshold` — consecutive failures before tripping to Open.
    /// * `success_threshold` — consecutive successes in HalfOpen before closing.
    /// * `open_duration_ms` — how long to stay Open before transitioning to HalfOpen.
    pub fn new(failure_threshold: u32, success_threshold: u32, open_duration_ms: u64) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            failure_threshold,
            success_threshold,
            open_duration_ms,
            last_failure_at: None,
            last_state_change_at: 0,
            total_trips: 0,
        }
    }

    /// Returns `true` if a request is allowed to proceed.
    ///
    /// When the breaker is **Open** and the open duration has elapsed, it
    /// automatically transitions to **HalfOpen** and returns `true`.
    pub fn can_execute(&mut self, now: u64) -> bool {
        match self.state {
            CircuitState::Closed | CircuitState::HalfOpen => true,
            CircuitState::Open => {
                if now.saturating_sub(self.last_state_change_at) >= self.open_duration_ms {
                    self.transition(CircuitState::HalfOpen, now);
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Record a successful API call.
    ///
    /// * **HalfOpen**: increments success count; if it meets the threshold the
    ///   breaker transitions back to **Closed**.
    /// * **Closed**: resets the failure counter.
    pub fn record_success(&mut self, now: u64) {
        match self.state {
            CircuitState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= self.success_threshold {
                    self.transition(CircuitState::Closed, now);
                }
            }
            CircuitState::Closed => {
                self.failure_count = 0;
            }
            CircuitState::Open => {}
        }
    }

    /// Record a failed API call.
    ///
    /// * **Closed**: increments failure count; if it meets the threshold the
    ///   breaker trips to **Open**.
    /// * **HalfOpen**: immediately trips back to **Open**.
    pub fn record_failure(&mut self, now: u64) {
        self.failure_count += 1;
        self.last_failure_at = Some(now);

        match self.state {
            CircuitState::Closed => {
                if self.failure_count >= self.failure_threshold {
                    self.trip(now);
                }
            }
            CircuitState::HalfOpen => {
                self.trip(now);
            }
            CircuitState::Open => {}
        }
    }

    /// Current state of the circuit breaker.
    pub fn state(&self) -> CircuitState {
        self.state
    }

    /// Force the breaker back to Closed, resetting all counters.
    pub fn reset(&mut self, now: u64) {
        self.transition(CircuitState::Closed, now);
    }

    /// Total number of times the breaker has tripped to Open.
    pub fn trip_count(&self) -> u32 {
        self.total_trips
    }

    // ---- internal helpers ----

    fn trip(&mut self, now: u64) {
        self.total_trips += 1;
        self.transition(CircuitState::Open, now);
    }

    fn transition(&mut self, new_state: CircuitState, now: u64) {
        self.state = new_state;
        self.last_state_change_at = now;
        self.failure_count = 0;
        self.success_count = 0;
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(5, 2, 30_000)
    }
}

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// Per-provider circuit breaker registry (per D-05).
///
/// NOTE: `Clone` is intentionally not derived. `tokio::sync::RwLock` does not
/// implement `Clone`. The registry is designed to be shared via `Arc` on
/// `AgentEngine`, not cloned directly.
#[derive(Debug)]
pub struct CircuitBreakerRegistry {
    breakers: RwLock<HashMap<String, Arc<Mutex<CircuitBreaker>>>>,
}

impl CircuitBreakerRegistry {
    /// Create a registry pre-populated with breakers for the given provider keys.
    pub fn from_provider_keys(keys: impl Iterator<Item = String>) -> Self {
        let breakers: HashMap<String, Arc<Mutex<CircuitBreaker>>> = keys
            .map(|id| (id, Arc::new(Mutex::new(CircuitBreaker::default()))))
            .collect();
        Self {
            breakers: RwLock::new(breakers),
        }
    }

    /// Get the breaker for a provider, creating one with default thresholds if
    /// it doesn't already exist.
    pub async fn get(&self, provider: &str) -> Arc<Mutex<CircuitBreaker>> {
        // Fast path: read lock
        {
            let read = self.breakers.read().await;
            if let Some(breaker) = read.get(provider) {
                return breaker.clone();
            }
        }
        // Slow path: write lock, insert new breaker
        let mut write = self.breakers.write().await;
        write
            .entry(provider.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(CircuitBreaker::default())))
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_starts_closed() {
        let cb = CircuitBreaker::default();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn can_execute_true_when_closed() {
        let mut cb = CircuitBreaker::default();
        assert!(cb.can_execute(0));
    }

    #[test]
    fn single_failure_stays_closed() {
        let mut cb = CircuitBreaker::default();
        cb.record_failure(1);
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.can_execute(2));
    }

    #[test]
    fn five_failures_trips_to_open() {
        let mut cb = CircuitBreaker::default();
        for t in 1..=5 {
            cb.record_failure(t);
        }
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn open_rejects_execution() {
        let mut cb = CircuitBreaker::default();
        for t in 1..=5 {
            cb.record_failure(t);
        }
        assert!(!cb.can_execute(6));
    }

    #[test]
    fn open_transitions_to_half_open_after_timeout() {
        let mut cb = CircuitBreaker::default();
        for t in 1..=5 {
            cb.record_failure(t);
        }
        // Timeout is 30_000ms. State changed at t=5 so HalfOpen at t >= 30_005.
        assert!(cb.can_execute(30_005));
        assert_eq!(cb.state(), CircuitState::HalfOpen);
    }

    #[test]
    fn half_open_allows_execution() {
        let mut cb = CircuitBreaker::default();
        for t in 1..=5 {
            cb.record_failure(t);
        }
        // Transition to HalfOpen.
        assert!(cb.can_execute(30_005));
        assert_eq!(cb.state(), CircuitState::HalfOpen);
        // Should still allow execution in HalfOpen.
        assert!(cb.can_execute(30_006));
    }

    #[test]
    fn success_in_half_open_closes_after_threshold() {
        let mut cb = CircuitBreaker::default();
        for t in 1..=5 {
            cb.record_failure(t);
        }
        // Transition to HalfOpen.
        cb.can_execute(30_005);
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        cb.record_success(30_006);
        assert_eq!(cb.state(), CircuitState::HalfOpen); // 1 < threshold(2)

        cb.record_success(30_007);
        assert_eq!(cb.state(), CircuitState::Closed); // 2 >= threshold(2)
    }

    #[test]
    fn failure_in_half_open_trips_back_to_open() {
        let mut cb = CircuitBreaker::default();
        for t in 1..=5 {
            cb.record_failure(t);
        }
        cb.can_execute(30_005); // -> HalfOpen
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        cb.record_failure(30_006);
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn reset_forces_closed() {
        let mut cb = CircuitBreaker::default();
        for t in 1..=5 {
            cb.record_failure(t);
        }
        assert_eq!(cb.state(), CircuitState::Open);

        cb.reset(100);
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.can_execute(101));
    }

    #[test]
    fn trip_count_increments() {
        let mut cb = CircuitBreaker::default();
        assert_eq!(cb.trip_count(), 0);

        // First trip.
        for t in 1..=5 {
            cb.record_failure(t);
        }
        assert_eq!(cb.trip_count(), 1);

        // Recover and trip again.
        cb.can_execute(30_005); // -> HalfOpen
        cb.record_failure(30_006); // -> Open (second trip)
        assert_eq!(cb.trip_count(), 2);
    }

    #[test]
    fn success_in_closed_resets_failure_count() {
        let mut cb = CircuitBreaker::default();

        // Accumulate 4 failures (one short of threshold).
        for t in 1..=4 {
            cb.record_failure(t);
        }
        assert_eq!(cb.state(), CircuitState::Closed);

        // A success should reset the failure counter.
        cb.record_success(5);

        // Now another 4 failures should NOT trip the breaker since counter was reset.
        for t in 6..=9 {
            cb.record_failure(t);
        }
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn custom_thresholds_are_respected() {
        let mut cb = CircuitBreaker::new(2, 1, 1000);

        cb.record_failure(1);
        assert_eq!(cb.state(), CircuitState::Closed);

        cb.record_failure(2);
        assert_eq!(cb.state(), CircuitState::Open);

        // Transition to HalfOpen after 1000ms.
        assert!(cb.can_execute(1003));
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        // Single success closes (threshold = 1).
        cb.record_success(1004);
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    /// FOUN-04: Per-provider circuit breaker isolation.
    #[tokio::test]
    async fn provider_circuit_breaker_isolation() {
        let registry = CircuitBreakerRegistry::from_provider_keys(
            vec!["openai".to_string(), "anthropic".to_string()].into_iter(),
        );

        // Trip OpenAI's breaker
        let openai_breaker = registry.get("openai").await;
        {
            let mut b = openai_breaker.lock().await;
            let now = 1000;
            for _ in 0..5 {
                b.record_failure(now);
            }
            assert_eq!(b.state(), CircuitState::Open);
        }

        // Anthropic's breaker should still be Closed
        let anthropic_breaker = registry.get("anthropic").await;
        {
            let mut b = anthropic_breaker.lock().await;
            assert_eq!(b.state(), CircuitState::Closed);
            assert!(b.can_execute(1000));
        }

        // Dynamic provider creation
        let new_breaker = registry.get("new_provider").await;
        {
            let b = new_breaker.lock().await;
            assert_eq!(b.state(), CircuitState::Closed);
        }
    }

    #[test]
    fn open_rejects_before_timeout_elapses() {
        let mut cb = CircuitBreaker::new(3, 1, 5000);
        for t in 1..=3 {
            cb.record_failure(t);
        }
        assert_eq!(cb.state(), CircuitState::Open);

        // Just before the timeout (state changed at t=3, need 5000ms).
        assert!(!cb.can_execute(5002));
        assert_eq!(cb.state(), CircuitState::Open);

        // Exactly at timeout boundary.
        assert!(cb.can_execute(5003));
        assert_eq!(cb.state(), CircuitState::HalfOpen);
    }
}
