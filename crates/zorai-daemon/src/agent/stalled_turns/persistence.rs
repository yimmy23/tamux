use super::runtime::StalledTurnCandidate;
use super::*;

impl AgentEngine {
    pub(super) async fn persist_stalled_turn_trace(
        &self,
        candidate: &StalledTurnCandidate,
        health_state: &str,
        intervention: &str,
        extra: serde_json::Value,
    ) {
        let indicators_json = serde_json::json!({
            "thread_id": candidate.thread_id,
            "task_id": candidate.task_id,
            "goal_run_id": candidate.goal_run_id,
            "last_message_id": candidate.last_message_id,
            "last_message_at": candidate.last_message_at,
            "retries_sent": candidate.retries_sent,
            "stall_class": candidate.class.as_str(),
            "last_message_excerpt": candidate.last_message_excerpt,
            "extra": extra,
        })
        .to_string();

        if let Err(error) = self
            .history
            .insert_health_log(
                &format!("stalled-turn-{}", Uuid::new_v4()),
                "thread",
                &candidate.thread_id,
                health_state,
                Some(&indicators_json),
                Some(intervention),
                now_millis(),
            )
            .await
        {
            tracing::warn!(
                thread_id = %candidate.thread_id,
                error = %error,
                "failed to persist stalled-turn trace"
            );
        }
    }
}
