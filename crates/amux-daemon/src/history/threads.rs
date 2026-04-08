use super::*;

impl HistoryStore {
    pub async fn create_thread(&self, thread: &AgentDbThread) -> Result<()> {
        let thread = thread.clone();
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO agent_threads \
             (id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview, metadata_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                thread.id,
                thread.workspace_id,
                thread.surface_id,
                thread.pane_id,
                thread.agent_name,
                thread.title,
                thread.created_at,
                thread.updated_at,
                thread.message_count,
                thread.total_tokens,
                thread.last_preview,
                thread.metadata_json,
            ],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn replace_thread_snapshot(
        &self,
        thread: &AgentDbThread,
        messages: &[AgentDbMessage],
    ) -> Result<()> {
        let thread = thread.clone();
        let messages = messages.to_vec();
        self.conn
            .call(move |conn| {
                let transaction = conn.transaction()?;

                transaction.execute(
                    "INSERT INTO agent_threads \
                     (id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview, metadata_json) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) \
                     ON CONFLICT(id) DO UPDATE SET \
                        workspace_id = excluded.workspace_id, \
                        surface_id = excluded.surface_id, \
                        pane_id = excluded.pane_id, \
                        agent_name = excluded.agent_name, \
                        title = excluded.title, \
                        created_at = excluded.created_at, \
                        updated_at = excluded.updated_at, \
                        message_count = excluded.message_count, \
                        total_tokens = excluded.total_tokens, \
                        last_preview = excluded.last_preview, \
                        metadata_json = excluded.metadata_json",
                    params![
                        thread.id,
                        thread.workspace_id,
                        thread.surface_id,
                        thread.pane_id,
                        thread.agent_name,
                        thread.title,
                        thread.created_at,
                        thread.updated_at,
                        thread.message_count,
                        thread.total_tokens,
                        thread.last_preview,
                        thread.metadata_json,
                    ],
                )?;

                transaction.execute(
                    "DELETE FROM agent_messages WHERE thread_id = ?1",
                    params![&thread.id],
                )?;

                for message in &messages {
                    transaction.execute(
                        "INSERT OR REPLACE INTO agent_messages \
                         (id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, reasoning, tool_calls_json, metadata_json) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                        params![
                            message.id,
                            message.thread_id,
                            message.created_at,
                            message.role,
                            message.content,
                            message.provider,
                            message.model,
                            message.input_tokens,
                            message.output_tokens,
                            message.total_tokens,
                            message.reasoning,
                            message.tool_calls_json,
                            message.metadata_json,
                        ],
                    )?;
                }

                if messages.is_empty() {
                    transaction.execute(
                        "UPDATE agent_threads SET message_count = ?2, total_tokens = ?3, last_preview = ?4, updated_at = ?5 WHERE id = ?1",
                        params![
                            &thread.id,
                            thread.message_count,
                            thread.total_tokens,
                            thread.last_preview,
                            thread.updated_at,
                        ],
                    )?;
                } else {
                    refresh_thread_stats(&transaction, &thread.id)?;
                }

                transaction.commit()?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_thread(&self, id: &str) -> Result<()> {
        let id = id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute("DELETE FROM agent_threads WHERE id = ?1", params![id])?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_threads(&self) -> Result<Vec<AgentDbThread>> {
        self.conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview, metadata_json \
             FROM agent_threads ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], map_agent_thread)?;
        Ok(rows.filter_map(|row| row.ok()).collect())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_thread(&self, id: &str) -> Result<Option<AgentDbThread>> {
        let id = id.to_string();
        self.conn.call(move |conn| {
        conn
            .query_row(
                "SELECT id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview, metadata_json \
                 FROM agent_threads WHERE id = ?1",
                params![id],
                map_agent_thread,
            )
            .optional()
            .map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn add_message(&self, message: &AgentDbMessage) -> Result<()> {
        let message = message.clone();
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO agent_messages \
             (id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, reasoning, tool_calls_json, metadata_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                message.id,
                message.thread_id,
                message.created_at,
                message.role,
                message.content,
                message.provider,
                message.model,
                message.input_tokens,
                message.output_tokens,
                message.total_tokens,
                message.reasoning,
                message.tool_calls_json,
                message.metadata_json,
            ],
        )?;
        refresh_thread_stats(conn, &message.thread_id)?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn update_message(&self, id: &str, patch: &AgentMessagePatch) -> Result<()> {
        let id = id.to_string();
        let patch = patch.clone();
        self.conn
            .call(move |conn| {
                let thread_id: Option<String> = conn
                    .query_row(
                        "SELECT thread_id FROM agent_messages WHERE id = ?1",
                        params![id],
                        |row| row.get(0),
                    )
                    .optional()?;

                if thread_id.is_none() {
                    return Ok(());
                }

                conn.execute(
                    "UPDATE agent_messages SET
                content = COALESCE(?2, content),
                provider = COALESCE(?3, provider),
                model = COALESCE(?4, model),
                input_tokens = COALESCE(?5, input_tokens),
                output_tokens = COALESCE(?6, output_tokens),
                total_tokens = COALESCE(?7, total_tokens),
                reasoning = COALESCE(?8, reasoning),
                tool_calls_json = COALESCE(?9, tool_calls_json),
                metadata_json = COALESCE(?10, metadata_json)
             WHERE id = ?1",
                    params![
                        id,
                        patch.content.as_deref(),
                        flatten_option_str(&patch.provider),
                        flatten_option_str(&patch.model),
                        flatten_option_i64(&patch.input_tokens),
                        flatten_option_i64(&patch.output_tokens),
                        flatten_option_i64(&patch.total_tokens),
                        flatten_option_str(&patch.reasoning),
                        flatten_option_str(&patch.tool_calls_json),
                        flatten_option_str(&patch.metadata_json),
                    ],
                )?;

                if let Some(thread_id) = thread_id {
                    refresh_thread_stats(conn, &thread_id)?;
                }
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Delete specific messages from a thread by their IDs.
    pub async fn delete_messages(&self, thread_id: &str, message_ids: &[&str]) -> Result<usize> {
        if message_ids.is_empty() {
            return Ok(0);
        }
        let thread_id = thread_id.to_string();
        let message_ids: Vec<String> = message_ids.iter().map(|s| s.to_string()).collect();
        self.conn
            .call(move |conn| {
                let placeholders: Vec<String> =
                    message_ids.iter().map(|_| "?".to_string()).collect();
                let sql = format!(
                    "DELETE FROM agent_messages WHERE thread_id = ? AND id IN ({})",
                    placeholders.join(", ")
                );
                let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
                params.push(Box::new(thread_id.to_string()));
                for id in message_ids {
                    params.push(Box::new(id.to_string()));
                }
                let refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();
                let count = conn.execute(&sql, refs.as_slice())?;
                refresh_thread_stats(conn, &thread_id)?;
                Ok(count)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_messages(
        &self,
        thread_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<AgentDbMessage>> {
        let thread_id = thread_id.to_string();
        self.conn.call(move |conn| {
        let limit = limit.unwrap_or(500).max(1) as i64;
        let mut stmt = conn.prepare(
            "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, reasoning, tool_calls_json, metadata_json \
             FROM agent_messages WHERE thread_id = ?1 ORDER BY created_at ASC, rowid ASC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![thread_id, limit], map_agent_message)?;
        Ok(rows.filter_map(|row| row.ok()).collect())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_recent_messages(
        &self,
        thread_id: &str,
        limit: usize,
    ) -> Result<Vec<AgentDbMessage>> {
        let thread_id = thread_id.to_string();
        self.conn
            .call(move |conn| {
                let limit = limit.clamp(1, 1000) as i64;
                let mut stmt = conn.prepare(
                    "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, reasoning, tool_calls_json, metadata_json \
                     FROM agent_messages WHERE thread_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![thread_id, limit], map_agent_message)?;
                let mut messages: Vec<AgentDbMessage> = rows.filter_map(|row| row.ok()).collect();
                messages.reverse();
                Ok(messages)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_worm_chain_tip(&self, kind: &str) -> Result<Option<WormChainTip>> {
        let kind = kind.to_string();
        let kind = kind.to_string();
        self.conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT kind, seq, hash FROM worm_chain_tip WHERE kind = ?1",
                    params![kind],
                    |row| {
                        Ok(WormChainTip {
                            kind: row.get(0)?,
                            seq: row.get(1)?,
                            hash: row.get(2)?,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn set_worm_chain_tip(&self, kind: &str, seq: i64, hash: &str) -> Result<()> {
        let kind = kind.to_string();
        let hash = hash.to_string();
        let kind = kind.to_string();
        let hash = hash.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO worm_chain_tip (kind, seq, hash) VALUES (?1, ?2, ?3) \
                 ON CONFLICT(kind) DO UPDATE SET seq = excluded.seq, hash = excluded.hash",
                    params![kind, seq, hash],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_transcript_index(&self, entry: &TranscriptIndexEntry) -> Result<()> {
        let entry = entry.clone();
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO transcript_index \
             (id, pane_id, workspace_id, surface_id, filename, reason, captured_at, size_bytes, preview) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                entry.id,
                entry.pane_id,
                entry.workspace_id,
                entry.surface_id,
                entry.filename,
                entry.reason,
                entry.captured_at,
                entry.size_bytes,
                entry.preview,
            ],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_transcript_index(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<Vec<TranscriptIndexEntry>> {
        let workspace_id = workspace_id.map(str::to_string);
        self.conn.call(move |conn| {
        let sql = if workspace_id.is_some() {
            "SELECT id, pane_id, workspace_id, surface_id, filename, reason, captured_at, size_bytes, preview \
             FROM transcript_index WHERE workspace_id = ?1 ORDER BY captured_at DESC"
        } else {
            "SELECT id, pane_id, workspace_id, surface_id, filename, reason, captured_at, size_bytes, preview \
             FROM transcript_index ORDER BY captured_at DESC"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = if let Some(workspace_id) = workspace_id {
            stmt.query_map(params![workspace_id], map_transcript_index_entry)?
        } else {
            stmt.query_map([], map_transcript_index_entry)?
        };
        Ok(rows.filter_map(|row| row.ok()).collect())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_snapshot_index(&self, entry: &SnapshotIndexEntry) -> Result<()> {
        let entry = entry.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO snapshot_index \
             (snapshot_id, workspace_id, session_id, kind, label, path, created_at, details_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        entry.snapshot_id,
                        entry.workspace_id,
                        entry.session_id,
                        entry.kind,
                        entry.label,
                        entry.path,
                        entry.created_at,
                        entry.details_json,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_snapshot_index(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<Vec<SnapshotIndexEntry>> {
        let workspace_id = workspace_id.map(str::to_string);
        self.conn.call(move |conn| {
        let sql = if workspace_id.is_some() {
            "SELECT snapshot_id, workspace_id, session_id, kind, label, path, created_at, details_json \
             FROM snapshot_index WHERE workspace_id = ?1 OR workspace_id IS NULL ORDER BY created_at DESC"
        } else {
            "SELECT snapshot_id, workspace_id, session_id, kind, label, path, created_at, details_json \
             FROM snapshot_index ORDER BY created_at DESC"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = if let Some(workspace_id) = workspace_id {
            stmt.query_map(params![workspace_id], map_snapshot_index_entry)?
        } else {
            stmt.query_map([], map_snapshot_index_entry)?
        };
        Ok(rows.filter_map(|row| row.ok()).collect())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_snapshot_index(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<SnapshotIndexEntry>> {
        let snapshot_id = snapshot_id.to_string();
        self.conn.call(move |conn| {
        conn
            .query_row(
                "SELECT snapshot_id, workspace_id, session_id, kind, label, path, created_at, details_json \
                 FROM snapshot_index WHERE snapshot_id = ?1",
                params![snapshot_id],
                map_snapshot_index_entry,
            )
            .optional()
            .map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_snapshot_index(&self, snapshot_id: &str) -> Result<bool> {
        let snapshot_id = snapshot_id.to_string();
        self.conn
            .call(move |conn| {
                let affected = conn.execute(
                    "DELETE FROM snapshot_index WHERE snapshot_id = ?1",
                    params![snapshot_id],
                )?;
                Ok(affected > 0)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
