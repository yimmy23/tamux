//! Heartbeat-driven per-goal-run health aggregation layer.
//!
//! The per-task `SubagentHealthState` evaluator at
//! `gateway_loop/run_loop.rs:432` produces health logs scoped to individual
//! supervised tasks. That's the right granularity for per-task interventions
//! (pause / replan a single subagent), but it leaves goal-run-level questions
//! unanswered: is the *goal* stuck, even though no single task crossed the
//! threshold? Are several tasks each at 60% error rate enough collectively
//! to flag the run? Are tool calls dropping across the run as a whole?
//!
//! This module wires the dormant `liveness::health_monitor::HealthMonitor`
//! (with its hysteresis state machine and 5-state model including
//! `WaitingForInput`) into the heartbeat cycle as a second, goal-run-scoped
//! layer. Per tick: list running goal runs → fetch their tasks → roll up the
//! subagent-runtime stats into a single `HealthIndicators` → feed each goal
//! run's persistent `HealthMonitor` → persist a `goal_run`-scoped row to
//! `agent_health_log` on state transition.
//!
//! Idempotency: the monitor itself is the dedup key — a tick that produces
//! the same state as the previous tick emits no log row (`report.changed`
//! gates persistence). Monitors for goal runs no longer in `Running` are
//! evicted lazily at the start of each sweep.

use super::*;
use crate::agent::engine::SubagentRuntimeStats;
use crate::agent::liveness::health_monitor::{HealthMonitor, HealthReport};
use crate::agent::liveness::state_layers::{HealthIndicators, HealthState};
use crate::agent::types::GoalRunStatus;
use std::collections::HashMap;
use uuid::Uuid;

/// Goal-run-level statuses that the aggregator should monitor. Anything
/// outside this set (Queued, Completed, Failed, Cancelled, Contained, …) is
/// terminal or pre-execution and has no live runtime signal to roll up.
const MONITORED_STATUSES: &[GoalRunStatus] = &[GoalRunStatus::Running];

impl AgentEngine {
    /// Heartbeat-level sweep: aggregate per-task subagent runtime stats into
    /// a per-goal-run `HealthIndicators`, run the goal run's persistent
    /// `HealthMonitor`, and write a `goal_run`-scoped row to
    /// `agent_health_log` whenever the state transitions. Returns the number
    /// of goal runs for which a state-change row was persisted (zero is the
    /// normal-path return).
    ///
    /// The dispatcher is the single production producer of the goal-run
    /// health-monitor path. Before this method was wired the
    /// `HealthMonitor` struct was dead surface — only exercised in its own
    /// test module. This layer sits *above* the existing per-task evaluator
    /// in `gateway_loop/run_loop.rs:432`; the two layers are complementary,
    /// not replacements.
    pub(super) async fn dispatch_goal_run_health_aggregation(&self) -> Result<usize> {
        let now = now_millis();
        let running = self
            .history
            .list_goal_run_brief_refs_for_statuses(MONITORED_STATUSES)
            .await?;
        let running_ids: Vec<String> = running.iter().map(|run| run.id.clone()).collect();

        // Evict monitors whose goal run is no longer in the monitored set.
        // Without this, completed/failed goal runs leak monitor entries
        // forever — the HashMap on AgentEngine would grow unbounded.
        self.evict_stale_goal_run_health_monitors(&running_ids)
            .await;

        if running_ids.is_empty() {
            return Ok(0);
        }

        let tasks_by_goal_run = self
            .history
            .list_agent_tasks_by_goal_run_ids(&running_ids)
            .await?;
        let subagent_runtime = self.subagent_runtime.read().await.clone();

        let mut persisted = 0usize;
        for goal_run_id in &running_ids {
            let tasks = match tasks_by_goal_run.get(goal_run_id) {
                Some(t) if !t.is_empty() => t,
                _ => continue,
            };
            let Some(indicators) = aggregate_goal_run_indicators(tasks, &subagent_runtime, now)
            else {
                continue;
            };

            let report = self
                .check_or_init_goal_run_monitor(goal_run_id, &indicators, now)
                .await;

            if !report.changed {
                continue;
            }

            if let Err(error) = self.persist_goal_run_health_log(goal_run_id, &report).await {
                tracing::warn!(
                    %goal_run_id,
                    %error,
                    "failed to persist goal-run health log row",
                );
                continue;
            }
            persisted += 1;
        }

        Ok(persisted)
    }

    async fn evict_stale_goal_run_health_monitors(&self, running_ids: &[String]) {
        let alive: std::collections::HashSet<&str> =
            running_ids.iter().map(String::as_str).collect();
        let mut monitors = self.goal_run_health_monitors.write().await;
        monitors.retain(|goal_run_id, _| alive.contains(goal_run_id.as_str()));
    }

    async fn check_or_init_goal_run_monitor(
        &self,
        goal_run_id: &str,
        indicators: &HealthIndicators,
        now: u64,
    ) -> HealthReport {
        let mut monitors = self.goal_run_health_monitors.write().await;
        let monitor = monitors
            .entry(goal_run_id.to_string())
            .or_insert_with(|| HealthMonitor::new(now));
        monitor.check(indicators, now)
    }

    async fn persist_goal_run_health_log(
        &self,
        goal_run_id: &str,
        report: &HealthReport,
    ) -> Result<()> {
        let indicators_json =
            serde_json::to_string(&report.indicators).unwrap_or_else(|_| "{}".to_string());
        let intervention = if report.recommendations.is_empty() {
            None
        } else {
            // Encode the recommendation list as a single string so the
            // existing `intervention TEXT` column can carry it without a
            // schema change. Anticipatory consumers already treat this
            // column as opaque metadata.
            Some(report.recommendations.join(" | "))
        };
        self.history
            .insert_health_log(
                &format!("health_{}", Uuid::new_v4()),
                "goal_run",
                goal_run_id,
                health_state_label(report.state),
                Some(&indicators_json),
                intervention.as_deref(),
                now_millis(),
            )
            .await?;
        Ok(())
    }
}

/// Roll up the per-task subagent runtime stats for a goal run into a single
/// `HealthIndicators`. Returns `None` when none of the goal run's tasks
/// have an entry in the live `subagent_runtime` map — without observed
/// runtime data we'd spuriously flip into `WaitingForInput` on every tick
/// (since `is_waiting_for_input` triggers on `total_tool_calls == 0 &&
/// consecutive_errors == 0`), polluting the health log with noise instead
/// of signal.
///
/// Aggregation choices:
/// - `last_progress_at`: most-recent across tasks (max) — any one task's
///   progress proves the run isn't fully idle.
/// - `tool_call_frequency`: sum of per-task frequencies, where each task
///   contributes `total / max(minutes_elapsed, 1.0)` (same formula as the
///   per-task path in `gateway_loop/run_loop.rs:460`).
/// - `error_rate`: pooled (`sum_failed / sum_total`), since rates from
///   different denominators can't be averaged straight.
/// - `context_utilization_pct`: max across tasks — one near-full context
///   stalls the whole run.
/// - `consecutive_errors`: max across tasks — one stuck subagent is enough
///   signal that the run is in trouble.
/// - `successful_tool_calls`, `total_tool_calls`: pooled sums.
/// - `context_growth_rate`: 0.0 — not tracked at any current source.
pub(super) fn aggregate_goal_run_indicators(
    tasks: &[crate::agent::types::AgentTask],
    runtimes: &HashMap<String, SubagentRuntimeStats>,
    now_ms: u64,
) -> Option<HealthIndicators> {
    let mut observed: Vec<&SubagentRuntimeStats> = Vec::new();
    for task in tasks {
        if let Some(stats) = runtimes.get(&task.id) {
            observed.push(stats);
        }
    }
    if observed.is_empty() {
        return None;
    }

    let now_secs = now_ms / 1000;
    let mut total_tool_calls: u32 = 0;
    let mut successful_tool_calls: u32 = 0;
    let mut total_failed: u32 = 0;
    let mut frequency_sum: f64 = 0.0;
    let mut max_context_pct: u32 = 0;
    let mut max_consecutive: u32 = 0;
    let mut max_last_progress: Option<u64> = None;

    for stats in &observed {
        total_tool_calls = total_tool_calls.saturating_add(stats.tool_calls_total);
        successful_tool_calls = successful_tool_calls.saturating_add(stats.tool_calls_succeeded);
        total_failed = total_failed.saturating_add(stats.tool_calls_failed);

        let started_secs = stats.started_at / 1000;
        let elapsed_minutes = if now_secs > started_secs {
            ((now_secs - started_secs) as f64 / 60.0).max(1.0)
        } else {
            1.0
        };
        frequency_sum += stats.tool_calls_total as f64 / elapsed_minutes;

        if stats.context_utilization_pct > max_context_pct {
            max_context_pct = stats.context_utilization_pct;
        }
        if stats.consecutive_errors > max_consecutive {
            max_consecutive = stats.consecutive_errors;
        }
        if let Some(last) = stats.last_progress_at {
            max_last_progress = Some(max_last_progress.map_or(last, |prev| prev.max(last)));
        }
    }

    let error_rate = if total_tool_calls == 0 {
        0.0
    } else {
        total_failed as f64 / total_tool_calls as f64
    };

    Some(HealthIndicators {
        last_progress_at: max_last_progress,
        tool_call_frequency: frequency_sum,
        error_rate,
        context_growth_rate: 0.0,
        context_utilization_pct: max_context_pct,
        consecutive_errors: max_consecutive,
        total_tool_calls,
        successful_tool_calls,
    })
}

fn health_state_label(state: HealthState) -> &'static str {
    match state {
        HealthState::Healthy => "healthy",
        HealthState::Degraded => "degraded",
        HealthState::Stuck => "stuck",
        HealthState::Crashed => "crashed",
        HealthState::WaitingForInput => "waiting_for_input",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::engine::SubagentRuntimeStats;
    use crate::agent::types::{AgentTask, SubagentHealthState, TaskPriority, TaskStatus};
    use std::collections::VecDeque;

    fn supervised_task(id: &str, goal_run_id: &str) -> AgentTask {
        AgentTask {
            id: id.to_string(),
            title: format!("Task {id}"),
            description: "fixture for goal-run health aggregation".to_string(),
            status: TaskStatus::InProgress,
            priority: TaskPriority::Normal,
            progress: 50,
            created_at: 100,
            started_at: Some(150),
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some(format!("thread-{id}")),
            source: "agent".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: Some(goal_run_id.to_string()),
            goal_run_title: Some("Goal".to_string()),
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 3,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: None,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            override_provider: None,
            override_model: None,
            override_system_prompt: None,
            sub_agent_def_id: None,
        }
    }

    fn stats(
        task_id: &str,
        total: u32,
        succeeded: u32,
        failed: u32,
        ctx_pct: u32,
        consecutive_errors: u32,
        started_at_ms: u64,
        last_progress_at: Option<u64>,
    ) -> SubagentRuntimeStats {
        SubagentRuntimeStats {
            task_id: task_id.to_string(),
            parent_task_id: None,
            thread_id: None,
            started_at: started_at_ms,
            created_at: started_at_ms,
            max_duration_secs: None,
            context_budget_tokens: None,
            last_tool_call_at: None,
            last_progress_at,
            tool_calls_total: total,
            tool_calls_succeeded: succeeded,
            tool_calls_failed: failed,
            consecutive_errors,
            recent_tool_names: VecDeque::new(),
            tokens_consumed: 0,
            context_utilization_pct: ctx_pct,
            health_state: SubagentHealthState::Healthy,
            updated_at: started_at_ms,
        }
    }

    #[test]
    fn aggregator_returns_none_when_no_runtime_data() {
        // A goal run with two tasks but zero supervised runtime entries
        // must not produce indicators — otherwise the monitor would see
        // total_tool_calls == 0 && consecutive_errors == 0 and flip to
        // WaitingForInput, polluting the health log for any goal run
        // that just isn't using supervised subagents.
        let tasks = vec![
            supervised_task("t1", "goal-1"),
            supervised_task("t2", "goal-1"),
        ];
        let runtimes: HashMap<String, SubagentRuntimeStats> = HashMap::new();
        assert!(aggregate_goal_run_indicators(&tasks, &runtimes, 1_000_000).is_none());
    }

    #[test]
    fn aggregator_pools_tool_call_counts_and_pools_error_rate() {
        // Pooled error rate must be (failed_sum / total_sum), not the mean
        // of the per-task rates. With 1/10 and 5/10 → 6/20 = 0.30,
        // whereas averaging rates gives (0.1 + 0.5)/2 = 0.30 in this
        // symmetric case but would diverge for unequal denominators.
        let tasks = vec![
            supervised_task("t1", "goal-1"),
            supervised_task("t2", "goal-1"),
        ];
        let mut runtimes = HashMap::new();
        runtimes.insert(
            "t1".to_string(),
            stats("t1", 10, 9, 1, 40, 0, 1_000, Some(2_000)),
        );
        runtimes.insert(
            "t2".to_string(),
            stats("t2", 10, 5, 5, 60, 0, 1_000, Some(3_000)),
        );

        let indicators =
            aggregate_goal_run_indicators(&tasks, &runtimes, 4_000).expect("indicators");
        assert_eq!(indicators.total_tool_calls, 20);
        assert_eq!(indicators.successful_tool_calls, 14);
        assert!((indicators.error_rate - 0.30).abs() < 1e-6);
    }

    #[test]
    fn aggregator_takes_max_for_context_and_consecutive_errors() {
        // The worst-positioned task in a goal run should drive the
        // worst-case signals — one stalled subagent at 95% context is
        // enough to stall the whole goal, even if the other task is
        // comfortable at 20%.
        let tasks = vec![
            supervised_task("t1", "goal-1"),
            supervised_task("t2", "goal-1"),
        ];
        let mut runtimes = HashMap::new();
        runtimes.insert("t1".to_string(), stats("t1", 5, 5, 0, 20, 0, 1_000, None));
        runtimes.insert("t2".to_string(), stats("t2", 5, 4, 1, 95, 4, 1_000, None));

        let indicators =
            aggregate_goal_run_indicators(&tasks, &runtimes, 60_000).expect("indicators");
        assert_eq!(indicators.context_utilization_pct, 95);
        assert_eq!(indicators.consecutive_errors, 4);
    }

    #[test]
    fn aggregator_takes_most_recent_last_progress_at() {
        // Most-recent progress across the goal run is the right signal —
        // if any task made progress recently, the goal isn't fully idle.
        let tasks = vec![
            supervised_task("t1", "goal-1"),
            supervised_task("t2", "goal-1"),
        ];
        let mut runtimes = HashMap::new();
        runtimes.insert(
            "t1".to_string(),
            stats("t1", 1, 1, 0, 10, 0, 1_000, Some(5_000)),
        );
        runtimes.insert(
            "t2".to_string(),
            stats("t2", 1, 1, 0, 10, 0, 1_000, Some(9_000)),
        );
        let indicators =
            aggregate_goal_run_indicators(&tasks, &runtimes, 60_000).expect("indicators");
        assert_eq!(indicators.last_progress_at, Some(9_000));
    }

    #[test]
    fn aggregator_skips_tasks_with_no_runtime_entry_but_uses_others() {
        // Sparse data is fine: one task may be supervised and emitting
        // runtime stats while another isn't yet (or never will be). The
        // aggregator should produce indicators based on whoever is
        // reporting rather than refusing to summarize the whole run.
        let tasks = vec![
            supervised_task("t1", "goal-1"),
            supervised_task("t2", "goal-1"),
        ];
        let mut runtimes = HashMap::new();
        runtimes.insert(
            "t2".to_string(),
            stats("t2", 7, 7, 0, 30, 0, 1_000, Some(2_000)),
        );
        let indicators =
            aggregate_goal_run_indicators(&tasks, &runtimes, 60_000).expect("indicators");
        assert_eq!(indicators.total_tool_calls, 7);
        assert_eq!(indicators.successful_tool_calls, 7);
        assert!(indicators.error_rate.abs() < 1e-6);
    }

    #[test]
    fn aggregator_floors_elapsed_minutes_at_one_to_avoid_blowing_up_frequency() {
        // A task started 10 seconds ago with 5 tool calls must not be
        // reported at 30 calls/min — the per-task path floors the
        // denominator at 1 minute exactly to avoid this; the
        // goal-run-level aggregator must do the same so its frequency
        // is comparable to the per-task one.
        let tasks = vec![supervised_task("t1", "goal-1")];
        let mut runtimes = HashMap::new();
        runtimes.insert("t1".to_string(), stats("t1", 5, 5, 0, 10, 0, 0, Some(0)));
        let indicators =
            aggregate_goal_run_indicators(&tasks, &runtimes, 10_000).expect("indicators");
        // 5 calls / max(elapsed_minutes, 1.0) = 5.0
        assert!((indicators.tool_call_frequency - 5.0).abs() < 1e-6);
    }

    #[test]
    fn health_state_label_covers_every_variant_including_waiting_for_input() {
        // If a new HealthState variant lands, this test forces an update
        // here — otherwise the persistence layer would silently write
        // the same label for the new variant as for an old one and the
        // health log would lose fidelity for that state.
        assert_eq!(health_state_label(HealthState::Healthy), "healthy");
        assert_eq!(health_state_label(HealthState::Degraded), "degraded");
        assert_eq!(health_state_label(HealthState::Stuck), "stuck");
        assert_eq!(health_state_label(HealthState::Crashed), "crashed");
        assert_eq!(
            health_state_label(HealthState::WaitingForInput),
            "waiting_for_input"
        );
    }
}
