use super::*;

impl HistoryStore {
    pub async fn upsert_event_trigger(&self, row: &EventTriggerRow) -> Result<()> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO event_triggers (id, event_family, event_kind, agent_id, target_state, thread_id, enabled, cooldown_secs, risk_label, notification_kind, prompt_template, title_template, body_template, created_at, updated_at, last_fired_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                    params![
                        row.id,
                        row.event_family,
                        row.event_kind,
                        row.agent_id,
                        row.target_state,
                        row.thread_id,
                        if row.enabled { 1i64 } else { 0i64 },
                        row.cooldown_secs as i64,
                        row.risk_label,
                        row.notification_kind,
                        row.prompt_template,
                        row.title_template,
                        row.body_template,
                        row.created_at as i64,
                        row.updated_at as i64,
                        row.last_fired_at.map(|value| value as i64),
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_event_triggers(
        &self,
        event_family: Option<&str>,
        event_kind: Option<&str>,
    ) -> Result<Vec<EventTriggerRow>> {
        fn map_event_trigger_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EventTriggerRow> {
            Ok(EventTriggerRow {
                id: row.get(0)?,
                event_family: row.get(1)?,
                event_kind: row.get(2)?,
                agent_id: row.get(3)?,
                target_state: row.get(4)?,
                thread_id: row.get(5)?,
                enabled: row.get::<_, i64>(6)? != 0,
                cooldown_secs: row.get::<_, i64>(7)?.max(0) as u64,
                risk_label: row.get(8)?,
                notification_kind: row.get(9)?,
                prompt_template: row.get(10)?,
                title_template: row.get(11)?,
                body_template: row.get(12)?,
                created_at: row.get::<_, i64>(13)?.max(0) as u64,
                updated_at: row.get::<_, i64>(14)?.max(0) as u64,
                last_fired_at: row
                    .get::<_, Option<i64>>(15)?
                    .map(|value| value.max(0) as u64),
            })
        }

        let event_family = event_family.map(str::to_string);
        let event_kind = event_kind.map(str::to_string);
        self.read_conn
            .call(move |conn| {
                let sql = match (event_family.is_some(), event_kind.is_some()) {
                    (true, true) => {
                        "SELECT id, event_family, event_kind, agent_id, target_state, thread_id, enabled, cooldown_secs, risk_label, notification_kind, prompt_template, title_template, body_template, created_at, updated_at, last_fired_at FROM event_triggers WHERE event_family = ?1 AND event_kind = ?2 ORDER BY updated_at DESC, id ASC"
                    }
                    (true, false) => {
                        "SELECT id, event_family, event_kind, agent_id, target_state, thread_id, enabled, cooldown_secs, risk_label, notification_kind, prompt_template, title_template, body_template, created_at, updated_at, last_fired_at FROM event_triggers WHERE event_family = ?1 ORDER BY updated_at DESC, id ASC"
                    }
                    (false, true) => {
                        "SELECT id, event_family, event_kind, agent_id, target_state, thread_id, enabled, cooldown_secs, risk_label, notification_kind, prompt_template, title_template, body_template, created_at, updated_at, last_fired_at FROM event_triggers WHERE event_kind = ?1 ORDER BY updated_at DESC, id ASC"
                    }
                    (false, false) => {
                        "SELECT id, event_family, event_kind, agent_id, target_state, thread_id, enabled, cooldown_secs, risk_label, notification_kind, prompt_template, title_template, body_template, created_at, updated_at, last_fired_at FROM event_triggers ORDER BY updated_at DESC, id ASC"
                    }
                };
                let mut stmt = conn.prepare(sql)?;
                match (event_family, event_kind) {
                    (Some(event_family), Some(event_kind)) => stmt
                        .query_map(params![event_family, event_kind], map_event_trigger_row)?
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .map_err(Into::into),
                    (Some(event_family), None) => stmt
                        .query_map(params![event_family], map_event_trigger_row)?
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .map_err(Into::into),
                    (None, Some(event_kind)) => stmt
                        .query_map(params![event_kind], map_event_trigger_row)?
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .map_err(Into::into),
                    (None, None) => stmt
                        .query_map([], map_event_trigger_row)?
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .map_err(Into::into),
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn record_event_trigger_fired(&self, id: &str, fired_at: u64) -> Result<()> {
        let id = id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE event_triggers SET last_fired_at = ?2, updated_at = CASE WHEN updated_at > ?2 THEN updated_at ELSE ?2 END WHERE id = ?1",
                    params![id, fired_at as i64],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
