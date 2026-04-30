use super::*;

fn map_routine_definition_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RoutineDefinitionRow> {
    Ok(RoutineDefinitionRow {
        id: row.get(0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        enabled: row.get::<_, i64>(3)? != 0,
        paused_at: row
            .get::<_, Option<i64>>(4)?
            .map(|value| value.max(0) as u64),
        schedule_expression: row.get(5)?,
        target_kind: row.get(6)?,
        target_payload_json: row.get(7)?,
        schema_version: row.get::<_, i64>(8)?.max(1) as u32,
        next_run_at: row
            .get::<_, Option<i64>>(9)?
            .map(|value| value.max(0) as u64),
        last_run_at: row
            .get::<_, Option<i64>>(10)?
            .map(|value| value.max(0) as u64),
        last_result: row.get(11)?,
        last_error: row.get(12)?,
        last_success_summary: row.get(13)?,
        created_at: row.get::<_, i64>(14)?.max(0) as u64,
        updated_at: row.get::<_, i64>(15)?.max(0) as u64,
    })
}

fn map_routine_run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RoutineRunRow> {
    Ok(RoutineRunRow {
        id: row.get(0)?,
        routine_id: row.get(1)?,
        trigger_kind: row.get(2)?,
        status: row.get(3)?,
        started_at: row.get::<_, i64>(4)?.max(0) as u64,
        finished_at: row
            .get::<_, Option<i64>>(5)?
            .map(|value| value.max(0) as u64),
        created_task_id: row.get(6)?,
        created_goal_run_id: row.get(7)?,
        payload_json: row.get(8)?,
        result_summary: row.get(9)?,
        error: row.get(10)?,
        rerun_of_run_id: row.get(11)?,
    })
}

impl HistoryStore {
    pub async fn upsert_routine_definition(&self, row: &RoutineDefinitionRow) -> Result<()> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO routine_definitions (id, title, description, enabled, paused_at, schedule_expression, target_kind, target_payload_json, schema_version, next_run_at, last_run_at, last_result, last_error, last_success_summary, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, NULL)",
                    params![
                        row.id,
                        row.title,
                        row.description,
                        if row.enabled { 1i64 } else { 0i64 },
                        row.paused_at.map(|value| value as i64),
                        row.schedule_expression,
                        row.target_kind,
                        row.target_payload_json,
                        i64::from(row.schema_version),
                        row.next_run_at.map(|value| value as i64),
                        row.last_run_at.map(|value| value as i64),
                        row.last_result,
                        row.last_error,
                        row.last_success_summary,
                        row.created_at as i64,
                        row.updated_at as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_routine_definitions(&self) -> Result<Vec<RoutineDefinitionRow>> {
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, title, description, enabled, paused_at, schedule_expression, target_kind, target_payload_json, schema_version, next_run_at, last_run_at, last_result, last_error, last_success_summary, created_at, updated_at FROM routine_definitions WHERE deleted_at IS NULL ORDER BY updated_at DESC, id ASC",
                )?;
                let rows = stmt.query_map([], map_routine_definition_row)?;
                Ok(rows.filter_map(|row| row.ok()).collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_due_routine_definitions(
        &self,
        now_ms: u64,
    ) -> Result<Vec<RoutineDefinitionRow>> {
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, title, description, enabled, paused_at, schedule_expression, target_kind, target_payload_json, schema_version, next_run_at, last_run_at, last_result, last_error, last_success_summary, created_at, updated_at FROM routine_definitions WHERE enabled = 1 AND paused_at IS NULL AND next_run_at IS NOT NULL AND next_run_at <= ?1 AND deleted_at IS NULL ORDER BY next_run_at ASC, updated_at DESC, id ASC",
                )?;
                let rows = stmt.query_map(params![now_ms as i64], map_routine_definition_row)?;
                Ok(rows.filter_map(|row| row.ok()).collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_routine_definition(&self, id: &str) -> Result<Option<RoutineDefinitionRow>> {
        let id = id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT id, title, description, enabled, paused_at, schedule_expression, target_kind, target_payload_json, schema_version, next_run_at, last_run_at, last_result, last_error, last_success_summary, created_at, updated_at FROM routine_definitions WHERE id = ?1 AND deleted_at IS NULL",
                    params![id],
                    map_routine_definition_row,
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_routine_definition(&self, id: &str) -> Result<bool> {
        let id = id.to_string();
        self.conn
            .call(move |conn| {
                let deleted = conn.execute(
                    "UPDATE routine_definitions SET deleted_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
                    params![id, now_ts() as i64],
                )?;
                Ok(deleted > 0)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn append_routine_run(&self, row: &RoutineRunRow) -> Result<()> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO routine_runs (id, routine_id, trigger_kind, status, started_at, finished_at, created_task_id, created_goal_run_id, payload_json, result_summary, error, rerun_of_run_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        row.id,
                        row.routine_id,
                        row.trigger_kind,
                        row.status,
                        row.started_at as i64,
                        row.finished_at.map(|value| value as i64),
                        row.created_task_id,
                        row.created_goal_run_id,
                        row.payload_json,
                        row.result_summary,
                        row.error,
                        row.rerun_of_run_id,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_routine_runs(
        &self,
        routine_id: &str,
        limit: usize,
    ) -> Result<Vec<RoutineRunRow>> {
        let routine_id = routine_id.to_string();
        let limit = limit.max(1) as i64;
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, routine_id, trigger_kind, status, started_at, finished_at, created_task_id, created_goal_run_id, payload_json, result_summary, error, rerun_of_run_id FROM routine_runs WHERE routine_id = ?1 ORDER BY started_at DESC, id DESC LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![routine_id, limit], map_routine_run_row)?;
                Ok(rows.filter_map(|row| row.ok()).collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_routine_run(&self, run_id: &str) -> Result<Option<RoutineRunRow>> {
        let run_id = run_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT id, routine_id, trigger_kind, status, started_at, finished_at, created_task_id, created_goal_run_id, payload_json, result_summary, error, rerun_of_run_id FROM routine_runs WHERE id = ?1",
                    params![run_id],
                    map_routine_run_row,
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
