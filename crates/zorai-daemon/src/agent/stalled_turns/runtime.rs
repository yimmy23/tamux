#![allow(dead_code)]

use super::types::{StalledTurnClass, ThreadStallObservation};
use super::*;
use serde::{Deserialize, Serialize};

pub(super) const INITIAL_GRACE_DELAY_MS: u64 = 30_000;
const SECOND_RETRY_DELAY_MS: u64 = 60_000;
const THIRD_RETRY_DELAY_MS: u64 = 120_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StalledTurnCandidate {
    pub(crate) thread_id: String,
    pub(crate) class: StalledTurnClass,
    pub(crate) last_message_id: String,
    pub(crate) last_message_at: u64,
    pub(crate) last_message_excerpt: String,
    pub(crate) task_id: Option<String>,
    pub(crate) goal_run_id: Option<String>,
    pub(crate) retries_sent: u32,
    pub(crate) next_evaluation_at: u64,
}

impl StalledTurnCandidate {
    pub(crate) fn new(
        thread_id: impl Into<String>,
        class: StalledTurnClass,
        created_at: u64,
    ) -> Self {
        Self {
            thread_id: thread_id.into(),
            class,
            last_message_id: String::new(),
            last_message_at: created_at,
            last_message_excerpt: String::new(),
            task_id: None,
            goal_run_id: None,
            retries_sent: 0,
            next_evaluation_at: created_at.saturating_add(INITIAL_GRACE_DELAY_MS),
        }
    }

    pub(crate) fn from_observation(observation: &ThreadStallObservation) -> Self {
        Self {
            thread_id: observation.thread_id.clone(),
            class: observation.class,
            last_message_id: observation.last_message_id.clone(),
            last_message_at: observation.last_message_at,
            last_message_excerpt: observation
                .last_assistant_message
                .chars()
                .take(180)
                .collect(),
            task_id: observation.task_id.clone(),
            goal_run_id: observation.goal_run_id.clone(),
            retries_sent: 0,
            next_evaluation_at: observation
                .last_message_at
                .saturating_add(INITIAL_GRACE_DELAY_MS),
        }
    }

    pub(crate) fn record_retry(&mut self, now: u64) {
        self.retries_sent = self.retries_sent.saturating_add(1);
        self.next_evaluation_at = now.saturating_add(delay_after_retry(self.retries_sent));
    }

    pub(crate) fn escalation_ready(&self, now: u64) -> bool {
        self.retries_sent >= 3 && now >= self.next_evaluation_at
    }
}

fn delay_after_retry(retries_sent: u32) -> u64 {
    match retries_sent {
        0 => INITIAL_GRACE_DELAY_MS,
        1 => SECOND_RETRY_DELAY_MS,
        _ => THIRD_RETRY_DELAY_MS,
    }
}

impl StalledTurnClass {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::PromiseWithoutAction => "promise_without_action",
            Self::PostToolResultNoFollowThrough => "post_tool_result_no_follow_through",
            Self::ActiveStreamIdle => "active_stream_idle",
            Self::ToolCallLoop => "tool_call_loop",
            Self::NoProgress => "no_progress",
        }
    }
}

impl AgentEngine {
    pub(in crate::agent) async fn supervise_stalled_turns(&self) -> Result<()> {
        let observations = self.collect_stalled_turn_observations().await;
        if !observations.is_empty() {
            tracing::info!(
                observation_count = observations.len(),
                thread_ids = ?observations
                    .iter()
                    .map(|observation| observation.thread_id.as_str())
                    .collect::<Vec<_>>(),
                "stalled-turn supervision found candidate threads"
            );
        }
        let observed_ids = observations
            .iter()
            .map(|observation| observation.thread_id.clone())
            .collect::<HashSet<_>>();
        let current_candidates = {
            let candidates = self.stalled_turn_candidates.lock().await;
            candidates.values().cloned().collect::<Vec<_>>()
        };
        let now = now_millis();
        let result = crate::agent::background_workers::run_background_worker_command(
            crate::agent::background_workers::protocol::BackgroundWorkerKind::Safety,
            crate::agent::background_workers::protocol::BackgroundWorkerCommand::TickSafety {
                observations,
                candidates: current_candidates,
                now_ms: now,
            },
        )
        .await?;

        let crate::agent::background_workers::protocol::BackgroundWorkerResult::SafetyTick {
            decisions,
        } = result
        else {
            anyhow::bail!("background safety worker returned unexpected response");
        };
        if !decisions.is_empty() {
            tracing::info!(
                decision_count = decisions.len(),
                "stalled-turn supervision produced recovery decisions"
            );
        }

        self.apply_safety_worker_decisions(decisions, observed_ids, now)
            .await?;

        Ok(())
    }

    async fn apply_safety_worker_decisions(
        &self,
        decisions: Vec<crate::agent::background_workers::protocol::SafetyDecision>,
        observed_ids: HashSet<String>,
        now: u64,
    ) -> Result<()> {
        {
            let mut candidates = self.stalled_turn_candidates.lock().await;
            candidates.retain(|thread_id, _| observed_ids.contains(thread_id));
        }

        for decision in decisions {
            match decision {
                crate::agent::background_workers::protocol::SafetyDecision::Retry { candidate } => {
                    self.stalled_turn_candidates
                        .lock()
                        .await
                        .entry(candidate.thread_id.clone())
                        .or_insert_with(|| candidate.clone());
                    self.perform_stalled_turn_retry(&candidate).await?;
                    {
                        let mut candidates = self.stalled_turn_candidates.lock().await;
                        if let Some(current) = candidates.get_mut(&candidate.thread_id) {
                            current.record_retry(now);
                        }
                    }
                    self.persist_stalled_turn_trace(
                        &candidate,
                        "degraded",
                        "retry_requested",
                        serde_json::json!({
                            "decision": "continue_required",
                            "attempt": candidate.retries_sent.saturating_add(1),
                        }),
                    )
                    .await;
                }
                crate::agent::background_workers::protocol::SafetyDecision::Escalate {
                    candidate,
                } => {
                    self.perform_stalled_turn_escalation(&candidate).await;
                    self.persist_stalled_turn_trace(
                        &candidate,
                        "stuck",
                        "escalated_after_retries",
                        serde_json::json!({"decision": "escalate"}),
                    )
                    .await;
                    self.stalled_turn_candidates
                        .lock()
                        .await
                        .remove(&candidate.thread_id);
                }
            }
        }
        Ok(())
    }
}
