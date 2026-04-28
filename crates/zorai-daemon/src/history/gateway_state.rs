use super::*;

impl HistoryStore {
    pub async fn upsert_gateway_thread_binding(
        &self,
        channel_key: &str,
        thread_id: &str,
        updated_at: u64,
    ) -> Result<()> {
        let channel_key = channel_key.to_string();
        let thread_id = thread_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO gateway_threads (channel_key, thread_id, updated_at) VALUES (?1, ?2, ?3)",
                    params![channel_key, thread_id, updated_at as i64],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_gateway_thread_binding(&self, channel_key: &str) -> Result<()> {
        let channel_key = channel_key.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM gateway_threads WHERE channel_key = ?1",
                    params![channel_key],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_gateway_thread_bindings(&self) -> Result<Vec<(String, String)>> {
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT channel_key, thread_id FROM gateway_threads ORDER BY updated_at DESC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
                Ok(rows.filter_map(|r| r.ok()).collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_gateway_route_mode(
        &self,
        channel_key: &str,
        route_mode: &str,
        updated_at: u64,
    ) -> Result<()> {
        let channel_key = channel_key.to_string();
        let route_mode = route_mode.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO gateway_channel_modes (channel_key, route_mode, updated_at) VALUES (?1, ?2, ?3)",
                    params![channel_key, route_mode, updated_at as i64],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_gateway_route_mode(&self, channel_key: &str) -> Result<()> {
        let channel_key = channel_key.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM gateway_channel_modes WHERE channel_key = ?1",
                    params![channel_key],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_gateway_route_modes(&self) -> Result<Vec<(String, String)>> {
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT channel_key, route_mode FROM gateway_channel_modes ORDER BY updated_at DESC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
                Ok(rows.filter_map(|r| r.ok()).collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_whatsapp_provider_state(
        &self,
        state: WhatsAppProviderStateRow,
    ) -> Result<()> {
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO whatsapp_provider_state \
                     (provider_id, linked_phone, auth_json, metadata_json, last_reset_at, last_linked_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        state.provider_id,
                        state.linked_phone,
                        state.auth_json,
                        state.metadata_json,
                        state.last_reset_at.map(|value| value as i64),
                        state.last_linked_at.map(|value| value as i64),
                        state.updated_at as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_whatsapp_provider_state(
        &self,
        provider_id: &str,
    ) -> Result<Option<WhatsAppProviderStateRow>> {
        let provider_id = provider_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT provider_id, linked_phone, auth_json, metadata_json, last_reset_at, last_linked_at, updated_at \
                     FROM whatsapp_provider_state WHERE provider_id = ?1",
                    params![provider_id],
                    |row| {
                        Ok(WhatsAppProviderStateRow {
                            provider_id: row.get(0)?,
                            linked_phone: row.get(1)?,
                            auth_json: row.get(2)?,
                            metadata_json: row.get(3)?,
                            last_reset_at: row.get::<_, Option<i64>>(4)?.map(|value| value as u64),
                            last_linked_at: row.get::<_, Option<i64>>(5)?.map(|value| value as u64),
                            updated_at: row.get::<_, i64>(6)? as u64,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_whatsapp_provider_state(&self, provider_id: &str) -> Result<()> {
        let provider_id = provider_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM whatsapp_provider_state WHERE provider_id = ?1",
                    params![provider_id],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn save_gateway_replay_cursor(
        &self,
        platform: &str,
        channel_id: &str,
        cursor_value: &str,
        cursor_type: &str,
    ) -> Result<()> {
        let platform = platform.to_string();
        let channel_id = channel_id.to_string();
        let cursor_value = cursor_value.to_string();
        let cursor_type = cursor_type.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO gateway_replay_cursors (platform, channel_id, cursor_value, cursor_type, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![platform, channel_id, cursor_value, cursor_type, now_ts() as i64],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn load_gateway_replay_cursor(
        &self,
        platform: &str,
        channel_id: &str,
    ) -> Result<Option<GatewayReplayCursorRow>> {
        let platform = platform.to_string();
        let channel_id = channel_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT platform, channel_id, cursor_value, cursor_type, updated_at FROM gateway_replay_cursors WHERE platform = ?1 AND channel_id = ?2",
                    params![platform, channel_id],
                    |row| {
                        Ok(GatewayReplayCursorRow {
                            platform: row.get(0)?,
                            channel_id: row.get(1)?,
                            cursor_value: row.get(2)?,
                            cursor_type: row.get(3)?,
                            updated_at: row.get::<_, i64>(4)? as u64,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn load_gateway_replay_cursors(
        &self,
        platform: &str,
    ) -> Result<Vec<GatewayReplayCursorRow>> {
        let platform = platform.to_string();
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT platform, channel_id, cursor_value, cursor_type, updated_at FROM gateway_replay_cursors WHERE platform = ?1 ORDER BY updated_at DESC",
                )?;
                let rows = stmt.query_map(params![platform], |row| {
                    Ok(GatewayReplayCursorRow {
                        platform: row.get(0)?,
                        channel_id: row.get(1)?,
                        cursor_value: row.get(2)?,
                        cursor_type: row.get(3)?,
                        updated_at: row.get::<_, i64>(4)? as u64,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_gateway_health_snapshot(
        &self,
        state: &GatewayHealthState,
        updated_at: u64,
    ) -> Result<()> {
        let platform = state.platform.to_string();
        let state_json =
            serde_json::to_string(state).context("failed to serialize health state")?;
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO gateway_health_snapshots (platform, state_json, updated_at) VALUES (?1, ?2, ?3)",
                    params![platform, state_json, updated_at as i64],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_gateway_health_snapshots(&self) -> Result<Vec<GatewayHealthSnapshotRow>> {
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT platform, state_json, updated_at FROM gateway_health_snapshots ORDER BY updated_at DESC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(GatewayHealthSnapshotRow {
                        platform: row.get(0)?,
                        state_json: row.get(1)?,
                        updated_at: row.get::<_, i64>(2)? as u64,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_operator_profile_session(
        &self,
        session_id: &str,
        kind: &str,
        session_json: &str,
        updated_at: u64,
    ) -> Result<()> {
        let session_id = session_id.to_string();
        let kind = kind.to_string();
        let session_json = session_json.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO operator_profile_sessions (session_id, kind, session_json, updated_at) VALUES (?1, ?2, ?3, ?4)",
                    params![session_id, kind, session_json, updated_at as i64],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_operator_profile_session(&self, session_id: &str) -> Result<()> {
        let session_id = session_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM operator_profile_sessions WHERE session_id = ?1",
                    params![session_id],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_operator_profile_sessions(&self) -> Result<Vec<OperatorProfileSessionRow>> {
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT session_id, kind, session_json, updated_at FROM operator_profile_sessions ORDER BY updated_at DESC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(OperatorProfileSessionRow {
                        session_id: row.get(0)?,
                        kind: row.get(1)?,
                        session_json: row.get(2)?,
                        updated_at: row.get::<_, i64>(3)? as u64,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
