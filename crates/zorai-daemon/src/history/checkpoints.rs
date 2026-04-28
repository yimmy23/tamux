use super::*;

impl HistoryStore {
    /// Insert or replace a checkpoint row in the `agent_checkpoints` table.
    pub async fn upsert_checkpoint(
        &self,
        id: &str,
        goal_run_id: &str,
        thread_id: Option<&str>,
        task_id: Option<&str>,
        checkpoint_type: CheckpointType,
        state_json: &str,
        context_summary: Option<&str>,
        created_at: u64,
    ) -> Result<()> {
        let id = id.to_string();
        let goal_run_id = goal_run_id.to_string();
        let thread_id = thread_id.map(str::to_string);
        let task_id = task_id.map(str::to_string);
        let state_json = state_json.to_string();
        let context_summary = context_summary.map(str::to_string);
        self.conn.call(move |conn| {
        let type_str = match checkpoint_type {
            CheckpointType::PreStep => "pre_step",
            CheckpointType::PostStep => "post_step",
            CheckpointType::Manual => "manual",
            CheckpointType::PreRecovery => "pre_recovery",
            CheckpointType::Periodic => "periodic",
        };
        conn.execute(
            "INSERT OR REPLACE INTO agent_checkpoints \
             (id, goal_run_id, thread_id, task_id, checkpoint_type, state_json, context_summary, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                goal_run_id,
                thread_id,
                task_id,
                type_str,
                state_json,
                context_summary,
                created_at as i64,
            ],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Load all checkpoint JSON blobs for a given goal run, ordered by
    /// `created_at` descending.
    pub async fn list_checkpoints_for_goal_run(&self, goal_run_id: &str) -> Result<Vec<String>> {
        let goal_run_id = goal_run_id.to_string();
        self.conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT state_json FROM agent_checkpoints WHERE goal_run_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt
            .query_map(params![goal_run_id], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Load a single checkpoint by ID.
    pub async fn get_checkpoint(&self, id: &str) -> Result<Option<String>> {
        let id = id.to_string();
        self.conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT state_json FROM agent_checkpoints WHERE id = ?1",
                    params![id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Delete checkpoints by their IDs.
    pub async fn delete_checkpoints(&self, ids: &[&str]) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
        self.conn
            .call(move |conn| {
                let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
                let sql = format!(
                    "DELETE FROM agent_checkpoints WHERE id IN ({})",
                    placeholders.join(", ")
                );
                let params: Vec<&dyn rusqlite::types::ToSql> = ids
                    .iter()
                    .map(|id| id as &dyn rusqlite::types::ToSql)
                    .collect();
                let deleted = conn.execute(&sql, params.as_slice())?;
                Ok(deleted)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_checkpoints_for_goal_run(&self, goal_run_id: &str) -> Result<usize> {
        let goal_run_id = goal_run_id.to_string();
        self.conn
            .call(move |conn| {
                let deleted = conn.execute(
                    "DELETE FROM agent_checkpoints WHERE goal_run_id = ?1",
                    params![goal_run_id],
                )?;
                Ok(deleted)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    // -- Health log --

    pub async fn insert_health_log(
        &self,
        id: &str,
        entity_type: &str,
        entity_id: &str,
        health_state: &str,
        indicators_json: Option<&str>,
        intervention: Option<&str>,
        created_at: u64,
    ) -> Result<()> {
        let id = id.to_string();
        let entity_type = entity_type.to_string();
        let entity_id = entity_id.to_string();
        let health_state = health_state.to_string();
        let indicators_json = indicators_json.map(str::to_string);
        let intervention = intervention.map(str::to_string);
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT INTO agent_health_log (id, entity_type, entity_id, health_state, indicators_json, intervention, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, entity_type, entity_id, health_state, indicators_json, intervention, created_at as i64],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_health_log(
        &self,
        limit: u32,
    ) -> Result<
        Vec<(
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            u64,
        )>,
    > {
        self.conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, entity_type, entity_id, health_state, indicators_json, intervention, created_at FROM agent_health_log ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, i64>(6)? as u64,
            ))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }
}
