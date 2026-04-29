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

    pub async fn upsert_transcript_index(&self, entry: &TranscriptIndexEntry) -> Result<()> {
        self.history.upsert_transcript_index(entry).await
    }

    pub async fn list_transcript_index(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<Vec<TranscriptIndexEntry>> {
        self.history.list_transcript_index(workspace_id).await
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

    pub async fn restore_snapshot(&self, snapshot_id: &str) -> Result<(bool, String)> {
        self.snapshots.restore(snapshot_id).await
    }
}
