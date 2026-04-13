#![allow(dead_code)]

use super::types::{StalledTurnClass, ThreadStallObservation};
use super::*;

pub(super) const INITIAL_GRACE_DELAY_MS: u64 = 30_000;
const SECOND_RETRY_DELAY_MS: u64 = 60_000;
const THIRD_RETRY_DELAY_MS: u64 = 120_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StalledTurnCandidate {
    pub(super) thread_id: String,
    pub(super) class: StalledTurnClass,
    pub(super) last_message_id: String,
    pub(super) last_message_at: u64,
    pub(super) last_message_excerpt: String,
    pub(super) task_id: Option<String>,
    pub(super) goal_run_id: Option<String>,
    pub(super) retries_sent: u32,
    pub(super) next_evaluation_at: u64,
}

impl StalledTurnCandidate {
    pub(super) fn new(
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

    pub(super) fn from_observation(observation: &ThreadStallObservation) -> Self {
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

    pub(super) fn record_retry(&mut self, now: u64) {
        self.retries_sent = self.retries_sent.saturating_add(1);
        self.next_evaluation_at = now.saturating_add(delay_after_retry(self.retries_sent));
    }

    pub(super) fn escalation_ready(&self, now: u64) -> bool {
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
        }
    }
}

impl AgentEngine {
    pub(in crate::agent) async fn supervise_stalled_turns(&self) -> Result<()> {
        let observations = self.collect_stalled_turn_observations().await;
        let observed_ids = observations
            .iter()
            .map(|observation| observation.thread_id.clone())
            .collect::<HashSet<_>>();

        {
            let mut candidates = self.stalled_turn_candidates.lock().await;
            candidates.retain(|thread_id, _| observed_ids.contains(thread_id));
        }

        for observation in observations {
            let action = {
                let mut candidates = self.stalled_turn_candidates.lock().await;
                let candidate = candidates
                    .entry(observation.thread_id.clone())
                    .or_insert_with(|| StalledTurnCandidate::from_observation(&observation));

                if candidate.last_message_id != observation.last_message_id {
                    *candidate = StalledTurnCandidate::from_observation(&observation);
                }

                let now = now_millis();
                if now < candidate.next_evaluation_at {
                    None
                } else if candidate.escalation_ready(now) {
                    Some((candidate.clone(), true))
                } else {
                    Some((candidate.clone(), false))
                }
            };

            let Some((candidate_snapshot, escalate)) = action else {
                continue;
            };

            if escalate {
                self.perform_stalled_turn_escalation(&candidate_snapshot)
                    .await;
                self.persist_stalled_turn_trace(
                    &candidate_snapshot,
                    "stuck",
                    "escalated_after_retries",
                    serde_json::json!({"decision": "escalate"}),
                )
                .await;
                self.stalled_turn_candidates
                    .lock()
                    .await
                    .remove(&candidate_snapshot.thread_id);
                continue;
            }

            self.perform_stalled_turn_retry(&candidate_snapshot).await?;
            {
                let mut candidates = self.stalled_turn_candidates.lock().await;
                if let Some(candidate) = candidates.get_mut(&candidate_snapshot.thread_id) {
                    candidate.record_retry(now_millis());
                }
            }
            self.persist_stalled_turn_trace(
                &candidate_snapshot,
                "degraded",
                "retry_requested",
                serde_json::json!({
                    "decision": "continue_required",
                    "attempt": candidate_snapshot.retries_sent.saturating_add(1),
                }),
            )
            .await;
        }

        Ok(())
    }
}
