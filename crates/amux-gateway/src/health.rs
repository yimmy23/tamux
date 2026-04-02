use std::time::{Duration, Instant};

use amux_protocol::GatewayConnectionStatus;

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

    pub fn on_success(&mut self, now_ms: u64) {
        self.status = GatewayConnectionStatus::Connected;
        self.last_success_at = Some(now_ms);
        self.consecutive_failure_count = 0;
        self.current_backoff_secs = 0;
        self.last_error = None;
    }

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

    pub fn should_retry(&self, now_ms: u64) -> bool {
        if self.current_backoff_secs == 0 {
            return true;
        }
        if let Some(last_err) = self.last_error_at {
            now_ms.saturating_sub(last_err) >= self.current_backoff_secs * 1000
        } else {
            true
        }
    }
}

#[derive(Debug, Clone)]
pub struct TokenBucket {
    pub capacity: u32,
    tokens: f64,
    refill_rate: f64,
    last_refill: Instant,
}

impl TokenBucket {
    pub fn new(capacity: u32, refill_rate: f64) -> Self {
        Self {
            capacity,
            tokens: capacity as f64,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    pub fn slack() -> Self {
        Self::new(1, 1.0)
    }

    pub fn discord() -> Self {
        Self::new(5, 5.0)
    }

    pub fn telegram() -> Self {
        Self::new(1, 0.5)
    }

    pub fn try_acquire(&mut self) -> Option<Duration> {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            None
        } else {
            let deficit = 1.0 - self.tokens;
            Some(Duration::from_secs_f64(deficit / self.refill_rate))
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity as f64);
        self.last_refill = now;
    }
}
