use super::*;

impl HistoryStore {
    pub async fn insert_event_log(&self, row: &EventLogRow) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT INTO event_log (id, event_family, event_kind, state, thread_id, payload_json, risk_label, handled_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                db::db_params![
                    row.id.clone(),
                    row.event_family.clone(),
                    row.event_kind.clone(),
                    row.state.clone(),
                    row.thread_id.clone(),
                    row.payload_json.clone(),
                    row.risk_label.clone(),
                    row.handled_at_ms as i64,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn list_event_log(
        &self,
        event_family: Option<&str>,
        event_kind: Option<&str>,
        limit: usize,
    ) -> Result<Vec<EventLogRow>> {
        let limit = limit.max(1) as i64;
        let (sql, params) = match (event_family, event_kind) {
            (Some(event_family), Some(event_kind)) => (
                "SELECT id, event_family, event_kind, state, thread_id, payload_json, risk_label, handled_at_ms FROM event_log WHERE event_family = ?1 AND event_kind = ?2 ORDER BY handled_at_ms DESC, id DESC LIMIT ?3",
                db::db_params![event_family, event_kind, limit],
            ),
            (Some(event_family), None) => (
                "SELECT id, event_family, event_kind, state, thread_id, payload_json, risk_label, handled_at_ms FROM event_log WHERE event_family = ?1 ORDER BY handled_at_ms DESC, id DESC LIMIT ?2",
                db::db_params![event_family, limit],
            ),
            (None, Some(event_kind)) => (
                "SELECT id, event_family, event_kind, state, thread_id, payload_json, risk_label, handled_at_ms FROM event_log WHERE event_kind = ?1 ORDER BY handled_at_ms DESC, id DESC LIMIT ?2",
                db::db_params![event_kind, limit],
            ),
            (None, None) => (
                "SELECT id, event_family, event_kind, state, thread_id, payload_json, risk_label, handled_at_ms FROM event_log ORDER BY handled_at_ms DESC, id DESC LIMIT ?1",
                db::db_params![limit],
            ),
        };
        let rows = self.read_db.query(sql, params).await?;
        rows.iter().map(map_event_log_row).collect()
    }
}
