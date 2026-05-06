use super::*;
use crate::history::{ConciergeGoalContext, HistoryStore};
use crate::session_manager::SessionManager;

impl ConciergeEngine {
    pub(crate) async fn recent_persisted_history_threads(
        &self,
        session_manager: &Arc<SessionManager>,
    ) -> Vec<ThreadSummary> {
        let query = crate::history::AgentThreadListQuery {
            excluded_ids: vec![CONCIERGE_THREAD_ID.to_string()],
            hidden_id_prefixes: vec![
                crate::agent::agent_identity::INTERNAL_DM_THREAD_PREFIX.to_string(),
                crate::agent::agent_identity::PARTICIPANT_PLAYGROUND_THREAD_PREFIX.to_string(),
            ],
            title_excluded_prefixes: vec![
                "HEARTBEAT SYNTHESIS".to_string(),
                "Heartbeat check:".to_string(),
            ],
            min_message_count: Some(1),
            limit: Some(1),
            ..crate::history::AgentThreadListQuery::default()
        };

        match session_manager.list_agent_threads_filtered(&query).await {
            Ok(threads) => threads
                .into_iter()
                .map(thread_summary_from_persisted_thread)
                .collect(),
            Err(error) => {
                tracing::warn!("concierge: failed to inspect persisted thread history: {error}");
                Vec::new()
            }
        }
    }

    pub(super) async fn gather_context(
        &self,
        history: &HistoryStore,
        detail_level: ConciergeDetailLevel,
        persisted_recent_threads: &[ThreadSummary],
    ) -> WelcomeContext {
        let goal_context = match history.concierge_goal_context().await {
            Ok(goal_context) => goal_context,
            Err(error) => {
                tracing::warn!("concierge: failed to inspect persisted goal context: {error}");
                ConciergeGoalContext::default()
            }
        };
        self.context_from_goal_context(detail_level, persisted_recent_threads, goal_context)
    }

    pub(super) async fn gather_gateway_context(
        &self,
        history: &HistoryStore,
        persisted_recent_threads: &[ThreadSummary],
    ) -> WelcomeContext {
        let goal_context = match history.concierge_goal_context().await {
            Ok(goal_context) => goal_context,
            Err(error) => {
                tracing::warn!(
                    "concierge: failed to inspect persisted gateway goal context: {error}"
                );
                ConciergeGoalContext::default()
            }
        };
        self.context_from_goal_context(
            ConciergeDetailLevel::ContextSummary,
            persisted_recent_threads,
            goal_context,
        )
    }

    pub(super) fn context_from_goal_context(
        &self,
        detail_level: ConciergeDetailLevel,
        persisted_recent_threads: &[ThreadSummary],
        goal_context: ConciergeGoalContext,
    ) -> WelcomeContext {
        let thread_limit = match detail_level {
            ConciergeDetailLevel::Minimal | ConciergeDetailLevel::ContextSummary => 1,
            ConciergeDetailLevel::ProactiveTriage | ConciergeDetailLevel::DailyBriefing => 5,
        };
        let message_limit = match detail_level {
            ConciergeDetailLevel::Minimal => 0,
            ConciergeDetailLevel::ContextSummary
            | ConciergeDetailLevel::ProactiveTriage
            | ConciergeDetailLevel::DailyBriefing => 5,
        };

        let recent_threads = persisted_recent_threads
            .iter()
            .take(thread_limit.min(1))
            .map(|thread| {
                let mut thread = thread.clone();
                if message_limit == 0 {
                    thread.opening_message = None;
                    thread.last_messages.clear();
                }
                thread
            })
            .collect();

        WelcomeContext {
            recent_threads,
            latest_goal_run: goal_context.latest_goal_run.as_ref().map(goal_run_summary),
            running_goal_total: goal_context.running_goal_total,
            paused_goal_total: goal_context.paused_goal_total,
        }
    }
}

pub(crate) fn is_user_hidden_task(task: &AgentTask) -> bool {
    task.sub_agent_def_id.as_deref()
        == Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
}

pub(crate) fn is_user_visible_thread(thread: &AgentThread) -> bool {
    thread.messages.iter().any(|message| {
        crate::agent::agent_identity::is_weles_agent_scope(
            &crate::agent::agent_identity::extract_persona_id(Some(&message.content))
                .unwrap_or_default(),
        )
    })
}

pub(super) fn is_heartbeat_thread(thread: &AgentThread) -> bool {
    thread.title.starts_with("HEARTBEAT SYNTHESIS")
        || thread.title.starts_with("Heartbeat check:")
        || thread.messages.iter().any(|message| {
            message.role == MessageRole::User
                && (message.content.starts_with("HEARTBEAT SYNTHESIS")
                    || message.content.starts_with("Heartbeat check:"))
        })
}

fn thread_summary_from_persisted_thread(thread: zorai_protocol::AgentDbThread) -> ThreadSummary {
    ThreadSummary {
        id: thread.id,
        title: thread.title,
        updated_at: thread.updated_at.max(0) as u64,
        message_count: thread.message_count.max(0) as usize,
        opening_message: None,
        last_messages: Vec::new(),
    }
}

fn goal_run_label(goal_run: &GoalRun) -> String {
    goal_run.title.trim().to_string()
}

fn goal_run_summary(goal_run: &GoalRun) -> GoalRunSummary {
    GoalRunSummary {
        label: goal_run_label(goal_run),
        prompt: non_empty_string(goal_run.goal.trim()),
        status: goal_run.status,
        updated_at: goal_run.updated_at,
        summary: goal_run_summary_excerpt(goal_run),
        latest_step_result: goal_run_latest_step_result(goal_run),
    }
}

fn goal_run_summary_excerpt(goal_run: &GoalRun) -> Option<String> {
    [
        goal_run.reflection_summary.as_deref(),
        goal_run.plan_summary.as_deref(),
        goal_run.current_step_title.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|value| !value.is_empty())
    .map(ToOwned::to_owned)
}

fn goal_run_latest_step_result(goal_run: &GoalRun) -> Option<String> {
    goal_run
        .steps
        .iter()
        .filter_map(|step| {
            let result = step
                .summary
                .as_deref()
                .or(step.error.as_deref())
                .and_then(|value| non_empty_string(value.trim()))?;
            let timestamp = step
                .completed_at
                .or(step.started_at)
                .unwrap_or(step.position as u64);
            Some((timestamp, step.position, result))
        })
        .max_by_key(|(timestamp, position, _)| (*timestamp, *position))
        .map(|(_, _, result)| result)
}

fn non_empty_string(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}
