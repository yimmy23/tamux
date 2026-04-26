use super::*;
use amux_protocol::{
    WorkspaceActor, WorkspaceNotice, WorkspacePriority, WorkspaceSettings, WorkspaceTask,
    WorkspaceTaskRuntimeHistoryEntry,
};
use serde::Serialize;

#[derive(Serialize)]
pub(super) struct WorkspaceMirror<'a> {
    pub(super) schema_version: u32,
    pub(super) generated_at: u64,
    pub(super) settings: &'a WorkspaceSettings,
    pub(super) tasks: &'a [WorkspaceTask],
    pub(super) notices: &'a [WorkspaceNotice],
}

pub(super) const WORKSPACE_MIRROR_SCHEMA_VERSION: u32 = 1;

pub(super) fn workspace_priority_label(priority: &WorkspacePriority) -> &'static str {
    match priority {
        WorkspacePriority::Low => "low",
        WorkspacePriority::Normal => "normal",
        WorkspacePriority::High => "high",
        WorkspacePriority::Urgent => "urgent",
    }
}

pub(super) fn workspace_task_priority(priority: &WorkspacePriority) -> TaskPriority {
    match priority {
        WorkspacePriority::Low => TaskPriority::Low,
        WorkspacePriority::Normal => TaskPriority::Normal,
        WorkspacePriority::High => TaskPriority::High,
        WorkspacePriority::Urgent => TaskPriority::Urgent,
    }
}

pub(super) fn actor_target(actor: &WorkspaceActor) -> Option<String> {
    match actor {
        WorkspaceActor::User => None,
        WorkspaceActor::Agent(agent_id) | WorkspaceActor::Subagent(agent_id) => {
            Some(agent_id.trim().to_string()).filter(|value| !value.is_empty())
        }
    }
}

pub(super) fn actor_label(actor: &WorkspaceActor) -> String {
    match actor {
        WorkspaceActor::User => "user".to_string(),
        WorkspaceActor::Agent(agent_id) => format!("agent:{agent_id}"),
        WorkspaceActor::Subagent(agent_id) => format!("subagent:{agent_id}"),
    }
}

pub(super) fn upsert_workspace_runtime_history_entry(
    task: &mut WorkspaceTask,
    entry: WorkspaceTaskRuntimeHistoryEntry,
) {
    if let Some(index) = task
        .runtime_history
        .iter()
        .position(|existing| runtime_history_entries_match(existing, &entry))
    {
        task.runtime_history[index] =
            merge_workspace_runtime_history_entry(task.runtime_history[index].clone(), entry);
        let updated = task.runtime_history.remove(index);
        task.runtime_history.insert(0, updated);
        return;
    }
    task.runtime_history.insert(0, entry);
}

fn runtime_history_entries_match(
    left: &WorkspaceTaskRuntimeHistoryEntry,
    right: &WorkspaceTaskRuntimeHistoryEntry,
) -> bool {
    left.agent_task_id.is_some() && left.agent_task_id == right.agent_task_id
        || left.thread_id.is_some() && left.thread_id == right.thread_id
        || left.goal_run_id.is_some() && left.goal_run_id == right.goal_run_id
}

fn merge_workspace_runtime_history_entry(
    mut existing: WorkspaceTaskRuntimeHistoryEntry,
    next: WorkspaceTaskRuntimeHistoryEntry,
) -> WorkspaceTaskRuntimeHistoryEntry {
    existing.task_type = next.task_type;
    existing.thread_id = next.thread_id.or(existing.thread_id);
    existing.goal_run_id = next.goal_run_id.or(existing.goal_run_id);
    existing.agent_task_id = next.agent_task_id.or(existing.agent_task_id);
    existing.source = next.source.or(existing.source);
    existing.title = next.title.or(existing.title);
    existing.review_path = next.review_path.or(existing.review_path);
    existing.review_feedback = next.review_feedback.or(existing.review_feedback);
    existing.archived_at = existing.archived_at.max(next.archived_at);
    existing
}

pub(super) fn task_run_prompt(task: &WorkspaceTask) -> String {
    let dod = task
        .definition_of_done
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("\n\nDefinition of done:\n{value}"))
        .unwrap_or_default();
    let failed_review = task
        .runtime_history
        .iter()
        .find(|entry| entry.review_path.is_some() || entry.review_feedback.is_some())
        .map(|entry| {
            let previous_runtime = match (&entry.thread_id, &entry.goal_run_id) {
                (Some(thread_id), _) => format!("Previous thread: {thread_id}"),
                (_, Some(goal_run_id)) => format!("Previous goal run: {goal_run_id}"),
                _ => "Previous runtime: not recorded".to_string(),
            };
            let review_path = entry
                .review_path
                .as_deref()
                .map(|path| format!("\nReview file: {path}"))
                .unwrap_or_default();
            let feedback = entry
                .review_feedback
                .as_deref()
                .map(|feedback| format!("\nReviewer feedback:\n{feedback}"))
                .unwrap_or_default();
            format!(
                "\n\nFailed review follow-up:\n{previous_runtime}{review_path}{feedback}\n\nAddress the failed review before submitting completion again."
            )
        })
        .unwrap_or_default();
    format!(
        "Workspace task: {}\n\nWorkspace task id: {}\n\nDescription:\n{}{}{}\n\nWhen the task is complete, call workspace_submit_completion with task_id={} and a concise summary of what you delivered.",
        task.title, task.id, task.description, dod, failed_review, task.id
    )
}

pub(super) fn reserved_thread_id(task_id: &str) -> String {
    format!("workspace-thread:{task_id}")
}

pub(super) fn reserved_goal_run_id(task_id: &str) -> String {
    format!("workspace-goal:{task_id}")
}

pub(super) fn workspace_root(history: &HistoryStore, workspace_id: &str) -> PathBuf {
    history
        .data_root()
        .join("workspaces")
        .join(format!("workspace-{workspace_id}"))
}
