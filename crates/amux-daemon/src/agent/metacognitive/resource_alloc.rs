//! Resource allocation — dynamic context budgets, sub-agent slots, and priority scheduling.

use serde::{Deserialize, Serialize};

use crate::agent::types::TaskPriority;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Complexity tier for a task — determines default token allocation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskComplexity {
    /// ~5,000 tokens
    Simple,
    /// ~15,000 tokens
    Moderate,
    /// ~30,000 tokens
    Complex,
    /// ~50,000 tokens
    Research,
}

/// A single slot reservation inside the resource pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotAllocation {
    pub task_id: String,
    pub context_budget: u32,
    pub priority: TaskPriority,
    pub allocated_at: u64,
}

/// Request to allocate a slot for a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllocationRequest {
    pub task_id: String,
    pub estimated_complexity: TaskComplexity,
    pub priority: TaskPriority,
    pub requested_tokens: Option<u32>,
}

/// Outcome of an allocation attempt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AllocationResult {
    /// Slot allocated with the given context budget.
    Allocated { context_budget: u32 },
    /// Request was queued because resources are temporarily unavailable.
    Queued { reason: String },
    /// Request was denied outright.
    Denied { reason: String },
}

/// Snapshot of how much pressure the resource pool is under.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourcePressure {
    /// Percentage of slots in use (0–100).
    pub slot_utilization_pct: u32,
    /// Percentage of token budget consumed (0–100).
    pub token_utilization_pct: u32,
    /// Number of requests waiting in the queue.
    pub queue_depth: usize,
    /// `true` when either utilisation metric exceeds 80 %.
    pub is_under_pressure: bool,
}

// ---------------------------------------------------------------------------
// ResourcePool
// ---------------------------------------------------------------------------

/// Pool that tracks concurrent sub-agent slots and token budgets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcePool {
    pub max_concurrent_subagents: usize,
    pub total_context_budget_tokens: u32,
    pub allocated_tokens: u32,
    pub active_slots: Vec<SlotAllocation>,
}

impl Default for ResourcePool {
    fn default() -> Self {
        Self::new(3, 100_000)
    }
}

impl ResourcePool {
    /// Create a new pool with the given slot limit and token budget.
    pub fn new(max_concurrent: usize, total_budget: u32) -> Self {
        Self {
            max_concurrent_subagents: max_concurrent,
            total_context_budget_tokens: total_budget,
            allocated_tokens: 0,
            active_slots: Vec::new(),
        }
    }

    /// Return the default token allocation for a given complexity tier.
    pub fn context_for_complexity(complexity: TaskComplexity) -> u32 {
        match complexity {
            TaskComplexity::Simple => 5_000,
            TaskComplexity::Moderate => 15_000,
            TaskComplexity::Complex => 30_000,
            TaskComplexity::Research => 50_000,
        }
    }

    /// Number of slots that are currently free.
    pub fn available_slots(&self) -> usize {
        self.max_concurrent_subagents.saturating_sub(self.active_slots.len())
    }

    /// Try to allocate a slot for the given request.
    ///
    /// # Allocation logic
    ///
    /// 1. If there are free slots **and** enough tokens, allocate directly.
    /// 2. If all slots are occupied and the request is *Urgent*, attempt to
    ///    preempt the lowest-priority non-urgent slot.
    /// 3. If tokens are tight, fall back to the minimum budget for the
    ///    requested complexity tier.
    /// 4. Otherwise the request is queued.
    pub fn allocate(&mut self, request: &AllocationRequest, now: u64) -> AllocationResult {
        let desired = request
            .requested_tokens
            .unwrap_or_else(|| Self::context_for_complexity(request.estimated_complexity));
        let remaining = self.total_context_budget_tokens.saturating_sub(self.allocated_tokens);

        // ---- Slot availability ------------------------------------------
        if self.available_slots() == 0 {
            // Urgent requests may preempt the lowest-priority non-urgent slot.
            if request.priority == TaskPriority::Urgent {
                if let Some(preempt_idx) = self.find_preemptable_slot() {
                    let freed = self.active_slots.remove(preempt_idx);
                    self.allocated_tokens =
                        self.allocated_tokens.saturating_sub(freed.context_budget);
                    return self.do_allocate(request, desired, now);
                }
                // All slots are Urgent — cannot preempt.
                return AllocationResult::Queued {
                    reason: "all slots occupied by urgent tasks".into(),
                };
            }
            return AllocationResult::Queued {
                reason: "no available slots".into(),
            };
        }

        // ---- Token availability -----------------------------------------
        if remaining == 0 {
            return AllocationResult::Denied {
                reason: "token budget exhausted".into(),
            };
        }

        self.do_allocate(request, desired, now)
    }

    /// Release the slot held by `task_id`, returning the allocation if found.
    pub fn release(&mut self, task_id: &str) -> Option<SlotAllocation> {
        if let Some(pos) = self.active_slots.iter().position(|s| s.task_id == task_id) {
            let slot = self.active_slots.remove(pos);
            self.allocated_tokens = self.allocated_tokens.saturating_sub(slot.context_budget);
            Some(slot)
        } else {
            None
        }
    }

    /// Compute the current resource pressure.
    pub fn pressure(&self) -> ResourcePressure {
        let slot_pct = if self.max_concurrent_subagents == 0 {
            100
        } else {
            ((self.active_slots.len() as u64 * 100) / self.max_concurrent_subagents as u64) as u32
        };
        let token_pct = if self.total_context_budget_tokens == 0 {
            100
        } else {
            ((self.allocated_tokens as u64 * 100) / self.total_context_budget_tokens as u64) as u32
        };

        ResourcePressure {
            slot_utilization_pct: slot_pct,
            token_utilization_pct: token_pct,
            queue_depth: 0, // No persistent queue in this implementation.
            is_under_pressure: slot_pct > 80 || token_pct > 80,
        }
    }

    // ----- internal helpers ----------------------------------------------

    /// Perform the actual allocation, clamping tokens to what is available.
    fn do_allocate(
        &mut self,
        request: &AllocationRequest,
        desired: u32,
        now: u64,
    ) -> AllocationResult {
        let remaining = self.total_context_budget_tokens.saturating_sub(self.allocated_tokens);
        let budget = desired.min(remaining);

        self.active_slots.push(SlotAllocation {
            task_id: request.task_id.clone(),
            context_budget: budget,
            priority: request.priority,
            allocated_at: now,
        });
        self.allocated_tokens += budget;

        AllocationResult::Allocated {
            context_budget: budget,
        }
    }

    /// Find the index of the lowest-priority non-urgent slot, if any.
    fn find_preemptable_slot(&self) -> Option<usize> {
        self.active_slots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.priority != TaskPriority::Urgent)
            .min_by_key(|(_, s)| s.priority)
            .map(|(i, _)| i)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_request(id: &str) -> AllocationRequest {
        AllocationRequest {
            task_id: id.into(),
            estimated_complexity: TaskComplexity::Simple,
            priority: TaskPriority::Normal,
            requested_tokens: None,
        }
    }

    fn request_with(
        id: &str,
        complexity: TaskComplexity,
        priority: TaskPriority,
    ) -> AllocationRequest {
        AllocationRequest {
            task_id: id.into(),
            estimated_complexity: complexity,
            priority,
            requested_tokens: None,
        }
    }

    // 1. Default pool has 3 slots and 100K tokens.
    #[test]
    fn default_pool_parameters() {
        let pool = ResourcePool::default();
        assert_eq!(pool.max_concurrent_subagents, 3);
        assert_eq!(pool.total_context_budget_tokens, 100_000);
        assert_eq!(pool.allocated_tokens, 0);
        assert!(pool.active_slots.is_empty());
    }

    // 2. Allocating a simple task succeeds.
    #[test]
    fn allocate_simple_task_succeeds() {
        let mut pool = ResourcePool::default();
        let result = pool.allocate(&simple_request("t1"), 1);
        assert_eq!(
            result,
            AllocationResult::Allocated {
                context_budget: 5_000
            }
        );
        assert_eq!(pool.active_slots.len(), 1);
        assert_eq!(pool.allocated_tokens, 5_000);
    }

    // 3. Allocating when all slots are full returns Queued.
    #[test]
    fn allocate_when_full_returns_queued() {
        let mut pool = ResourcePool::new(2, 100_000);
        pool.allocate(&simple_request("t1"), 1);
        pool.allocate(&simple_request("t2"), 2);
        let result = pool.allocate(&simple_request("t3"), 3);
        assert!(matches!(result, AllocationResult::Queued { .. }));
    }

    // 4. Releasing a slot frees it.
    #[test]
    fn release_frees_slot() {
        let mut pool = ResourcePool::default();
        pool.allocate(&simple_request("t1"), 1);
        assert_eq!(pool.available_slots(), 2);
        pool.release("t1");
        assert_eq!(pool.available_slots(), 3);
        assert_eq!(pool.allocated_tokens, 0);
    }

    // 5. Urgent preempts the lowest-priority non-urgent slot.
    #[test]
    fn urgent_preempts_low_priority() {
        let mut pool = ResourcePool::new(2, 100_000);
        pool.allocate(
            &request_with("low", TaskComplexity::Simple, TaskPriority::Low),
            1,
        );
        pool.allocate(
            &request_with("normal", TaskComplexity::Simple, TaskPriority::Normal),
            2,
        );
        assert_eq!(pool.available_slots(), 0);

        let result = pool.allocate(
            &request_with("urgent", TaskComplexity::Simple, TaskPriority::Urgent),
            3,
        );
        assert!(matches!(result, AllocationResult::Allocated { .. }));
        // The Low task should have been evicted.
        assert!(pool.active_slots.iter().all(|s| s.task_id != "low"));
        assert_eq!(pool.active_slots.len(), 2);
    }

    // 6. Urgent cannot preempt another Urgent.
    #[test]
    fn urgent_cannot_preempt_urgent() {
        let mut pool = ResourcePool::new(1, 100_000);
        pool.allocate(
            &request_with("u1", TaskComplexity::Simple, TaskPriority::Urgent),
            1,
        );
        let result = pool.allocate(
            &request_with("u2", TaskComplexity::Simple, TaskPriority::Urgent),
            2,
        );
        assert!(matches!(result, AllocationResult::Queued { .. }));
    }

    // 7. Pressure reflects utilisation.
    #[test]
    fn pressure_reflects_utilization() {
        let mut pool = ResourcePool::new(2, 100_000);
        pool.allocate(&simple_request("t1"), 1);
        let p = pool.pressure();
        assert_eq!(p.slot_utilization_pct, 50);
        assert_eq!(p.token_utilization_pct, 5); // 5_000 / 100_000 = 5 %
        assert!(!p.is_under_pressure);

        // Fill to > 80 % of slots.
        pool.allocate(&simple_request("t2"), 2);
        let p = pool.pressure();
        assert_eq!(p.slot_utilization_pct, 100);
        assert!(p.is_under_pressure);
    }

    // 8. Context allocation is proportional to complexity.
    #[test]
    fn context_proportional_to_complexity() {
        assert_eq!(ResourcePool::context_for_complexity(TaskComplexity::Simple), 5_000);
        assert_eq!(
            ResourcePool::context_for_complexity(TaskComplexity::Moderate),
            15_000
        );
        assert_eq!(
            ResourcePool::context_for_complexity(TaskComplexity::Complex),
            30_000
        );
        assert_eq!(
            ResourcePool::context_for_complexity(TaskComplexity::Research),
            50_000
        );
    }

    // 9. Multiple allocations track tokens correctly.
    #[test]
    fn multiple_allocations_track_tokens() {
        let mut pool = ResourcePool::default();
        pool.allocate(
            &request_with("a", TaskComplexity::Simple, TaskPriority::Normal),
            1,
        );
        pool.allocate(
            &request_with("b", TaskComplexity::Moderate, TaskPriority::Normal),
            2,
        );
        pool.allocate(
            &request_with("c", TaskComplexity::Complex, TaskPriority::Normal),
            3,
        );
        assert_eq!(pool.allocated_tokens, 5_000 + 15_000 + 30_000);
        assert_eq!(pool.active_slots.len(), 3);
    }

    // 10. Release returns the original allocation.
    #[test]
    fn release_returns_original_allocation() {
        let mut pool = ResourcePool::default();
        pool.allocate(
            &request_with("x", TaskComplexity::Research, TaskPriority::High),
            42,
        );
        let released = pool.release("x").expect("should find slot");
        assert_eq!(released.task_id, "x");
        assert_eq!(released.context_budget, 50_000);
        assert_eq!(released.priority, TaskPriority::High);
        assert_eq!(released.allocated_at, 42);
    }

    // 11. Release of unknown task returns None.
    #[test]
    fn release_unknown_returns_none() {
        let mut pool = ResourcePool::default();
        assert!(pool.release("nonexistent").is_none());
    }

    // 12. Token budget is clamped when insufficient.
    #[test]
    fn token_budget_clamped_when_insufficient() {
        // Only 10K tokens available — a Research task (50K) should be clamped.
        let mut pool = ResourcePool::new(3, 10_000);
        let result = pool.allocate(
            &request_with("r1", TaskComplexity::Research, TaskPriority::Normal),
            1,
        );
        assert_eq!(
            result,
            AllocationResult::Allocated {
                context_budget: 10_000
            }
        );
        assert_eq!(pool.allocated_tokens, 10_000);
    }

    // 13. Denied when token budget fully exhausted.
    #[test]
    fn denied_when_tokens_exhausted() {
        let mut pool = ResourcePool::new(5, 5_000);
        pool.allocate(&simple_request("t1"), 1);
        // 5_000 consumed — nothing left.
        let result = pool.allocate(&simple_request("t2"), 2);
        assert!(matches!(result, AllocationResult::Denied { .. }));
    }
}
