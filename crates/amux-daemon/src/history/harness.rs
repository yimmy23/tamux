use super::*;
use crate::agent::harness::{HarnessRecordEnvelope, HarnessStateProjection};
use rusqlite::params;

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
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO harness_state_records (entry_id, entity_id, thread_id, goal_run_id, task_id, record_kind, status, summary, payload_json, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
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
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn list_harness_state_records(
        &self,
        thread_id: Option<&str>,
        goal_run_id: Option<&str>,
        task_id: Option<&str>,
    ) -> Result<Vec<HarnessRecordEnvelope>> {
        let thread_id = thread_id.map(str::to_string);
        let goal_run_id = goal_run_id.map(str::to_string);
        let task_id = task_id.map(str::to_string);
        let rows: Vec<HarnessStateRecordRow> = self
            .read_conn
            .call(move |conn| {
                let sql = match (thread_id.is_some(), goal_run_id.is_some(), task_id.is_some()) {
                    (true, true, true) => "SELECT entry_id, entity_id, thread_id, goal_run_id, task_id, record_kind, status, summary, payload_json, created_at_ms FROM harness_state_records WHERE thread_id = ?1 AND goal_run_id = ?2 AND task_id = ?3 ORDER BY created_at_ms ASC, entry_id ASC",
                    (true, true, false) => "SELECT entry_id, entity_id, thread_id, goal_run_id, task_id, record_kind, status, summary, payload_json, created_at_ms FROM harness_state_records WHERE thread_id = ?1 AND goal_run_id = ?2 ORDER BY created_at_ms ASC, entry_id ASC",
                    (true, false, true) => "SELECT entry_id, entity_id, thread_id, goal_run_id, task_id, record_kind, status, summary, payload_json, created_at_ms FROM harness_state_records WHERE thread_id = ?1 AND task_id = ?2 ORDER BY created_at_ms ASC, entry_id ASC",
                    (true, false, false) => "SELECT entry_id, entity_id, thread_id, goal_run_id, task_id, record_kind, status, summary, payload_json, created_at_ms FROM harness_state_records WHERE thread_id = ?1 ORDER BY created_at_ms ASC, entry_id ASC",
                    (false, true, true) => "SELECT entry_id, entity_id, thread_id, goal_run_id, task_id, record_kind, status, summary, payload_json, created_at_ms FROM harness_state_records WHERE goal_run_id = ?1 AND task_id = ?2 ORDER BY created_at_ms ASC, entry_id ASC",
                    (false, true, false) => "SELECT entry_id, entity_id, thread_id, goal_run_id, task_id, record_kind, status, summary, payload_json, created_at_ms FROM harness_state_records WHERE goal_run_id = ?1 ORDER BY created_at_ms ASC, entry_id ASC",
                    (false, false, true) => "SELECT entry_id, entity_id, thread_id, goal_run_id, task_id, record_kind, status, summary, payload_json, created_at_ms FROM harness_state_records WHERE task_id = ?1 ORDER BY created_at_ms ASC, entry_id ASC",
                    (false, false, false) => "SELECT entry_id, entity_id, thread_id, goal_run_id, task_id, record_kind, status, summary, payload_json, created_at_ms FROM harness_state_records ORDER BY created_at_ms ASC, entry_id ASC",
                };
                let mut stmt = conn.prepare(sql)?;
                let map_row = |row: &rusqlite::Row<'_>| {
                    Ok(HarnessStateRecordRow {
                        entry_id: row.get(0)?,
                        entity_id: row.get(1)?,
                        thread_id: row.get(2)?,
                        goal_run_id: row.get(3)?,
                        task_id: row.get(4)?,
                        record_kind: row.get(5)?,
                        status: row.get(6)?,
                        summary: row.get(7)?,
                        payload_json: row.get(8)?,
                        created_at_ms: row.get::<_, i64>(9)?.max(0) as u64,
                    })
                };
                let rows = match (thread_id, goal_run_id, task_id) {
                    (Some(thread_id), Some(goal_run_id), Some(task_id)) => stmt
                        .query_map(params![thread_id, goal_run_id, task_id], map_row)?,
                    (Some(thread_id), Some(goal_run_id), None) => stmt
                        .query_map(params![thread_id, goal_run_id], map_row)?,
                    (Some(thread_id), None, Some(task_id)) => {
                        stmt.query_map(params![thread_id, task_id], map_row)?
                    }
                    (Some(thread_id), None, None) => stmt.query_map(params![thread_id], map_row)?,
                    (None, Some(goal_run_id), Some(task_id)) => {
                        stmt.query_map(params![goal_run_id, task_id], map_row)?
                    }
                    (None, Some(goal_run_id), None) => {
                        stmt.query_map(params![goal_run_id], map_row)?
                    }
                    (None, None, Some(task_id)) => stmt.query_map(params![task_id], map_row)?,
                    _ => stmt.query_map([], map_row)?,
                };
                Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

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
                            return Err(anyhow::anyhow!("unknown harness record kind: {other}"))
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
