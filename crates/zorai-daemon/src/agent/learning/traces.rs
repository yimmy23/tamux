//! Execution trace collection — record tool calls, outcomes, and metrics for learning.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single tool-call step within an execution trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepTrace {
    pub tool_name: String,
    /// SHA-256 first-16-hex-chars hash of the arguments (for dedup, not full args).
    pub tool_arguments_hash: String,
    pub succeeded: bool,
    pub duration_ms: u64,
    pub tokens_used: u32,
    pub error: Option<String>,
    pub timestamp: u64,
}

/// Full execution trace covering one task / goal run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTrace {
    pub trace_id: String,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    /// Task classification label (from `classify_task`).
    pub task_type: String,
    pub steps: Vec<StepTrace>,
    pub outcome: TraceOutcome,
    pub total_duration_ms: u64,
    pub total_tokens_used: u32,
    /// Optional quality score in the range `0.0..=1.0`.
    pub quality_score: Option<f64>,
    pub created_at: u64,
}

/// Decision-classified causal trace for explaining why the agent chose an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalTrace {
    pub trace_id: String,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub decision_type: DecisionType,
    pub selected: DecisionOption,
    pub rejected_options: Vec<DecisionOption>,
    pub context_hash: String,
    pub causal_factors: Vec<CausalFactor>,
    pub outcome: CausalTraceOutcome,
    pub model_used: Option<String>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionType {
    ToolSelection,
    PlanSelection,
    ReplanSelection,
    Recovery,
    ContextCompression,
    SkillSelection,
    GovernanceEvaluation,
    CollaborationResolution,
    CollaborationOutcome,
    PredictiveHydration,
}

impl DecisionType {
    pub fn family_label(&self) -> &'static str {
        match self {
            Self::ToolSelection => "tool_selection",
            Self::PlanSelection => "plan_selection",
            Self::ReplanSelection => "replan_selection",
            Self::Recovery => "recovery",
            Self::ContextCompression => "context_compression",
            Self::SkillSelection => "skill_selection",
            Self::GovernanceEvaluation => "governance_evaluation",
            Self::CollaborationResolution => "collaboration_resolution",
            Self::CollaborationOutcome => "collaboration_outcome",
            Self::PredictiveHydration => "predictive_hydration",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionOption {
    pub option_type: String,
    pub reasoning: String,
    pub rejection_reason: Option<String>,
    pub estimated_success_prob: Option<f64>,
    pub arguments_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalFactor {
    pub factor_type: FactorType,
    pub description: String,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FactorType {
    PastSuccess,
    PastFailure,
    ResourceConstraint,
    OperatorPreference,
    PatternMatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum CausalTraceOutcome {
    Success,
    Failure {
        reason: String,
    },
    NearMiss {
        what_went_wrong: String,
        how_recovered: String,
    },
    Unresolved,
}

/// Outcome of a traced execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TraceOutcome {
    Success,
    Failure { reason: String },
    Partial { completed_pct: f64 },
    Cancelled,
}

// ---------------------------------------------------------------------------
// TraceCollector
// ---------------------------------------------------------------------------

/// Accumulates [`StepTrace`]s during an execution and produces a final
/// [`ExecutionTrace`] via [`TraceCollector::finalize`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceCollector {
    current_steps: Vec<StepTrace>,
    started_at: u64,
    task_type: String,
}

impl TraceCollector {
    /// Create a new collector for the given task type, recording the start time.
    pub fn new(task_type: &str, now: u64) -> Self {
        Self {
            current_steps: Vec::new(),
            started_at: now,
            task_type: task_type.to_string(),
        }
    }

    /// Record a completed tool-call step.
    pub fn record_step(
        &mut self,
        tool_name: &str,
        args_hash: &str,
        succeeded: bool,
        duration_ms: u64,
        tokens: u32,
        error: Option<String>,
        now: u64,
    ) {
        self.current_steps.push(StepTrace {
            tool_name: tool_name.to_string(),
            tool_arguments_hash: args_hash.to_string(),
            succeeded,
            duration_ms,
            tokens_used: tokens,
            error,
            timestamp: now,
        });
    }

    /// Consume the collector and produce a finalised [`ExecutionTrace`].
    ///
    /// `total_duration_ms` is computed as `now - started_at` and
    /// `total_tokens_used` is the sum of all step tokens.
    pub fn finalize(
        self,
        outcome: TraceOutcome,
        goal_run_id: Option<String>,
        task_id: Option<String>,
        quality_score: Option<f64>,
        now: u64,
    ) -> ExecutionTrace {
        let total_duration_ms = now.saturating_sub(self.started_at);
        let total_tokens_used: u32 = self.current_steps.iter().map(|s| s.tokens_used).sum();

        ExecutionTrace {
            trace_id: format!("trace_{}", Uuid::new_v4()),
            goal_run_id,
            task_id,
            task_type: self.task_type,
            steps: self.current_steps,
            outcome,
            total_duration_ms,
            total_tokens_used,
            quality_score,
            created_at: now,
        }
    }

    /// Start timestamp of the active trace in milliseconds since epoch.
    pub fn started_at_ms(&self) -> u64 {
        self.started_at
    }

    /// Number of steps recorded so far.
    pub fn step_count(&self) -> usize {
        self.current_steps.len()
    }

    /// Fraction of recorded steps that succeeded (`0.0` when empty).
    pub fn success_rate(&self) -> f64 {
        if self.current_steps.is_empty() {
            return 0.0;
        }
        let ok = self.current_steps.iter().filter(|s| s.succeeded).count() as f64;
        ok / self.current_steps.len() as f64
    }

    /// Successful prior uses of `tool_name` within the active trace.
    pub fn success_count_for_tool(&self, tool_name: &str) -> usize {
        self.current_steps
            .iter()
            .filter(|step| step.tool_name == tool_name && step.succeeded)
            .count()
    }

    /// Failed prior uses of `tool_name` within the active trace.
    pub fn failure_count_for_tool(&self, tool_name: &str) -> usize {
        self.current_steps
            .iter()
            .filter(|step| step.tool_name == tool_name && !step.succeeded)
            .count()
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Return the first 16 hex characters of the SHA-256 digest of `args`.
pub fn hash_arguments(args: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(args.as_bytes());
    let digest = hasher.finalize();
    // Each byte → 2 hex chars, so 8 bytes → 16 hex chars.
    digest[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
}

/// Return the first 16 hex characters of the SHA-256 digest of a context blob.
pub fn hash_context_blob(blob: &str) -> String {
    hash_arguments(blob)
}

/// Extract the ordered list of tool names from a trace.
pub fn extract_tool_sequence(trace: &ExecutionTrace) -> Vec<String> {
    trace.steps.iter().map(|s| s.tool_name.clone()).collect()
}

/// Compute a quality score in `0.0..=1.0` based on success rate and
/// efficiency (fewer steps → higher score, capped contribution).
pub fn compute_quality_score(trace: &ExecutionTrace) -> f64 {
    if trace.steps.is_empty() {
        return 0.0;
    }

    // Success component (70 % weight).
    let success_count = trace.steps.iter().filter(|s| s.succeeded).count() as f64;
    let success_rate = success_count / trace.steps.len() as f64;

    // Efficiency component (30 % weight): fewer steps is better.
    // We treat <= 3 steps as "ideal" and scale down linearly up to 20 steps.
    let step_count = trace.steps.len() as f64;
    let efficiency = (1.0 - ((step_count - 3.0).max(0.0) / 17.0)).max(0.0);

    let raw = success_rate * 0.7 + efficiency * 0.3;
    raw.clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_collector_starts_empty() {
        let c = TraceCollector::new("code_edit", 1000);
        assert_eq!(c.step_count(), 0);
        assert_eq!(c.task_type, "code_edit");
    }

    #[test]
    fn record_step_adds_to_steps() {
        let mut c = TraceCollector::new("code_edit", 1000);
        c.record_step("read_file", "abc123", true, 50, 100, None, 1050);
        assert_eq!(c.step_count(), 1);
        c.record_step("write_file", "def456", true, 30, 80, None, 1100);
        assert_eq!(c.step_count(), 2);
    }

    #[test]
    fn finalize_creates_valid_trace_with_uuid() {
        let mut c = TraceCollector::new("code_edit", 1000);
        c.record_step("read_file", "abc", true, 50, 100, None, 1050);
        let trace = c.finalize(TraceOutcome::Success, None, None, None, 2000);
        assert!(trace.trace_id.starts_with("trace_"));
        // UUID portion is 36 chars (8-4-4-4-12 with hyphens).
        assert_eq!(trace.trace_id.len(), 6 + 36);
    }

    #[test]
    fn total_duration_computed_correctly() {
        let mut c = TraceCollector::new("code_edit", 1000);
        c.record_step("tool_a", "h1", true, 100, 10, None, 1100);
        c.record_step("tool_b", "h2", true, 200, 20, None, 1300);
        let trace = c.finalize(TraceOutcome::Success, None, None, None, 2000);
        assert_eq!(trace.total_duration_ms, 1000);
    }

    #[test]
    fn total_tokens_summed() {
        let mut c = TraceCollector::new("code_edit", 0);
        c.record_step("a", "h", true, 10, 100, None, 10);
        c.record_step("b", "h", true, 10, 250, None, 20);
        c.record_step("c", "h", false, 10, 50, Some("err".into()), 30);
        let trace = c.finalize(TraceOutcome::Success, None, None, None, 40);
        assert_eq!(trace.total_tokens_used, 400);
    }

    #[test]
    fn success_rate_calculation() {
        let mut c = TraceCollector::new("debug", 0);
        c.record_step("a", "h", true, 10, 10, None, 1);
        c.record_step("b", "h", false, 10, 10, Some("fail".into()), 2);
        c.record_step("c", "h", true, 10, 10, None, 3);
        c.record_step("d", "h", true, 10, 10, None, 4);
        // 3 out of 4 → 0.75
        assert!((c.success_rate() - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn success_rate_empty_collector() {
        let c = TraceCollector::new("debug", 0);
        assert!((c.success_rate() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn tool_sequence_extraction() {
        let mut c = TraceCollector::new("code_edit", 0);
        c.record_step("read", "h", true, 10, 10, None, 1);
        c.record_step("edit", "h", true, 10, 10, None, 2);
        c.record_step("test", "h", true, 10, 10, None, 3);
        let trace = c.finalize(TraceOutcome::Success, None, None, None, 10);
        assert_eq!(extract_tool_sequence(&trace), vec!["read", "edit", "test"]);
    }

    #[test]
    fn quality_score_all_success_trace() {
        let mut c = TraceCollector::new("code_edit", 0);
        // 3 successful steps (ideal count).
        c.record_step("a", "h", true, 10, 10, None, 1);
        c.record_step("b", "h", true, 10, 10, None, 2);
        c.record_step("c", "h", true, 10, 10, None, 3);
        let trace = c.finalize(TraceOutcome::Success, None, None, None, 10);
        let score = compute_quality_score(&trace);
        // success_rate = 1.0 → 0.7; efficiency at 3 steps → 1.0 → 0.3; total = 1.0
        assert!((score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn quality_score_mixed_trace() {
        let mut c = TraceCollector::new("code_edit", 0);
        c.record_step("a", "h", true, 10, 10, None, 1);
        c.record_step("b", "h", false, 10, 10, Some("err".into()), 2);
        c.record_step("c", "h", true, 10, 10, None, 3);
        c.record_step("d", "h", false, 10, 10, Some("err".into()), 4);
        let trace = c.finalize(
            TraceOutcome::Partial { completed_pct: 0.5 },
            None,
            None,
            None,
            10,
        );
        let score = compute_quality_score(&trace);
        // success_rate = 0.5 → 0.35; efficiency: (4-3)/17 ≈ 0.059 → (1-0.059)*0.3 ≈ 0.282
        // total ≈ 0.632
        assert!(score > 0.0 && score < 1.0, "score = {score}");
    }

    #[test]
    fn hash_arguments_consistent() {
        let h1 = hash_arguments("hello world");
        let h2 = hash_arguments("hello world");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16);

        // Different input → different hash.
        let h3 = hash_arguments("goodbye world");
        assert_ne!(h1, h3);
    }

    #[test]
    fn empty_trace_finalization() {
        let c = TraceCollector::new("empty_task", 500);
        let trace = c.finalize(TraceOutcome::Cancelled, None, None, None, 500);
        assert!(trace.steps.is_empty());
        assert_eq!(trace.total_duration_ms, 0);
        assert_eq!(trace.total_tokens_used, 0);
        assert!(trace.trace_id.starts_with("trace_"));
        assert_eq!(trace.task_type, "empty_task");
        assert!(matches!(trace.outcome, TraceOutcome::Cancelled));
    }
}
