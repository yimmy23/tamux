//! Context budget enforcement for sub-agents.
//!
//! Each sub-agent can have a token budget that limits how much context it
//! may accumulate. When exceeded, the configured overflow action determines
//! whether to compress, truncate, or error out.

use crate::agent::types::ContextOverflowAction;

/// Tracks and enforces a context token budget for a sub-agent.
#[derive(Debug, Clone)]
pub struct ContextBudget {
    max_tokens: u32,
    overflow_action: ContextOverflowAction,
    consumed: u32,
}

/// Result of a budget check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetStatus {
    /// Under budget — proceed normally.
    Ok,
    /// Over budget — the overflow action should be taken.
    Exceeded {
        consumed: u32,
        max: u32,
        overflow_action: ContextOverflowAction,
    },
    /// Approaching the limit (> 90% consumed).
    Warning { consumed: u32, max: u32 },
}

impl ContextBudget {
    /// Create a new context budget.
    pub fn new(max_tokens: u32, overflow_action: ContextOverflowAction) -> Self {
        Self {
            max_tokens: max_tokens.max(256),
            overflow_action,
            consumed: 0,
        }
    }

    /// Record token consumption. Returns the new budget status.
    pub fn record(&mut self, tokens: u32) -> BudgetStatus {
        self.consumed = self.consumed.saturating_add(tokens);
        self.check()
    }

    /// Set the consumed count directly (e.g. after re-estimating from thread).
    pub fn set_consumed(&mut self, tokens: u32) {
        self.consumed = tokens;
    }

    /// Check current budget status without recording new tokens.
    pub fn check(&self) -> BudgetStatus {
        if self.consumed > self.max_tokens {
            BudgetStatus::Exceeded {
                consumed: self.consumed,
                max: self.max_tokens,
                overflow_action: self.overflow_action,
            }
        } else if self.consumed > self.max_tokens * 9 / 10 {
            BudgetStatus::Warning {
                consumed: self.consumed,
                max: self.max_tokens,
            }
        } else {
            BudgetStatus::Ok
        }
    }

    /// Remaining tokens before the budget is exceeded.
    pub fn remaining(&self) -> u32 {
        self.max_tokens.saturating_sub(self.consumed)
    }

    /// Current utilization as a percentage (0–100+).
    pub fn utilization_pct(&self) -> u32 {
        if self.max_tokens == 0 {
            return 100;
        }
        (self.consumed as u64 * 100 / self.max_tokens as u64) as u32
    }

    /// The configured maximum.
    pub fn max_tokens(&self) -> u32 {
        self.max_tokens
    }

    /// Currently consumed tokens.
    pub fn consumed(&self) -> u32 {
        self.consumed
    }

    /// The configured overflow action.
    pub fn overflow_action(&self) -> ContextOverflowAction {
        self.overflow_action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_budget_starts_at_zero() {
        let budget = ContextBudget::new(10_000, ContextOverflowAction::Compress);
        assert_eq!(budget.consumed(), 0);
        assert_eq!(budget.remaining(), 10_000);
        assert_eq!(budget.utilization_pct(), 0);
    }

    #[test]
    fn minimum_budget_is_256() {
        let budget = ContextBudget::new(10, ContextOverflowAction::Compress);
        assert_eq!(budget.max_tokens(), 256);
    }

    #[test]
    fn record_tracks_consumption() {
        let mut budget = ContextBudget::new(10_000, ContextOverflowAction::Compress);
        budget.record(3000);
        assert_eq!(budget.consumed(), 3000);
        assert_eq!(budget.remaining(), 7000);
        assert_eq!(budget.utilization_pct(), 30);
    }

    #[test]
    fn check_ok_when_under_budget() {
        let mut budget = ContextBudget::new(10_000, ContextOverflowAction::Compress);
        budget.record(5000);
        assert_eq!(budget.check(), BudgetStatus::Ok);
    }

    #[test]
    fn check_warning_above_90_percent() {
        let mut budget = ContextBudget::new(10_000, ContextOverflowAction::Compress);
        budget.record(9100);
        assert!(matches!(budget.check(), BudgetStatus::Warning { .. }));
    }

    #[test]
    fn check_exceeded_over_budget() {
        let mut budget = ContextBudget::new(10_000, ContextOverflowAction::Compress);
        let status = budget.record(10_001);
        assert!(matches!(status, BudgetStatus::Exceeded { .. }));
    }

    #[test]
    fn exceeded_carries_overflow_action_compress() {
        let mut budget = ContextBudget::new(1000, ContextOverflowAction::Compress);
        let status = budget.record(1500);
        match status {
            BudgetStatus::Exceeded {
                overflow_action, ..
            } => {
                assert_eq!(overflow_action, ContextOverflowAction::Compress);
            }
            _ => panic!("expected Exceeded"),
        }
    }

    #[test]
    fn exceeded_carries_overflow_action_error() {
        let mut budget = ContextBudget::new(1000, ContextOverflowAction::Error);
        let status = budget.record(2000);
        match status {
            BudgetStatus::Exceeded {
                overflow_action, ..
            } => {
                assert_eq!(overflow_action, ContextOverflowAction::Error);
            }
            _ => panic!("expected Exceeded"),
        }
    }

    #[test]
    fn record_returns_ok_then_warning_then_exceeded() {
        let mut budget = ContextBudget::new(10_000, ContextOverflowAction::Truncate);
        assert_eq!(budget.record(5000), BudgetStatus::Ok);
        assert!(matches!(budget.record(4200), BudgetStatus::Warning { .. }));
        assert!(matches!(budget.record(1000), BudgetStatus::Exceeded { .. }));
    }

    #[test]
    fn set_consumed_updates_state() {
        let mut budget = ContextBudget::new(10_000, ContextOverflowAction::Compress);
        budget.set_consumed(8000);
        assert_eq!(budget.consumed(), 8000);
        assert_eq!(budget.remaining(), 2000);
    }

    #[test]
    fn utilization_pct_at_exact_limit() {
        let mut budget = ContextBudget::new(10_000, ContextOverflowAction::Compress);
        budget.set_consumed(10_000);
        assert_eq!(budget.utilization_pct(), 100);
    }

    #[test]
    fn utilization_pct_over_limit() {
        let mut budget = ContextBudget::new(10_000, ContextOverflowAction::Compress);
        budget.set_consumed(15_000);
        assert_eq!(budget.utilization_pct(), 150);
    }

    #[test]
    fn saturating_addition_on_record() {
        let mut budget = ContextBudget::new(10_000, ContextOverflowAction::Compress);
        budget.record(u32::MAX - 10);
        budget.record(100);
        assert_eq!(budget.consumed(), u32::MAX);
    }
}
