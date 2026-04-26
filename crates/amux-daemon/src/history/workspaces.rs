use super::*;
use amux_protocol::{
    WorkspaceActor, WorkspaceNotice, WorkspaceOperator, WorkspacePriority, WorkspaceSettings,
    WorkspaceTask, WorkspaceTaskRuntimeHistoryEntry, WorkspaceTaskStatus, WorkspaceTaskType,
};

fn workspace_operator_to_str(operator: WorkspaceOperator) -> &'static str {
    match operator {
        WorkspaceOperator::User => "user",
        WorkspaceOperator::Svarog => "svarog",
    }
}

fn parse_workspace_operator(value: &str) -> WorkspaceOperator {
    match value {
        "svarog" => WorkspaceOperator::Svarog,
        _ => WorkspaceOperator::User,
    }
}

fn workspace_task_type_to_str(task_type: WorkspaceTaskType) -> &'static str {
    match task_type {
        WorkspaceTaskType::Thread => "thread",
        WorkspaceTaskType::Goal => "goal",
    }
}

fn parse_workspace_task_type(value: &str) -> WorkspaceTaskType {
    match value {
        "thread" => WorkspaceTaskType::Thread,
        _ => WorkspaceTaskType::Goal,
    }
}

fn workspace_task_status_to_str(status: WorkspaceTaskStatus) -> &'static str {
    match status {
        WorkspaceTaskStatus::Todo => "todo",
        WorkspaceTaskStatus::InProgress => "in_progress",
        WorkspaceTaskStatus::InReview => "in_review",
        WorkspaceTaskStatus::Done => "done",
    }
}

fn parse_workspace_task_status(value: &str) -> WorkspaceTaskStatus {
    match value {
        "in_progress" => WorkspaceTaskStatus::InProgress,
        "in_review" => WorkspaceTaskStatus::InReview,
        "done" => WorkspaceTaskStatus::Done,
        _ => WorkspaceTaskStatus::Todo,
    }
}

fn workspace_priority_to_str(priority: WorkspacePriority) -> &'static str {
    match priority {
        WorkspacePriority::Low => "low",
        WorkspacePriority::Normal => "normal",
        WorkspacePriority::High => "high",
        WorkspacePriority::Urgent => "urgent",
    }
}

fn parse_workspace_priority(value: &str) -> WorkspacePriority {
    match value {
        "normal" => WorkspacePriority::Normal,
        "high" => WorkspacePriority::High,
        "urgent" => WorkspacePriority::Urgent,
        _ => WorkspacePriority::Low,
    }
}

fn serialize_actor(actor: &WorkspaceActor) -> std::result::Result<String, tokio_rusqlite::Error> {
    serde_json::to_string(actor).call_err()
}

fn serialize_optional_actor(
    actor: &Option<WorkspaceActor>,
) -> std::result::Result<Option<String>, tokio_rusqlite::Error> {
    actor.as_ref().map(serialize_actor).transpose()
}

fn parse_actor_json(value: String) -> WorkspaceActor {
    serde_json::from_str(&value).unwrap_or(WorkspaceActor::User)
}

fn parse_optional_actor_json(value: Option<String>) -> Option<WorkspaceActor> {
    value.and_then(|json| serde_json::from_str(&json).ok())
}

fn serialize_runtime_history(
    history: &[WorkspaceTaskRuntimeHistoryEntry],
) -> std::result::Result<String, tokio_rusqlite::Error> {
    serde_json::to_string(history).call_err()
}

fn parse_runtime_history_json(value: String) -> Vec<WorkspaceTaskRuntimeHistoryEntry> {
    serde_json::from_str(&value).unwrap_or_default()
}

fn map_workspace_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkspaceTask> {
    Ok(WorkspaceTask {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        title: row.get(2)?,
        task_type: parse_workspace_task_type(&row.get::<_, String>(3)?),
        description: row.get(4)?,
        definition_of_done: row.get(5)?,
        priority: parse_workspace_priority(&row.get::<_, String>(6)?),
        status: parse_workspace_task_status(&row.get::<_, String>(7)?),
        sort_order: row.get(8)?,
        reporter: parse_actor_json(row.get(9)?),
        assignee: parse_optional_actor_json(row.get(10)?),
        reviewer: parse_optional_actor_json(row.get(11)?),
        thread_id: row.get(12)?,
        goal_run_id: row.get(13)?,
        runtime_history: parse_runtime_history_json(row.get(14)?),
        created_at: row.get::<_, i64>(15)? as u64,
        updated_at: row.get::<_, i64>(16)? as u64,
        started_at: row.get::<_, Option<i64>>(17)?.map(|value| value as u64),
        completed_at: row.get::<_, Option<i64>>(18)?.map(|value| value as u64),
        deleted_at: row.get::<_, Option<i64>>(19)?.map(|value| value as u64),
        last_notice_id: row.get(20)?,
    })
}

fn map_workspace_notice(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkspaceNotice> {
    Ok(WorkspaceNotice {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        task_id: row.get(2)?,
        notice_type: row.get(3)?,
        message: row.get(4)?,
        actor: parse_optional_actor_json(row.get(5)?),
        created_at: row.get::<_, i64>(6)? as u64,
    })
}

impl HistoryStore {
    pub async fn upsert_workspace_settings(&self, settings: &WorkspaceSettings) -> Result<()> {
        let settings = settings.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO workspace_settings \
                     (workspace_id, workspace_root, operator, created_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        settings.workspace_id,
                        settings.workspace_root,
                        workspace_operator_to_str(settings.operator),
                        settings.created_at as i64,
                        settings.updated_at as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    pub async fn get_workspace_settings(
        &self,
        workspace_id: &str,
    ) -> Result<Option<WorkspaceSettings>> {
        let workspace_id = workspace_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT workspace_id, workspace_root, operator, created_at, updated_at \
                     FROM workspace_settings WHERE workspace_id = ?1",
                    params![workspace_id],
                    |row| {
                        Ok(WorkspaceSettings {
                            workspace_id: row.get(0)?,
                            workspace_root: row.get(1)?,
                            operator: parse_workspace_operator(&row.get::<_, String>(2)?),
                            created_at: row.get::<_, i64>(3)? as u64,
                            updated_at: row.get::<_, i64>(4)? as u64,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    pub async fn list_workspace_settings(&self) -> Result<Vec<WorkspaceSettings>> {
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT workspace_id, workspace_root, operator, created_at, updated_at \
                     FROM workspace_settings ORDER BY workspace_id ASC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(WorkspaceSettings {
                        workspace_id: row.get(0)?,
                        workspace_root: row.get(1)?,
                        operator: parse_workspace_operator(&row.get::<_, String>(2)?),
                        created_at: row.get::<_, i64>(3)? as u64,
                        updated_at: row.get::<_, i64>(4)? as u64,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    pub async fn upsert_workspace_task(&self, task: &WorkspaceTask) -> Result<()> {
        let task = task.clone();
        self.conn
            .call(move |conn| {
                let reporter_json = serialize_actor(&task.reporter)?;
                let assignee_json = serialize_optional_actor(&task.assignee)?;
                let reviewer_json = serialize_optional_actor(&task.reviewer)?;
                let runtime_history_json = serialize_runtime_history(&task.runtime_history)?;
                conn.execute(
                    "INSERT OR REPLACE INTO workspace_tasks \
                     (id, workspace_id, title, task_type, description, definition_of_done, priority, status, sort_order, reporter_json, assignee_json, reviewer_json, thread_id, goal_run_id, runtime_history_json, created_at, updated_at, started_at, completed_at, deleted_at, last_notice_id) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
                    params![
                        task.id,
                        task.workspace_id,
                        task.title,
                        workspace_task_type_to_str(task.task_type),
                        task.description,
                        task.definition_of_done,
                        workspace_priority_to_str(task.priority),
                        workspace_task_status_to_str(task.status),
                        task.sort_order,
                        reporter_json,
                        assignee_json,
                        reviewer_json,
                        task.thread_id,
                        task.goal_run_id,
                        runtime_history_json,
                        task.created_at as i64,
                        task.updated_at as i64,
                        task.started_at.map(|value| value as i64),
                        task.completed_at.map(|value| value as i64),
                        task.deleted_at.map(|value| value as i64),
                        task.last_notice_id,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    pub async fn get_workspace_task(&self, task_id: &str) -> Result<Option<WorkspaceTask>> {
        let task_id = task_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT id, workspace_id, title, task_type, description, definition_of_done, priority, status, sort_order, reporter_json, assignee_json, reviewer_json, thread_id, goal_run_id, runtime_history_json, created_at, updated_at, started_at, completed_at, deleted_at, last_notice_id \
                     FROM workspace_tasks WHERE id = ?1",
                    params![task_id],
                    map_workspace_task,
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    pub async fn get_workspace_task_by_thread_id(
        &self,
        thread_id: &str,
    ) -> Result<Option<WorkspaceTask>> {
        let thread_id = thread_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT id, workspace_id, title, task_type, description, definition_of_done, priority, status, sort_order, reporter_json, assignee_json, reviewer_json, thread_id, goal_run_id, runtime_history_json, created_at, updated_at, started_at, completed_at, deleted_at, last_notice_id \
                     FROM workspace_tasks WHERE thread_id = ?1 AND deleted_at IS NULL ORDER BY updated_at DESC LIMIT 1",
                    params![thread_id],
                    map_workspace_task,
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    pub async fn list_workspace_tasks(
        &self,
        workspace_id: &str,
        include_deleted: bool,
    ) -> Result<Vec<WorkspaceTask>> {
        let workspace_id = workspace_id.to_string();
        self.read_conn
            .call(move |conn| {
                let where_deleted = if include_deleted {
                    ""
                } else {
                    " AND deleted_at IS NULL"
                };
                let sql = format!(
                    "SELECT id, workspace_id, title, task_type, description, definition_of_done, priority, status, sort_order, reporter_json, assignee_json, reviewer_json, thread_id, goal_run_id, runtime_history_json, created_at, updated_at, started_at, completed_at, deleted_at, last_notice_id \
                     FROM workspace_tasks WHERE workspace_id = ?1{where_deleted} \
                     ORDER BY CASE status WHEN 'todo' THEN 0 WHEN 'in_progress' THEN 1 WHEN 'in_review' THEN 2 WHEN 'done' THEN 3 ELSE 4 END ASC, sort_order ASC, created_at ASC"
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params![workspace_id], map_workspace_task)?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    pub async fn max_workspace_task_sort_order(
        &self,
        workspace_id: &str,
        status: WorkspaceTaskStatus,
        exclude_task_id: Option<&str>,
    ) -> Result<Option<i64>> {
        let workspace_id = workspace_id.to_string();
        let status = workspace_task_status_to_str(status).to_string();
        let exclude_task_id = exclude_task_id.map(str::to_string);
        self.read_conn
            .call(move |conn| {
                match exclude_task_id {
                    Some(exclude_task_id) => conn
                        .query_row(
                            "SELECT MAX(sort_order) FROM workspace_tasks \
                             WHERE workspace_id = ?1 AND status = ?2 AND id != ?3 AND deleted_at IS NULL",
                            params![workspace_id, status, exclude_task_id],
                            |row| row.get::<_, Option<i64>>(0),
                        )
                        .map_err(Into::into),
                    None => conn
                        .query_row(
                            "SELECT MAX(sort_order) FROM workspace_tasks \
                             WHERE workspace_id = ?1 AND status = ?2 AND deleted_at IS NULL",
                            params![workspace_id, status],
                            |row| row.get::<_, Option<i64>>(0),
                        )
                        .map_err(Into::into),
                }
            })
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    pub async fn insert_workspace_notice(&self, notice: &WorkspaceNotice) -> Result<()> {
        let notice = notice.clone();
        self.conn
            .call(move |conn| {
                let actor_json = serialize_optional_actor(&notice.actor)?;
                conn.execute(
                    "INSERT OR REPLACE INTO workspace_notices \
                     (id, workspace_id, task_id, notice_type, message, actor_json, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        notice.id,
                        notice.workspace_id,
                        notice.task_id,
                        notice.notice_type,
                        notice.message,
                        actor_json,
                        notice.created_at as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    pub async fn list_workspace_notices(
        &self,
        workspace_id: &str,
        task_id: Option<&str>,
    ) -> Result<Vec<WorkspaceNotice>> {
        let workspace_id = workspace_id.to_string();
        let task_id = task_id.map(str::to_string);
        self.read_conn
            .call(move |conn| match task_id {
                Some(task_id) => {
                    let mut stmt = conn.prepare(
                        "SELECT id, workspace_id, task_id, notice_type, message, actor_json, created_at \
                         FROM workspace_notices WHERE workspace_id = ?1 AND task_id = ?2 ORDER BY created_at ASC",
                    )?;
                    let rows = stmt.query_map(params![workspace_id, task_id], map_workspace_notice)?;
                    rows.collect::<std::result::Result<Vec<_>, _>>()
                        .map_err(Into::into)
                }
                None => {
                    let mut stmt = conn.prepare(
                        "SELECT id, workspace_id, task_id, notice_type, message, actor_json, created_at \
                         FROM workspace_notices WHERE workspace_id = ?1 ORDER BY created_at ASC",
                    )?;
                    let rows = stmt.query_map(params![workspace_id], map_workspace_notice)?;
                    rows.collect::<std::result::Result<Vec<_>, _>>()
                        .map_err(Into::into)
                }
            })
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }
}
