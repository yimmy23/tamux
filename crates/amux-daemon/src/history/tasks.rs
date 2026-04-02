use super::*;

impl HistoryStore {
    pub async fn upsert_notification(
        &self,
        notification: &amux_protocol::InboxNotification,
    ) -> Result<()> {
        let row = crate::notifications::notification_event_row(notification)?;
        self.upsert_agent_event(&row).await
    }

    pub async fn list_notifications(
        &self,
        include_inactive: bool,
        limit: Option<usize>,
    ) -> Result<Vec<amux_protocol::InboxNotification>> {
        let rows = self
            .list_agent_events(
                Some(crate::notifications::NOTIFICATION_CATEGORY),
                None,
                limit,
            )
            .await?;
        let mut notifications: Vec<amux_protocol::InboxNotification> = rows
            .iter()
            .filter_map(crate::notifications::parse_notification_row)
            .filter(|notification| {
                include_inactive
                    || (notification.archived_at.is_none() && notification.deleted_at.is_none())
            })
            .collect();
        notifications.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| right.created_at.cmp(&left.created_at))
        });
        Ok(notifications)
    }

    pub async fn upsert_agent_event(&self, entry: &AgentEventRow) -> Result<()> {
        let entry = entry.clone();
        self.conn.call(move |conn| {
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
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
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

        transaction.execute(
            "INSERT OR REPLACE INTO agent_tasks \
             (id, title, description, status, priority, progress, created_at, started_at, completed_at, error, result, thread_id, source, notify_on_complete, notify_channels_json, command, session_id, goal_run_id, goal_run_title, goal_step_id, goal_step_title, parent_task_id, parent_thread_id, runtime, retry_count, max_retries, next_retry_at, scheduled_at, blocked_reason, awaiting_approval_id, lane_id, last_error, override_provider, override_model, override_system_prompt, sub_agent_def_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36)",
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
                &task.lane_id,
                &task.last_error,
                &task.override_provider,
                &task.override_model,
                &task.override_system_prompt,
                &task.sub_agent_def_id,
            ],
        )?;

        transaction.execute(
            "DELETE FROM agent_task_dependencies WHERE task_id = ?1",
            params![&task.id],
        )?;
        for (ordinal, dependency) in task.dependencies.iter().enumerate() {
            transaction.execute(
                "INSERT INTO agent_task_dependencies (task_id, depends_on_task_id, ordinal) VALUES (?1, ?2, ?3)",
                params![&task.id, dependency, ordinal as i64],
            )?;
        }

        transaction.execute(
            "DELETE FROM agent_task_logs WHERE task_id = ?1",
            params![&task.id],
        )?;
        for log in &task.logs {
            transaction.execute(
                "INSERT OR REPLACE INTO agent_task_logs (id, task_id, timestamp, level, phase, message, details, attempt) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
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

        transaction.commit()?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_agent_task(&self, task_id: &str) -> Result<()> {
        let task_id = task_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute("DELETE FROM agent_tasks WHERE id = ?1", params![task_id])?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_agent_tasks(&self) -> Result<Vec<AgentTask>> {
        self.conn.call(move |conn| {
        let mut dependency_stmt = conn.prepare(
            "SELECT task_id, depends_on_task_id FROM agent_task_dependencies ORDER BY ordinal ASC",
        )?;
        let dependency_rows = dependency_stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut dependency_map = std::collections::HashMap::<String, Vec<String>>::new();
        for row in dependency_rows {
            let (task_id, dependency) = row?;
            dependency_map.entry(task_id).or_default().push(dependency);
        }

        let mut log_stmt = conn.prepare(
            "SELECT id, task_id, timestamp, level, phase, message, details, attempt FROM agent_task_logs ORDER BY timestamp ASC",
        )?;
        let log_rows = log_stmt.query_map([], |row| {
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

        let mut stmt = conn.prepare(
            "SELECT id, title, description, status, priority, progress, created_at, started_at, completed_at, error, result, thread_id, source, notify_on_complete, notify_channels_json, command, session_id, goal_run_id, goal_run_title, goal_step_id, goal_step_title, parent_task_id, parent_thread_id, runtime, retry_count, max_retries, next_retry_at, scheduled_at, blocked_reason, awaiting_approval_id, lane_id, last_error, override_provider, override_model, override_system_prompt, sub_agent_def_id \
             FROM agent_tasks \
             ORDER BY CASE status \
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
        )?;
        let rows = stmt.query_map([], |row| {
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
                lane_id: row.get(30)?,
                last_error: row.get(31)?,
                logs: Vec::new(),
                tool_whitelist: None,
                tool_blacklist: None,
                context_budget_tokens: None,
                context_overflow_action: None,
                termination_conditions: None,
                success_criteria: None,
                max_duration_secs: None,
                supervisor_config: None,
                override_provider: row.get(32)?,
                override_model: row.get(33)?,
                override_system_prompt: row.get(34)?,
                sub_agent_def_id: row.get(35)?,
            })
        })?;

        let mut tasks = Vec::new();
        for row in rows {
            let mut task = row?;
            task.dependencies = dependency_map.remove(&task.id).unwrap_or_default();
            task.logs = log_map.remove(&task.id).unwrap_or_default();
            tasks.push(task);
        }
        Ok(tasks)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }
}
