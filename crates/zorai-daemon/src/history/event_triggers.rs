use super::*;

impl HistoryStore {
    pub async fn upsert_event_trigger(&self, row: &EventTriggerRow) -> Result<()> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO event_triggers (id, event_family, event_kind, agent_id, target_state, thread_id, enabled, cooldown_secs, risk_label, notification_kind, prompt_template, tool_name, tool_payload_json, title_template, body_template, created_at, updated_at, last_fired_at, max_retries) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
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
                        row.tool_name,
                        row.tool_payload_json,
                        row.title_template,
                        row.body_template,
                        row.created_at as i64,
                        row.updated_at as i64,
                        row.last_fired_at.map(|value| value as i64),
                        row.max_retries as i64,
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
                tool_name: row.get(11)?,
                tool_payload_json: row.get(12)?,
                title_template: row.get(13)?,
                body_template: row.get(14)?,
                created_at: row.get::<_, i64>(15)?.max(0) as u64,
                updated_at: row.get::<_, i64>(16)?.max(0) as u64,
                last_fired_at: row
                    .get::<_, Option<i64>>(17)?
                    .map(|value| value.max(0) as u64),
                max_retries: row.get::<_, i64>(18)?.max(0) as u32,
            })
        }

        let event_family = event_family.map(str::to_string);
        let event_kind = event_kind.map(str::to_string);
        self.read_conn
            .call(move |conn| {
                let sql = match (event_family.is_some(), event_kind.is_some()) {
                    (true, true) => {
                        "SELECT id, event_family, event_kind, agent_id, target_state, thread_id, enabled, cooldown_secs, risk_label, notification_kind, prompt_template, tool_name, tool_payload_json, title_template, body_template, created_at, updated_at, last_fired_at, max_retries FROM event_triggers WHERE event_family = ?1 AND event_kind = ?2 ORDER BY updated_at DESC, id ASC"
                    }
                    (true, false) => {
                        "SELECT id, event_family, event_kind, agent_id, target_state, thread_id, enabled, cooldown_secs, risk_label, notification_kind, prompt_template, tool_name, tool_payload_json, title_template, body_template, created_at, updated_at, last_fired_at, max_retries FROM event_triggers WHERE event_family = ?1 ORDER BY updated_at DESC, id ASC"
                    }
                    (false, true) => {
                        "SELECT id, event_family, event_kind, agent_id, target_state, thread_id, enabled, cooldown_secs, risk_label, notification_kind, prompt_template, tool_name, tool_payload_json, title_template, body_template, created_at, updated_at, last_fired_at, max_retries FROM event_triggers WHERE event_kind = ?1 ORDER BY updated_at DESC, id ASC"
                    }
                    (false, false) => {
                        "SELECT id, event_family, event_kind, agent_id, target_state, thread_id, enabled, cooldown_secs, risk_label, notification_kind, prompt_template, tool_name, tool_payload_json, title_template, body_template, created_at, updated_at, last_fired_at, max_retries FROM event_triggers ORDER BY updated_at DESC, id ASC"
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

    pub async fn insert_trigger_fire_history(&self, row: &TriggerFireHistoryRow) -> Result<()> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO trigger_fire_history (id, trigger_id, event_family, event_kind, status, fired_at_ms, completed_at_ms, retry_count, error_message, created_task_id, notice_id, payload_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        row.id,
                        row.trigger_id,
                        row.event_family,
                        row.event_kind,
                        row.status,
                        row.fired_at_ms as i64,
                        row.completed_at_ms.map(|value| value as i64),
                        row.retry_count as i64,
                        row.error_message,
                        row.created_task_id,
                        row.notice_id,
                        row.payload_json,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn update_trigger_fire_status(
        &self,
        fire_id: &str,
        status: &str,
        error_message: Option<&str>,
        created_task_id: Option<&str>,
    ) -> Result<()> {
        let fire_id = fire_id.to_string();
        let status = status.to_string();
        let error_message = error_message.map(str::to_string);
        let created_task_id = created_task_id.map(str::to_string);
        let now = {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64
        };
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE trigger_fire_history SET status = ?2, completed_at_ms = ?3, error_message = ?4, created_task_id = COALESCE(?5, created_task_id) WHERE id = ?1",
                    params![fire_id, status, now, error_message, created_task_id],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_trigger_fire_history(
        &self,
        trigger_id: Option<&str>,
        status: Option<&str>,
        limit: usize,
    ) -> Result<Vec<TriggerFireHistoryRow>> {
        let trigger_id = trigger_id.map(str::to_string);
        let status = status.map(str::to_string);
        self.read_conn
            .call(move |conn| {
                let (sql, params_builder): (&str, Box<dyn FnOnce() -> Vec<Box<dyn rusqlite::types::ToSql>>>) = match (trigger_id.is_some(), status.is_some()) {
                    (true, true) => (
                        "SELECT id, trigger_id, event_family, event_kind, status, fired_at_ms, completed_at_ms, retry_count, error_message, created_task_id, notice_id, payload_json FROM trigger_fire_history WHERE trigger_id = ?1 AND status = ?2 ORDER BY fired_at_ms DESC LIMIT ?3",
                        {
                            let tid = trigger_id.unwrap();
                            let st = status.unwrap();
                            Box::new(move || vec![
                                Box::new(tid) as Box<dyn rusqlite::types::ToSql>,
                                Box::new(st),
                                Box::new(limit as i64),
                            ])
                        },
                    ),
                    (true, false) => (
                        "SELECT id, trigger_id, event_family, event_kind, status, fired_at_ms, completed_at_ms, retry_count, error_message, created_task_id, notice_id, payload_json FROM trigger_fire_history WHERE trigger_id = ?1 ORDER BY fired_at_ms DESC LIMIT ?2",
                        {
                            let tid = trigger_id.unwrap();
                            Box::new(move || vec![
                                Box::new(tid) as Box<dyn rusqlite::types::ToSql>,
                                Box::new(limit as i64),
                            ])
                        },
                    ),
                    (false, true) => (
                        "SELECT id, trigger_id, event_family, event_kind, status, fired_at_ms, completed_at_ms, retry_count, error_message, created_task_id, notice_id, payload_json FROM trigger_fire_history WHERE status = ?1 ORDER BY fired_at_ms DESC LIMIT ?2",
                        {
                            let st = status.unwrap();
                            Box::new(move || vec![
                                Box::new(st) as Box<dyn rusqlite::types::ToSql>,
                                Box::new(limit as i64),
                            ])
                        },
                    ),
                    (false, false) => (
                        "SELECT id, trigger_id, event_family, event_kind, status, fired_at_ms, completed_at_ms, retry_count, error_message, created_task_id, notice_id, payload_json FROM trigger_fire_history ORDER BY fired_at_ms DESC LIMIT ?1",
                        Box::new(move || vec![
                            Box::new(limit as i64) as Box<dyn rusqlite::types::ToSql>,
                        ]),
                    ),
                };
                let params = params_builder();
                let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
                let mut stmt = conn.prepare(sql)?;
                let rows = stmt
                    .query_map(param_refs.as_slice(), |row| {
                        Ok(TriggerFireHistoryRow {
                            id: row.get(0)?,
                            trigger_id: row.get(1)?,
                            event_family: row.get(2)?,
                            event_kind: row.get(3)?,
                            status: row.get(4)?,
                            fired_at_ms: row.get::<_, i64>(5)?.max(0) as u64,
                            completed_at_ms: row.get::<_, Option<i64>>(6)?.map(|value| value.max(0) as u64),
                            retry_count: row.get::<_, i64>(7)?.max(0) as u64,
                            error_message: row.get(8)?,
                            created_task_id: row.get(9)?,
                            notice_id: row.get(10)?,
                            payload_json: row.get(11)?,
                        })
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
