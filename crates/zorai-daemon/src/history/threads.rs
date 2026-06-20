use super::*;

fn now_millis_i64() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ThreadMessagePinState {
    pub message_chars: usize,
    pub pinned_for_compaction: bool,
    pub counts_toward_pinned_chars: bool,
    pub current_pinned_chars: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ThreadUserPacing {
    pub recent_message_count: u32,
    pub avg_gap_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MessageFeedbackState {
    pub role: String,
    pub reaction: Option<String>,
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
    fn message_fixture_sets_thread_id() {
        assert_eq!(message("m1", "body").thread_id, "thread-1");
    }
}

impl HistoryStore {
    /// Returns the highest `created_at` already persisted for this thread,
    /// or `None` if the thread has no rows yet. Used by
    /// `persist_thread_snapshot` to skip rebuilding `AgentDbMessage` (with
    /// metadata_json / tool_calls_json serialization) for messages that are
    /// already on disk. Without this, persisting a 3,500-message thread
    /// after a single new turn re-encoded every old message's JSON twice
    /// (once on the Rust side, once via `INSERT OR REPLACE` in
    /// `reconcile_thread_snapshot`).
    pub(crate) async fn thread_latest_persisted_message_ts(
        &self,
        thread_id: &str,
    ) -> Result<Option<i64>> {
        let thread_id = thread_id.to_string();
        self.interactive_read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT MAX(created_at) FROM agent_messages \
                     WHERE thread_id = ?1 AND deleted_at IS NULL",
                    params![&thread_id],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn reconcile_thread_snapshot(
        &self,
        thread: &AgentDbThread,
        messages: &[AgentDbMessage],
    ) -> Result<()> {
        self.caches.thread_metadata_json.invalidate(&thread.id);
        let thread = thread.clone();
        let messages = messages.to_vec();
        self.conn
            .call(move |conn| {
                let transaction = conn.transaction()?;
                // Use the thread row's full message count, not
                // `messages.len()` — the caller filters the message
                // batch by a cutoff timestamp, so a re-persist that only
                // carries metadata changes has `messages` near zero and
                // would spuriously trip the stale-snapshot gate even
                // though the conversation has not shrunk.
                let incoming_message_count = thread.message_count;
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
                                WHERE thread_id = ?1 AND deleted_at IS NULL
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
                        return Ok(());
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

                let cutoff_ts = existing_snapshot
                    .map(|(_, _, latest)| latest)
                    .unwrap_or(i64::MIN);
                let mut written = 0usize;
                for message in &messages {
                    if message.created_at < cutoff_ts {
                        continue;
                    }
                    transaction.execute(
                        "INSERT OR REPLACE INTO agent_messages \
                         (id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json, deleted_at) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, NULL)",
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
                    embedding_queue::enqueue_message_embedding_job(
                        &transaction,
                        message,
                        thread.workspace_id.as_deref(),
                        now_ts() as i64,
                    )?;
                    written += 1;
                }
                tracing::debug!(
                    thread_id = %thread.id,
                    in_memory_count = messages.len(),
                    written_count = written,
                    cutoff_ts,
                    "thread snapshot persisted (incremental)"
                );

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

    pub async fn create_thread(&self, thread: &AgentDbThread) -> Result<()> {
        self.caches.thread_metadata_json.invalidate(&thread.id);
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
        self.caches.thread_metadata_json.invalidate(&thread.id);
        let thread = thread.clone();
        let messages = messages.to_vec();
        self.conn
            .call(move |conn| {
                let transaction = conn.transaction()?;
                // Use the thread row's full message count, not
                // `messages.len()` — the caller filters the message
                // batch by a cutoff timestamp, so a re-persist that only
                // carries metadata changes has `messages` near zero and
                // would spuriously trip the stale-snapshot gate even
                // though the conversation has not shrunk.
                let incoming_message_count = thread.message_count;
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
                                WHERE thread_id = ?1 AND deleted_at IS NULL
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
                        return Ok(());
                    }
                }

                let mut existing_stmt = transaction.prepare(
                    "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
                     FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL",
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
                let stale_ids = existing_messages
                    .keys()
                    .filter(|id| !incoming_ids.contains(*id))
                    .cloned()
                    .collect::<Vec<_>>();

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

                if !stale_ids.is_empty() {
                    let placeholders =
                        std::iter::repeat_n("?", stale_ids.len()).collect::<Vec<_>>().join(", ");
                    let sql = format!(
                        "UPDATE agent_messages SET deleted_at = ? WHERE thread_id = ? AND deleted_at IS NULL AND id IN ({placeholders})"
                    );
                    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
                    params.push(Box::new(now_millis_i64()));
                    params.push(Box::new(thread.id.clone()));
                    for id in &stale_ids {
                        params.push(Box::new(id.clone()));
                    }
                    let refs: Vec<&dyn rusqlite::types::ToSql> =
                        params.iter().map(|value| value.as_ref()).collect();
                    transaction.execute(&sql, refs.as_slice())?;
                    for id in &stale_ids {
                        embedding_queue::queue_embedding_deletion_on_connection(
                            &transaction,
                            "agent_message",
                            id,
                            now_ts() as i64,
                        )?;
                    }
                }

                for message in &messages {
                    transaction.execute(
                        "INSERT OR REPLACE INTO agent_messages \
                         (id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json, deleted_at) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, NULL)",
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
                    embedding_queue::enqueue_message_embedding_job(
                        &transaction,
                        message,
                        thread.workspace_id.as_deref(),
                        now_ts() as i64,
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
        self.caches.thread_metadata_json.invalidate(&id.to_string());
        let id = id.to_string();
        self.conn
            .call(move |conn| {
                let deleted_at = now_millis_i64();
                let message_ids = {
                    let mut stmt = conn.prepare(
                        "SELECT id FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL",
                    )?;
                    let rows = stmt.query_map(params![id], |row| row.get::<_, String>(0))?;
                    rows.collect::<std::result::Result<Vec<_>, _>>()?
                };
                embedding_queue::queue_embedding_deletions_on_connection(
                    conn,
                    "agent_message",
                    &message_ids,
                    now_ts() as i64,
                )?;
                conn.execute(
                    "UPDATE agent_threads SET deleted_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
                    params![id, deleted_at],
                )?;
                conn.execute(
                    "UPDATE agent_messages SET deleted_at = ?2 WHERE thread_id = ?1 AND deleted_at IS NULL",
                    params![id, deleted_at],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_threads(&self) -> Result<Vec<AgentDbThread>> {
        self.interactive_read_conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview, metadata_json \
             FROM agent_threads WHERE deleted_at IS NULL ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], map_agent_thread)?;
        Ok(rows.filter_map(|row| row.ok()).collect())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn list_threads_filtered(
        &self,
        query: &AgentThreadListQuery,
    ) -> Result<Vec<AgentDbThread>> {
        let query = query.clone();
        self.interactive_read_conn
            .call(move |conn| {
                let mut sql = String::from(
                    "SELECT id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview, metadata_json \
                     FROM agent_threads t WHERE t.deleted_at IS NULL",
                );
                let mut values = Vec::<rusqlite::types::Value>::new();

                if let Some(created_after) = query.created_after {
                    sql.push_str(" AND t.created_at >= ?");
                    values.push(rusqlite::types::Value::Integer(created_after));
                }
                if let Some(created_before) = query.created_before {
                    sql.push_str(" AND t.created_at <= ?");
                    values.push(rusqlite::types::Value::Integer(created_before));
                }
                if let Some(updated_after) = query.updated_after {
                    sql.push_str(" AND t.updated_at >= ?");
                    values.push(rusqlite::types::Value::Integer(updated_after));
                }
                if let Some(updated_before) = query.updated_before {
                    sql.push_str(" AND t.updated_at <= ?");
                    values.push(rusqlite::types::Value::Integer(updated_before));
                }
                if let Some(title_query) = query
                    .title_query
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    sql.push_str(" AND instr(lower(t.title), lower(?)) > 0");
                    values.push(rusqlite::types::Value::Text(title_query.to_string()));
                }
                for prefix in &query.title_excluded_prefixes {
                    sql.push_str(" AND t.title NOT LIKE ?");
                    values.push(rusqlite::types::Value::Text(format!("{prefix}%")));
                }
                if let Some(min_message_count) = query.min_message_count {
                    sql.push_str(" AND t.message_count >= ?");
                    values.push(rusqlite::types::Value::Integer(min_message_count.max(0)));
                }
                if let Some(pinned) = query.pinned {
                    sql.push_str(" AND t.pinned = ?");
                    values.push(rusqlite::types::Value::Integer(if pinned { 1 } else { 0 }));
                }
                if !query.agent_names.is_empty() || query.include_empty_agent_name {
                    let mut clauses = Vec::new();
                    if query.include_empty_agent_name {
                        clauses.push("(t.agent_name IS NULL OR trim(t.agent_name) = '')".to_string());
                    }
                    if !query.agent_names.is_empty() {
                        let placeholders = std::iter::repeat_n("?", query.agent_names.len())
                            .collect::<Vec<_>>()
                            .join(", ");
                        clauses.push(format!("lower(trim(t.agent_name)) IN ({placeholders})"));
                        values.extend(query.agent_names.iter().map(|name| {
                            rusqlite::types::Value::Text(name.trim().to_ascii_lowercase())
                        }));
                    }
                    sql.push_str(" AND (");
                    sql.push_str(&clauses.join(" OR "));
                    sql.push(')');
                }
                if !query.excluded_ids.is_empty() {
                    let placeholders = std::iter::repeat_n("?", query.excluded_ids.len())
                        .collect::<Vec<_>>()
                        .join(", ");
                    sql.push_str(&format!(" AND t.id NOT IN ({placeholders})"));
                    values.extend(
                        query
                            .excluded_ids
                            .iter()
                            .map(|id| rusqlite::types::Value::Text(id.to_string())),
                    );
                }
                if !query.include_internal {
                    for prefix in &query.hidden_id_prefixes {
                        sql.push_str(" AND t.id NOT LIKE ?");
                        values.push(rusqlite::types::Value::Text(format!("{prefix}%")));
                    }
                    if !query.hidden_message_substrings.is_empty() {
                        sql.push_str(
                            " AND NOT EXISTS (
                                SELECT 1 FROM agent_messages m
                                WHERE m.thread_id = t.id
                                  AND m.deleted_at IS NULL
                                  AND (",
                        );
                        let clauses = std::iter::repeat_n(
                            "instr(lower(m.content), lower(?)) > 0",
                            query.hidden_message_substrings.len(),
                        )
                        .collect::<Vec<_>>()
                        .join(" OR ");
                        sql.push_str(&clauses);
                        sql.push_str("))");
                        values.extend(query.hidden_message_substrings.iter().map(|substring| {
                            rusqlite::types::Value::Text(substring.to_string())
                        }));
                    }
                }

                sql.push_str(" ORDER BY t.updated_at DESC, t.id ASC");
                if let Some(limit) = query.limit {
                    sql.push_str(" LIMIT ?");
                    values.push(rusqlite::types::Value::Integer(limit as i64));
                    if query.offset > 0 {
                        sql.push_str(" OFFSET ?");
                        values.push(rusqlite::types::Value::Integer(query.offset as i64));
                    }
                } else if query.offset > 0 {
                    sql.push_str(" LIMIT -1 OFFSET ?");
                    values.push(rusqlite::types::Value::Integer(query.offset as i64));
                }

                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), map_agent_thread)?;
                Ok(rows.filter_map(|row| row.ok()).collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_thread(&self, id: &str) -> Result<Option<AgentDbThread>> {
        let id = id.to_string();
        self.interactive_read_conn.call(move |conn| {
        conn
            .query_row(
                "SELECT id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview, metadata_json \
                 FROM agent_threads WHERE id = ?1 AND deleted_at IS NULL",
                params![id],
                map_agent_thread,
            )
            .optional()
            .map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn thread_recall_match_rows(
        &self,
        tokens: &[String],
        thread_limit: usize,
    ) -> Result<Vec<ThreadRecallMatchRow>> {
        let tokens = tokens
            .iter()
            .map(|token| token.trim().to_ascii_lowercase())
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>();
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let thread_limit = thread_limit.max(1) as i64;

        self.read_conn
            .call(move |conn| {
                let message_clauses = std::iter::repeat_n("lower(m.content) LIKE ?", tokens.len())
                    .collect::<Vec<_>>()
                    .join(" OR ");
                let exists_clauses =
                    std::iter::repeat_n("lower(candidate.content) LIKE ?", tokens.len())
                        .collect::<Vec<_>>()
                        .join(" OR ");
                let title_clauses = std::iter::repeat_n("lower(t.title) LIKE ?", tokens.len())
                    .collect::<Vec<_>>()
                    .join(" OR ");
                let sql = format!(
                    "WITH matched_threads AS (
                         SELECT t.id
                           FROM agent_threads t
                          WHERE t.deleted_at IS NULL
                            AND (
                                  {title_clauses}
                                  OR EXISTS (
                                      SELECT 1
                                        FROM agent_messages candidate
                                       WHERE candidate.thread_id = t.id
                                         AND candidate.deleted_at IS NULL
                                         AND ({exists_clauses})
                                  )
                            )
                          ORDER BY t.updated_at DESC, t.id ASC
                          LIMIT ?
                     )
                     SELECT t.id,
                            t.title,
                            t.updated_at,
                            t.message_count,
                            t.metadata_json,
                            m.role,
                            m.content
                       FROM matched_threads mt
                       JOIN agent_threads t ON t.id = mt.id
                       LEFT JOIN agent_messages m
                         ON m.thread_id = t.id
                        AND m.deleted_at IS NULL
                        AND ({message_clauses})
                      ORDER BY t.updated_at DESC, t.id ASC, m.created_at DESC, m.rowid DESC"
                );
                let mut values = Vec::<rusqlite::types::Value>::new();
                values.extend(
                    tokens
                        .iter()
                        .map(|token| rusqlite::types::Value::Text(format!("%{token}%"))),
                );
                values.extend(
                    tokens
                        .iter()
                        .map(|token| rusqlite::types::Value::Text(format!("%{token}%"))),
                );
                values.push(rusqlite::types::Value::Integer(thread_limit));
                values.extend(
                    tokens
                        .iter()
                        .map(|token| rusqlite::types::Value::Text(format!("%{token}%"))),
                );

                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
                    Ok(ThreadRecallMatchRow {
                        thread_id: row.get(0)?,
                        title: row.get(1)?,
                        updated_at: row.get::<_, i64>(2)?.max(0) as u64,
                        message_count: row.get::<_, i64>(3)?.max(0) as u32,
                        metadata_json: row.get(4)?,
                        message_role: row.get(5)?,
                        message_content: row.get(6)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn has_thread_id(&self, id: &str) -> Result<bool> {
        let id = id.to_string();
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT 1 FROM agent_threads WHERE id = ?1 AND deleted_at IS NULL LIMIT 1",
                )?;
                match stmt.query_row(params![id], |_| Ok(())) {
                    Ok(()) => Ok(true),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
                    Err(error) => Err(error.into()),
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn thread_created_at(&self, id: &str) -> Result<Option<u64>> {
        let id = id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT created_at FROM agent_threads WHERE id = ?1 AND deleted_at IS NULL",
                    params![id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map(|value| value.map(|created_at| created_at.max(0) as u64))
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn thread_metadata_json(&self, id: &str) -> Result<Option<String>> {
        let id_owned = id.to_string();
        if let Some(cached) = self.caches.thread_metadata_json.get(&id_owned) {
            return Ok(cached);
        }
        let id_for_query = id_owned.clone();
        let value: Option<String> = self
            .read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT metadata_json FROM agent_threads WHERE id = ?1 AND deleted_at IS NULL",
                    params![id_for_query],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()
                .map(Option::flatten)
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        self.caches
            .thread_metadata_json
            .insert(id_owned, value.clone());
        Ok(value)
    }

    pub(crate) async fn thread_delegate_payload_context(
        &self,
        id: &str,
        message_limit: usize,
    ) -> Result<Option<ThreadDelegatePayloadContext>> {
        let id = id.to_string();
        let message_limit = message_limit.min(i64::MAX as usize) as i64;
        self.read_conn
            .call(move |conn| {
                let Some(title) = conn
                    .query_row(
                        "SELECT title FROM agent_threads WHERE id = ?1 AND deleted_at IS NULL",
                        params![id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
                else {
                    return Ok(None);
                };

                let messages = if message_limit == 0 {
                    Vec::new()
                } else {
                    let mut stmt = conn.prepare(
                        "SELECT role, content FROM (\
                         SELECT role, content, created_at, rowid \
                         FROM agent_messages \
                         WHERE thread_id = ?1 AND deleted_at IS NULL \
                         ORDER BY created_at DESC, rowid DESC \
                         LIMIT ?2\
                         ) ORDER BY created_at ASC, rowid ASC",
                    )?;
                    let rows = stmt.query_map(params![id, message_limit], |row| {
                        Ok(ThreadDelegatePayloadMessageRef {
                            role: row.get(0)?,
                            content: row.get(1)?,
                        })
                    })?;
                    rows.collect::<std::result::Result<Vec<_>, _>>()?
                };

                Ok(Some(ThreadDelegatePayloadContext { title, messages }))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn has_non_heartbeat_user_message_after(
        &self,
        timestamp_ms: u64,
    ) -> Result<bool> {
        let timestamp_ms = timestamp_ms.min(i64::MAX as u64) as i64;
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT EXISTS (
                        SELECT 1
                        FROM agent_messages message
                        JOIN agent_threads thread
                          ON thread.id = message.thread_id
                         AND thread.deleted_at IS NULL
                        WHERE message.role = 'user'
                          AND message.deleted_at IS NULL
                          AND message.created_at > ?1
                          AND thread.title NOT GLOB 'HEARTBEAT SYNTHESIS*'
                          AND thread.title NOT GLOB 'Heartbeat check:*'
                          AND NOT EXISTS (
                            SELECT 1
                            FROM agent_messages heartbeat
                            WHERE heartbeat.thread_id = message.thread_id
                              AND heartbeat.role = 'user'
                              AND heartbeat.deleted_at IS NULL
                              AND (
                                heartbeat.content GLOB 'HEARTBEAT SYNTHESIS*'
                                OR heartbeat.content GLOB 'Heartbeat check:*'
                              )
                          )
                        LIMIT 1
                    )",
                    params![timestamp_ms],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn thread_has_unanswered_tool_calls(&self, id: &str) -> Result<bool> {
        let owned = vec![id.to_string()];
        let unanswered = self.thread_ids_with_unanswered_tool_calls(&owned).await?;
        Ok(!unanswered.is_empty())
    }

    pub(crate) async fn thread_ids_with_unanswered_tool_calls(
        &self,
        thread_ids: &[String],
    ) -> Result<Vec<String>> {
        if thread_ids.is_empty() {
            return Ok(Vec::new());
        }

        let thread_ids_query = thread_ids.to_vec();
        self.read_conn
            .call(move |conn| {
                let placeholders = std::iter::repeat("?")
                    .take(thread_ids_query.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    "SELECT
                         thread_id,
                         role,
                         CASE
                             WHEN role = 'assistant'
                                  AND typeof(tool_calls_json) = 'text'
                                  AND json_valid(tool_calls_json)
                                  AND json_array_length(tool_calls_json) > 0
                             THEN tool_calls_json
                             ELSE NULL
                         END AS tool_calls_json,
                         CASE
                             WHEN role = 'tool'
                                  AND typeof(metadata_json) = 'text'
                                  AND json_valid(metadata_json)
                             THEN COALESCE(
                                 json_extract(metadata_json, '$.tool_call_id'),
                                 json_extract(metadata_json, '$.toolCallId')
                             )
                             ELSE NULL
                         END AS tool_call_id
                     FROM agent_messages
                     WHERE thread_id IN ({placeholders})
                       AND deleted_at IS NULL
                     ORDER BY thread_id ASC, created_at ASC, id ASC"
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows =
                    stmt.query_map(rusqlite::params_from_iter(thread_ids_query.iter()), |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                        ))
                    })?;

                let mut unanswered: Vec<String> = Vec::new();
                let mut current_thread: Option<String> = None;
                let mut buffer: Vec<(String, Option<String>, Option<String>)> = Vec::new();
                for row in rows {
                    let (thread_id, role, tool_calls_json, tool_call_id) = row?;
                    if current_thread.as_deref() != Some(thread_id.as_str()) {
                        if let Some(prev_id) = current_thread.take() {
                            if scan_messages_for_unanswered_tool_calls(&buffer) {
                                unanswered.push(prev_id);
                            }
                        }
                        current_thread = Some(thread_id);
                        buffer.clear();
                    }
                    buffer.push((role, tool_calls_json, tool_call_id));
                }
                if let Some(prev_id) = current_thread.take() {
                    if scan_messages_for_unanswered_tool_calls(&buffer) {
                        unanswered.push(prev_id);
                    }
                }
                Ok(unanswered)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn latest_thread_id_by_message_timestamp(&self) -> Result<Option<String>> {
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT m.thread_id
                     FROM agent_messages m
                     JOIN agent_threads t
                       ON t.id = m.thread_id AND t.deleted_at IS NULL
                     WHERE m.deleted_at IS NULL
                     ORDER BY m.created_at DESC, m.id DESC
                     LIMIT 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn latest_thread_context_hint(&self) -> Result<Option<(String, String, u64)>> {
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT id, trim(last_preview), updated_at
                     FROM agent_threads
                     WHERE deleted_at IS NULL
                       AND message_count > 0
                       AND trim(last_preview) <> ''
                     ORDER BY updated_at DESC, id ASC
                     LIMIT 1",
                    [],
                    |row| {
                        let updated_at = row.get::<_, i64>(2)?.max(0) as u64;
                        Ok((row.get(0)?, row.get(1)?, updated_at))
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn thread_has_message_substring(
        &self,
        thread_id: &str,
        substrings: &[String],
    ) -> Result<bool> {
        if substrings.is_empty() {
            return Ok(false);
        }

        let thread_id = thread_id.to_string();
        let substrings = substrings
            .iter()
            .map(|value| value.to_lowercase())
            .collect::<Vec<_>>();
        self.interactive_read_conn
            .call(move |conn| {
                let predicates =
                    std::iter::repeat_n("instr(lower(content), ?) > 0", substrings.len())
                        .collect::<Vec<_>>()
                        .join(" OR ");
                let sql = format!(
                    "SELECT EXISTS(
                         SELECT 1
                         FROM agent_messages
                         WHERE thread_id = ?
                           AND deleted_at IS NULL
                           AND ({predicates})
                         LIMIT 1
                     )"
                );
                let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
                    vec![Box::new(thread_id) as Box<dyn rusqlite::types::ToSql>];
                for substring in substrings {
                    params.push(Box::new(substring));
                }
                let param_refs = params
                    .iter()
                    .map(|param| param.as_ref())
                    .collect::<Vec<&dyn rusqlite::types::ToSql>>();
                conn.query_row(&sql, param_refs.as_slice(), |row| {
                    Ok(row.get::<_, i64>(0)? != 0)
                })
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn latest_non_empty_message_content_for_thread_ids(
        &self,
        thread_ids: &[String],
    ) -> Result<std::collections::HashMap<String, String>> {
        if thread_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let thread_ids = thread_ids.to_vec();
        self.read_conn
            .call(move |conn| {
                let placeholders = std::iter::repeat("?")
                    .take(thread_ids.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    "WITH ranked_messages AS (
                        SELECT
                            message.thread_id,
                            message.content,
                            ROW_NUMBER() OVER (
                                PARTITION BY message.thread_id
                                ORDER BY message.created_at DESC, message.rowid DESC
                            ) AS row_number
                        FROM agent_messages message
                        JOIN agent_threads thread
                          ON thread.id = message.thread_id
                         AND thread.deleted_at IS NULL
                        WHERE message.thread_id IN ({placeholders})
                          AND message.deleted_at IS NULL
                          AND TRIM(
                            message.content,
                            char(9) || char(10) || char(11) || char(12) || char(13) || char(32)
                          ) != ''
                     )
                     SELECT thread_id, content
                     FROM ranked_messages
                     WHERE row_number = 1"
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(rusqlite::params_from_iter(thread_ids.iter()), |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })?;
                let mut messages = std::collections::HashMap::new();
                for row in rows {
                    let (thread_id, content) = row?;
                    messages.insert(thread_id, content);
                }
                Ok(messages)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn gateway_approval_ids_for_thread(
        &self,
        thread_id: &str,
    ) -> Result<Vec<String>> {
        let thread_id = thread_id.to_string();
        self.read_conn
            .call(move |conn| {
                let whitespace =
                    "char(9) || char(10) || char(11) || char(12) || char(13) || char(32)";
                let sql = format!(
                    "WITH matching_messages AS (
                        SELECT
                            message.created_at,
                            message.rowid,
                            TRIM(
                                substr(
                                    message.content,
                                    instr(message.content, 'Approval ID:') + length('Approval ID:')
                                ),
                                {whitespace}
                            ) AS remainder
                        FROM agent_messages message
                        JOIN agent_threads thread
                          ON thread.id = message.thread_id
                         AND thread.deleted_at IS NULL
                        WHERE message.thread_id = ?1
                          AND message.deleted_at IS NULL
                          AND instr(message.content, 'Approval ID:') > 0
                     ),
                     normalized_messages AS (
                        SELECT
                            created_at,
                            rowid,
                            replace(
                                replace(
                                    replace(
                                        replace(
                                            replace(remainder, char(9), ' '),
                                            char(10),
                                            ' '
                                        ),
                                        char(11),
                                        ' '
                                    ),
                                    char(12),
                                    ' '
                                ),
                                char(13),
                                ' '
                            ) AS remainder
                        FROM matching_messages
                        WHERE remainder != ''
                     )
                     SELECT
                        CASE
                            WHEN instr(remainder, ' ') > 0
                            THEN substr(remainder, 1, instr(remainder, ' ') - 1)
                            ELSE remainder
                        END AS approval_id
                     FROM normalized_messages
                     WHERE approval_id != ''
                     ORDER BY created_at DESC, rowid DESC"
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params![thread_id], |row| row.get::<_, String>(0))?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn gateway_turn_auto_send_projection(
        &self,
        thread_id: &str,
    ) -> Result<Option<GatewayTurnAutoSendProjection>> {
        let thread_id = thread_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "WITH turn_start AS (
                        SELECT message.created_at, message.rowid AS row_id
                        FROM agent_messages message
                        JOIN agent_threads thread
                          ON thread.id = message.thread_id
                         AND thread.deleted_at IS NULL
                        WHERE message.thread_id = ?1
                          AND message.deleted_at IS NULL
                          AND message.role = 'user'
                        ORDER BY message.created_at DESC, message.rowid DESC
                        LIMIT 1
                     ),
                     turn_messages AS (
                        SELECT message.*, message.rowid AS row_id
                        FROM agent_messages message
                        JOIN agent_threads thread
                          ON thread.id = message.thread_id
                         AND thread.deleted_at IS NULL
                        JOIN turn_start start
                        WHERE message.thread_id = ?1
                          AND message.deleted_at IS NULL
                          AND (
                            message.created_at > start.created_at
                            OR (
                                message.created_at = start.created_at
                                AND message.rowid >= start.row_id
                            )
                          )
                     ),
                     latest_assistant AS (
                        SELECT
                            message.created_at,
                            message.row_id,
                            message.tool_calls_json
                        FROM turn_messages message
                        WHERE message.role = 'assistant'
                          AND (
                            message.content != ''
                            OR (
                                message.tool_calls_json IS NOT NULL
                                AND json_valid(message.tool_calls_json)
                                AND json_array_length(message.tool_calls_json) > 0
                            )
                          )
                        ORDER BY message.created_at DESC, message.row_id DESC
                        LIMIT 1
                     ),
                     latest_response AS (
                        SELECT message.content
                        FROM turn_messages message
                        WHERE message.role = 'assistant'
                          AND message.content != ''
                        ORDER BY message.created_at DESC, message.row_id DESC
                        LIMIT 1
                     ),
                     assistant_send_call AS (
                        SELECT 1
                        FROM latest_assistant assistant,
                             json_each(
                                CASE
                                    WHEN assistant.tool_calls_json IS NOT NULL
                                     AND json_valid(assistant.tool_calls_json)
                                    THEN assistant.tool_calls_json
                                    ELSE '[]'
                                END
                             ) call
                        WHERE substr(
                            COALESCE(json_extract(call.value, '$.function.name'), ''),
                            1,
                            5
                        ) = 'send_'
                        LIMIT 1
                     ),
                     send_tool_after_latest_assistant AS (
                        SELECT 1
                        FROM turn_messages message
                        JOIN latest_assistant assistant
                        WHERE message.role = 'tool'
                          AND (
                            message.created_at > assistant.created_at
                            OR (
                                message.created_at = assistant.created_at
                                AND message.row_id > assistant.row_id
                            )
                          )
                          AND substr(
                            COALESCE(
                                json_extract(
                                    CASE
                                        WHEN message.metadata_json IS NOT NULL
                                         AND json_valid(message.metadata_json)
                                        THEN message.metadata_json
                                        ELSE '{}'
                                    END,
                                    '$.tool_name'
                                ),
                                json_extract(
                                    CASE
                                        WHEN message.metadata_json IS NOT NULL
                                         AND json_valid(message.metadata_json)
                                        THEN message.metadata_json
                                        ELSE '{}'
                                    END,
                                    '$.toolName'
                                ),
                                ''
                            ),
                            1,
                            5
                          ) = 'send_'
                        LIMIT 1
                     ),
                     send_tool_without_assistant AS (
                        SELECT 1
                        FROM turn_messages message
                        WHERE NOT EXISTS (SELECT 1 FROM latest_assistant)
                          AND message.role = 'tool'
                          AND substr(
                            COALESCE(
                                json_extract(
                                    CASE
                                        WHEN message.metadata_json IS NOT NULL
                                         AND json_valid(message.metadata_json)
                                        THEN message.metadata_json
                                        ELSE '{}'
                                    END,
                                    '$.tool_name'
                                ),
                                json_extract(
                                    CASE
                                        WHEN message.metadata_json IS NOT NULL
                                         AND json_valid(message.metadata_json)
                                        THEN message.metadata_json
                                        ELSE '{}'
                                    END,
                                    '$.toolName'
                                ),
                                ''
                            ),
                            1,
                            5
                          ) = 'send_'
                        LIMIT 1
                     )
                     SELECT
                        CASE
                            WHEN EXISTS (SELECT 1 FROM assistant_send_call)
                              OR EXISTS (SELECT 1 FROM send_tool_after_latest_assistant)
                              OR EXISTS (SELECT 1 FROM send_tool_without_assistant)
                            THEN 1
                            ELSE 0
                        END AS used_send_tool,
                        (SELECT content FROM latest_response) AS latest_assistant_response
                     WHERE EXISTS (SELECT 1 FROM turn_start)",
                    params![thread_id],
                    |row| {
                        Ok(GatewayTurnAutoSendProjection {
                            used_send_tool: row.get::<_, i64>(0)? != 0,
                            latest_assistant_response: row.get(1)?,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn thread_message_count(&self, thread_id: &str) -> Result<Option<usize>> {
        let thread_id = thread_id.to_string();
        self.read_conn
            .call(move |conn| {
                let count: Option<i64> = conn.query_row(
                    "SELECT CASE
                         WHEN EXISTS(
                             SELECT 1 FROM agent_threads
                             WHERE id = ?1 AND deleted_at IS NULL
                         )
                         THEN (
                             SELECT COUNT(*) FROM agent_messages
                             WHERE thread_id = ?1 AND deleted_at IS NULL
                         )
                         ELSE NULL
                     END",
                    params![thread_id],
                    |row| row.get::<_, Option<i64>>(0),
                )?;
                Ok(count.map(|value| value.max(0) as usize))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn update_thread_metadata_json(
        &self,
        id: &str,
        metadata_json: Option<String>,
    ) -> Result<()> {
        let id_owned = id.to_string();
        self.caches.thread_metadata_json.invalidate(&id_owned);
        let id_for_query = id_owned;
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE agent_threads SET metadata_json = ?2 WHERE id = ?1 AND deleted_at IS NULL",
                    params![id_for_query, metadata_json],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn add_message(&self, message: &AgentDbMessage) -> Result<()> {
        let message = message.clone();
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO agent_messages \
             (id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json, deleted_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, NULL)",
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
        let workspace_id = conn
            .query_row(
                "SELECT workspace_id FROM agent_threads WHERE id = ?1",
                params![message.thread_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();
        embedding_queue::enqueue_message_embedding_job(
            conn,
            &message,
            workspace_id.as_deref(),
            now_ts() as i64,
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
                let content_changed = patch.content.is_some();

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
                    if content_changed {
                        let message = conn
                            .query_row(
                                "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
                                 FROM agent_messages WHERE id = ?1",
                                params![&id],
                                map_agent_message,
                            )
                            .optional()?;
                        if let Some(message) = message {
                            let workspace_id = conn
                                .query_row(
                                    "SELECT workspace_id FROM agent_threads WHERE id = ?1",
                                    params![&thread_id],
                                    |row| row.get::<_, Option<String>>(0),
                                )
                                .optional()?
                                .flatten();
                            embedding_queue::enqueue_message_embedding_job(
                                conn,
                                &message,
                                workspace_id.as_deref(),
                                now_ts() as i64,
                            )?;
                        }
                    }
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
        let deleted_ids = self.conn
            .call(move |conn| {
                let placeholders: Vec<String> =
                    message_ids.iter().map(|_| "?".to_string()).collect();
                let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
                params.push(Box::new(thread_id.to_string()));
                for id in &message_ids {
                    params.push(Box::new(id.to_string()));
                }
                let refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();

                let select_sql = format!(
                    "SELECT id FROM agent_messages WHERE thread_id = ? AND deleted_at IS NULL AND id IN ({})",
                    placeholders.join(", ")
                );
                let mut stmt = conn.prepare(&select_sql)?;
                let rows = stmt.query_map(refs.as_slice(), |row| row.get::<_, String>(0))?;
                let deleted_ids = rows.collect::<std::result::Result<Vec<_>, _>>()?;
                drop(stmt);
                if deleted_ids.is_empty() {
                    return Ok(deleted_ids);
                }

                let update_sql = format!(
                    "UPDATE agent_messages SET deleted_at = ? WHERE thread_id = ? AND deleted_at IS NULL AND id IN ({})",
                    placeholders.join(", ")
                );
                let mut update_params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
                update_params.push(Box::new(now_millis_i64()));
                update_params.push(Box::new(thread_id.to_string()));
                for id in message_ids {
                    update_params.push(Box::new(id));
                }
                let update_refs: Vec<&dyn rusqlite::types::ToSql> =
                    update_params.iter().map(|p| p.as_ref()).collect();
                conn.execute(&update_sql, update_refs.as_slice())?;
                for id in &deleted_ids {
                    embedding_queue::queue_embedding_deletion_on_connection(
                        conn,
                        "agent_message",
                        id,
                        now_ts() as i64,
                    )?;
                }
                refresh_thread_stats(conn, &thread_id)?;
                Ok(deleted_ids)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let count = deleted_ids.len();
        Ok(count)
    }

    pub async fn restore_messages(&self, thread_id: &str, message_ids: &[&str]) -> Result<usize> {
        if message_ids.is_empty() {
            return Ok(0);
        }
        let thread_id = thread_id.to_string();
        let message_ids: Vec<String> = message_ids.iter().map(|s| s.to_string()).collect();
        let restored_messages = self
            .conn
            .call(move |conn| {
                let placeholders: Vec<String> =
                    message_ids.iter().map(|_| "?".to_string()).collect();
                let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
                params.push(Box::new(thread_id.to_string()));
                for id in &message_ids {
                    params.push(Box::new(id.to_string()));
                }
                let refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();

                let select_sql = format!(
                    "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
                     FROM agent_messages WHERE thread_id = ? AND deleted_at IS NOT NULL AND id IN ({})",
                    placeholders.join(", ")
                );
                let mut stmt = conn.prepare(&select_sql)?;
                let rows = stmt.query_map(refs.as_slice(), map_agent_message)?;
                let restored_messages = rows.filter_map(|row| row.ok()).collect::<Vec<_>>();
                drop(stmt);
                if restored_messages.is_empty() {
                    return Ok(restored_messages);
                }

                let update_sql = format!(
                    "UPDATE agent_messages SET deleted_at = NULL WHERE thread_id = ? AND deleted_at IS NOT NULL AND id IN ({})",
                    placeholders.join(", ")
                );
                conn.execute(&update_sql, refs.as_slice())?;
                let workspace_id = conn
                    .query_row(
                        "SELECT workspace_id FROM agent_threads WHERE id = ?1",
                        params![&thread_id],
                        |row| row.get::<_, Option<String>>(0),
                    )
                    .optional()?
                    .flatten();
                for message in &restored_messages {
                    embedding_queue::enqueue_message_embedding_job(
                        conn,
                        message,
                        workspace_id.as_deref(),
                        now_ts() as i64,
                    )?;
                }
                refresh_thread_stats(conn, &thread_id)?;
                Ok(restored_messages)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let count = restored_messages.len();
        Ok(count)
    }

    pub async fn list_messages(
        &self,
        thread_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<AgentDbMessage>> {
        self.list_messages_with_deleted_flag(thread_id, limit, false)
            .await
    }

    pub async fn list_messages_with_deleted(
        &self,
        thread_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<AgentDbMessage>> {
        self.list_messages_with_deleted_flag(thread_id, limit, true)
            .await
    }

    async fn list_messages_with_deleted_flag(
        &self,
        thread_id: &str,
        limit: Option<usize>,
        include_deleted: bool,
    ) -> Result<Vec<AgentDbMessage>> {
        let thread_id = thread_id.to_string();
        self.interactive_read_conn.call(move |conn| {
            let deleted_filter = if include_deleted { "" } else { " AND deleted_at IS NULL" };
            let messages = if let Some(limit) = limit {
                let limit = limit.max(1) as i64;
                let sql = format!(
                    "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
                     FROM agent_messages WHERE thread_id = ?1{deleted_filter} ORDER BY created_at DESC, rowid DESC LIMIT ?2",
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params![thread_id, limit], map_agent_message)?;
                let mut messages: Vec<AgentDbMessage> = rows.filter_map(|row| row.ok()).collect();
                messages.reverse();
                messages
            } else {
                let sql = format!(
                    "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
                     FROM agent_messages WHERE thread_id = ?1{deleted_filter} ORDER BY created_at ASC, rowid ASC",
                );
                let mut stmt = conn.prepare(&sql)?;
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
        self.interactive_read_conn
            .call(move |conn| {
                let total_count = conn.query_row(
                    "SELECT COUNT(*) FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL",
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
                     FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL ORDER BY created_at ASC, rowid ASC LIMIT ?2 OFFSET ?3",
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

    /// Lean fetch for the concierge welcome: returns the thread's opening
    /// user message (first message with role='user') and the *trailing*
    /// `tail_limit` messages — but if the most recent compaction-artifact
    /// marker falls inside that tail window, the trailing slice starts at
    /// the compaction marker instead so we don't expose pre-compaction
    /// content. Designed to read at most ~6 rows total even on threads
    /// with thousands of messages — no full-table scan, no JSON blob
    /// hydration.
    pub async fn concierge_thread_context_summary(
        &self,
        thread_id: &str,
        tail_limit: usize,
    ) -> Result<(Option<(String, String)>, Vec<(String, String)>)> {
        let thread_id = thread_id.to_string();
        let tail_limit = tail_limit.max(1);
        self.interactive_read_conn
            .call(move |conn| {
                let mut tail_stmt = conn.prepare(
                    "SELECT role, content, metadata_json \
                     FROM agent_messages \
                     WHERE thread_id = ?1 AND deleted_at IS NULL \
                     ORDER BY created_at DESC, id DESC \
                     LIMIT ?2",
                )?;
                let tail_rows =
                    tail_stmt.query_map(params![&thread_id, tail_limit as i64], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                        ))
                    })?;
                let mut tail_desc: Vec<(String, String, Option<String>)> =
                    tail_rows.filter_map(|row| row.ok()).collect();

                let compaction_idx_in_tail = tail_desc.iter().position(|(_, content, meta)| {
                    let kind_is_compaction = meta
                        .as_deref()
                        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                        .and_then(|value| {
                            value
                                .get("message_kind")
                                .and_then(|kind| kind.as_str())
                                .map(|kind| kind == "compaction_artifact")
                        })
                        .unwrap_or(false);
                    kind_is_compaction
                        || content.starts_with("[Compacted earlier context]")
                        || content.starts_with("Pre-compaction context:")
                });
                if let Some(idx) = compaction_idx_in_tail {
                    tail_desc.truncate(idx + 1);
                }
                tail_desc.reverse();
                let tail: Vec<(String, String)> = tail_desc
                    .into_iter()
                    .map(|(role, content, _)| (role, content))
                    .collect();

                let opening = conn
                    .query_row(
                        "SELECT role, content \
                         FROM agent_messages \
                         WHERE thread_id = ?1 AND deleted_at IS NULL AND role = 'user' \
                         ORDER BY created_at ASC, rowid ASC \
                         LIMIT 1",
                        params![&thread_id],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                    )
                    .optional()?;

                Ok((opening, tail))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_active_context_window(
        &self,
        thread_id: &str,
    ) -> Result<(Vec<AgentDbMessage>, usize, usize)> {
        let thread_id = thread_id.to_string();
        self.interactive_read_conn
            .call(move |conn| {
                let total_count = conn.query_row(
                    "SELECT COUNT(*) FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL",
                    params![&thread_id],
                    |row| row.get::<_, i64>(0),
                )?;
                let total_count = total_count.max(0) as usize;

                let start = conn
                    .query_row(
                        "SELECT absolute_index FROM (
                            SELECT ROW_NUMBER() OVER (ORDER BY created_at ASC, rowid ASC) - 1 AS absolute_index,
                                   content,
                                   metadata_json
                            FROM agent_messages
                            WHERE thread_id = ?1 AND deleted_at IS NULL
                         )
                         WHERE (
                            metadata_json IS NOT NULL
                            AND json_valid(metadata_json)
                            AND json_extract(metadata_json, '$.message_kind') = 'compaction_artifact'
                         )
                         OR content LIKE '[Compacted earlier context]%'
                         OR content LIKE 'Pre-compaction context:%'
                         ORDER BY absolute_index DESC
                         LIMIT 1",
                        params![&thread_id],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()?
                    .unwrap_or(0)
                    .max(0) as usize;

                if start >= total_count {
                    return Ok((Vec::new(), total_count, total_count));
                }

                let mut stmt = conn.prepare(
                    "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
                     FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL ORDER BY created_at ASC, rowid ASC LIMIT ?2 OFFSET ?3",
                )?;
                let rows = stmt.query_map(
                    params![
                        &thread_id,
                        total_count.saturating_sub(start) as i64,
                        start as i64
                    ],
                    map_agent_message,
                )?;
                Ok((rows.filter_map(|row| row.ok()).collect(), start, total_count))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn trim_thread_messages_to_recent_tail_by_prefix(
        &self,
        thread_id_prefix: &str,
        max_messages: usize,
    ) -> Result<usize> {
        let thread_id_prefix = thread_id_prefix.to_string();
        let max_messages = max_messages.max(1) as i64;
        self.conn
            .call(move |conn| {
                let transaction = conn.transaction()?;
                let thread_ids = {
                    let mut stmt = transaction.prepare(
                        "SELECT id
                         FROM agent_threads
                         WHERE deleted_at IS NULL
                           AND id LIKE ?1 || '%'
                           AND message_count > ?2",
                    )?;
                    let rows =
                        stmt.query_map(params![&thread_id_prefix, max_messages], |row| {
                            row.get::<_, String>(0)
                        })?;
                    rows.collect::<std::result::Result<Vec<_>, _>>()?
                };

                let mut trimmed = 0usize;
                let deleted_at = now_millis_i64();
                for thread_id in thread_ids {
                    let total_count = transaction.query_row(
                        "SELECT COUNT(*)
                         FROM agent_messages
                         WHERE thread_id = ?1 AND deleted_at IS NULL",
                        params![&thread_id],
                        |row| row.get::<_, i64>(0),
                    )?;
                    if total_count <= max_messages {
                        continue;
                    }

                    let tail_start = total_count.saturating_sub(max_messages);
                    let latest_artifact_index = transaction
                        .query_row(
                            "SELECT absolute_index
                             FROM (
                                SELECT ROW_NUMBER() OVER (ORDER BY created_at ASC, rowid ASC) - 1 AS absolute_index,
                                       content,
                                       metadata_json
                                FROM agent_messages
                                WHERE thread_id = ?1 AND deleted_at IS NULL
                             )
                             WHERE (
                                metadata_json IS NOT NULL
                                AND json_valid(metadata_json)
                                AND json_extract(metadata_json, '$.message_kind') = 'compaction_artifact'
                             )
                             OR content LIKE '[Compacted earlier context]%'
                             OR content LIKE 'Pre-compaction context:%'
                             ORDER BY absolute_index DESC
                             LIMIT 1",
                            params![&thread_id],
                            |row| row.get::<_, i64>(0),
                        )
                        .optional()?;
                    let retained_artifact_index =
                        latest_artifact_index.filter(|index| *index < tail_start);
                    let recent_start = if retained_artifact_index.is_some() {
                        total_count.saturating_sub(max_messages.saturating_sub(1))
                    } else {
                        tail_start
                    };

                    let deleted_ids = {
                        let mut stmt = transaction.prepare(
                            "WITH ordered AS (
                                SELECT id,
                                       ROW_NUMBER() OVER (ORDER BY created_at ASC, rowid ASC) - 1 AS absolute_index
                                FROM agent_messages
                                WHERE thread_id = ?1 AND deleted_at IS NULL
                             )
                             SELECT id
                             FROM ordered
                             WHERE absolute_index < ?2
                               AND (?3 IS NULL OR absolute_index != ?3)",
                        )?;
                        let rows = stmt.query_map(
                            params![&thread_id, recent_start, retained_artifact_index],
                            |row| row.get::<_, String>(0),
                        )?;
                        rows.collect::<std::result::Result<Vec<_>, _>>()?
                    };

                    if deleted_ids.is_empty() {
                        continue;
                    }

                    transaction.execute(
                        "WITH ordered AS (
                            SELECT id,
                                   ROW_NUMBER() OVER (ORDER BY created_at ASC, rowid ASC) - 1 AS absolute_index
                            FROM agent_messages
                            WHERE thread_id = ?1 AND deleted_at IS NULL
                         )
                         UPDATE agent_messages
                         SET deleted_at = ?4
                         WHERE thread_id = ?1
                           AND deleted_at IS NULL
                           AND id IN (
                                SELECT id
                                FROM ordered
                                WHERE absolute_index < ?2
                                  AND (?3 IS NULL OR absolute_index != ?3)
                           )",
                        params![
                            &thread_id,
                            recent_start,
                            retained_artifact_index,
                            deleted_at
                        ],
                    )?;
                    for message_id in &deleted_ids {
                        embedding_queue::queue_embedding_deletion_on_connection(
                            &transaction,
                            "agent_message",
                            message_id,
                            now_ts() as i64,
                        )?;
                    }
                    refresh_thread_stats(&transaction, &thread_id)?;
                    transaction.execute(
                        "UPDATE agent_threads
                         SET updated_at = MAX(updated_at, ?2)
                         WHERE id = ?1",
                        params![&thread_id, deleted_at],
                    )?;
                    trimmed = trimmed.saturating_add(1);
                }

                transaction.commit()?;
                Ok(trimmed)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn thread_message_token_totals(&self, thread_id: &str) -> Result<(u64, u64)> {
        let thread_id = thread_id.to_string();
        self.interactive_read_conn
            .call(move |conn| {
                let (input_tokens, output_tokens): (i64, i64) = conn.query_row(
                    "SELECT
                        COALESCE(SUM(COALESCE(input_tokens, 0)), 0),
                        COALESCE(SUM(COALESCE(output_tokens, 0)), 0)
                     FROM agent_messages
                     WHERE thread_id = ?1 AND deleted_at IS NULL",
                    params![&thread_id],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )?;
                Ok((input_tokens.max(0) as u64, output_tokens.max(0) as u64))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_pinned_messages_for_compaction(
        &self,
        thread_id: &str,
    ) -> Result<Vec<(usize, AgentDbMessage)>> {
        let thread_id = thread_id.to_string();
        self.interactive_read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT absolute_index, id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
                     FROM (
                        SELECT ROW_NUMBER() OVER (ORDER BY created_at ASC, rowid ASC) - 1 AS absolute_index,
                               id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json
                        FROM agent_messages
                        WHERE thread_id = ?1 AND deleted_at IS NULL
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
                     FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL ORDER BY created_at DESC, rowid DESC LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![thread_id, limit], map_agent_message)?;
                let mut messages: Vec<AgentDbMessage> = rows.filter_map(|row| row.ok()).collect();
                messages.reverse();
                Ok(messages)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn latest_user_message_content(&self, thread_id: &str) -> Result<Option<String>> {
        let thread_id = thread_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT content \
                     FROM agent_messages \
                     WHERE thread_id = ?1 AND role = 'user' AND deleted_at IS NULL \
                     ORDER BY created_at DESC, rowid DESC \
                     LIMIT 1",
                    params![thread_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn latest_assistant_message(
        &self,
        thread_id: &str,
    ) -> Result<Option<AgentDbMessage>> {
        let thread_id = thread_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
                     FROM agent_messages \
                     WHERE thread_id = ?1 \
                       AND role = 'assistant' \
                       AND deleted_at IS NULL \
                       AND TRIM(content) != '' \
                     ORDER BY created_at DESC, rowid DESC \
                     LIMIT 1",
                    params![thread_id],
                    map_agent_message,
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn latest_participant_assistant_message(
        &self,
        thread_id: &str,
    ) -> Result<Option<(String, Option<String>, String)>> {
        let thread_id = thread_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT \
                        COALESCE( \
                            json_extract(metadata_json, '$.author_agent_id'), \
                            json_extract(metadata_json, '$.authorAgentId') \
                        ) AS author_agent_id, \
                        COALESCE( \
                            json_extract(metadata_json, '$.author_agent_name'), \
                            json_extract(metadata_json, '$.authorAgentName') \
                        ) AS author_agent_name, \
                        TRIM(content) \
                     FROM agent_messages \
                     WHERE thread_id = ?1 \
                       AND role = 'assistant' \
                       AND deleted_at IS NULL \
                       AND TRIM(content) != '' \
                       AND metadata_json IS NOT NULL \
                       AND json_valid(metadata_json) \
                       AND COALESCE( \
                            json_extract(metadata_json, '$.author_agent_id'), \
                            json_extract(metadata_json, '$.authorAgentId') \
                       ) IS NOT NULL \
                     ORDER BY created_at DESC, rowid DESC \
                     LIMIT 1",
                    params![thread_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn latest_visible_main_assistant_message_timestamp(
        &self,
        thread_id: &str,
        participant_agent_ids: &[String],
    ) -> Result<Option<u64>> {
        let thread_id = thread_id.to_string();
        let participant_agent_ids = participant_agent_ids
            .iter()
            .map(|id| id.to_ascii_lowercase())
            .collect::<Vec<_>>();
        self.read_conn
            .call(move |conn| {
                let author_exclusion = if participant_agent_ids.is_empty() {
                    String::new()
                } else {
                    let placeholders = std::iter::repeat("?")
                        .take(participant_agent_ids.len())
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        " AND ( \
                            COALESCE( \
                                json_extract(latest.valid_metadata_json, '$.author_agent_id'), \
                                json_extract(latest.valid_metadata_json, '$.authorAgentId') \
                            ) IS NULL \
                            OR lower(COALESCE( \
                                json_extract(latest.valid_metadata_json, '$.author_agent_id'), \
                                json_extract(latest.valid_metadata_json, '$.authorAgentId') \
                            )) NOT IN ({placeholders}) \
                        )"
                    )
                };
                let sql = format!(
                    "SELECT latest.created_at \
                     FROM ( \
                        SELECT created_at, \
                               role, \
                               content, \
                               CASE \
                                   WHEN metadata_json IS NOT NULL AND json_valid(metadata_json) \
                                   THEN metadata_json \
                                   ELSE '{{}}' \
                               END AS valid_metadata_json \
                        FROM agent_messages \
                        WHERE thread_id = ? \
                          AND deleted_at IS NULL \
                          AND role IN ('user', 'assistant') \
                          AND ( \
                              COALESCE( \
                                  json_extract( \
                                      CASE \
                                          WHEN metadata_json IS NOT NULL AND json_valid(metadata_json) \
                                          THEN metadata_json \
                                          ELSE '{{}}' \
                                      END, \
                                      '$.tool_name' \
                                  ), \
                                  json_extract( \
                                      CASE \
                                          WHEN metadata_json IS NOT NULL AND json_valid(metadata_json) \
                                          THEN metadata_json \
                                          ELSE '{{}}' \
                                      END, \
                                      '$.toolName' \
                                  ) \
                              ) IS NULL \
                              OR COALESCE( \
                                  json_extract( \
                                      CASE \
                                          WHEN metadata_json IS NOT NULL AND json_valid(metadata_json) \
                                          THEN metadata_json \
                                          ELSE '{{}}' \
                                      END, \
                                      '$.tool_name' \
                                  ), \
                                  json_extract( \
                                      CASE \
                                          WHEN metadata_json IS NOT NULL AND json_valid(metadata_json) \
                                          THEN metadata_json \
                                          ELSE '{{}}' \
                                      END, \
                                      '$.toolName' \
                                  ) \
                              ) != 'internal_delegate' \
                          ) \
                        ORDER BY created_at DESC, rowid DESC \
                        LIMIT 1 \
                     ) latest \
                     WHERE latest.role = 'assistant' \
                       AND TRIM(latest.content) != ''{author_exclusion}"
                );
                let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
                    vec![Box::new(thread_id) as Box<dyn rusqlite::types::ToSql>];
                for participant_agent_id in participant_agent_ids {
                    params.push(Box::new(participant_agent_id));
                }
                let param_refs = params
                    .iter()
                    .map(|param| param.as_ref())
                    .collect::<Vec<&dyn rusqlite::types::ToSql>>();
                conn.query_row(&sql, param_refs.as_slice(), |row| {
                    Ok(row.get::<_, i64>(0)?.max(0) as u64)
                })
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn thread_user_pacing(
        &self,
        thread_id: &str,
        now_ms: u64,
        window_ms: u64,
        gap_sample_limit: usize,
    ) -> Result<ThreadUserPacing> {
        if gap_sample_limit == 0 {
            return Ok(ThreadUserPacing {
                recent_message_count: 0,
                avg_gap_secs: 0,
            });
        }

        let thread_id = thread_id.to_string();
        let cutoff_ms = now_ms.saturating_sub(window_ms).min(i64::MAX as u64) as i64;
        let gap_sample_limit = gap_sample_limit.min(i64::MAX as usize) as i64;

        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "WITH recent_user_messages AS (
                        SELECT created_at
                        FROM agent_messages
                        WHERE thread_id = ?1
                          AND role = 'user'
                          AND deleted_at IS NULL
                        ORDER BY created_at DESC, rowid DESC
                        LIMIT ?3
                     ),
                     ordered_user_messages AS (
                        SELECT created_at
                        FROM recent_user_messages
                        ORDER BY created_at ASC
                     ),
                     user_gaps AS (
                        SELECT created_at - LAG(created_at) OVER (ORDER BY created_at ASC) AS gap_ms
                        FROM ordered_user_messages
                     )
                     SELECT
                        (SELECT COUNT(*)
                         FROM agent_messages
                         WHERE thread_id = ?1
                           AND role = 'user'
                           AND deleted_at IS NULL
                           AND created_at >= ?2),
                        COALESCE((SELECT CAST(AVG(gap_ms) AS INTEGER) FROM user_gaps WHERE gap_ms IS NOT NULL) / 1000, 0)",
                    params![thread_id, cutoff_ms, gap_sample_limit],
                    |row| {
                        let recent_message_count = row.get::<_, i64>(0)?.max(0) as u32;
                        let avg_gap_secs = row.get::<_, i64>(1)?.max(0) as u64;
                        Ok(ThreadUserPacing {
                            recent_message_count,
                            avg_gap_secs,
                        })
                    },
                )
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn visible_continuation_obsoleted_by_progress(
        &self,
        thread_id: &str,
        queued_at_ms: u64,
        target_agent_id: &str,
    ) -> Result<bool> {
        if queued_at_ms == 0 {
            return Ok(false);
        }
        let thread_id = thread_id.to_string();
        let queued_at_ms = queued_at_ms.min(i64::MAX as u64) as i64;
        let target_agent_id = target_agent_id.to_ascii_lowercase();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT
                        EXISTS (
                            SELECT 1
                            FROM agent_messages tool
                            WHERE tool.thread_id = ?1
                              AND tool.role = 'tool'
                              AND tool.deleted_at IS NULL
                              AND tool.created_at >= ?2
                              AND TRIM(tool.content) != ''
                        )
                        AND EXISTS (
                            SELECT 1
                            FROM agent_messages assistant
                            WHERE assistant.thread_id = ?1
                              AND assistant.role = 'assistant'
                              AND assistant.deleted_at IS NULL
                              AND assistant.created_at >= ?2
                              AND TRIM(assistant.content) != ''
                              AND (
                                assistant.metadata_json IS NULL
                                OR NOT json_valid(assistant.metadata_json)
                                OR COALESCE(
                                    json_extract(assistant.metadata_json, '$.author_agent_id'),
                                    json_extract(assistant.metadata_json, '$.authorAgentId')
                                ) IS NULL
                                OR lower(COALESCE(
                                    json_extract(assistant.metadata_json, '$.author_agent_id'),
                                    json_extract(assistant.metadata_json, '$.authorAgentId')
                                )) = ?3
                              )
                        )",
                    params![thread_id, queued_at_ms, target_agent_id],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn latest_unanswered_assistant_message(
        &self,
        thread_id: &str,
    ) -> Result<Option<(String, u64)>> {
        let thread_id = thread_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT assistant.content, assistant.created_at \
                     FROM agent_messages assistant \
                     WHERE assistant.thread_id = ?1 \
                       AND assistant.role = 'assistant' \
                       AND assistant.deleted_at IS NULL \
                       AND NOT EXISTS ( \
                           SELECT 1 \
                           FROM agent_messages user_message \
                           WHERE user_message.thread_id = assistant.thread_id \
                             AND user_message.role = 'user' \
                             AND user_message.deleted_at IS NULL \
                             AND user_message.created_at > assistant.created_at \
                       ) \
                     ORDER BY assistant.created_at DESC, assistant.rowid DESC \
                     LIMIT 1",
                    params![thread_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?.max(0) as u64,
                        ))
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn thread_message_pin_state(
        &self,
        thread_id: &str,
        message_id: &str,
    ) -> Result<Option<ThreadMessagePinState>> {
        let thread_id = thread_id.to_string();
        let message_id = message_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT
                        length(candidate.content),
                        CASE
                            WHEN candidate.metadata_json IS NOT NULL
                             AND json_valid(candidate.metadata_json)
                             AND (
                                json_extract(candidate.metadata_json, '$.pinned_for_compaction') = 1
                                OR json_extract(candidate.metadata_json, '$.pinnedForCompaction') = 1
                             )
                            THEN 1
                            ELSE 0
                        END,
                        CASE
                            WHEN (
                                candidate.metadata_json IS NOT NULL
                                AND json_valid(candidate.metadata_json)
                                AND json_extract(candidate.metadata_json, '$.message_kind') = 'compaction_artifact'
                            )
                            OR candidate.content LIKE '[Compacted earlier context]%'
                            OR candidate.content LIKE 'Pre-compaction context:%'
                            THEN 0
                            ELSE 1
                        END,
                        COALESCE((
                            SELECT SUM(length(pinned.content))
                            FROM agent_messages pinned
                            WHERE pinned.thread_id = ?1
                              AND pinned.deleted_at IS NULL
                              AND pinned.metadata_json IS NOT NULL
                              AND json_valid(pinned.metadata_json)
                              AND (
                                json_extract(pinned.metadata_json, '$.pinned_for_compaction') = 1
                                OR json_extract(pinned.metadata_json, '$.pinnedForCompaction') = 1
                              )
                              AND NOT (
                                json_extract(pinned.metadata_json, '$.message_kind') = 'compaction_artifact'
                                OR pinned.content LIKE '[Compacted earlier context]%'
                                OR pinned.content LIKE 'Pre-compaction context:%'
                              )
                        ), 0)
                     FROM agent_messages candidate
                     WHERE candidate.thread_id = ?1
                       AND candidate.id = ?2
                       AND candidate.deleted_at IS NULL",
                    params![thread_id, message_id],
                    |row| {
                        Ok(ThreadMessagePinState {
                            message_chars: row.get::<_, i64>(0)?.max(0) as usize,
                            pinned_for_compaction: row.get::<_, i64>(1)? != 0,
                            counts_toward_pinned_chars: row.get::<_, i64>(2)? != 0,
                            current_pinned_chars: row.get::<_, i64>(3)?.max(0) as usize,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Returns the signed mean of recent `user_feedback` behavioral-event
    /// signals on the thread. The window is the union of the most recent
    /// 20 events and all events from the last 30 minutes — whichever yields
    /// more rows, both feed in. Returns `None` if the window is empty.
    pub(crate) async fn aggregate_user_feedback_score(
        &self,
        thread_id: &str,
    ) -> Result<Option<f64>> {
        const RECENT_EVENT_COUNT: usize = 20;
        const TIME_WINDOW_MS: i64 = 30 * 60 * 1000;
        const HARD_FETCH_LIMIT: i64 = 200;

        let thread_id = thread_id.to_string();
        let cutoff = now_millis_i64().saturating_sub(TIME_WINDOW_MS);
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT payload_json, timestamp
                     FROM agent_events
                     WHERE category = 'behavioral'
                       AND kind = 'user_feedback'
                       AND pane_id = ?1
                     ORDER BY timestamp DESC
                     LIMIT ?2",
                )?;
                let rows: Vec<(String, i64)> = stmt
                    .query_map(params![thread_id, HARD_FETCH_LIMIT], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                    })?
                    .filter_map(|r| r.ok())
                    .collect();

                let mut signals: Vec<f64> = Vec::new();
                for (idx, (payload_json, timestamp)) in rows.iter().enumerate() {
                    let in_window = idx < RECENT_EVENT_COUNT || *timestamp >= cutoff;
                    if !in_window {
                        break;
                    }
                    if let Ok(value) =
                        serde_json::from_str::<serde_json::Value>(payload_json.as_str())
                    {
                        // Behavioral payload wraps the recorded payload in a
                        // `payload` key; signal lives at `$.payload.signal`.
                        if let Some(signal) = value
                            .get("payload")
                            .and_then(|p| p.get("signal"))
                            .and_then(|s| s.as_f64())
                        {
                            signals.push(signal);
                        }
                    }
                }
                if signals.is_empty() {
                    Ok(None)
                } else {
                    let mean = signals.iter().sum::<f64>() / signals.len() as f64;
                    Ok(Some(mean.clamp(-1.0, 1.0)))
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn message_feedback_state(
        &self,
        thread_id: &str,
        message_id: &str,
    ) -> Result<Option<MessageFeedbackState>> {
        let thread_id = thread_id.to_string();
        let message_id = message_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT
                        role,
                        CASE
                            WHEN metadata_json IS NOT NULL AND json_valid(metadata_json)
                            THEN COALESCE(
                                json_extract(metadata_json, '$.feedback'),
                                json_extract(metadata_json, '$.feedback.reaction')
                            )
                            ELSE NULL
                        END
                     FROM agent_messages
                     WHERE thread_id = ?1
                       AND id = ?2
                       AND deleted_at IS NULL",
                    params![thread_id, message_id],
                    |row| {
                        Ok(MessageFeedbackState {
                            role: row.get::<_, String>(0)?,
                            reaction: row.get::<_, Option<String>>(1)?,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn message_id_at_absolute_index(
        &self,
        thread_id: &str,
        absolute_index: usize,
    ) -> Result<Option<String>> {
        let thread_id = thread_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT id
                     FROM agent_messages
                     WHERE thread_id = ?1
                       AND deleted_at IS NULL
                     ORDER BY created_at ASC, rowid ASC
                     LIMIT 1 OFFSET ?2",
                    params![thread_id, absolute_index as i64],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn set_message_feedback(
        &self,
        thread_id: &str,
        message_id: &str,
        reaction: Option<&str>,
    ) -> Result<bool> {
        let thread_id = thread_id.to_string();
        let message_id = message_id.to_string();
        let reaction = reaction.map(ToOwned::to_owned);
        self.conn
            .call(move |conn| {
                let updated = match reaction.as_deref() {
                    Some(value) => conn.execute(
                        "UPDATE agent_messages
                         SET metadata_json = json_set(
                            CASE
                                WHEN metadata_json IS NOT NULL AND json_valid(metadata_json)
                                THEN metadata_json
                                ELSE '{}'
                            END,
                            '$.feedback', ?3
                         )
                         WHERE thread_id = ?1
                           AND id = ?2
                           AND deleted_at IS NULL",
                        params![thread_id, message_id, value],
                    )?,
                    None => conn.execute(
                        "UPDATE agent_messages
                         SET metadata_json = CASE
                            WHEN metadata_json IS NOT NULL AND json_valid(metadata_json)
                            THEN json_remove(metadata_json, '$.feedback')
                            ELSE metadata_json
                         END
                         WHERE thread_id = ?1
                           AND id = ?2
                           AND deleted_at IS NULL",
                        params![thread_id, message_id],
                    )?,
                };

                if updated > 0 {
                    conn.execute(
                        "UPDATE agent_threads
                         SET updated_at = MAX(updated_at, ?2)
                         WHERE id = ?1",
                        params![thread_id, now_millis_i64()],
                    )?;
                }

                Ok(updated > 0)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn set_message_pinned_for_compaction(
        &self,
        thread_id: &str,
        message_id: &str,
        pinned: bool,
    ) -> Result<bool> {
        let thread_id = thread_id.to_string();
        let message_id = message_id.to_string();
        let pinned_json = if pinned { "true" } else { "false" }.to_string();
        self.conn
            .call(move |conn| {
                let updated = conn.execute(
                    "UPDATE agent_messages
                     SET metadata_json = json_set(
                        CASE
                            WHEN metadata_json IS NOT NULL AND json_valid(metadata_json)
                            THEN metadata_json
                            ELSE '{}'
                        END,
                        '$.pinned_for_compaction', json(?3),
                        '$.pinnedForCompaction', json(?3)
                     )
                     WHERE thread_id = ?1
                       AND id = ?2
                       AND deleted_at IS NULL",
                    params![thread_id, message_id, pinned_json],
                )?;

                if updated > 0 {
                    conn.execute(
                        "UPDATE agent_threads
                         SET updated_at = MAX(updated_at, ?2)
                         WHERE id = ?1",
                        params![thread_id, now_millis_i64()],
                    )?;
                }

                Ok(updated > 0)
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
                             WHERE thread_id = ?1 AND deleted_at IS NULL AND (created_at > ?2 OR (created_at = ?2 AND id > ?3)) \
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
                             WHERE thread_id = ?1 AND deleted_at IS NULL AND (created_at > ?2 OR (created_at = ?2 AND id > ?3)) \
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
                             FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL ORDER BY created_at ASC, id ASC LIMIT ?2",
                        )?;
                        let rows = stmt.query_map(params![thread_id, limit], map_agent_message)?;
                        rows.filter_map(|row| row.ok()).collect()
                    }
                    (None, None) => {
                        let mut stmt = conn.prepare(
                            "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json \
                             FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL ORDER BY created_at ASC, id ASC",
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
        self.list_transcript_index_limited(workspace_id, None).await
    }

    pub async fn list_transcript_index_limited(
        &self,
        workspace_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<TranscriptIndexEntry>> {
        let workspace_id = workspace_id.map(str::to_string);
        self.read_conn
            .call(move |conn| {
                let mut sql = String::from(
                    "SELECT id, pane_id, workspace_id, surface_id, filename, reason, captured_at, size_bytes, preview FROM transcript_index",
                );
                let mut values = Vec::<rusqlite::types::Value>::new();
                if let Some(workspace_id) = workspace_id {
                    sql.push_str(" WHERE workspace_id = ?");
                    values.push(rusqlite::types::Value::Text(workspace_id));
                }
                sql.push_str(" ORDER BY captured_at DESC");
                if let Some(limit) = limit {
                    sql.push_str(" LIMIT ?");
                    values.push(rusqlite::types::Value::Integer(limit.max(1) as i64));
                }
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(
                    rusqlite::params_from_iter(values.iter()),
                    map_transcript_index_entry,
                )?;
                Ok(rows.filter_map(|row| row.ok()).collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_transcript_index_matching(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<TranscriptIndexEntry>> {
        let query = query.trim().to_ascii_lowercase();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let like = format!("%{query}%");
        let limit = limit.max(1) as i64;
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, pane_id, workspace_id, surface_id, filename, reason, captured_at, size_bytes, preview \
                     FROM transcript_index \
                     WHERE lower(filename) LIKE ?1 OR lower(COALESCE(preview, '')) LIKE ?1 \
                     ORDER BY captured_at DESC \
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![like, limit], map_transcript_index_entry)?;
                Ok(rows.filter_map(|row| row.ok()).collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_snapshot_index(&self, entry: &SnapshotIndexEntry) -> Result<()> {
        let entry = entry.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO snapshot_index \
             (snapshot_id, workspace_id, session_id, kind, label, path, created_at, details_json, deleted_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)",
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
        self.list_snapshot_index_ordered(workspace_id, false).await
    }

    pub async fn list_snapshot_index_ordered(
        &self,
        workspace_id: Option<&str>,
        oldest_first: bool,
    ) -> Result<Vec<SnapshotIndexEntry>> {
        self.list_snapshot_index_ordered_limited(workspace_id, oldest_first, None)
            .await
    }

    pub(crate) async fn list_snapshot_index_ordered_limited(
        &self,
        workspace_id: Option<&str>,
        oldest_first: bool,
        limit: Option<usize>,
    ) -> Result<Vec<SnapshotIndexEntry>> {
        let workspace_id = workspace_id.map(str::to_string);
        self.read_conn
            .call(move |conn| {
                let order = if oldest_first { "ASC" } else { "DESC" };
                let mut sql = String::from(
                    "SELECT snapshot_id, workspace_id, session_id, kind, label, path, created_at, details_json \
                     FROM snapshot_index WHERE ",
                );
                let mut values = Vec::<rusqlite::types::Value>::new();
                if let Some(workspace_id) = workspace_id {
                    sql.push_str("(workspace_id = ? OR workspace_id IS NULL) AND deleted_at IS NULL");
                    values.push(rusqlite::types::Value::Text(workspace_id));
                } else {
                    sql.push_str("deleted_at IS NULL");
                }
                sql.push_str(&format!(" ORDER BY created_at {order}"));
                if let Some(limit) = limit {
                    sql.push_str(" LIMIT ?");
                    values.push(rusqlite::types::Value::Integer(limit.max(1) as i64));
                }
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(
                    rusqlite::params_from_iter(values.iter()),
                    map_snapshot_index_entry,
                )?;
                Ok(rows.filter_map(|row| row.ok()).collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
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
                 FROM snapshot_index WHERE snapshot_id = ?1 AND deleted_at IS NULL",
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
                    "UPDATE snapshot_index SET deleted_at = ?2 WHERE snapshot_id = ?1 AND deleted_at IS NULL",
                    params![snapshot_id, now_ts() as i64],
                )?;
                Ok(affected > 0)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

/// Linear pass that mirrors the in-memory
/// `agent::stalled_turns::history_scan::thread_has_unanswered_tool_calls`
/// algorithm, but operates on the lean projection produced by
/// `thread_ids_with_unanswered_tool_calls`'s SQL.
///
/// Each tuple is `(role, tool_calls_json_for_assistants, tool_call_id_for_tools)`,
/// in the thread's chronological order. Returns true on the first assistant
/// message whose contiguous trailing run of tool replies doesn't satisfy
/// every tool-call id it declared.
fn scan_messages_for_unanswered_tool_calls(
    messages: &[(String, Option<String>, Option<String>)],
) -> bool {
    let mut idx = 0usize;
    while idx < messages.len() {
        let (role, tool_calls_json, _) = &messages[idx];
        if role != "assistant" {
            idx += 1;
            continue;
        }
        let Some(tool_calls_json) = tool_calls_json.as_deref() else {
            idx += 1;
            continue;
        };
        let calls: Vec<serde_json::Value> = match serde_json::from_str(tool_calls_json) {
            Ok(value) => value,
            Err(_) => {
                idx += 1;
                continue;
            }
        };
        let mut expected: std::collections::HashSet<String> =
            std::collections::HashSet::with_capacity(calls.len());
        for call in &calls {
            if let Some(id) = call.get("id").and_then(|value| value.as_str()) {
                if !id.is_empty() {
                    expected.insert(id.to_string());
                }
            }
        }
        if expected.is_empty() {
            idx += 1;
            continue;
        }
        let mut matched: std::collections::HashSet<String> =
            std::collections::HashSet::with_capacity(expected.len());
        let mut walk = idx + 1;
        while walk < messages.len() && messages[walk].0 == "tool" {
            if let Some(tool_call_id) = messages[walk].2.as_deref() {
                if expected.contains(tool_call_id) {
                    matched.insert(tool_call_id.to_string());
                }
            }
            walk += 1;
        }
        if matched.len() != expected.len() {
            return true;
        }
        idx = walk;
    }
    false
}
