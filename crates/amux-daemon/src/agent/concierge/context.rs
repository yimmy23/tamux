use super::*;
use crate::session_manager::SessionManager;

impl ConciergeEngine {
    pub(crate) async fn recent_persisted_history_thread_ids(
        &self,
        session_manager: &Arc<SessionManager>,
        limit: usize,
    ) -> Vec<String> {
        match session_manager.list_agent_threads().await {
            Ok(mut threads) => {
                threads.retain(|thread| include_persisted_thread_in_concierge_context(thread));
                threads.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
                threads.truncate(limit.max(1));
                threads.into_iter().map(|thread| thread.id).collect()
            }
            Err(error) => {
                tracing::warn!("concierge: failed to inspect persisted thread history: {error}");
                Vec::new()
            }
        }
    }

    pub(super) async fn gather_context(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
        _tasks: &tokio::sync::Mutex<std::collections::VecDeque<AgentTask>>,
        goal_runs: &tokio::sync::Mutex<std::collections::VecDeque<GoalRun>>,
        detail_level: ConciergeDetailLevel,
    ) -> WelcomeContext {
        let threads_guard = threads.read().await;
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

        let mut recent_threads: Vec<ThreadSummary> = threads_guard
            .values()
            .filter(|thread| include_thread_in_concierge_context(thread))
            .map(|thread| {
                let opening_message = thread
                    .messages
                    .iter()
                    .find(|message| {
                        message.role == MessageRole::User && !message.content.is_empty()
                    })
                    .or_else(|| {
                        thread
                            .messages
                            .iter()
                            .find(|message| !message.content.is_empty())
                    })
                    .map(format_message_snippet);
                let last_messages: Vec<String> = thread
                    .messages
                    .iter()
                    .rev()
                    .take(message_limit)
                    .map(format_message_snippet)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                ThreadSummary {
                    id: thread.id.clone(),
                    title: thread.title.clone(),
                    updated_at: thread.updated_at,
                    message_count: thread.messages.len(),
                    opening_message,
                    last_messages,
                }
            })
            .collect();
        recent_threads.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        recent_threads.truncate(thread_limit);
        drop(threads_guard);

        let goal_runs_guard = goal_runs.lock().await;
        let latest_goal_run = latest_goal_run(goal_runs_guard.iter());
        let running_goal_total = goal_runs_guard
            .iter()
            .filter(|goal_run| goal_run.status == GoalRunStatus::Running)
            .count();
        let paused_goal_total = goal_runs_guard
            .iter()
            .filter(|goal_run| goal_run.status == GoalRunStatus::Paused)
            .count();

        WelcomeContext {
            recent_threads,
            latest_goal_run,
            running_goal_total,
            paused_goal_total,
        }
    }

    pub(super) async fn gather_gateway_context(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
        goal_runs: &tokio::sync::Mutex<std::collections::VecDeque<GoalRun>>,
    ) -> WelcomeContext {
        let threads_guard = threads.read().await;
        let recent_threads = threads_guard
            .values()
            .filter(|thread| include_thread_in_concierge_context(thread))
            .max_by_key(|thread| thread.updated_at)
            .map(|thread| {
                vec![ThreadSummary {
                    id: thread.id.clone(),
                    title: thread.title.clone(),
                    updated_at: thread.updated_at,
                    message_count: thread.messages.len(),
                    opening_message: None,
                    last_messages: Vec::new(),
                }]
            })
            .unwrap_or_default();
        drop(threads_guard);

        let goal_runs_guard = goal_runs.lock().await;
        let latest_goal_run = latest_goal_run(goal_runs_guard.iter());
        let running_goal_total = goal_runs_guard
            .iter()
            .filter(|goal_run| goal_run.status == GoalRunStatus::Running)
            .count();
        let paused_goal_total = goal_runs_guard
            .iter()
            .filter(|goal_run| goal_run.status == GoalRunStatus::Paused)
            .count();

        WelcomeContext {
            recent_threads,
            latest_goal_run,
            running_goal_total,
            paused_goal_total,
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

fn include_thread_in_concierge_context(thread: &AgentThread) -> bool {
    thread.id != CONCIERGE_THREAD_ID
        && !crate::agent::agent_identity::is_internal_dm_thread(&thread.id)
        && !crate::agent::agent_identity::is_participant_playground_thread(&thread.id)
        && !is_heartbeat_thread(thread)
        && !is_user_visible_thread(thread)
        && thread
            .messages
            .iter()
            .any(|message| message.role == MessageRole::User && !message.content.is_empty())
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

fn include_persisted_thread_in_concierge_context(thread: &amux_protocol::AgentDbThread) -> bool {
    thread.id != CONCIERGE_THREAD_ID
        && !crate::agent::agent_identity::is_internal_dm_thread(&thread.id)
        && !crate::agent::agent_identity::is_participant_playground_thread(&thread.id)
        && !thread.title.starts_with("HEARTBEAT SYNTHESIS")
        && !thread.title.starts_with("Heartbeat check:")
        && thread.message_count > 0
}

fn format_message_snippet(message: &AgentMessage) -> String {
    let role = match message.role {
        MessageRole::User => "User",
        MessageRole::Assistant => "Assistant",
        _ => "System",
    };
    let snippet: String = message.content.chars().take(120).collect();
    format!("{role}: {snippet}")
}

fn goal_run_label(goal_run: &GoalRun) -> String {
    goal_run.title.trim().to_string()
}

fn goal_run_summary(goal_run: &GoalRun) -> GoalRunSummary {
    GoalRunSummary {
        label: goal_run_label(goal_run),
        status: goal_run.status,
        updated_at: goal_run.updated_at,
        summary: goal_run_summary_excerpt(goal_run),
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

fn latest_goal_run<'a, I>(goal_runs: I) -> Option<GoalRunSummary>
where
    I: IntoIterator<Item = &'a GoalRun>,
{
    goal_runs
        .into_iter()
        .max_by_key(|goal_run| goal_run.updated_at)
        .map(goal_run_summary)
}
