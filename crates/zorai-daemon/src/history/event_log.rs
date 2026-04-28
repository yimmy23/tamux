use super::*;

impl HistoryStore {
    pub async fn insert_event_log(&self, row: &EventLogRow) -> Result<()> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO event_log (id, event_family, event_kind, state, thread_id, payload_json, risk_label, handled_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        row.id,
                        row.event_family,
                        row.event_kind,
                        row.state,
                        row.thread_id,
                        row.payload_json,
                        row.risk_label,
                        row.handled_at_ms as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_event_log(
        &self,
        event_family: Option<&str>,
        event_kind: Option<&str>,
        limit: usize,
    ) -> Result<Vec<EventLogRow>> {
        let event_family = event_family.map(str::to_string);
        let event_kind = event_kind.map(str::to_string);
        let limit = limit.max(1) as i64;
        self.read_conn
            .call(move |conn| {
                let sql = match (event_family.is_some(), event_kind.is_some()) {
                    (true, true) => {
                        "SELECT id, event_family, event_kind, state, thread_id, payload_json, risk_label, handled_at_ms FROM event_log WHERE event_family = ?1 AND event_kind = ?2 ORDER BY handled_at_ms DESC, id DESC LIMIT ?3"
                    }
                    (true, false) => {
                        "SELECT id, event_family, event_kind, state, thread_id, payload_json, risk_label, handled_at_ms FROM event_log WHERE event_family = ?1 ORDER BY handled_at_ms DESC, id DESC LIMIT ?2"
                    }
                    (false, true) => {
                        "SELECT id, event_family, event_kind, state, thread_id, payload_json, risk_label, handled_at_ms FROM event_log WHERE event_kind = ?1 ORDER BY handled_at_ms DESC, id DESC LIMIT ?2"
                    }
                    (false, false) => {
                        "SELECT id, event_family, event_kind, state, thread_id, payload_json, risk_label, handled_at_ms FROM event_log ORDER BY handled_at_ms DESC, id DESC LIMIT ?1"
                    }
                };
                let mut stmt = conn.prepare(sql)?;
                let map = |row: &rusqlite::Row<'_>| {
                    Ok(EventLogRow {
                        id: row.get(0)?,
                        event_family: row.get(1)?,
                        event_kind: row.get(2)?,
                        state: row.get(3)?,
                        thread_id: row.get(4)?,
                        payload_json: row.get(5)?,
                        risk_label: row.get(6)?,
                        handled_at_ms: row.get::<_, i64>(7)?.max(0) as u64,
                    })
                };
                match (event_family, event_kind) {
                    (Some(event_family), Some(event_kind)) => stmt
                        .query_map(params![event_family, event_kind, limit], map)?
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .map_err(Into::into),
                    (Some(event_family), None) => stmt
                        .query_map(params![event_family, limit], map)?
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .map_err(Into::into),
                    (None, Some(event_kind)) => stmt
                        .query_map(params![event_kind, limit], map)?
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .map_err(Into::into),
                    (None, None) => stmt
                        .query_map(params![limit], map)?
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .map_err(Into::into),
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
