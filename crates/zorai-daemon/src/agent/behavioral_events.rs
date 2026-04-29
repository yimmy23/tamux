//! Lightweight behavioral event bus backed by the existing agent_events store.

use serde_json::{json, Value};
use zorai_protocol::AgentEventRow;

use super::*;

pub(super) struct BehavioralEventContext<'a> {
    pub thread_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub goal_run_id: Option<&'a str>,
    pub approval_id: Option<&'a str>,
}

impl AgentEngine {
    pub(super) async fn record_behavioral_event(
        &self,
        kind: &str,
        context: BehavioralEventContext<'_>,
        payload: Value,
    ) -> Result<()> {
        let timestamp = now_millis();
        let correlation_id = behavioral_correlation_id(&context);
        let row = AgentEventRow {
            id: format!("behavioral_{}_{}", kind, Uuid::new_v4()),
            category: "behavioral".to_string(),
            kind: kind.to_string(),
            pane_id: context.thread_id.map(ToOwned::to_owned),
            workspace_id: None,
            surface_id: None,
            session_id: context
                .task_id
                .or(context.goal_run_id)
                .map(ToOwned::to_owned),
            payload_json: serde_json::to_string(&json!({
                "correlation_id": correlation_id,
                "thread_id": context.thread_id,
                "task_id": context.task_id,
                "goal_run_id": context.goal_run_id,
                "approval_id": context.approval_id,
                "payload": payload,
            }))?,
            timestamp: timestamp as i64,
        };
        self.history.upsert_agent_event(&row).await
    }
}

fn behavioral_correlation_id(context: &BehavioralEventContext<'_>) -> String {
    if let (Some(goal_run_id), Some(task_id)) = (context.goal_run_id, context.task_id) {
        return format!("goal:{goal_run_id}:task:{task_id}");
    }
    if let Some(goal_run_id) = context.goal_run_id {
        return format!("goal:{goal_run_id}");
    }
    if let Some(task_id) = context.task_id {
        return format!("task:{task_id}");
    }
    if let Some(thread_id) = context.thread_id {
        return format!("thread:{thread_id}");
    }
    if let Some(approval_id) = context.approval_id {
        return format!("approval:{approval_id}");
    }
    format!("behavioral:{}", Uuid::new_v4())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correlation_id_prefers_goal_and_task() {
        let context = BehavioralEventContext {
            thread_id: Some("thread-1"),
            task_id: Some("task-1"),
            goal_run_id: Some("goal-1"),
            approval_id: None,
        };
        assert_eq!(
            behavioral_correlation_id(&context),
            "goal:goal-1:task:task-1"
        );
    }
}
