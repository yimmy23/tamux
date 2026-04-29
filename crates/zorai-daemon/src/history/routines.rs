use super::*;

impl HistoryStore {
    pub async fn upsert_routine_definition(&self, row: &RoutineDefinitionRow) -> Result<()> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO routine_definitions (id, title, description, enabled, paused_at, schedule_expression, target_kind, target_payload_json, next_run_at, last_run_at, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, NULL)",
                    params![
                        row.id,
                        row.title,
                        row.description,
                        if row.enabled { 1i64 } else { 0i64 },
                        row.paused_at.map(|value| value as i64),
                        row.schedule_expression,
                        row.target_kind,
                        row.target_payload_json,
                        row.next_run_at.map(|value| value as i64),
                        row.last_run_at.map(|value| value as i64),
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
                    "SELECT id, title, description, enabled, paused_at, schedule_expression, target_kind, target_payload_json, next_run_at, last_run_at, created_at, updated_at FROM routine_definitions WHERE deleted_at IS NULL ORDER BY updated_at DESC, id ASC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(RoutineDefinitionRow {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        description: row.get(2)?,
                        enabled: row.get::<_, i64>(3)? != 0,
                        paused_at: row.get::<_, Option<i64>>(4)?.map(|value| value.max(0) as u64),
                        schedule_expression: row.get(5)?,
                        target_kind: row.get(6)?,
                        target_payload_json: row.get(7)?,
                        next_run_at: row.get::<_, Option<i64>>(8)?.map(|value| value.max(0) as u64),
                        last_run_at: row.get::<_, Option<i64>>(9)?.map(|value| value.max(0) as u64),
                        created_at: row.get::<_, i64>(10)?.max(0) as u64,
                        updated_at: row.get::<_, i64>(11)?.max(0) as u64,
                    })
                })?;
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
                    "SELECT id, title, description, enabled, paused_at, schedule_expression, target_kind, target_payload_json, next_run_at, last_run_at, created_at, updated_at FROM routine_definitions WHERE enabled = 1 AND paused_at IS NULL AND next_run_at IS NOT NULL AND next_run_at <= ?1 AND deleted_at IS NULL ORDER BY next_run_at ASC, updated_at DESC, id ASC",
                )?;
                let rows = stmt.query_map(params![now_ms as i64], |row| {
                    Ok(RoutineDefinitionRow {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        description: row.get(2)?,
                        enabled: row.get::<_, i64>(3)? != 0,
                        paused_at: row.get::<_, Option<i64>>(4)?.map(|value| value.max(0) as u64),
                        schedule_expression: row.get(5)?,
                        target_kind: row.get(6)?,
                        target_payload_json: row.get(7)?,
                        next_run_at: row.get::<_, Option<i64>>(8)?.map(|value| value.max(0) as u64),
                        last_run_at: row.get::<_, Option<i64>>(9)?.map(|value| value.max(0) as u64),
                        created_at: row.get::<_, i64>(10)?.max(0) as u64,
                        updated_at: row.get::<_, i64>(11)?.max(0) as u64,
                    })
                })?;
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
                    "SELECT id, title, description, enabled, paused_at, schedule_expression, target_kind, target_payload_json, next_run_at, last_run_at, created_at, updated_at FROM routine_definitions WHERE id = ?1 AND deleted_at IS NULL",
                    params![id],
                    |row| {
                        Ok(RoutineDefinitionRow {
                            id: row.get(0)?,
                            title: row.get(1)?,
                            description: row.get(2)?,
                            enabled: row.get::<_, i64>(3)? != 0,
                            paused_at: row.get::<_, Option<i64>>(4)?.map(|value| value.max(0) as u64),
                            schedule_expression: row.get(5)?,
                            target_kind: row.get(6)?,
                            target_payload_json: row.get(7)?,
                            next_run_at: row.get::<_, Option<i64>>(8)?.map(|value| value.max(0) as u64),
                            last_run_at: row.get::<_, Option<i64>>(9)?.map(|value| value.max(0) as u64),
                            created_at: row.get::<_, i64>(10)?.max(0) as u64,
                            updated_at: row.get::<_, i64>(11)?.max(0) as u64,
                        })
                    },
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
}
