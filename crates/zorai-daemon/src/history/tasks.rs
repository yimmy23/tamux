use super::*;

fn parse_string_vec_json(value: Option<String>) -> Option<Vec<String>> {
    value.and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
}

fn serialize_string_vec_json(
    value: &Option<Vec<String>>,
) -> std::result::Result<Option<String>, tokio_rusqlite::Error> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .call_err()
}

fn serialize_supervisor_config_json(
    value: &Option<crate::agent::types::SupervisorConfig>,
) -> std::result::Result<Option<String>, tokio_rusqlite::Error> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .call_err()
}

fn parse_supervisor_config_json(
    value: Option<String>,
) -> Option<crate::agent::types::SupervisorConfig> {
    value.and_then(|json| serde_json::from_str::<crate::agent::types::SupervisorConfig>(&json).ok())
}

fn preserve_notification_lifecycle_state(
    notification: &mut zorai_protocol::InboxNotification,
    existing: zorai_protocol::InboxNotification,
) {
    notification.read_at = notification.read_at.or(existing.read_at);
    notification.archived_at = notification.archived_at.or(existing.archived_at);
    notification.deleted_at = notification.deleted_at.or(existing.deleted_at);
}

impl HistoryStore {
    pub async fn upsert_notification(
        &self,
        notification: &zorai_protocol::InboxNotification,
    ) -> Result<()> {
        let row = crate::notifications::notification_event_row(notification)?;
        self.upsert_agent_event(&row).await
    }

    async fn get_notification_by_id(
        &self,
        notification_id: &str,
    ) -> Result<Option<zorai_protocol::InboxNotification>> {
        let notification_id = notification_id.to_string();
        self.conn
            .call(move |conn| {
                let payload_json: Option<String> = conn
                    .query_row(
                        "SELECT payload_json FROM agent_events WHERE id = ?1 AND category = ?2",
                        params![notification_id, crate::notifications::NOTIFICATION_CATEGORY],
                        |row| row.get(0),
                    )
                    .optional()?;
                Ok(payload_json.and_then(|json| {
                    serde_json::from_str::<zorai_protocol::InboxNotification>(&json).ok()
                }))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_notifications(
        &self,
        include_inactive: bool,
        limit: Option<usize>,
    ) -> Result<Vec<zorai_protocol::InboxNotification>> {
        let limit = limit.unwrap_or(500).max(1) as i64;
        self.read_conn
            .call(move |conn| {
                let inactive_filter = if include_inactive {
                    ""
                } else {
                    " AND json_extract(payload_json, '$.archived_at') IS NULL
                      AND json_extract(payload_json, '$.deleted_at') IS NULL"
                };
                let sql = format!(
                    "SELECT id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp
                     FROM agent_events
                     WHERE category = ?1
                       AND json_valid(payload_json)
                       {inactive_filter}
                     ORDER BY
                       COALESCE(json_extract(payload_json, '$.updated_at'), timestamp) DESC,
                       COALESCE(json_extract(payload_json, '$.created_at'), timestamp) DESC
                     LIMIT ?2"
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(
                    params![crate::notifications::NOTIFICATION_CATEGORY, limit],
                    map_agent_event_row,
                )?;
                Ok(rows
                    .filter_map(|row| row.ok())
                    .filter_map(|row| crate::notifications::parse_notification_row(&row))
                    .collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_notifications_by_source(
        &self,
        source: &str,
        include_inactive: bool,
        limit: Option<usize>,
    ) -> Result<Vec<zorai_protocol::InboxNotification>> {
        let source = source.to_string();
        let limit = limit.unwrap_or(500).max(1) as i64;
        self.read_conn
            .call(move |conn| {
                let inactive_filter = if include_inactive {
                    ""
                } else {
                    " AND json_extract(payload_json, '$.archived_at') IS NULL
                      AND json_extract(payload_json, '$.deleted_at') IS NULL"
                };
                let sql = format!(
                    "SELECT id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp
                     FROM agent_events
                     WHERE category = ?1
                       AND json_valid(payload_json)
                       AND json_extract(payload_json, '$.source') = ?2
                       {inactive_filter}
                     ORDER BY
                       COALESCE(json_extract(payload_json, '$.updated_at'), timestamp) DESC,
                       COALESCE(json_extract(payload_json, '$.created_at'), timestamp) DESC
                     LIMIT ?3"
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(
                    params![crate::notifications::NOTIFICATION_CATEGORY, source, limit],
                    map_agent_event_row,
                )?;
                Ok(rows
                    .filter_map(|row| row.ok())
                    .filter_map(|row| crate::notifications::parse_notification_row(&row))
                    .collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_agent_event(&self, entry: &AgentEventRow) -> Result<()> {
        let mut entry = entry.clone();
        if let Some(mut notification) = crate::notifications::parse_notification_row(&entry) {
            if let Some(existing) = self.get_notification_by_id(&notification.id).await? {
                preserve_notification_lifecycle_state(&mut notification, existing);
                entry = crate::notifications::notification_event_row(&notification)?;
            }
        }
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO agent_events \
                     (id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        entry.id,
                        entry.category,
                        entry.kind,
                        entry.pane_id,
                        entry.workspace_id,
                        entry.surface_id,
                        entry.session_id,
                        entry.payload_json,
                        entry.timestamp,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_agent_events(
        &self,
        category: Option<&str>,
        pane_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<AgentEventRow>> {
        let category = category.map(str::to_string);
        let pane_id = pane_id.map(str::to_string);
        self.conn.call(move |conn| {
        let limit = limit.unwrap_or(500).max(1) as i64;
        let sql = match (category.is_some(), pane_id.is_some()) {
            (true, true) => {
                "SELECT id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp \
                 FROM agent_events WHERE category = ?1 AND pane_id = ?2 ORDER BY timestamp DESC LIMIT ?3"
            }
            (true, false) => {
                "SELECT id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp \
                 FROM agent_events WHERE category = ?1 ORDER BY timestamp DESC LIMIT ?2"
            }
            (false, true) => {
                "SELECT id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp \
                 FROM agent_events WHERE pane_id = ?1 ORDER BY timestamp DESC LIMIT ?2"
            }
            (false, false) => {
                "SELECT id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp \
                 FROM agent_events ORDER BY timestamp DESC LIMIT ?1"
            }
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = match (category, pane_id) {
            (Some(category), Some(pane_id)) => {
                stmt.query_map(params![category, pane_id, limit], map_agent_event_row)?
            }
            (Some(category), None) => {
                stmt.query_map(params![category, limit], map_agent_event_row)?
            }
            (None, Some(pane_id)) => {
                stmt.query_map(params![pane_id, limit], map_agent_event_row)?
            }
            (None, None) => stmt.query_map(params![limit], map_agent_event_row)?,
        };
        Ok(rows.filter_map(|row| row.ok()).collect())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_agent_task(&self, task: &AgentTask) -> Result<()> {
        let task = task.clone();
        self.conn.call(move |conn| {
        let transaction = conn.transaction()?;
        let notify_channels_json = serde_json::to_string(&task.notify_channels).call_err()?;
        let tool_whitelist_json = serialize_string_vec_json(&task.tool_whitelist)?;
        let tool_blacklist_json = serialize_string_vec_json(&task.tool_blacklist)?;
        let supervisor_config_json = serialize_supervisor_config_json(&task.supervisor_config)?;

        transaction.execute(
            "INSERT OR REPLACE INTO agent_tasks \
             (id, title, description, status, priority, progress, created_at, started_at, completed_at, error, result, thread_id, source, notify_on_complete, notify_channels_json, command, session_id, goal_run_id, goal_run_title, goal_step_id, goal_step_title, parent_task_id, parent_thread_id, runtime, retry_count, max_retries, next_retry_at, scheduled_at, blocked_reason, awaiting_approval_id, policy_fingerprint, approval_expires_at, containment_scope, compensation_status, compensation_summary, lane_id, last_error, override_provider, override_model, override_system_prompt, sub_agent_def_id, tool_whitelist_json, tool_blacklist_json, context_budget_tokens, context_overflow_action, termination_conditions, success_criteria, max_duration_secs, supervisor_config_json, deleted_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42, ?43, ?44, ?45, ?46, ?47, ?48, ?49, NULL)",
            params![
                &task.id,
                &task.title,
                &task.description,
                task_status_to_str(task.status),
                task_priority_to_str(task.priority),
                task.progress as i64,
                task.created_at as i64,
                task.started_at.map(|value| value as i64),
                task.completed_at.map(|value| value as i64),
                &task.error,
                &task.result,
                &task.thread_id,
                &task.source,
                if task.notify_on_complete { 1 } else { 0 },
                notify_channels_json,
                &task.command,
                &task.session_id,
                &task.goal_run_id,
                &task.goal_run_title,
                &task.goal_step_id,
                &task.goal_step_title,
                &task.parent_task_id,
                &task.parent_thread_id,
                &task.runtime,
                task.retry_count as i64,
                task.max_retries as i64,
                task.next_retry_at.map(|value| value as i64),
                task.scheduled_at.map(|value| value as i64),
                &task.blocked_reason,
                &task.awaiting_approval_id,
                &task.policy_fingerprint,
                task.approval_expires_at.map(|value| value as i64),
                &task.containment_scope,
                &task.compensation_status,
                &task.compensation_summary,
                &task.lane_id,
                &task.last_error,
                &task.override_provider,
                &task.override_model,
                &task.override_system_prompt,
                &task.sub_agent_def_id,
                tool_whitelist_json,
                tool_blacklist_json,
                task.context_budget_tokens.map(|value| value as i64),
                task.context_overflow_action.map(context_overflow_action_to_str),
                &task.termination_conditions,
                &task.success_criteria,
                task.max_duration_secs.map(|value| value as i64),
                supervisor_config_json,
            ],
        )?;

        transaction.execute(
            "UPDATE agent_task_dependencies SET deleted_at = ?2 WHERE task_id = ?1 AND deleted_at IS NULL",
            params![&task.id, now_ts() as i64],
        )?;
        for (ordinal, dependency) in task.dependencies.iter().enumerate() {
            transaction.execute(
                "INSERT OR REPLACE INTO agent_task_dependencies (task_id, depends_on_task_id, ordinal, deleted_at) VALUES (?1, ?2, ?3, NULL)",
                params![&task.id, dependency, ordinal as i64],
            )?;
        }

        transaction.execute(
            "UPDATE agent_task_logs SET deleted_at = ?2 WHERE task_id = ?1 AND deleted_at IS NULL",
            params![&task.id, now_ts() as i64],
        )?;
        for log in &task.logs {
            transaction.execute(
                "INSERT OR REPLACE INTO agent_task_logs (id, task_id, timestamp, level, phase, message, details, attempt, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)",
                params![
                    &log.id,
                    &task.id,
                    log.timestamp as i64,
                    task_log_level_to_str(log.level),
                    &log.phase,
                    &log.message,
                    &log.details,
                    log.attempt as i64,
                ],
            )?;
        }

        embedding_queue::enqueue_task_embedding_job(&transaction, &task, now_ts() as i64)?;

        transaction.commit()?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_agent_task(&self, task_id: &str) -> Result<()> {
        let task_id = task_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE agent_tasks SET deleted_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
                    params![task_id, now_ts() as i64],
                )?;
                embedding_queue::queue_embedding_deletion_on_connection(
                    conn,
                    "agent_task",
                    &task_id,
                    now_ts() as i64,
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_agent_tasks(&self) -> Result<Vec<AgentTask>> {
        self.list_agent_tasks_filtered(&AgentTaskListQuery::default())
            .await
    }

    pub(crate) async fn list_agent_tasks_filtered(
        &self,
        query: &AgentTaskListQuery,
    ) -> Result<Vec<AgentTask>> {
        let query = query.clone();
        self.read_conn.call(move |conn| {
        let mut task_sql = "SELECT id, title, description, status, priority, progress, created_at, started_at, completed_at, error, result, thread_id, source, notify_on_complete, notify_channels_json, command, session_id, goal_run_id, goal_run_title, goal_step_id, goal_step_title, parent_task_id, parent_thread_id, runtime, retry_count, max_retries, next_retry_at, scheduled_at, blocked_reason, awaiting_approval_id, policy_fingerprint, approval_expires_at, containment_scope, compensation_status, compensation_summary, lane_id, last_error, override_provider, override_model, override_system_prompt, sub_agent_def_id, tool_whitelist_json, tool_blacklist_json, context_budget_tokens, context_overflow_action, termination_conditions, success_criteria, max_duration_secs, supervisor_config_json \
             FROM agent_tasks WHERE deleted_at IS NULL".to_string();
        let mut task_values = Vec::<rusqlite::types::Value>::new();
        if let Some(id) = query.id.as_deref().filter(|value| !value.is_empty()) {
            task_sql.push_str(" AND id = ?");
            task_values.push(rusqlite::types::Value::Text(id.to_string()));
        }
        if let Some(status) = query.status.as_deref().filter(|value| !value.is_empty()) {
            task_sql.push_str(" AND status = ?");
            task_values.push(rusqlite::types::Value::Text(status.to_string()));
        }
        let statuses = query
            .statuses
            .iter()
            .map(|status| status.trim())
            .filter(|status| !status.is_empty())
            .collect::<Vec<_>>();
        if !statuses.is_empty() {
            let placeholders = std::iter::repeat("?")
                .take(statuses.len())
                .collect::<Vec<_>>()
                .join(", ");
            task_sql.push_str(&format!(" AND status IN ({placeholders})"));
            for status in statuses {
                task_values.push(rusqlite::types::Value::Text(status.to_string()));
            }
        }
        if let Some(source) = query.source.as_deref().filter(|value| !value.is_empty()) {
            task_sql.push_str(" AND source = ?");
            task_values.push(rusqlite::types::Value::Text(source.to_string()));
        }
        if let Some(thread_id) = query.thread_id.as_deref().filter(|value| !value.is_empty()) {
            task_sql.push_str(" AND thread_id = ?");
            task_values.push(rusqlite::types::Value::Text(thread_id.to_string()));
        }
        let thread_ids = query
            .thread_ids
            .iter()
            .map(|thread_id| thread_id.trim())
            .filter(|thread_id| !thread_id.is_empty())
            .collect::<Vec<_>>();
        if !thread_ids.is_empty() {
            let placeholders = std::iter::repeat("?")
                .take(thread_ids.len())
                .collect::<Vec<_>>()
                .join(", ");
            task_sql.push_str(&format!(" AND thread_id IN ({placeholders})"));
            for thread_id in thread_ids {
                task_values.push(rusqlite::types::Value::Text(thread_id.to_string()));
            }
        }
        if let Some(goal_run_id) = query
            .goal_run_id
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            task_sql.push_str(" AND goal_run_id = ?");
            task_values.push(rusqlite::types::Value::Text(goal_run_id.to_string()));
        }
        if let Some(parent_task_id) = query
            .parent_task_id
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            task_sql.push_str(" AND parent_task_id = ?");
            task_values.push(rusqlite::types::Value::Text(parent_task_id.to_string()));
        }
        if let Some(approval_id) = query
            .awaiting_approval_id
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            task_sql.push_str(" AND awaiting_approval_id = ?");
            task_values.push(rusqlite::types::Value::Text(approval_id.to_string()));
        }
        if query.supervisor_config_present {
            task_sql.push_str(
                " AND supervisor_config_json IS NOT NULL AND TRIM(supervisor_config_json) <> ''",
            );
        }
        if query.exclude_terminal_statuses {
            task_sql.push_str(
                " AND status NOT IN ('completed', 'budget_exceeded', 'failed', 'cancelled')",
            );
        }
        if query.order_by_recent_activity_desc {
            task_sql.push_str(
                " ORDER BY COALESCE(completed_at, started_at, created_at) DESC, created_at DESC",
            );
        } else {
            task_sql.push_str(
                " ORDER BY CASE status \
                 WHEN 'in_progress' THEN 0 \
                 WHEN 'awaiting_approval' THEN 1 \
                 WHEN 'failed_analyzing' THEN 2 \
                 WHEN 'blocked' THEN 3 \
                 WHEN 'queued' THEN 4 \
                 WHEN 'failed' THEN 5 \
                 WHEN 'completed' THEN 6 \
                 ELSE 7 END, \
                 CASE priority \
                 WHEN 'urgent' THEN 0 \
                 WHEN 'high' THEN 1 \
                 WHEN 'normal' THEN 2 \
                 ELSE 3 END, \
                 created_at DESC",
            );
        }
        if let Some(limit) = query.limit {
            task_sql.push_str(" LIMIT ?");
            task_values.push(rusqlite::types::Value::Integer(limit.max(1) as i64));
        }

        let mut stmt = conn.prepare(&task_sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(task_values.iter()), |row| {
            let task_id: String = row.get(0)?;
            let notify_channels_json: String = row.get(14)?;
            Ok(AgentTask {
                id: task_id,
                title: row.get(1)?,
                description: row.get(2)?,
                status: parse_task_status(&row.get::<_, String>(3)?),
                priority: parse_task_priority(&row.get::<_, String>(4)?),
                progress: row.get::<_, i64>(5)? as u8,
                created_at: row.get::<_, i64>(6)? as u64,
                started_at: row.get::<_, Option<i64>>(7)?.map(|value| value as u64),
                completed_at: row.get::<_, Option<i64>>(8)?.map(|value| value as u64),
                error: row.get(9)?,
                result: row.get(10)?,
                thread_id: row.get(11)?,
                source: row.get(12)?,
                notify_on_complete: row.get::<_, i64>(13)? != 0,
                notify_channels: serde_json::from_str(&notify_channels_json).unwrap_or_default(),
                dependencies: Vec::new(),
                command: row.get(15)?,
                session_id: row.get(16)?,
                goal_run_id: row.get(17)?,
                goal_run_title: row.get(18)?,
                goal_step_id: row.get(19)?,
                goal_step_title: row.get(20)?,
                parent_task_id: row.get(21)?,
                parent_thread_id: row.get(22)?,
                runtime: row
                    .get::<_, Option<String>>(23)?
                    .unwrap_or_else(|| "daemon".to_string()),
                retry_count: row.get::<_, i64>(24)? as u32,
                max_retries: row.get::<_, i64>(25)? as u32,
                next_retry_at: row.get::<_, Option<i64>>(26)?.map(|value| value as u64),
                scheduled_at: row.get::<_, Option<i64>>(27)?.map(|value| value as u64),
                blocked_reason: row.get(28)?,
                awaiting_approval_id: row.get(29)?,
                policy_fingerprint: row.get(30)?,
                approval_expires_at: row.get::<_, Option<i64>>(31)?.map(|value| value as u64),
                containment_scope: row.get(32)?,
                compensation_status: row.get(33)?,
                compensation_summary: row.get(34)?,
                lane_id: row.get(35)?,
                last_error: row.get(36)?,
                logs: Vec::new(),
                override_provider: row.get(37)?,
                override_model: row.get(38)?,
                override_system_prompt: row.get(39)?,
                sub_agent_def_id: row.get(40)?,
                tool_whitelist: parse_string_vec_json(row.get(41)?),
                tool_blacklist: parse_string_vec_json(row.get(42)?),
                context_budget_tokens: row.get::<_, Option<i64>>(43)?.map(|value| value as u32),
                context_overflow_action: row
                    .get::<_, Option<String>>(44)?
                    .map(|value| parse_context_overflow_action(&value)),
                termination_conditions: row.get(45)?,
                success_criteria: row.get(46)?,
                max_duration_secs: row.get::<_, Option<i64>>(47)?.map(|value| value as u64),
                supervisor_config: parse_supervisor_config_json(row.get(48)?),
            })
        })?;

        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(row?);
        }
        if tasks.is_empty() {
            return Ok(tasks);
        }

        let task_ids = tasks
            .iter()
            .map(|task| task.id.clone())
            .collect::<Vec<_>>();
        let id_placeholders = std::iter::repeat("?")
            .take(task_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let id_values = task_ids
            .iter()
            .cloned()
            .map(rusqlite::types::Value::Text)
            .collect::<Vec<_>>();

        let mut dependency_stmt = conn.prepare(&format!(
            "SELECT task_id, depends_on_task_id \
             FROM agent_task_dependencies \
             WHERE deleted_at IS NULL AND task_id IN ({id_placeholders}) \
             ORDER BY task_id ASC, ordinal ASC"
        ))?;
        let dependency_rows = dependency_stmt.query_map(
            rusqlite::params_from_iter(id_values.iter()),
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?;
        let mut dependency_map = std::collections::HashMap::<String, Vec<String>>::new();
        for row in dependency_rows {
            let (task_id, dependency) = row?;
            dependency_map.entry(task_id).or_default().push(dependency);
        }

        let mut log_stmt = conn.prepare(&format!(
            "SELECT id, task_id, timestamp, level, phase, message, details, attempt \
             FROM agent_task_logs \
             WHERE deleted_at IS NULL AND task_id IN ({id_placeholders}) \
             ORDER BY task_id ASC, timestamp ASC"
        ))?;
        let log_rows = log_stmt.query_map(rusqlite::params_from_iter(id_values.iter()), |row| {
            Ok((
                row.get::<_, String>(1)?,
                AgentTaskLogEntry {
                    id: row.get(0)?,
                    timestamp: row.get::<_, i64>(2)? as u64,
                    level: parse_task_log_level(&row.get::<_, String>(3)?),
                    phase: row.get(4)?,
                    message: row.get(5)?,
                    details: row.get(6)?,
                    attempt: row.get::<_, i64>(7)? as u32,
                },
            ))
        })?;
        let mut log_map = std::collections::HashMap::<String, Vec<AgentTaskLogEntry>>::new();
        for row in log_rows {
            let (task_id, log) = row?;
            log_map.entry(task_id).or_default().push(log);
        }

        for task in &mut tasks {
            task.dependencies = dependency_map.remove(&task.id).unwrap_or_default();
            task.logs = log_map.remove(&task.id).unwrap_or_default();
        }
        Ok(tasks)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn latest_agent_task_session_for_thread(
        &self,
        thread_id: &str,
    ) -> Result<Option<String>> {
        let thread_id = thread_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT session_id
                     FROM agent_tasks
                     WHERE deleted_at IS NULL
                       AND thread_id = ?1
                       AND session_id IS NOT NULL
                       AND TRIM(session_id) <> ''
                     ORDER BY created_at DESC, id DESC
                     LIMIT 1",
                    params![thread_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn has_agent_task_pending_approval(&self) -> Result<bool> {
        self.read_conn
            .call(move |conn| {
                let count: i64 = conn.query_row(
                    "SELECT COUNT(1)
                     FROM agent_tasks
                     WHERE deleted_at IS NULL
                       AND awaiting_approval_id IS NOT NULL
                       AND TRIM(awaiting_approval_id) <> ''",
                    [],
                    |row| row.get(0),
                )?;
                Ok(count > 0)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn has_awaiting_approval_task_for_thread(
        &self,
        thread_id: &str,
    ) -> Result<bool> {
        let thread_id = thread_id.to_string();
        self.read_conn
            .call(move |conn| {
                let count: i64 = conn.query_row(
                    "SELECT COUNT(1)
                     FROM agent_tasks
                     WHERE deleted_at IS NULL
                       AND status = 'awaiting_approval'
                       AND (thread_id = ?1 OR parent_thread_id = ?1)",
                    params![thread_id],
                    |row| row.get(0),
                )?;
                Ok(count > 0)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn pending_agent_task_approval_command(
        &self,
        approval_id: &str,
    ) -> Result<Option<String>> {
        const APPROVAL_REASON_PREFIX: &str = "waiting for operator approval: ";

        let approval_id = approval_id.to_string();
        self.read_conn
            .call(move |conn| {
                let row = conn
                    .query_row(
                        "SELECT command, blocked_reason
                         FROM agent_tasks
                         WHERE deleted_at IS NULL
                           AND awaiting_approval_id = ?1
                         ORDER BY created_at DESC
                         LIMIT 1",
                        params![approval_id],
                        |row| {
                            Ok((
                                row.get::<_, Option<String>>(0)?,
                                row.get::<_, Option<String>>(1)?,
                            ))
                        },
                    )
                    .optional()?;
                let Some((command, blocked_reason)) = row else {
                    return Ok(None);
                };
                if let Some(command) = command.map(|value| value.trim().to_string()) {
                    if !command.is_empty() {
                        return Ok(Some(command));
                    }
                }
                Ok(blocked_reason
                    .as_deref()
                    .and_then(|reason| reason.strip_prefix(APPROVAL_REASON_PREFIX))
                    .map(str::trim)
                    .filter(|command| !command.is_empty())
                    .map(ToOwned::to_owned))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
