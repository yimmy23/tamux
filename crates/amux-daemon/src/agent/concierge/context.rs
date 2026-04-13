use super::*;

impl ConciergeEngine {
    pub(super) async fn gather_context(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
        tasks: &tokio::sync::Mutex<std::collections::VecDeque<AgentTask>>,
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
            .map(|t| {
                let opening_message = t
                    .messages
                    .iter()
                    .find(|message| {
                        message.role == MessageRole::User && !message.content.is_empty()
                    })
                    .or_else(|| {
                        t.messages
                            .iter()
                            .find(|message| !message.content.is_empty())
                    })
                    .map(format_message_snippet);
                let last_messages: Vec<String> = t
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
                    id: t.id.clone(),
                    title: t.title.clone(),
                    updated_at: t.updated_at,
                    message_count: t.messages.len(),
                    opening_message,
                    last_messages,
                }
            })
            .collect();
        recent_threads.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        recent_threads.truncate(thread_limit);
        drop(threads_guard);

        let (pending_task_total, pending_tasks) = if matches!(
            detail_level,
            ConciergeDetailLevel::ContextSummary
                | ConciergeDetailLevel::ProactiveTriage
                | ConciergeDetailLevel::DailyBriefing
        ) {
            let tasks_guard = tasks.lock().await;
            let preferred_thread_id = recent_threads.first().map(|thread| thread.id.as_str());
            sample_pending_tasks(tasks_guard.iter(), preferred_thread_id)
        } else {
            (0, Vec::new())
        };

        WelcomeContext {
            recent_threads,
            pending_task_total,
            pending_tasks,
        }
    }

    pub(super) async fn gather_gateway_context(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
        tasks: &tokio::sync::Mutex<std::collections::VecDeque<AgentTask>>,
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

        let tasks_guard = tasks.lock().await;
        let pending_task_total = tasks_guard
            .iter()
            .filter(|task| {
                matches!(
                    task.status,
                    TaskStatus::Queued | TaskStatus::InProgress | TaskStatus::Blocked
                ) && !is_user_hidden_task(task)
            })
            .count();

        WelcomeContext {
            recent_threads,
            pending_task_total,
            pending_tasks: Vec::new(),
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

fn format_message_snippet(message: &AgentMessage) -> String {
    let role = match message.role {
        MessageRole::User => "User",
        MessageRole::Assistant => "Assistant",
        _ => "System",
    };
    let snippet: String = message.content.chars().take(120).collect();
    format!("{role}: {snippet}")
}

fn sample_pending_tasks<'a, I>(tasks: I, preferred_thread_id: Option<&str>) -> (usize, Vec<String>)
where
    I: IntoIterator<Item = &'a AgentTask>,
{
    let unresolved: Vec<&AgentTask> = tasks
        .into_iter()
        .filter(|task| {
            matches!(
                task.status,
                TaskStatus::Queued | TaskStatus::InProgress | TaskStatus::Blocked
            ) && !is_user_hidden_task(task)
        })
        .collect();
    let total = unresolved.len();
    if total == 0 {
        return (0, Vec::new());
    }

    let mut sampled = Vec::new();
    if let Some(thread_id) = preferred_thread_id {
        let preferred: Vec<&AgentTask> = unresolved
            .iter()
            .copied()
            .filter(|task| task_belongs_to_thread(task, thread_id))
            .collect();
        append_task_slice(&mut sampled, &preferred, 5);
    }

    if sampled.len() < 5 {
        append_task_slice(&mut sampled, &unresolved, 5);
    }

    let entries = sampled
        .into_iter()
        .map(|task| {
            format!(
                "- [{}] {} ({})",
                format!("{:?}", task.status),
                task.title,
                format_timestamp(task.created_at)
            )
        })
        .collect();

    (total, entries)
}

fn append_task_slice<'a>(
    sampled: &mut Vec<&'a AgentTask>,
    tasks: &[&'a AgentTask],
    target_len: usize,
) {
    if sampled.len() >= target_len || tasks.is_empty() {
        return;
    }

    let mut sorted = tasks.to_vec();
    sorted.sort_by_key(|task| task.created_at);
    sorted.retain(|task| !sampled.iter().any(|existing| existing.id == task.id));
    if sorted.is_empty() {
        return;
    }

    let remaining_slots = target_len.saturating_sub(sampled.len());
    let oldest_quota = match remaining_slots {
        0 => 0,
        1 => 1,
        _ => std::cmp::min(2, remaining_slots / 2),
    };
    let newest_quota = remaining_slots.saturating_sub(oldest_quota);

    for task in sorted.iter().take(oldest_quota) {
        sampled.push(*task);
    }

    for task in sorted.iter().rev().take(newest_quota).rev() {
        if sampled.iter().any(|existing| existing.id == task.id) {
            continue;
        }
        sampled.push(*task);
    }
}

fn task_belongs_to_thread(task: &AgentTask, thread_id: &str) -> bool {
    task.thread_id.as_deref() == Some(thread_id)
        || task.parent_thread_id.as_deref() == Some(thread_id)
}
