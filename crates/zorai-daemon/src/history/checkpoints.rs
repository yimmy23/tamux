use super::*;

type HealthLogTuple = (
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    u64,
);

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
        let type_str = match checkpoint_type {
            CheckpointType::PreStep => "pre_step",
            CheckpointType::PostStep => "post_step",
            CheckpointType::Manual => "manual",
            CheckpointType::PreRecovery => "pre_recovery",
            CheckpointType::Periodic => "periodic",
        };
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO agent_checkpoints \
             (id, goal_run_id, thread_id, task_id, checkpoint_type, state_json, context_summary, created_at, deleted_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)",
                db::db_params![
                    id,
                    goal_run_id,
                    thread_id,
                    task_id,
                    type_str,
                    state_json,
                    context_summary,
                    created_at as i64,
                ],
            )
            .await?;
        Ok(())
    }

    /// Load all checkpoint JSON blobs for a given goal run, ordered by
    /// `created_at` descending.
    pub async fn list_checkpoints_for_goal_run(&self, goal_run_id: &str) -> Result<Vec<String>> {
        let rows = self
            .conn_db
            .query(
                "SELECT state_json FROM agent_checkpoints WHERE goal_run_id = ?1 AND deleted_at IS NULL ORDER BY created_at DESC",
                db::db_params![goal_run_id],
            )
            .await?;
        Ok(rows
            .iter()
            .filter_map(|row| row.get::<String>(0).ok())
            .collect())
    }

    /// Load a single checkpoint by ID.
    pub async fn get_checkpoint(&self, id: &str) -> Result<Option<String>> {
        let row = self
            .conn_db
            .query_opt(
                "SELECT state_json FROM agent_checkpoints WHERE id = ?1 AND deleted_at IS NULL",
                db::db_params![id],
            )
            .await?;
        row.map(|row| row.get::<String>(0)).transpose()
    }

    /// Delete checkpoints by their IDs.
    pub async fn delete_checkpoints(&self, ids: &[&str]) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let placeholders = vec!["?"; ids.len()].join(", ");
        let sql = format!(
            "UPDATE agent_checkpoints SET deleted_at = ? WHERE deleted_at IS NULL AND id IN ({placeholders})"
        );
        let mut values = vec![db::Value::Integer(now_ts() as i64)];
        values.extend(ids.iter().map(|id| db::Value::Text(id.to_string())));
        let deleted = self
            .conn_db
            .execute(&sql, db::Params::Positional(values))
            .await?;
        Ok(deleted as usize)
    }

    pub async fn delete_checkpoints_for_goal_run(&self, goal_run_id: &str) -> Result<usize> {
        let deleted = self
            .conn_db
            .execute(
                "UPDATE agent_checkpoints SET deleted_at = ?2 WHERE goal_run_id = ?1 AND deleted_at IS NULL",
                db::db_params![goal_run_id, now_ts() as i64],
            )
            .await?;
        Ok(deleted as usize)
    }

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
        self.conn_db
            .execute(
                "INSERT INTO agent_health_log (id, entity_type, entity_id, health_state, indicators_json, intervention, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                db::db_params![id, entity_type, entity_id, health_state, indicators_json, intervention, created_at as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn list_health_log(&self, limit: u32) -> Result<Vec<HealthLogTuple>> {
        let rows = self
            .conn_db
            .query(
                "SELECT id, entity_type, entity_id, health_state, indicators_json, intervention, created_at FROM agent_health_log ORDER BY created_at DESC LIMIT ?1",
                db::db_params![limit as i64],
            )
            .await?;
        rows.iter()
            .map(|row| -> anyhow::Result<HealthLogTuple> {
                Ok((
                    row.get::<String>(0)?,
                    row.get::<String>(1)?,
                    row.get::<String>(2)?,
                    row.get::<String>(3)?,
                    row.get::<Option<String>>(4)?,
                    row.get::<Option<String>>(5)?,
                    row.get::<i64>(6)? as u64,
                ))
            })
            .collect()
    }

    pub async fn list_degraded_health_log_since(
        &self,
        since: u64,
        intervention_contains: Option<&str>,
        limit: u32,
    ) -> Result<Vec<HealthLogTuple>> {
        let intervention_contains = intervention_contains
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let limit = limit.max(1) as i64;
        let since = since as i64;
        let (sql, params) = match intervention_contains {
            Some(text) => (
                "SELECT id, entity_type, entity_id, health_state, indicators_json, intervention, created_at
                     FROM agent_health_log
                     WHERE health_state != 'healthy'
                       AND created_at >= ?1
                       AND intervention LIKE ?2
                     ORDER BY created_at DESC
                     LIMIT ?3",
                db::db_params![since, format!("%{text}%"), limit],
            ),
            None => (
                "SELECT id, entity_type, entity_id, health_state, indicators_json, intervention, created_at
                     FROM agent_health_log
                     WHERE health_state != 'healthy'
                       AND created_at >= ?1
                     ORDER BY created_at DESC
                     LIMIT ?2",
                db::db_params![since, limit],
            ),
        };
        let rows = self.read_db.query(sql, params).await?;
        rows.iter()
            .map(|row| -> anyhow::Result<HealthLogTuple> {
                Ok((
                    row.get::<String>(0)?,
                    row.get::<String>(1)?,
                    row.get::<String>(2)?,
                    row.get::<String>(3)?,
                    row.get::<Option<String>>(4)?,
                    row.get::<Option<String>>(5)?,
                    row.get::<i64>(6)?.max(0) as u64,
                ))
            })
            .collect()
    }
}
