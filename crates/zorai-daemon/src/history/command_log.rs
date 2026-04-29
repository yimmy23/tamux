use super::*;

impl HistoryStore {
    pub async fn append_command_log(&self, entry: &CommandLogEntry) -> Result<()> {
        let entry = entry.clone();
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO command_log \
             (id, command, timestamp, path, cwd, workspace_id, surface_id, pane_id, exit_code, duration_ms, deleted_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL)",
            params![
                entry.id,
                entry.command,
                entry.timestamp,
                entry.path,
                entry.cwd,
                entry.workspace_id,
                entry.surface_id,
                entry.pane_id,
                entry.exit_code,
                entry.duration_ms,
            ],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn complete_command_log(
        &self,
        id: &str,
        exit_code: Option<i32>,
        duration_ms: Option<i64>,
    ) -> Result<()> {
        let id = id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE command_log SET exit_code = ?2, duration_ms = ?3 WHERE id = ?1 AND deleted_at IS NULL",
                    params![id, exit_code, duration_ms],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn query_command_log(
        &self,
        workspace_id: Option<&str>,
        pane_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<CommandLogEntry>> {
        let workspace_id = workspace_id.map(str::to_string);
        let pane_id = pane_id.map(str::to_string);
        self.conn.call(move |conn| {
        let limit = limit.unwrap_or(200).max(1) as i64;

        let sql = match (workspace_id.is_some(), pane_id.is_some()) {
            (true, true) => {
                "SELECT id, command, timestamp, path, cwd, workspace_id, surface_id, pane_id, exit_code, duration_ms \
                 FROM command_log WHERE workspace_id = ?1 AND pane_id = ?2 AND deleted_at IS NULL \
                 ORDER BY timestamp DESC LIMIT ?3"
            }
            (true, false) => {
                "SELECT id, command, timestamp, path, cwd, workspace_id, surface_id, pane_id, exit_code, duration_ms \
                 FROM command_log WHERE workspace_id = ?1 AND deleted_at IS NULL \
                 ORDER BY timestamp DESC LIMIT ?2"
            }
            (false, true) => {
                "SELECT id, command, timestamp, path, cwd, workspace_id, surface_id, pane_id, exit_code, duration_ms \
                 FROM command_log WHERE pane_id = ?1 AND deleted_at IS NULL \
                 ORDER BY timestamp DESC LIMIT ?2"
            }
            (false, false) => {
                "SELECT id, command, timestamp, path, cwd, workspace_id, surface_id, pane_id, exit_code, duration_ms \
                 FROM command_log WHERE deleted_at IS NULL ORDER BY timestamp DESC LIMIT ?1"
            }
        };

        let mut stmt = conn.prepare(sql)?;
        let rows = match (workspace_id, pane_id) {
            (Some(workspace_id), Some(pane_id)) => {
                stmt.query_map(params![workspace_id, pane_id, limit], map_command_log_entry)?
            }
            (Some(workspace_id), None) => {
                stmt.query_map(params![workspace_id, limit], map_command_log_entry)?
            }
            (None, Some(pane_id)) => {
                stmt.query_map(params![pane_id, limit], map_command_log_entry)?
            }
            (None, None) => stmt.query_map(params![limit], map_command_log_entry)?,
        };

        Ok(rows.filter_map(|row| row.ok()).collect())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn clear_command_log(&self) -> Result<()> {
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE command_log SET deleted_at = ?1 WHERE deleted_at IS NULL",
                    params![now_ts() as i64],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
