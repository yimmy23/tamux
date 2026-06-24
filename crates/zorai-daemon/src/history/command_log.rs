use super::*;

impl HistoryStore {
    pub async fn append_command_log(&self, entry: &CommandLogEntry) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO command_log \
                 (id, command, timestamp, path, cwd, workspace_id, surface_id, pane_id, exit_code, duration_ms, deleted_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL)",
                db::db_params![
                    entry.id.clone(),
                    entry.command.clone(),
                    entry.timestamp,
                    entry.path.clone(),
                    entry.cwd.clone(),
                    entry.workspace_id.clone(),
                    entry.surface_id.clone(),
                    entry.pane_id.clone(),
                    entry.exit_code,
                    entry.duration_ms,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn complete_command_log(
        &self,
        id: &str,
        exit_code: Option<i32>,
        duration_ms: Option<i64>,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "UPDATE command_log SET exit_code = ?2, duration_ms = ?3 WHERE id = ?1 AND deleted_at IS NULL",
                db::db_params![id, exit_code, duration_ms],
            )
            .await?;
        Ok(())
    }

    pub async fn query_command_log(
        &self,
        workspace_id: Option<&str>,
        pane_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<CommandLogEntry>> {
        let limit = limit.unwrap_or(200).max(1) as i64;

        let (sql, params) = match (workspace_id, pane_id) {
            (Some(workspace_id), Some(pane_id)) => (
                "SELECT id, command, timestamp, path, cwd, workspace_id, surface_id, pane_id, exit_code, duration_ms \
                 FROM command_log WHERE workspace_id = ?1 AND pane_id = ?2 AND deleted_at IS NULL \
                 ORDER BY timestamp DESC LIMIT ?3",
                db::db_params![workspace_id, pane_id, limit],
            ),
            (Some(workspace_id), None) => (
                "SELECT id, command, timestamp, path, cwd, workspace_id, surface_id, pane_id, exit_code, duration_ms \
                 FROM command_log WHERE workspace_id = ?1 AND deleted_at IS NULL \
                 ORDER BY timestamp DESC LIMIT ?2",
                db::db_params![workspace_id, limit],
            ),
            (None, Some(pane_id)) => (
                "SELECT id, command, timestamp, path, cwd, workspace_id, surface_id, pane_id, exit_code, duration_ms \
                 FROM command_log WHERE pane_id = ?1 AND deleted_at IS NULL \
                 ORDER BY timestamp DESC LIMIT ?2",
                db::db_params![pane_id, limit],
            ),
            (None, None) => (
                "SELECT id, command, timestamp, path, cwd, workspace_id, surface_id, pane_id, exit_code, duration_ms \
                 FROM command_log WHERE deleted_at IS NULL ORDER BY timestamp DESC LIMIT ?1",
                db::db_params![limit],
            ),
        };

        let rows = self.conn_db.query(sql, params).await?;
        Ok(rows
            .iter()
            .filter_map(|row| map_command_log_entry_row(row).ok())
            .collect())
    }

    pub async fn clear_command_log(&self) -> Result<()> {
        self.conn_db
            .execute(
                "UPDATE command_log SET deleted_at = ?1 WHERE deleted_at IS NULL",
                db::db_params![now_ts() as i64],
            )
            .await?;
        Ok(())
    }
}
