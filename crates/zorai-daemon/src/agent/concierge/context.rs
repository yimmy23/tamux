use super::*;
use crate::session_manager::SessionManager;

impl ConciergeEngine {
    pub(crate) async fn recent_persisted_history_threads(
        &self,
        session_manager: &Arc<SessionManager>,
        limit: usize,
    ) -> Vec<ThreadSummary> {
        match session_manager.list_agent_threads().await {
            Ok(mut threads) => {
                threads.retain(|thread| include_persisted_thread_in_concierge_context(thread));
                threads.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
                threads.truncate(limit.max(1));
                threads
                    .into_iter()
                    .map(thread_summary_from_persisted_thread)
                    .collect()
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
        persisted_recent_threads: &[ThreadSummary],
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

        let goal_runs_guard = goal_runs.lock().await;
        let goal_thread_ids = goal_thread_ids(goal_runs_guard.iter());
        let latest_goal_run = latest_goal_run(goal_runs_guard.iter());
        let running_goal_total = goal_runs_guard
            .iter()
            .filter(|goal_run| goal_run.status == GoalRunStatus::Running)
            .count();
        let paused_goal_total = goal_runs_guard
            .iter()
            .filter(|goal_run| goal_run.status == GoalRunStatus::Paused)
            .count();
        drop(goal_runs_guard);

        let threads_guard = threads.read().await;
        let mut recent_threads: Vec<ThreadSummary> = threads_guard
            .values()
            .filter(|thread| {
                include_thread_in_concierge_context(thread) && !goal_thread_ids.contains(&thread.id)
            })
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
        if recent_threads.len() < thread_limit {
            let missing_count = thread_limit - recent_threads.len();
            let present_thread_ids: std::collections::HashSet<String> = recent_threads
                .iter()
                .map(|thread| thread.id.clone())
                .collect();
            recent_threads.extend(
                persisted_recent_threads
                    .iter()
                    .filter(|thread| {
                        !present_thread_ids.contains(&thread.id)
                            && !goal_thread_ids.contains(&thread.id)
                    })
                    .take(missing_count)
                    .cloned(),
            );
        }
        recent_threads.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        recent_threads.truncate(thread_limit);
        drop(threads_guard);

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
        let goal_runs_guard = goal_runs.lock().await;
        let goal_thread_ids = goal_thread_ids(goal_runs_guard.iter());
        let latest_goal_run = latest_goal_run(goal_runs_guard.iter());
        let running_goal_total = goal_runs_guard
            .iter()
            .filter(|goal_run| goal_run.status == GoalRunStatus::Running)
            .count();
        let paused_goal_total = goal_runs_guard
            .iter()
            .filter(|goal_run| goal_run.status == GoalRunStatus::Paused)
            .count();
        drop(goal_runs_guard);

        let threads_guard = threads.read().await;
        let recent_threads = threads_guard
            .values()
            .filter(|thread| {
                include_thread_in_concierge_context(thread) && !goal_thread_ids.contains(&thread.id)
            })
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

fn include_persisted_thread_in_concierge_context(thread: &zorai_protocol::AgentDbThread) -> bool {
    thread.id != CONCIERGE_THREAD_ID
        && !crate::agent::agent_identity::is_internal_dm_thread(&thread.id)
        && !crate::agent::agent_identity::is_participant_playground_thread(&thread.id)
        && !thread.title.starts_with("HEARTBEAT SYNTHESIS")
        && !thread.title.starts_with("Heartbeat check:")
        && thread.message_count > 0
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

fn latest_goal_run<'a, I>(goal_runs: I) -> Option<GoalRunSummary>
where
    I: IntoIterator<Item = &'a GoalRun>,
{
    goal_runs
        .into_iter()
        .max_by_key(|goal_run| goal_run.updated_at)
        .map(goal_run_summary)
}

fn goal_thread_ids<'a, I>(goal_runs: I) -> std::collections::HashSet<String>
where
    I: IntoIterator<Item = &'a GoalRun>,
{
    let mut ids = std::collections::HashSet::new();
    for goal_run in goal_runs {
        insert_goal_thread_id(&mut ids, goal_run.thread_id.as_deref());
        insert_goal_thread_id(&mut ids, goal_run.root_thread_id.as_deref());
        insert_goal_thread_id(&mut ids, goal_run.active_thread_id.as_deref());
        for thread_id in &goal_run.execution_thread_ids {
            insert_goal_thread_id(&mut ids, Some(thread_id));
        }
    }
    ids
}

fn insert_goal_thread_id(ids: &mut std::collections::HashSet<String>, thread_id: Option<&str>) {
    let Some(thread_id) = thread_id
        .map(str::trim)
        .filter(|thread_id| !thread_id.is_empty())
    else {
        return;
    };
    ids.insert(thread_id.to_string());
}
