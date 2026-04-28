use super::*;

impl HistoryStore {
    pub async fn upsert_collaboration_session(
        &self,
        parent_task_id: &str,
        session_json: &str,
        updated_at: u64,
    ) -> Result<()> {
        let parent_task_id = parent_task_id.to_string();
        let session_json = session_json.to_string();
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO collaboration_sessions (parent_task_id, session_json, updated_at) VALUES (?1, ?2, ?3)",
            params![parent_task_id, session_json, updated_at as i64],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_collaboration_sessions(&self) -> Result<Vec<CollaborationSessionRow>> {
        self.read_conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT parent_task_id, session_json, updated_at FROM collaboration_sessions ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CollaborationSessionRow {
                parent_task_id: row.get(0)?,
                session_json: row.get(1)?,
                updated_at: row.get::<_, i64>(2)? as u64,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    // ── Operator Profile: fields ────────────────────────────────────────

    pub async fn upsert_profile_field(
        &self,
        field_key: &str,
        field_value_json: &str,
        confidence: f64,
        source: &str,
    ) -> Result<()> {
        let field_key = field_key.to_string();
        let field_value_json = field_value_json.to_string();
        let source = source.to_string();
        let now = now_ts() as i64;
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO operator_profile_fields \
                     (field_key, field_value_json, confidence, source, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![field_key, field_value_json, confidence, source, now],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_profile_field(
        &self,
        field_key: &str,
    ) -> Result<Option<OperatorProfileFieldRow>> {
        let field_key = field_key.to_string();
        self.conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT field_key, field_value_json, confidence, source, updated_at \
                     FROM operator_profile_fields WHERE field_key = ?1",
                    params![field_key],
                    |row| {
                        Ok(OperatorProfileFieldRow {
                            field_key: row.get(0)?,
                            field_value_json: row.get(1)?,
                            confidence: row.get(2)?,
                            source: row.get(3)?,
                            updated_at: row.get(4)?,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_profile_fields(&self) -> Result<Vec<OperatorProfileFieldRow>> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT field_key, field_value_json, confidence, source, updated_at \
                     FROM operator_profile_fields ORDER BY updated_at DESC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(OperatorProfileFieldRow {
                        field_key: row.get(0)?,
                        field_value_json: row.get(1)?,
                        confidence: row.get(2)?,
                        source: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    // ── Operator Profile: consents ──────────────────────────────────────

    pub async fn upsert_profile_consent(&self, consent_key: &str, granted: bool) -> Result<()> {
        let consent_key = consent_key.to_string();
        let now = now_ts() as i64;
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO operator_profile_consents \
                     (consent_key, granted, updated_at) VALUES (?1, ?2, ?3)",
                    params![consent_key, granted as i64, now],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_profile_consent(
        &self,
        consent_key: &str,
    ) -> Result<Option<OperatorProfileConsentRow>> {
        let consent_key = consent_key.to_string();
        self.conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT consent_key, granted, updated_at \
                     FROM operator_profile_consents WHERE consent_key = ?1",
                    params![consent_key],
                    |row| {
                        Ok(OperatorProfileConsentRow {
                            consent_key: row.get(0)?,
                            granted: row.get::<_, i64>(1)? != 0,
                            updated_at: row.get(2)?,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_profile_consents(&self) -> Result<Vec<OperatorProfileConsentRow>> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT consent_key, granted, updated_at \
                     FROM operator_profile_consents ORDER BY consent_key ASC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(OperatorProfileConsentRow {
                        consent_key: row.get(0)?,
                        granted: row.get::<_, i64>(1)? != 0,
                        updated_at: row.get(2)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    // ── Operator Profile: events ────────────────────────────────────────

    pub async fn append_profile_event(
        &self,
        id: &str,
        event_type: &str,
        field_key: Option<&str>,
        value_json: Option<&str>,
        source: &str,
        metadata_json: Option<&str>,
    ) -> Result<()> {
        let id = id.to_string();
        let event_type = event_type.to_string();
        let field_key = field_key.map(str::to_string);
        let value_json = value_json.map(str::to_string);
        let source = source.to_string();
        let metadata_json = metadata_json.map(str::to_string);
        let now = now_ts() as i64;
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO operator_profile_events \
                     (id, event_type, field_key, value_json, source, metadata_json, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        id,
                        event_type,
                        field_key,
                        value_json,
                        source,
                        metadata_json,
                        now
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_profile_events(&self, limit: usize) -> Result<Vec<OperatorProfileEventRow>> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, event_type, field_key, value_json, source, metadata_json, created_at \
                     FROM operator_profile_events ORDER BY created_at DESC LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit as i64], |row| {
                    Ok(OperatorProfileEventRow {
                        id: row.get(0)?,
                        event_type: row.get(1)?,
                        field_key: row.get(2)?,
                        value_json: row.get(3)?,
                        source: row.get(4)?,
                        metadata_json: row.get(5)?,
                        created_at: row.get(6)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    // ── Operator Profile: checkins ──────────────────────────────────────

    pub async fn upsert_profile_checkin(&self, row: OperatorProfileCheckinRow) -> Result<()> {
        let now = now_ts() as i64;
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO operator_profile_checkins \
                     (id, kind, scheduled_at, shown_at, status, response_json, created_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
                     ON CONFLICT(id) DO UPDATE SET \
                       kind         = excluded.kind, \
                       scheduled_at = excluded.scheduled_at, \
                       shown_at     = excluded.shown_at, \
                       status       = excluded.status, \
                       response_json = excluded.response_json, \
                       updated_at   = excluded.updated_at",
                    params![
                        row.id,
                        row.kind,
                        row.scheduled_at,
                        row.shown_at,
                        row.status,
                        row.response_json,
                        row.created_at,
                        now,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_profile_checkin(&self, id: &str) -> Result<Option<OperatorProfileCheckinRow>> {
        let id = id.to_string();
        self.conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT id, kind, scheduled_at, shown_at, status, response_json, created_at, updated_at \
                     FROM operator_profile_checkins WHERE id = ?1",
                    params![id],
                    |row| {
                        Ok(OperatorProfileCheckinRow {
                            id: row.get(0)?,
                            kind: row.get(1)?,
                            scheduled_at: row.get(2)?,
                            shown_at: row.get(3)?,
                            status: row.get(4)?,
                            response_json: row.get(5)?,
                            created_at: row.get(6)?,
                            updated_at: row.get(7)?,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_profile_checkins(&self) -> Result<Vec<OperatorProfileCheckinRow>> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, kind, scheduled_at, shown_at, status, response_json, created_at, updated_at \
                     FROM operator_profile_checkins ORDER BY scheduled_at ASC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(OperatorProfileCheckinRow {
                        id: row.get(0)?,
                        kind: row.get(1)?,
                        scheduled_at: row.get(2)?,
                        shown_at: row.get(3)?,
                        status: row.get(4)?,
                        response_json: row.get(5)?,
                        created_at: row.get(6)?,
                        updated_at: row.get(7)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
