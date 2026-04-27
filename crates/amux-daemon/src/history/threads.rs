use super::*;

fn agent_message_search_document(message: &AgentDbMessage) -> super::search_index::SearchDocument {
    let mut tags = vec![message.role.clone()];
    if let Some(provider) = &message.provider {
        tags.push(provider.clone());
    }
    if let Some(model) = &message.model {
        tags.push(model.clone());
    }
    super::search_index::SearchDocument {
        source_kind: super::search_index::SearchSourceKind::AgentMessage,
        source_id: message.id.clone(),
        title: format!("{} message", message.role),
        body: [
            message.content.as_str(),
            message.reasoning.as_deref().unwrap_or_default(),
            message.tool_calls_json.as_deref().unwrap_or_default(),
            message.metadata_json.as_deref().unwrap_or_default(),
        ]
        .join("\n"),
        tags,
        workspace_id: None,
        thread_id: Some(message.thread_id.clone()),
        agent_id: None,
        timestamp: message.created_at,
        metadata_json: message.metadata_json.clone(),
    }
}

fn index_agent_messages(store: &HistoryStore, messages: &[AgentDbMessage]) {
    let documents = messages
        .iter()
        .map(agent_message_search_document)
        .collect::<Vec<_>>();
    store.upsert_search_documents_background(documents);
}

fn agent_message_needs_upsert(existing: &AgentDbMessage, incoming: &AgentDbMessage) -> bool {
    existing.thread_id != incoming.thread_id
        || existing.created_at != incoming.created_at
        || existing.role != incoming.role
        || existing.content != incoming.content
        || existing.provider != incoming.provider
        || existing.model != incoming.model
        || existing.input_tokens != incoming.input_tokens
        || existing.output_tokens != incoming.output_tokens
        || existing.total_tokens != incoming.total_tokens
        || existing.cost_usd != incoming.cost_usd
        || existing.reasoning != incoming.reasoning
        || existing.tool_calls_json != incoming.tool_calls_json
        || existing.metadata_json != incoming.metadata_json
}

fn messages_requiring_search_reindex(
    existing_messages: &std::collections::HashMap<String, AgentDbMessage>,
    messages: &[AgentDbMessage],
) -> Vec<AgentDbMessage> {
    messages
        .iter()
        .filter(|message| {
            existing_messages
                .get(&message.id)
                .map(|existing| agent_message_needs_upsert(existing, message))
                .unwrap_or(true)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: &str, content: &str) -> AgentDbMessage {
        AgentDbMessage {
            id: id.to_string(),
            thread_id: "thread-1".to_string(),
            created_at: 100,
            role: "user".to_string(),
            content: content.to_string(),
            provider: None,
            model: None,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        }
    }

    #[test]
    fn messages_requiring_search_reindex_ignores_unchanged_messages() {
        let existing = [message("m1", "unchanged")]
            .into_iter()
            .map(|message| (message.id.clone(), message))
            .collect::<std::collections::HashMap<_, _>>();
        let incoming = vec![message("m1", "unchanged"), message("m2", "new")];

        let changed_ids = messages_requiring_search_reindex(&existing, &incoming)
            .into_iter()
            .map(|message| message.id)
            .collect::<Vec<_>>();

        assert_eq!(changed_ids, vec!["m2"]);
    }
}

impl HistoryStore {
    pub async fn reconcile_thread_snapshot(
        &self,
        thread: &AgentDbThread,
        messages: &[AgentDbMessage],
    ) -> Result<()> {
        let thread = thread.clone();
        let messages = messages.to_vec();
        self.conn
            .call(move |conn| {
                let transaction = conn.transaction()?;
                let incoming_message_count = messages.len() as i64;
                let incoming_latest_created_at = messages
                    .iter()
                    .map(|message| message.created_at)
                    .max()
                    .unwrap_or(thread.updated_at);

                let existing_snapshot = transaction
                    .query_row(
                        "SELECT
                            updated_at,
                            message_count,
                            COALESCE((
                                SELECT MAX(created_at)
                                FROM agent_messages
                                WHERE thread_id = ?1
                            ), 0)
                         FROM agent_threads
                         WHERE id = ?1",
                        params![&thread.id],
                        |row| {
                            Ok((
                                row.get::<_, i64>(0)?,
                                row.get::<_, i64>(1)?,
                                row.get::<_, i64>(2)?,
                            ))
                        },
                    )
                    .optional()?;

                if let Some((
                    existing_updated_at,
                    existing_message_count,
                    existing_latest_created_at,
                )) = existing_snapshot
                {
                    let stale_snapshot = existing_updated_at > thread.updated_at
                        || (existing_updated_at == thread.updated_at
                            && (existing_message_count > incoming_message_count
                                || existing_latest_created_at > incoming_latest_created_at));
                    if stale_snapshot {
                        tracing::debug!(
                            thread_id = %thread.id,
                            existing_updated_at,
                            incoming_updated_at = thread.updated_at,
                            existing_message_count,
                            incoming_message_count,
                            existing_latest_created_at,
                            incoming_latest_created_at,
                            "skipping stale thread snapshot persistence"
                        );
                        return Ok(Vec::new());
                    }
                }

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

                let mut existing_stmt = transaction.prepare(
                    "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
                     FROM agent_messages WHERE thread_id = ?1",
                )?;
                let existing_rows = existing_stmt.query_map(params![&thread.id], map_agent_message)?;
                let mut existing_messages =
                    std::collections::HashMap::<String, AgentDbMessage>::new();
                for row in existing_rows {
                    let message = row?;
                    existing_messages.insert(message.id.clone(), message);
                }
                drop(existing_stmt);

                let incoming_ids = messages
                    .iter()
                    .map(|message| message.id.clone())
                    .collect::<std::collections::HashSet<_>>();

                let changed_messages =
                    messages_requiring_search_reindex(&existing_messages, &messages);
                for message in &changed_messages {
                    transaction.execute(
                        "INSERT OR REPLACE INTO agent_messages \
                         (id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
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
                            message.cost_usd,
                            message.reasoning,
                            message.tool_calls_json,
                            message.metadata_json,
                        ],
                    )?;
                }

                let stale_ids = existing_messages
                    .keys()
                    .filter(|id| !incoming_ids.contains(*id))
                    .cloned()
                    .collect::<Vec<_>>();
                if !stale_ids.is_empty() {
                    let placeholders =
                        std::iter::repeat_n("?", stale_ids.len()).collect::<Vec<_>>().join(", ");
                    let sql = format!(
                        "DELETE FROM agent_messages WHERE thread_id = ? AND id IN ({placeholders})"
                    );
                    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
                    params.push(Box::new(thread.id.clone()));
                    for id in stale_ids {
                        params.push(Box::new(id));
                    }
                    let refs: Vec<&dyn rusqlite::types::ToSql> =
                        params.iter().map(|value| value.as_ref()).collect();
                    transaction.execute(&sql, refs.as_slice())?;
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
                Ok(changed_messages)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
            .map(|changed_messages| {
                index_agent_messages(self, &changed_messages);
            })
    }

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
        let messages_for_index = messages.clone();
        self.conn
            .call(move |conn| {
                let transaction = conn.transaction()?;
                let incoming_message_count = messages.len() as i64;
                let incoming_latest_created_at = messages
                    .iter()
                    .map(|message| message.created_at)
                    .max()
                    .unwrap_or(thread.updated_at);

                let existing_snapshot = transaction
                    .query_row(
                        "SELECT
                            updated_at,
                            message_count,
                            COALESCE((
                                SELECT MAX(created_at)
                                FROM agent_messages
                                WHERE thread_id = ?1
                            ), 0)
                         FROM agent_threads
                         WHERE id = ?1",
                        params![&thread.id],
                        |row| {
                            Ok((
                                row.get::<_, i64>(0)?,
                                row.get::<_, i64>(1)?,
                                row.get::<_, i64>(2)?,
                            ))
                        },
                    )
                    .optional()?;

                if let Some((
                    existing_updated_at,
                    existing_message_count,
                    existing_latest_created_at,
                )) = existing_snapshot
                {
                    let stale_snapshot = existing_updated_at > thread.updated_at
                        || (existing_updated_at == thread.updated_at
                            && (existing_message_count > incoming_message_count
                                || existing_latest_created_at > incoming_latest_created_at));
                    if stale_snapshot {
                        tracing::debug!(
                            thread_id = %thread.id,
                            existing_updated_at,
                            incoming_updated_at = thread.updated_at,
                            existing_message_count,
                            incoming_message_count,
                            existing_latest_created_at,
                            incoming_latest_created_at,
                            "skipping stale thread snapshot replace"
                        );
                        return Ok(false);
                    }
                }

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
                         (id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
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
                            message.cost_usd,
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
                Ok(true)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
            .map(|persisted| {
                if persisted {
                    index_agent_messages(self, &messages_for_index);
                }
            })
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
        self.read_conn.call(move |conn| {
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
        self.read_conn.call(move |conn| {
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
        let search_document = agent_message_search_document(&message);
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO agent_messages \
             (id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
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
                message.cost_usd,
                message.reasoning,
                message.tool_calls_json,
                message.metadata_json,
            ],
        )?;
        refresh_thread_stats(conn, &message.thread_id)?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        self.upsert_search_document(search_document);
        Ok(())
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
                cost_usd = COALESCE(?8, cost_usd),
                reasoning = COALESCE(?9, reasoning),
                tool_calls_json = COALESCE(?10, tool_calls_json),
                metadata_json = COALESCE(?11, metadata_json)
             WHERE id = ?1",
                    params![
                        id,
                        patch.content.as_deref(),
                        flatten_option_str(&patch.provider),
                        flatten_option_str(&patch.model),
                        flatten_option_i64(&patch.input_tokens),
                        flatten_option_i64(&patch.output_tokens),
                        flatten_option_i64(&patch.total_tokens),
                        flatten_option_f64(&patch.cost_usd),
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
        self.read_conn.call(move |conn| {
            let messages = if let Some(limit) = limit {
                let limit = limit.max(1) as i64;
                let mut stmt = conn.prepare(
                    "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
                     FROM agent_messages WHERE thread_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![thread_id, limit], map_agent_message)?;
                let mut messages: Vec<AgentDbMessage> = rows.filter_map(|row| row.ok()).collect();
                messages.reverse();
                messages
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
                     FROM agent_messages WHERE thread_id = ?1 ORDER BY created_at ASC, rowid ASC",
                )?;
                let rows = stmt.query_map(params![thread_id], map_agent_message)?;
                rows.filter_map(|row| row.ok()).collect()
            };
            Ok(messages)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_message_window(
        &self,
        thread_id: &str,
        limit: usize,
        offset_from_end: usize,
    ) -> Result<(Vec<AgentDbMessage>, usize, usize, usize)> {
        let thread_id = thread_id.to_string();
        self.read_conn
            .call(move |conn| {
                let total_count = conn.query_row(
                    "SELECT COUNT(*) FROM agent_messages WHERE thread_id = ?1",
                    params![&thread_id],
                    |row| row.get::<_, i64>(0),
                )?;
                let total_count = total_count.max(0) as usize;
                let end = total_count.saturating_sub(offset_from_end);
                let start = end.saturating_sub(limit);
                if start == end {
                    return Ok((Vec::new(), total_count, start, end));
                }

                let mut stmt = conn.prepare(
                    "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
                     FROM agent_messages WHERE thread_id = ?1 ORDER BY created_at ASC, rowid ASC LIMIT ?2 OFFSET ?3",
                )?;
                let rows = stmt.query_map(
                    params![&thread_id, end.saturating_sub(start) as i64, start as i64],
                    map_agent_message,
                )?;
                Ok((
                    rows.filter_map(|row| row.ok()).collect(),
                    total_count,
                    start,
                    end,
                ))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_pinned_messages_for_compaction(
        &self,
        thread_id: &str,
    ) -> Result<Vec<(usize, AgentDbMessage)>> {
        let thread_id = thread_id.to_string();
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT absolute_index, id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
                     FROM (
                        SELECT ROW_NUMBER() OVER (ORDER BY created_at ASC, rowid ASC) - 1 AS absolute_index,
                               id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json
                        FROM agent_messages
                        WHERE thread_id = ?1
                     )
                     WHERE metadata_json IS NOT NULL
                       AND json_valid(metadata_json)
                       AND (
                         json_extract(metadata_json, '$.pinned_for_compaction') = 1
                         OR json_extract(metadata_json, '$.pinnedForCompaction') = 1
                       )
                     ORDER BY absolute_index ASC",
                )?;
                let rows = stmt.query_map(params![&thread_id], |row| {
                    Ok((
                        row.get::<_, i64>(0)?.max(0) as usize,
                        AgentDbMessage {
                            id: row.get(1)?,
                            thread_id: row.get(2)?,
                            created_at: row.get(3)?,
                            role: row.get(4)?,
                            content: row.get(5)?,
                            provider: row.get(6)?,
                            model: row.get(7)?,
                            input_tokens: row.get(8)?,
                            output_tokens: row.get(9)?,
                            total_tokens: row.get(10)?,
                            cost_usd: row.get(11)?,
                            reasoning: row.get(12)?,
                            tool_calls_json: row.get(13)?,
                            metadata_json: row.get(14)?,
                        },
                    ))
                })?;
                Ok(rows.filter_map(|row| row.ok()).collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_recent_messages(
        &self,
        thread_id: &str,
        limit: usize,
    ) -> Result<Vec<AgentDbMessage>> {
        let thread_id = thread_id.to_string();
        self.read_conn
            .call(move |conn| {
                let limit = limit.clamp(1, 1000) as i64;
                let mut stmt = conn.prepare(
                    "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
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

    pub async fn list_messages_after_cursor(
        &self,
        thread_id: &str,
        after: Option<&AgentMessageCursor>,
        limit: Option<usize>,
    ) -> Result<Vec<AgentDbMessage>> {
        let thread_id = thread_id.to_string();
        let after = after.cloned();
        self.read_conn
            .call(move |conn| {
                let messages = match (after.as_ref(), limit) {
                    (Some(cursor), Some(limit)) => {
                        let limit = limit.max(1) as i64;
                        let mut stmt = conn.prepare(
                            "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
                             FROM agent_messages \
                             WHERE thread_id = ?1 AND (created_at > ?2 OR (created_at = ?2 AND id > ?3)) \
                             ORDER BY created_at ASC, id ASC LIMIT ?4",
                        )?;
                        let rows = stmt.query_map(
                            params![thread_id, cursor.created_at, cursor.message_id, limit],
                            map_agent_message,
                        )?;
                        rows.filter_map(|row| row.ok()).collect()
                    }
                    (Some(cursor), None) => {
                        let mut stmt = conn.prepare(
                            "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
                             FROM agent_messages \
                             WHERE thread_id = ?1 AND (created_at > ?2 OR (created_at = ?2 AND id > ?3)) \
                             ORDER BY created_at ASC, id ASC",
                        )?;
                        let rows = stmt.query_map(
                            params![thread_id, cursor.created_at, cursor.message_id],
                            map_agent_message,
                        )?;
                        rows.filter_map(|row| row.ok()).collect()
                    }
                    (None, Some(limit)) => {
                        let limit = limit.max(1) as i64;
                        let mut stmt = conn.prepare(
                            "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
                             FROM agent_messages WHERE thread_id = ?1 ORDER BY created_at ASC, id ASC LIMIT ?2",
                        )?;
                        let rows = stmt.query_map(params![thread_id, limit], map_agent_message)?;
                        rows.filter_map(|row| row.ok()).collect()
                    }
                    (None, None) => {
                        let mut stmt = conn.prepare(
                            "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
                             FROM agent_messages WHERE thread_id = ?1 ORDER BY created_at ASC, id ASC",
                        )?;
                        let rows = stmt.query_map(params![thread_id], map_agent_message)?;
                        rows.filter_map(|row| row.ok()).collect()
                    }
                };
                Ok(messages)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_memory_distillation_progress(
        &self,
        source_thread_id: &str,
    ) -> Result<Option<MemoryDistillationProgressRow>> {
        let source_thread_id = source_thread_id.to_string();
        self.conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT source_thread_id, last_processed_created_at_ms, last_processed_message_id, last_processed_span_json, last_run_at_ms, updated_at_ms, agent_id \
                     FROM memory_distillation_progress WHERE source_thread_id = ?1",
                    params![source_thread_id],
                    map_memory_distillation_progress_row,
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_memory_distillation_progress(
        &self,
        progress: &MemoryDistillationProgressRow,
    ) -> Result<()> {
        let progress = progress.clone();
        let last_processed_span_json = progress
            .last_processed_span
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO memory_distillation_progress \
                     (source_thread_id, last_processed_created_at_ms, last_processed_message_id, last_processed_span_json, last_run_at_ms, updated_at_ms, agent_id) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
                     ON CONFLICT(source_thread_id) DO UPDATE SET \
                        last_processed_created_at_ms = excluded.last_processed_created_at_ms, \
                        last_processed_message_id = excluded.last_processed_message_id, \
                        last_processed_span_json = excluded.last_processed_span_json, \
                        last_run_at_ms = excluded.last_run_at_ms, \
                        updated_at_ms = excluded.updated_at_ms, \
                        agent_id = excluded.agent_id",
                    params![
                        progress.source_thread_id,
                        progress.last_processed_cursor.created_at,
                        progress.last_processed_cursor.message_id,
                        last_processed_span_json,
                        progress.last_run_at_ms,
                        progress.updated_at_ms,
                        progress.agent_id,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn append_memory_distillation_log(
        &self,
        source_thread_id: &str,
        source_message_range: Option<&str>,
        source_message_span: Option<&AgentMessageSpan>,
        distilled_fact: &str,
        target_file: &str,
        category: &str,
        confidence: f64,
        created_at_ms: i64,
        applied_to_memory: bool,
        agent_id: &str,
    ) -> Result<()> {
        let source_thread_id = source_thread_id.to_string();
        let source_message_range = source_message_range.map(str::to_string);
        let source_message_span_json =
            source_message_span.map(serde_json::to_string).transpose()?;
        let distilled_fact = distilled_fact.to_string();
        let target_file = target_file.to_string();
        let category = category.to_string();
        let applied_flag = if applied_to_memory { 1_i64 } else { 0_i64 };
        let agent_id = agent_id.to_string();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO memory_distillation_log \
                     (source_thread_id, source_message_range, source_message_span_json, distilled_fact, target_file, category, confidence, created_at_ms, applied_to_memory, agent_id) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        source_thread_id,
                        source_message_range,
                        source_message_span_json,
                        distilled_fact,
                        target_file,
                        category,
                        confidence,
                        created_at_ms,
                        applied_flag,
                        agent_id,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_memory_distillation_log(
        &self,
        limit: usize,
    ) -> Result<Vec<MemoryDistillationLogRow>> {
        let limit = limit.max(1) as i64;
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, source_thread_id, source_message_range, source_message_span_json, distilled_fact, target_file, category, confidence, created_at_ms, applied_to_memory, agent_id \
                     FROM memory_distillation_log ORDER BY created_at_ms DESC, id DESC LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit], map_memory_distillation_log_row)?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_memory_distillation_progress(
        &self,
        limit: usize,
    ) -> Result<Vec<MemoryDistillationProgressRow>> {
        let limit = limit.max(1) as i64;
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT source_thread_id, last_processed_created_at_ms, last_processed_message_id, last_processed_span_json, last_run_at_ms, updated_at_ms, agent_id \
                     FROM memory_distillation_progress ORDER BY updated_at_ms DESC, source_thread_id ASC LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit], map_memory_distillation_progress_row)?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_forge_pass_log(&self, limit: usize) -> Result<Vec<ForgePassLogRow>> {
        let limit = limit.max(1) as i64;
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, agent_id, period_start_ms, period_end_ms, traces_analyzed, patterns_found, hints_applied, hints_logged, completed_at_ms \
                     FROM forge_pass_log ORDER BY completed_at_ms DESC, id DESC LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit], map_forge_pass_log_row)?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_recent_handoff_routing(
        &self,
        limit: usize,
    ) -> Result<Vec<HandoffRoutingRow>> {
        let limit = limit.max(1) as i64;
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, to_specialist_id, capability_tags_json, routing_method, routing_score, fallback_used, created_at \
                     FROM handoff_log ORDER BY created_at DESC, id DESC LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit], map_handoff_routing_row)?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
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
