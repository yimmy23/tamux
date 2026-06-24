use super::*;
use crate::agent::harness::{HarnessRecordEnvelope, HarnessStateProjection};

impl HistoryStore {
    pub(crate) async fn append_harness_state_record(
        &self,
        record: &HarnessRecordEnvelope,
    ) -> Result<()> {
        let row = HarnessStateRecordRow {
            entry_id: record.entry_id.clone(),
            entity_id: record.entity_id.clone(),
            thread_id: record.thread_id.clone(),
            goal_run_id: record.goal_run_id.clone(),
            task_id: record.task_id.clone(),
            record_kind: record.kind.as_str().to_string(),
            status: record.status.clone(),
            summary: record.summary.clone(),
            payload_json: serde_json::to_string(&record.payload)?,
            created_at_ms: record.created_at_ms,
        };
        self.conn_db
            .execute(
                "INSERT INTO harness_state_records (entry_id, entity_id, thread_id, goal_run_id, task_id, record_kind, status, summary, payload_json, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                db::db_params![
                    row.entry_id,
                    row.entity_id,
                    row.thread_id,
                    row.goal_run_id,
                    row.task_id,
                    row.record_kind,
                    row.status,
                    row.summary,
                    row.payload_json,
                    row.created_at_ms as i64,
                ],
            )
            .await?;
        Ok(())
    }

    pub(crate) async fn list_harness_state_records(
        &self,
        thread_id: Option<&str>,
        goal_run_id: Option<&str>,
        task_id: Option<&str>,
    ) -> Result<Vec<HarnessRecordEnvelope>> {
        let (sql, params) = match (thread_id, goal_run_id, task_id) {
            (Some(thread_id), Some(goal_run_id), Some(task_id)) => (
                "SELECT entry_id, entity_id, thread_id, goal_run_id, task_id, record_kind, status, summary, payload_json, created_at_ms FROM harness_state_records WHERE thread_id = ?1 AND goal_run_id = ?2 AND task_id = ?3 ORDER BY created_at_ms ASC, entry_id ASC",
                db::db_params![thread_id, goal_run_id, task_id],
            ),
            (Some(thread_id), Some(goal_run_id), None) => (
                "SELECT entry_id, entity_id, thread_id, goal_run_id, task_id, record_kind, status, summary, payload_json, created_at_ms FROM harness_state_records WHERE thread_id = ?1 AND goal_run_id = ?2 ORDER BY created_at_ms ASC, entry_id ASC",
                db::db_params![thread_id, goal_run_id],
            ),
            (Some(thread_id), None, Some(task_id)) => (
                "SELECT entry_id, entity_id, thread_id, goal_run_id, task_id, record_kind, status, summary, payload_json, created_at_ms FROM harness_state_records WHERE thread_id = ?1 AND task_id = ?2 ORDER BY created_at_ms ASC, entry_id ASC",
                db::db_params![thread_id, task_id],
            ),
            (Some(thread_id), None, None) => (
                "SELECT entry_id, entity_id, thread_id, goal_run_id, task_id, record_kind, status, summary, payload_json, created_at_ms FROM harness_state_records WHERE thread_id = ?1 ORDER BY created_at_ms ASC, entry_id ASC",
                db::db_params![thread_id],
            ),
            (None, Some(goal_run_id), Some(task_id)) => (
                "SELECT entry_id, entity_id, thread_id, goal_run_id, task_id, record_kind, status, summary, payload_json, created_at_ms FROM harness_state_records WHERE goal_run_id = ?1 AND task_id = ?2 ORDER BY created_at_ms ASC, entry_id ASC",
                db::db_params![goal_run_id, task_id],
            ),
            (None, Some(goal_run_id), None) => (
                "SELECT entry_id, entity_id, thread_id, goal_run_id, task_id, record_kind, status, summary, payload_json, created_at_ms FROM harness_state_records WHERE goal_run_id = ?1 ORDER BY created_at_ms ASC, entry_id ASC",
                db::db_params![goal_run_id],
            ),
            (None, None, Some(task_id)) => (
                "SELECT entry_id, entity_id, thread_id, goal_run_id, task_id, record_kind, status, summary, payload_json, created_at_ms FROM harness_state_records WHERE task_id = ?1 ORDER BY created_at_ms ASC, entry_id ASC",
                db::db_params![task_id],
            ),
            (None, None, None) => (
                "SELECT entry_id, entity_id, thread_id, goal_run_id, task_id, record_kind, status, summary, payload_json, created_at_ms FROM harness_state_records ORDER BY created_at_ms ASC, entry_id ASC",
                db::Params::None,
            ),
        };
        let result_rows = self.read_db.query(sql, params).await?;
        let rows: Vec<HarnessStateRecordRow> = result_rows
            .iter()
            .map(map_harness_state_record_row)
            .collect::<Result<Vec<_>>>()?;

        rows.into_iter()
            .map(|row| {
                Ok(HarnessRecordEnvelope {
                    entry_id: row.entry_id,
                    entity_id: row.entity_id,
                    thread_id: row.thread_id,
                    goal_run_id: row.goal_run_id,
                    task_id: row.task_id,
                    kind: match row.record_kind.as_str() {
                        "observation" => crate::agent::harness::HarnessRecordKind::Observation,
                        "belief" => crate::agent::harness::HarnessRecordKind::Belief,
                        "goal" => crate::agent::harness::HarnessRecordKind::Goal,
                        "world_state" => crate::agent::harness::HarnessRecordKind::WorldState,
                        "tension" => crate::agent::harness::HarnessRecordKind::Tension,
                        "commitment" => crate::agent::harness::HarnessRecordKind::Commitment,
                        "effect" => crate::agent::harness::HarnessRecordKind::Effect,
                        "verification" => crate::agent::harness::HarnessRecordKind::Verification,
                        "procedure" => crate::agent::harness::HarnessRecordKind::Procedure,
                        "effect_contract" => {
                            crate::agent::harness::HarnessRecordKind::EffectContract
                        }
                        other => {
                            return Err(anyhow::anyhow!("unknown harness record kind: {other}"));
                        }
                    },
                    status: row.status,
                    summary: row.summary,
                    payload: serde_json::from_str(&row.payload_json)?,
                    created_at_ms: row.created_at_ms,
                })
            })
            .collect()
    }

    pub(crate) async fn project_harness_state(
        &self,
        thread_id: Option<&str>,
        goal_run_id: Option<&str>,
        task_id: Option<&str>,
    ) -> Result<HarnessStateProjection> {
        let records = self
            .list_harness_state_records(thread_id, goal_run_id, task_id)
            .await?;
        crate::agent::harness::project_harness_state(&records)
    }
}
