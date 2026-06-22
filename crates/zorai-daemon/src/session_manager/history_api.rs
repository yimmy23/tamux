use super::*;

impl SessionManager {
    pub async fn search_history(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<(String, Vec<HistorySearchHit>)> {
        self.history.search(query, limit).await
    }

    pub async fn generate_skill(
        &self,
        query: Option<&str>,
        title: Option<&str>,
    ) -> Result<(String, String)> {
        self.history.generate_skill(query, title).await
    }

    pub async fn append_command_log(&self, entry: &CommandLogEntry) -> Result<()> {
        self.history.append_command_log(entry).await
    }

    pub async fn complete_command_log(
        &self,
        id: &str,
        exit_code: Option<i32>,
        duration_ms: Option<i64>,
    ) -> Result<()> {
        self.history
            .complete_command_log(id, exit_code, duration_ms)
            .await
    }

    pub async fn query_command_log(
        &self,
        workspace_id: Option<&str>,
        pane_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<CommandLogEntry>> {
        self.history
            .query_command_log(workspace_id, pane_id, limit)
            .await
    }

    pub async fn clear_command_log(&self) -> Result<()> {
        self.history.clear_command_log().await
    }

    pub async fn create_agent_thread(&self, thread: &AgentDbThread) -> Result<()> {
        self.history.create_thread(thread).await
    }

    pub async fn delete_agent_thread(&self, thread_id: &str) -> Result<()> {
        self.history.delete_thread(thread_id).await
    }

    pub async fn list_agent_threads(&self) -> Result<Vec<AgentDbThread>> {
        self.history.list_threads().await
    }

    pub(crate) async fn list_agent_threads_filtered(
        &self,
        query: &crate::history::AgentThreadListQuery,
    ) -> Result<Vec<AgentDbThread>> {
        self.history.list_threads_filtered(query).await
    }

    pub(crate) async fn thread_recall_match_rows(
        &self,
        tokens: &[String],
        thread_limit: usize,
    ) -> Result<Vec<crate::history::ThreadRecallMatchRow>> {
        self.history
            .thread_recall_match_rows(tokens, thread_limit)
            .await
    }

    pub async fn get_agent_thread(&self, thread_id: &str) -> Result<Option<AgentDbThread>> {
        self.history.get_thread(thread_id).await
    }

    pub async fn add_agent_message(&self, message: &AgentDbMessage) -> Result<()> {
        self.history.add_message(message).await
    }

    pub async fn list_agent_messages(
        &self,
        thread_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<AgentDbMessage>> {
        self.history.list_messages(thread_id, limit).await
    }

    pub async fn list_agent_messages_with_deleted(
        &self,
        thread_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<AgentDbMessage>> {
        self.history
            .list_messages_with_deleted(thread_id, limit)
            .await
    }

    pub async fn export_agent_thread(&self, thread_id: &str) -> Result<std::path::PathBuf> {
        let thread = self
            .history
            .get_thread(thread_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("thread {thread_id} not found"))?;
        let messages = self
            .history
            .list_messages_with_deleted(thread_id, None)
            .await?;
        let exported_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0);
        let payload = serde_json::json!({
            "schema": "zorai.thread-export",
            "version": 1,
            "exported_at": exported_at,
            "thread": thread,
            "messages": messages,
        });
        let json = serde_json::to_string_pretty(&payload)?;
        let dir = zorai_protocol::thread_inventory_dir(self.history.data_root(), thread_id);
        std::fs::create_dir_all(&dir)?;
        let file_path = dir.join(format!("thread-export-{exported_at}.json"));
        std::fs::write(&file_path, json)?;
        Ok(file_path)
    }

    pub async fn fork_agent_thread(
        &self,
        thread_id: &str,
        message_id: &str,
    ) -> Result<(String, String)> {
        let parent = self
            .history
            .get_thread(thread_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("thread {thread_id} not found"))?;
        let messages = self.history.list_messages(thread_id, None).await?;
        let cut = messages
            .iter()
            .position(|message| message.id == message_id)
            .ok_or_else(|| anyhow::anyhow!("message {message_id} not found in thread {thread_id}"))?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0);
        let fork_thread_id = format!("fork-{thread_id}-{now}");
        let forked: Vec<AgentDbMessage> = messages[..=cut]
            .iter()
            .enumerate()
            .map(|(index, message)| {
                let mut forked = message.clone();
                forked.id = format!("{fork_thread_id}-msg-{index}");
                forked.thread_id = fork_thread_id.clone();
                forked
            })
            .collect();

        let mut metadata = parent
            .metadata_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(object) = metadata.as_object_mut() {
            object.insert("thread_id".into(), serde_json::json!(fork_thread_id));
            object.insert("threadId".into(), serde_json::json!(fork_thread_id));
            object.insert("source".into(), serde_json::json!("gui_message_fork"));
            object.insert("upstream_thread_id".into(), serde_json::json!(thread_id));
            object.insert("upstreamThreadId".into(), serde_json::json!(thread_id));
            object.insert("forked_from_thread_id".into(), serde_json::json!(thread_id));
            object.insert("forkedFromThreadId".into(), serde_json::json!(thread_id));
            object.insert("forked_message_index".into(), serde_json::json!(cut));
            object.insert("forkedMessageIndex".into(), serde_json::json!(cut));
            object.insert("forked_message_id".into(), serde_json::json!(message_id));
            object.insert("forkedMessageId".into(), serde_json::json!(message_id));
        }

        let base_title = forked
            .last()
            .map(|message| message.content.trim())
            .filter(|content| !content.is_empty())
            .unwrap_or(parent.title.as_str());
        let base_title: String = base_title.chars().take(48).collect();
        let title = if base_title.trim().is_empty() {
            "Forked thread".to_string()
        } else {
            format!("Fork: {}", base_title.trim())
        };

        let total_input: i64 = forked.iter().filter_map(|message| message.input_tokens).sum();
        let total_output: i64 = forked.iter().filter_map(|message| message.output_tokens).sum();
        let last_preview: String = forked
            .last()
            .map(|message| message.content.chars().take(240).collect())
            .unwrap_or_default();

        let fork_thread = AgentDbThread {
            id: fork_thread_id.clone(),
            workspace_id: parent.workspace_id.clone(),
            surface_id: parent.surface_id.clone(),
            pane_id: parent.pane_id.clone(),
            agent_name: parent.agent_name.clone(),
            title: title.clone(),
            created_at: now as i64,
            updated_at: now as i64,
            message_count: forked.len() as i64,
            total_tokens: total_input.saturating_add(total_output),
            last_preview,
            metadata_json: Some(metadata.to_string()),
        };
        self.history.create_thread(&fork_thread).await?;
        for message in &forked {
            self.history.add_message(message).await?;
        }
        Ok((fork_thread_id, title))
    }

    pub async fn upsert_transcript_index(&self, entry: &TranscriptIndexEntry) -> Result<()> {
        self.history.upsert_transcript_index(entry).await
    }

    pub async fn list_transcript_index(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<Vec<TranscriptIndexEntry>> {
        self.history.list_transcript_index(workspace_id).await
    }

    pub(crate) async fn list_transcript_index_limited(
        &self,
        workspace_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<TranscriptIndexEntry>> {
        self.history
            .list_transcript_index_limited(workspace_id, limit)
            .await
    }

    pub async fn upsert_snapshot_index(&self, entry: &SnapshotIndexEntry) -> Result<()> {
        self.history.upsert_snapshot_index(entry).await
    }

    pub async fn list_snapshot_index(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<Vec<SnapshotIndexEntry>> {
        self.history.list_snapshot_index(workspace_id).await
    }

    pub(crate) async fn list_snapshot_index_limited(
        &self,
        workspace_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<SnapshotIndexEntry>> {
        self.history
            .list_snapshot_index_ordered_limited(workspace_id, false, limit)
            .await
    }

    pub async fn upsert_agent_event(&self, entry: &AgentEventRow) -> Result<()> {
        self.history.upsert_agent_event(entry).await
    }

    pub async fn list_agent_events(
        &self,
        category: Option<&str>,
        pane_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<AgentEventRow>> {
        self.history
            .list_agent_events(category, pane_id, limit)
            .await
    }

    pub async fn list_notifications(
        &self,
        include_inactive: bool,
        limit: Option<usize>,
    ) -> Result<Vec<zorai_protocol::InboxNotification>> {
        self.history
            .list_notifications(include_inactive, limit)
            .await
    }

    pub async fn mark_all_notifications_read(&self, read_at: i64) -> Result<usize> {
        self.history.mark_all_notifications_read(read_at).await
    }

    pub async fn archive_read_notifications(&self, archived_at: i64) -> Result<usize> {
        self.history.archive_read_notifications(archived_at).await
    }

    pub(crate) async fn agent_event_recall_matches(
        &self,
        category: &str,
        tokens: &[String],
        limit: usize,
    ) -> Result<Vec<AgentEventRow>> {
        self.history
            .agent_event_recall_matches(category, tokens, limit)
            .await
    }

    pub async fn list_database_tables(&self) -> Result<Vec<DatabaseTableSummary>> {
        self.history.list_database_tables().await
    }

    pub async fn query_database_table_rows(
        &self,
        table_name: &str,
        offset: usize,
        limit: usize,
        sort_column: Option<&str>,
        sort_direction: Option<&str>,
    ) -> Result<DatabaseTablePage> {
        self.history
            .query_database_table_rows(table_name, offset, limit, sort_column, sort_direction)
            .await
    }

    pub async fn update_database_table_rows(
        &self,
        table_name: &str,
        updates: Vec<DatabaseRowUpdate>,
    ) -> Result<usize> {
        self.history
            .update_database_table_rows(table_name, updates)
            .await
    }

    pub async fn execute_database_sql(&self, sql: &str) -> Result<DatabaseSqlResult> {
        self.history.execute_database_sql(sql).await
    }

    pub async fn queue_semantic_backfill(
        &self,
        limit: Option<usize>,
    ) -> Result<crate::history::SemanticBackfillResult> {
        self.history.queue_semantic_backfill(limit).await
    }

    pub async fn semantic_index_status(
        &self,
        embedding_model: &str,
        dimensions: u32,
    ) -> Result<crate::history::SemanticIndexStatus> {
        self.history
            .semantic_index_status(embedding_model, dimensions)
            .await
    }

    pub fn find_symbol_matches(
        &self,
        workspace_root: &str,
        symbol: &str,
        limit: usize,
    ) -> Vec<SymbolMatch> {
        find_symbol(workspace_root, symbol, limit)
    }

    pub async fn list_snapshots(&self, workspace_id: Option<&str>) -> Result<Vec<SnapshotInfo>> {
        self.snapshots.list(workspace_id).await
    }

    pub(crate) async fn list_snapshots_limited(
        &self,
        workspace_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SnapshotInfo>> {
        self.snapshots.list_limited(workspace_id, limit).await
    }

    pub async fn restore_snapshot(&self, snapshot_id: &str) -> Result<(bool, String)> {
        let result = self.snapshots.restore(snapshot_id).await;
        let outcome = crate::governance::snapshot_restore_outcome(&result);
        crate::governance::record_transition_audit(
            &self.history,
            crate::governance::TransitionKind::CompensationEntry,
            crate::governance::TransitionAuditIds {
                run_id: Some(snapshot_id.to_string()),
                ..Default::default()
            },
            serde_json::json!({
                "snapshot_id": snapshot_id,
            }),
            &outcome,
        )
        .await;
        result
    }
}
