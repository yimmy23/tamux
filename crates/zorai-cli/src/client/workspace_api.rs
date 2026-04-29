use anyhow::Result;
use zorai_protocol::{
    ClientMessage, DaemonMessage, WorkspaceNotice, WorkspaceOperator, WorkspaceReviewSubmission,
    WorkspaceSettings, WorkspaceTask, WorkspaceTaskCreate, WorkspaceTaskMove, WorkspaceTaskUpdate,
};

use super::connection::roundtrip;

pub async fn send_workspace_settings(workspace_id: String) -> Result<WorkspaceSettings> {
    match roundtrip(ClientMessage::AgentGetWorkspaceSettings { workspace_id }).await? {
        DaemonMessage::AgentWorkspaceSettings { settings } => Ok(settings),
        DaemonMessage::AgentWorkspaceError { message }
        | DaemonMessage::AgentError { message }
        | DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_workspace_operator(
    workspace_id: String,
    operator: WorkspaceOperator,
) -> Result<WorkspaceSettings> {
    match roundtrip(ClientMessage::AgentSetWorkspaceOperator {
        workspace_id,
        operator,
    })
    .await?
    {
        DaemonMessage::AgentWorkspaceSettings { settings } => Ok(settings),
        DaemonMessage::AgentWorkspaceError { message }
        | DaemonMessage::AgentError { message }
        | DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_workspace_task_create(request: WorkspaceTaskCreate) -> Result<WorkspaceTask> {
    match roundtrip(ClientMessage::AgentCreateWorkspaceTask { request }).await? {
        DaemonMessage::AgentWorkspaceTaskUpdated { task } => Ok(task),
        DaemonMessage::AgentWorkspaceError { message }
        | DaemonMessage::AgentError { message }
        | DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_workspace_task_list(
    workspace_id: String,
    include_deleted: bool,
) -> Result<Vec<WorkspaceTask>> {
    match roundtrip(ClientMessage::AgentListWorkspaceTasks {
        workspace_id,
        include_deleted,
    })
    .await?
    {
        DaemonMessage::AgentWorkspaceTaskList { tasks, .. } => Ok(tasks),
        DaemonMessage::AgentWorkspaceError { message }
        | DaemonMessage::AgentError { message }
        | DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_workspace_task_get(task_id: String) -> Result<Option<WorkspaceTask>> {
    match roundtrip(ClientMessage::AgentGetWorkspaceTask { task_id }).await? {
        DaemonMessage::AgentWorkspaceTaskDetail { task } => Ok(task),
        DaemonMessage::AgentWorkspaceError { message }
        | DaemonMessage::AgentError { message }
        | DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_workspace_task_update(
    task_id: String,
    update: WorkspaceTaskUpdate,
) -> Result<Option<WorkspaceTask>> {
    match roundtrip(ClientMessage::AgentUpdateWorkspaceTask { task_id, update }).await? {
        DaemonMessage::AgentWorkspaceTaskUpdated { task } => Ok(Some(task)),
        DaemonMessage::AgentWorkspaceTaskDetail { task } => Ok(task),
        DaemonMessage::AgentWorkspaceError { message }
        | DaemonMessage::AgentError { message }
        | DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_workspace_task_move(request: WorkspaceTaskMove) -> Result<Option<WorkspaceTask>> {
    match roundtrip(ClientMessage::AgentMoveWorkspaceTask { request }).await? {
        DaemonMessage::AgentWorkspaceTaskUpdated { task } => Ok(Some(task)),
        DaemonMessage::AgentWorkspaceTaskDetail { task } => Ok(task),
        DaemonMessage::AgentWorkspaceError { message }
        | DaemonMessage::AgentError { message }
        | DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_workspace_task_run(task_id: String) -> Result<WorkspaceTask> {
    match roundtrip(ClientMessage::AgentRunWorkspaceTask { task_id }).await? {
        DaemonMessage::AgentWorkspaceTaskUpdated { task } => Ok(task),
        DaemonMessage::AgentWorkspaceError { message }
        | DaemonMessage::AgentError { message }
        | DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_workspace_task_pause(task_id: String) -> Result<Option<WorkspaceTask>> {
    match roundtrip(ClientMessage::AgentPauseWorkspaceTask { task_id }).await? {
        DaemonMessage::AgentWorkspaceTaskUpdated { task } => Ok(Some(task)),
        DaemonMessage::AgentWorkspaceTaskDetail { task } => Ok(task),
        DaemonMessage::AgentWorkspaceError { message }
        | DaemonMessage::AgentError { message }
        | DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_workspace_task_stop(task_id: String) -> Result<Option<WorkspaceTask>> {
    match roundtrip(ClientMessage::AgentStopWorkspaceTask { task_id }).await? {
        DaemonMessage::AgentWorkspaceTaskUpdated { task } => Ok(Some(task)),
        DaemonMessage::AgentWorkspaceTaskDetail { task } => Ok(task),
        DaemonMessage::AgentWorkspaceError { message }
        | DaemonMessage::AgentError { message }
        | DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_workspace_task_delete(task_id: String) -> Result<(String, Option<u64>)> {
    match roundtrip(ClientMessage::AgentDeleteWorkspaceTask { task_id }).await? {
        DaemonMessage::AgentWorkspaceTaskDeleted {
            task_id,
            deleted_at,
        } => Ok((task_id, deleted_at)),
        DaemonMessage::AgentWorkspaceTaskDetail { task: None } => Ok((String::new(), None)),
        DaemonMessage::AgentWorkspaceError { message }
        | DaemonMessage::AgentError { message }
        | DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_workspace_review(
    review: WorkspaceReviewSubmission,
) -> Result<Option<WorkspaceTask>> {
    match roundtrip(ClientMessage::AgentSubmitWorkspaceReview { review }).await? {
        DaemonMessage::AgentWorkspaceTaskUpdated { task } => Ok(Some(task)),
        DaemonMessage::AgentWorkspaceTaskDetail { task } => Ok(task),
        DaemonMessage::AgentWorkspaceError { message }
        | DaemonMessage::AgentError { message }
        | DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_workspace_notice_list(
    workspace_id: String,
    task_id: Option<String>,
) -> Result<Vec<WorkspaceNotice>> {
    match roundtrip(ClientMessage::AgentListWorkspaceNotices {
        workspace_id,
        task_id,
    })
    .await?
    {
        DaemonMessage::AgentWorkspaceNoticeList { notices, .. } => Ok(notices),
        DaemonMessage::AgentWorkspaceError { message }
        | DaemonMessage::AgentError { message }
        | DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}
