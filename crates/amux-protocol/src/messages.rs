use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Identifiers
// ---------------------------------------------------------------------------

/// Unique identifier for a terminal session.
pub type SessionId = Uuid;

/// Unique identifier for a workspace (passed as string).
pub type WorkspaceId = String;

// ---------------------------------------------------------------------------
// Client -> Daemon requests
// ---------------------------------------------------------------------------

/// Messages sent from a client to the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Spawn a new terminal session.
    SpawnSession {
        /// Optional shell override (e.g. "pwsh", "bash"). Daemon picks the
        /// default shell when `None`.
        shell: Option<String>,
        /// Initial working directory. Falls back to user home if `None`.
        cwd: Option<String>,
        /// Optional environment variable overrides.
        env: Option<Vec<(String, String)>>,
        /// Workspace ID for grouping.
        workspace_id: Option<WorkspaceId>,
        /// Initial terminal width in columns.
        cols: u16,
        /// Initial terminal height in rows.
        rows: u16,
    },

    /// Attach to an existing session (start receiving output).
    AttachSession { id: SessionId },

    /// Clone an existing session into a new independent PTY session.
    CloneSession {
        /// Source session to clone metadata/scrollback from.
        source_id: SessionId,
        /// Optional workspace override for the cloned session.
        workspace_id: Option<WorkspaceId>,
        /// Optional terminal width override (defaults to source cols).
        cols: Option<u16>,
        /// Optional terminal height override (defaults to source rows).
        rows: Option<u16>,
        /// Whether to preload source scrollback into the clone.
        replay_scrollback: bool,
        /// Fallback CWD when the source session's CWD cannot be resolved
        /// (e.g. on Windows where /proc is unavailable).
        cwd: Option<String>,
    },

    /// Detach from a session (stop receiving output; session keeps running).
    DetachSession { id: SessionId },

    /// Kill / close a session.
    KillSession { id: SessionId },

    /// Send raw terminal input bytes to a session.
    Input { id: SessionId, data: Vec<u8> },

    /// Execute a daemon-managed command inside a session lane.
    ExecuteManagedCommand {
        id: SessionId,
        request: ManagedCommandRequest,
    },

    /// Resolve a previously issued approval request.
    ResolveApproval {
        id: SessionId,
        approval_id: String,
        decision: ApprovalDecision,
    },

    /// Notify the daemon about a terminal resize.
    Resize { id: SessionId, cols: u16, rows: u16 },

    /// List all active sessions.
    ListSessions,

    /// List sessions belonging to a specific workspace.
    ListWorkspaceSessions { workspace_id: WorkspaceId },

    /// Request the scrollback buffer for a session.
    GetScrollback {
        id: SessionId,
        /// Maximum number of lines to return (from the tail).
        max_lines: Option<usize>,
    },

    /// Ask the daemon to analyze a session's recent output (AI sidecar).
    AnalyzeSession {
        id: SessionId,
        max_lines: Option<usize>,
    },

    /// Search daemon-managed command/transcript history.
    SearchHistory { query: String, limit: Option<usize> },

    /// Append a terminal command log entry to the daemon database.
    AppendCommandLog { entry_json: String },

    /// Complete a previously inserted command log entry.
    CompleteCommandLog {
        id: String,
        exit_code: Option<i32>,
        duration_ms: Option<i64>,
    },

    /// Query command log entries from the daemon database.
    QueryCommandLog {
        workspace_id: Option<WorkspaceId>,
        pane_id: Option<String>,
        limit: Option<usize>,
    },

    /// Remove all command log entries from the daemon database.
    ClearCommandLog,

    /// Create a persisted agent thread record.
    CreateAgentThread { thread_json: String },

    /// Delete a persisted agent thread record.
    DeleteAgentThread { thread_id: String },

    /// List persisted agent threads.
    ListAgentThreads,

    /// Fetch a persisted agent thread and its metadata.
    GetAgentThread { thread_id: String },

    /// Append a persisted agent message record.
    AddAgentMessage { message_json: String },

    /// Delete specific agent messages from a thread by their IDs.
    DeleteAgentMessages {
        thread_id: String,
        message_ids: Vec<String>,
    },

    /// List persisted agent messages for a thread.
    ListAgentMessages {
        thread_id: String,
        limit: Option<usize>,
    },

    /// Insert or update an indexed transcript record.
    UpsertTranscriptIndex { entry_json: String },

    /// List indexed transcript records.
    ListTranscriptIndex { workspace_id: Option<WorkspaceId> },

    /// Insert or update an indexed snapshot record.
    UpsertSnapshotIndex { entry_json: String },

    /// List indexed snapshot records.
    ListSnapshotIndex { workspace_id: Option<WorkspaceId> },

    /// Insert or update an agent event record.
    UpsertAgentEvent { event_json: String },

    /// List agent event records.
    ListAgentEvents {
        category: Option<String>,
        pane_id: Option<String>,
        limit: Option<usize>,
    },

    /// Generate a reusable skill document from historical executions.
    GenerateSkill {
        query: Option<String>,
        title: Option<String>,
    },

    /// Search for symbols using daemon-side semantic indexing.
    FindSymbol {
        workspace_root: String,
        symbol: String,
        limit: Option<usize>,
    },

    /// List recorded workspace snapshots/checkpoints.
    ListSnapshots { workspace_id: Option<WorkspaceId> },

    /// Restore a previously recorded snapshot.
    RestoreSnapshot { snapshot_id: String },

    /// Request git status for a working directory.
    GetGitStatus { path: String },

    /// Request git diff text for a repository path and optional file.
    GetGitDiff {
        repo_path: String,
        file_path: Option<String>,
    },

    /// Request a text preview for a file path.
    GetFilePreview {
        path: String,
        max_bytes: Option<usize>,
    },

    /// Subscribe to OSC notifications for sessions (the daemon will push
    /// `OscNotification` messages for any attached session).
    SubscribeNotifications,

    /// Request the daemon to scrub sensitive data from a string.
    ScrubSensitive { text: String },

    /// Verify WORM telemetry ledger integrity.
    VerifyTelemetryIntegrity,

    /// Checkpoint a session's process state using CRIU.
    CheckpointSession { id: SessionId },

    /// Ping / health-check.
    Ping,

    // -----------------------------------------------------------------------
    // Agent engine
    // -----------------------------------------------------------------------
    /// Send a message to the agent (triggers an LLM turn with tool loop).
    AgentSendMessage {
        thread_id: Option<String>,
        content: String,
        session_id: Option<String>,
        /// JSON-encoded Vec<AgentDbMessage> for seeding thread context.
        context_messages_json: Option<String>,
    },

    /// Stop the current agent stream on a thread.
    AgentStopStream { thread_id: String },

    /// List all agent threads.
    AgentListThreads,

    /// Get a specific agent thread with full message history.
    AgentGetThread { thread_id: String },

    /// Delete an agent thread.
    AgentDeleteThread { thread_id: String },

    /// Add a task to the agent's task queue.
    AgentAddTask {
        title: String,
        description: String,
        priority: String,
        command: Option<String>,
        session_id: Option<String>,
        scheduled_at: Option<u64>,
        #[serde(default)]
        dependencies: Vec<String>,
    },

    /// Start a durable autonomous goal run.
    AgentStartGoalRun {
        goal: String,
        title: Option<String>,
        thread_id: Option<String>,
        session_id: Option<String>,
        priority: Option<String>,
        client_request_id: Option<String>,
    },

    /// Cancel a queued or running agent task.
    AgentCancelTask { task_id: String },

    /// List all agent tasks.
    AgentListTasks,

    /// List projected agent runs and subagent runs.
    AgentListRuns,

    /// Get a specific projected agent run.
    AgentGetRun { run_id: String },

    /// List all goal runs.
    AgentListGoalRuns,

    /// Get a specific goal run.
    AgentGetGoalRun { goal_run_id: String },

    /// Control a goal run lifecycle.
    AgentControlGoalRun {
        goal_run_id: String,
        action: String,
        step_index: Option<usize>,
    },

    /// List daemon-side todos for all threads.
    AgentListTodos,

    /// Get daemon-side todos for a specific thread.
    AgentGetTodos { thread_id: String },

    /// Get daemon-side work context for a specific thread.
    AgentGetWorkContext { thread_id: String },

    /// Get current agent configuration.
    AgentGetConfig,

    /// Update a single agent configuration item identified by JSON pointer.
    AgentSetConfigItem {
        key_path: String,
        value_json: String,
    },

    /// Fetch available models from a provider.
    AgentFetchModels {
        provider_id: String,
        base_url: String,
        api_key: String,
    },

    /// Get heartbeat check items.
    AgentHeartbeatGetItems,

    /// Set heartbeat check items.
    AgentHeartbeatSetItems { items_json: String },

    /// Resolve a task approval (approve/deny a managed command).
    AgentResolveTaskApproval {
        approval_id: String,
        decision: String,
    },

    /// Subscribe to agent event broadcasts.
    AgentSubscribe,

    /// Unsubscribe from agent event broadcasts.
    AgentUnsubscribe,

    /// Get sub-agent health metrics for a specific task.
    AgentGetSubagentMetrics { task_id: String },

    /// List checkpoints for a goal run.
    AgentListCheckpoints { goal_run_id: String },

    /// Restore a goal run from a checkpoint.
    AgentRestoreCheckpoint { checkpoint_id: String },

    /// Get health status for the agent system.
    AgentGetHealthStatus,

    /// List health log entries.
    AgentListHealthLog {
        #[serde(default)]
        limit: Option<u32>,
    },

    /// Get the current aggregate operator model.
    AgentGetOperatorModel,

    /// Reset the current operator model back to an empty aggregate state.
    AgentResetOperatorModel,

    /// Record the operator's current UI attention surface and optional target.
    AgentRecordAttention {
        surface: String,
        #[serde(default)]
        thread_id: Option<String>,
        #[serde(default)]
        goal_run_id: Option<String>,
    },

    /// Summarize causal trace outcomes for a specific tool or option type.
    AgentGetCausalTraceReport {
        option_type: String,
        #[serde(default)]
        limit: Option<u32>,
    },

    /// Summarize likely outcome for a candidate command/tool family from recent causal history.
    AgentGetCounterfactualReport {
        option_type: String,
        command_family: String,
        #[serde(default)]
        limit: Option<u32>,
    },

    /// Inspect durable memory provenance with recency-based confidence and status.
    AgentGetMemoryProvenanceReport {
        #[serde(default)]
        target: Option<String>,
        #[serde(default)]
        limit: Option<u32>,
    },

    /// Inspect trusted execution provenance events and attestation validity.
    AgentGetProvenanceReport {
        #[serde(default)]
        limit: Option<u32>,
    },

    /// Generate a SOC2-style audit artifact from recent provenance events.
    AgentGenerateSoc2Artifact {
        #[serde(default)]
        period_days: Option<u32>,
    },

    /// Inspect active collaboration sessions and disagreements.
    AgentGetCollaborationSessions {
        #[serde(default)]
        parent_task_id: Option<String>,
    },

    /// List runtime-generated tools registered in the local daemon.
    AgentListGeneratedTools,

    /// Generate a guarded runtime tool from CLI or OpenAPI metadata.
    AgentSynthesizeTool { request_json: String },

    /// Execute a generated runtime tool by name.
    AgentRunGeneratedTool {
        tool_name: String,
        args_json: String,
    },

    /// Promote a generated runtime tool into the generated skills library.
    AgentPromoteGeneratedTool { tool_name: String },

    /// Activate a generated runtime tool after review.
    AgentActivateGeneratedTool { tool_name: String },

    /// Validate provider credentials by testing connectivity.
    AgentValidateProvider {
        provider_id: String,
        base_url: String,
        api_key: String,
        auth_source: String,
    },

    /// Get authentication states for all configured providers.
    AgentGetProviderAuthStates,

    /// Set a provider's API key (surgical update, no full config round-trip).
    AgentLoginProvider {
        provider_id: String,
        api_key: String,
        #[serde(default)]
        base_url: String,
    },

    /// Clear a provider's API key.
    AgentLogoutProvider { provider_id: String },

    /// Create or update a sub-agent definition.
    AgentSetSubAgent { sub_agent_json: String },

    /// Remove a sub-agent definition by ID.
    AgentRemoveSubAgent { sub_agent_id: String },

    /// List all sub-agent definitions.
    AgentListSubAgents,

    /// Get concierge configuration.
    AgentGetConciergeConfig,

    /// Update concierge configuration.
    AgentSetConciergeConfig { config_json: String },

    /// Dismiss/prune the current welcome message.
    AgentDismissConciergeWelcome,

    /// Request a concierge welcome (sent by frontend on app mount).
    AgentRequestConciergeWelcome,

    /// Query audit trail with optional filters. Per D-08/TRNS-03.
    AuditQuery {
        action_types: Option<Vec<String>>,
        since: Option<u64>,
        limit: Option<usize>,
    },

    /// Cancel active escalation and return control to user. Per D-13/TRNS-05.
    EscalationCancel { thread_id: String },

    /// Dismiss an audit entry (user feedback signal). Per BEAT-09/D-04.
    AuditDismiss { entry_id: String },

    /// List skill variants with optional status filter. Per SKIL-03/D-09.
    SkillList {
        /// Filter by maturity status (draft, testing, active, proven, promoted_to_canonical).
        status: Option<String>,
        /// Maximum entries to return.
        limit: usize,
    },

    /// Inspect a specific skill by name or variant_id. Per SKIL-03/D-09.
    SkillInspect {
        /// Skill name or variant ID.
        identifier: String,
    },

    /// Reject (delete) a draft skill. Per SKIL-03/D-09.
    SkillReject {
        /// Skill name or variant ID.
        identifier: String,
    },

    /// Fast-promote a skill to a target status. Per SKIL-03/D-09.
    SkillPromote {
        /// Skill name or variant ID.
        identifier: String,
        /// Target status (e.g. "active").
        target_status: String,
    },

    /// Search the community skill registry. Per SKIL-07/D-07.
    SkillSearch { query: String },

    /// Import a community skill by name or URL. Per SKIL-07/D-07/D-08.
    SkillImport {
        /// Skill name from registry or direct URL.
        source: String,
        /// Override security warnings. Per D-05.
        force: bool,
        /// Whether the registry marks the publisher as verified. Per D-06.
        publisher_verified: bool,
    },

    /// Export a local skill to a file. Per SKIL-10/D-11.
    SkillExport {
        /// Skill name or variant ID.
        identifier: String,
        /// Output format: "tamux" or "agentskills".
        format: String,
        /// Output directory path.
        output_dir: String,
    },

    /// Publish a local skill to the community registry. Per SKIL-07/D-02.
    SkillPublish {
        /// Skill name or variant ID.
        identifier: String,
    },

    /// Query the agent's current capability tier, feature flags, and status snapshot.
    AgentStatusQuery,

    /// Set capability tier override (from settings UI). None clears override.
    AgentSetTierOverride {
        tier: Option<String>,
    },

    /// List all installed plugins. Per PLUG-09.
    PluginList {},

    /// Get details for a specific plugin by name. Per PLUG-09.
    PluginGet { name: String },

    /// Enable a plugin by name. Per PLUG-09.
    PluginEnable { name: String },

    /// Disable a plugin by name. Per PLUG-09.
    PluginDisable { name: String },

    /// Register a newly-installed plugin. Per INST-01/02/03/D-04.
    PluginInstall {
        /// Plugin directory name under ~/.tamux/plugins/
        dir_name: String,
        /// Install source identifier (e.g., "npm:package-name", "github:user/repo", "local:/path")
        install_source: String,
    },

    /// Uninstall a plugin: deregister from SQLite and memory. Per INST-04/D-06.
    PluginUninstall { name: String },

    /// Get all settings for a plugin. Per PSET-06.
    PluginGetSettings { name: String },

    /// Update a single plugin setting. Per PSET-06/D-06.
    PluginUpdateSettings {
        plugin_name: String,
        key: String,
        value: String,
        is_secret: bool,
    },

    /// Test plugin API connectivity. Per PSET-05/D-10.
    PluginTestConnection { name: String },

    /// Call a plugin API endpoint. Per APRX-02. Params is a JSON string.
    PluginApiCall {
        plugin_name: String,
        endpoint_name: String,
        params: String,
    },

    /// List all registered plugin commands. Per PSKL-05.
    PluginListCommands {},
}

// ---------------------------------------------------------------------------
// Daemon -> Client responses
// ---------------------------------------------------------------------------

/// Messages sent from the daemon back to a client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonMessage {
    /// Confirmation that a session was spawned.
    SessionSpawned { id: SessionId },

    /// Confirmation that a session was cloned.
    SessionCloned {
        source_id: SessionId,
        id: SessionId,
        /// The command that was actively running in the source session
        /// (detected via shell integration), if any.
        active_command: Option<String>,
    },

    /// Confirmation that the client is now attached.
    SessionAttached { id: SessionId },

    /// Confirmation of detach.
    SessionDetached { id: SessionId },

    /// Session was killed.
    SessionKilled { id: SessionId },

    /// Session exited on its own (process exited).
    SessionExited {
        id: SessionId,
        exit_code: Option<i32>,
    },

    /// Terminal output bytes from a session.
    Output { id: SessionId, data: Vec<u8> },

    /// Shell lifecycle marker emitted by the daemon.
    CommandStarted { id: SessionId, command: String },

    /// Shell lifecycle marker emitted by the daemon.
    CommandFinished {
        id: SessionId,
        exit_code: Option<i32>,
    },

    /// Managed command has been queued for serial execution.
    ManagedCommandQueued {
        id: SessionId,
        execution_id: String,
        position: usize,
        snapshot: Option<SnapshotInfo>,
    },

    /// Managed command is blocked pending approval.
    ApprovalRequired {
        id: SessionId,
        approval: ApprovalPayload,
    },

    /// Approval decision has been recorded.
    ApprovalResolved {
        id: SessionId,
        approval_id: String,
        decision: ApprovalDecision,
    },

    /// Managed command has started executing.
    ManagedCommandStarted {
        id: SessionId,
        execution_id: String,
        command: String,
        source: ManagedCommandSource,
    },

    /// Managed command completed.
    ManagedCommandFinished {
        id: SessionId,
        execution_id: String,
        command: String,
        exit_code: Option<i32>,
        duration_ms: Option<u64>,
        snapshot: Option<SnapshotInfo>,
    },

    /// Managed command was rejected by validation or policy.
    ManagedCommandRejected {
        id: SessionId,
        execution_id: Option<String>,
        message: String,
    },

    /// Reply to `ListSessions`.
    SessionList { sessions: Vec<SessionInfo> },

    /// Scrollback data reply.
    Scrollback { id: SessionId, data: Vec<u8> },

    /// AI analysis result.
    AnalysisResult { id: SessionId, result: String },

    /// Search results from SQLite/FTS-backed history.
    HistorySearchResult {
        query: String,
        summary: String,
        hits: Vec<HistorySearchHit>,
    },

    /// Reply containing command log rows serialized as JSON.
    CommandLogEntries { entries_json: String },

    /// Generic ack for command log write operations.
    CommandLogAck,

    /// Reply containing persisted agent thread summaries.
    AgentDbThreadList { threads_json: String },

    /// Reply containing a persisted agent thread plus its messages.
    AgentDbThreadDetail {
        thread_json: String,
        messages_json: String,
    },

    /// Generic ack for agent message writes.
    AgentDbMessageAck,

    /// Reply containing transcript index rows.
    TranscriptIndexEntries { entries_json: String },

    /// Reply containing snapshot index rows.
    SnapshotIndexEntries { entries_json: String },

    /// Reply containing agent event rows.
    AgentEventRows { events_json: String },

    /// Generated procedural skill document.
    SkillGenerated { title: String, path: String },

    /// Semantic symbol search results.
    SymbolSearchResult {
        symbol: String,
        matches: Vec<SymbolMatch>,
    },

    /// Recorded snapshots/checkpoints.
    SnapshotList { snapshots: Vec<SnapshotInfo> },

    /// Snapshot restore result.
    SnapshotRestored {
        snapshot_id: String,
        ok: bool,
        message: String,
    },

    /// OSC notification parsed from terminal output.
    OscNotification {
        id: SessionId,
        notification: OscNotificationPayload,
    },

    /// Git status reply.
    GitStatus { path: String, info: GitInfo },

    /// Git diff reply.
    GitDiff {
        repo_path: String,
        file_path: Option<String>,
        diff: String,
    },

    /// File preview reply.
    FilePreview {
        path: String,
        content: String,
        truncated: bool,
        is_text: bool,
    },

    /// Scrubbed text reply.
    ScrubResult { text: String },

    /// CWD change detected from a session.
    CwdChanged { id: SessionId, cwd: String },

    /// Telemetry integrity verification result.
    TelemetryIntegrityResult { results: Vec<TelemetryLedgerStatus> },

    /// Result of a CRIU checkpoint operation.
    SessionCheckpointed {
        id: SessionId,
        ok: bool,
        path: Option<String>,
        message: String,
    },

    /// Pong.
    Pong,

    /// Generic error.
    Error { message: String },

    // -----------------------------------------------------------------------
    // Agent engine responses
    // -----------------------------------------------------------------------
    /// Streamed agent event (delta, tool call, done, etc.).
    AgentEvent { event_json: String },

    /// Response to AgentListThreads.
    AgentThreadList { threads_json: String },

    /// Response to AgentGetThread.
    AgentThreadDetail { thread_json: String },

    /// Response to AgentListTasks.
    AgentTaskList { tasks_json: String },

    /// Response to AgentListRuns.
    AgentRunList { runs_json: String },

    /// Response to AgentGetRun.
    AgentRunDetail { run_json: String },

    /// Response to AgentAddTask.
    AgentTaskEnqueued { task_json: String },

    /// Response to AgentCancelTask.
    AgentTaskCancelled { task_id: String, cancelled: bool },

    /// Response to AgentStartGoalRun.
    AgentGoalRunStarted { goal_run_json: String },

    /// Response to AgentListGoalRuns.
    AgentGoalRunList { goal_runs_json: String },

    /// Response to AgentGetGoalRun.
    AgentGoalRunDetail { goal_run_json: String },

    /// Response to AgentControlGoalRun.
    AgentGoalRunControlled { goal_run_id: String, ok: bool },

    /// Response to AgentListTodos.
    AgentTodoList { todos_json: String },

    /// Response to AgentGetTodos.
    AgentTodoDetail {
        thread_id: String,
        todos_json: String,
    },

    /// Response to AgentGetWorkContext.
    AgentWorkContextDetail {
        thread_id: String,
        context_json: String,
    },

    /// Response to AgentGetConfig.
    AgentConfigResponse { config_json: String },

    /// Response to AgentFetchModels.
    AgentModelsResponse { models_json: String },

    /// Error response for agent operations.
    AgentError { message: String },

    /// Response to AgentHeartbeatGetItems.
    AgentHeartbeatItems { items_json: String },

    /// Response to AgentGetSubagentMetrics.
    AgentSubagentMetrics { metrics_json: String },

    /// Response to AgentListCheckpoints.
    AgentCheckpointList { checkpoints_json: String },

    /// Response to AgentRestoreCheckpoint.
    AgentCheckpointRestored { outcome_json: String },

    /// Response to AgentGetHealthStatus.
    AgentHealthStatus { status_json: String },

    /// Response to AgentListHealthLog.
    AgentHealthLog { entries_json: String },

    /// Response to AgentGetOperatorModel.
    AgentOperatorModel { model_json: String },

    /// Response to AgentResetOperatorModel.
    AgentOperatorModelReset { ok: bool },

    /// Response to AgentGetCausalTraceReport.
    AgentCausalTraceReport { report_json: String },

    /// Response to AgentGetCounterfactualReport.
    AgentCounterfactualReport { report_json: String },

    /// Response to AgentGetMemoryProvenanceReport.
    AgentMemoryProvenanceReport { report_json: String },

    /// Response to AgentGetProvenanceReport.
    AgentProvenanceReport { report_json: String },

    /// Response to AgentGenerateSoc2Artifact.
    AgentSoc2Artifact { artifact_path: String },

    /// Response to AgentGetCollaborationSessions.
    AgentCollaborationSessions { sessions_json: String },

    /// Response to AgentListGeneratedTools.
    AgentGeneratedTools { tools_json: String },

    /// Response to AgentSynthesizeTool / AgentRunGeneratedTool / AgentPromoteGeneratedTool.
    AgentGeneratedToolResult {
        #[serde(default)]
        tool_name: Option<String>,
        result_json: String,
    },

    /// Response to AgentValidateProvider.
    AgentProviderValidation {
        provider_id: String,
        valid: bool,
        #[serde(default)]
        error: Option<String>,
        #[serde(default)]
        models_json: Option<String>,
    },

    /// Response to AgentGetProviderAuthStates.
    AgentProviderAuthStates { states_json: String },

    /// Response to AgentListSubAgents.
    AgentSubAgentList { sub_agents_json: String },

    /// Confirmation of AgentSetSubAgent.
    AgentSubAgentUpdated { sub_agent_json: String },

    /// Confirmation of AgentRemoveSubAgent.
    AgentSubAgentRemoved { sub_agent_id: String },

    /// Response to AgentGetConciergeConfig.
    AgentConciergeConfig { config_json: String },

    /// Confirmation that welcome was dismissed.
    AgentConciergeWelcomeDismissed,

    /// Audit trail query response. Per D-08/TRNS-03.
    AuditList {
        /// Serialized `Vec<AuditEntryPublic>` as JSON.
        entries_json: String,
    },

    /// Escalation cancel result. Per D-13/TRNS-05.
    EscalationCancelResult { success: bool, message: String },

    /// Audit dismiss result. Per BEAT-09/D-04.
    AuditDismissResult { success: bool, message: String },

    /// Response to SkillList -- list of skill variant records. Per SKIL-03/D-09.
    SkillListResult { variants: Vec<SkillVariantPublic> },

    /// Response to SkillInspect -- single skill detail with content. Per SKIL-03/D-09.
    SkillInspectResult {
        variant: Option<SkillVariantPublic>,
        content: Option<String>,
    },

    /// Response to SkillReject/SkillPromote. Per SKIL-03/D-09.
    SkillActionResult { success: bool, message: String },

    /// Response to SkillSearch. Per SKIL-07/D-07.
    SkillSearchResult { entries: Vec<CommunitySkillEntry> },

    /// Response to SkillImport. Per SKIL-07/D-08/D-09.
    SkillImportResult {
        success: bool,
        message: String,
        variant_id: Option<String>,
        scan_verdict: Option<String>,
        findings_count: u32,
    },

    /// Response to SkillExport. Per SKIL-10/D-11.
    SkillExportResult {
        success: bool,
        message: String,
        output_path: Option<String>,
    },

    /// Response to SkillPublish. Per SKIL-07/D-02.
    SkillPublishResult { success: bool, message: String },

    /// Capability tier changed (pushed to all clients on tier transitions).
    AgentTierChanged {
        previous_tier: String,
        new_tier: String,
        reason: String,
    },

    /// Agent status snapshot (response to AgentStatusQuery).
    AgentStatusResponse {
        tier: String,
        feature_flags_json: String,
        activity: String,
        #[serde(default)]
        active_thread_id: Option<String>,
        #[serde(default)]
        active_goal_run_id: Option<String>,
        #[serde(default)]
        active_goal_run_title: Option<String>,
        provider_health_json: String,
        gateway_statuses_json: String,
        recent_actions_json: String,
    },

    /// Response to PluginList -- list of installed plugins. Per PLUG-09.
    PluginListResult { plugins: Vec<PluginInfo> },

    /// Response to PluginGet -- single plugin detail with optional settings schema JSON. Per PLUG-09.
    PluginGetResult {
        plugin: Option<PluginInfo>,
        settings_schema: Option<String>,
    },

    /// Response to PluginEnable/PluginDisable. Per PLUG-09.
    PluginActionResult { success: bool, message: String },

    /// Response to PluginGetSettings. Per PSET-06.
    PluginSettingsResult {
        plugin_name: String,
        /// Key-value pairs. Secret values are masked with "********".
        settings: Vec<(String, String, bool)>, // (key, value, is_secret)
    },

    /// Response to PluginTestConnection. Per PSET-05/D-10.
    PluginTestConnectionResult {
        plugin_name: String,
        success: bool,
        message: String,
    },

    /// Response to PluginApiCall. Per APRX-02.
    PluginApiCallResult {
        plugin_name: String,
        endpoint_name: String,
        success: bool,
        /// Result body (rendered response on success, error message on failure).
        result: String,
        /// Error type key (e.g. "ssrf_blocked", "rate_limited", "timeout") on failure.
        error_type: Option<String>,
    },

    /// Response to PluginListCommands. Per PSKL-05.
    PluginCommandsResult {
        commands: Vec<PluginCommandInfo>,
    },
}

// ---------------------------------------------------------------------------
// Supporting types
// ---------------------------------------------------------------------------

/// Public audit entry type shared across all crates. Per D-06.
/// The daemon maps `AuditEntryRow` -> `AuditEntryPublic` for IPC responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntryPublic {
    pub id: String,
    pub timestamp: i64,
    pub action_type: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence_band: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causal_trace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

/// Public skill variant record shared across all crates. Per SKIL-03/D-09.
/// The daemon maps `SkillVariantRecord` -> `SkillVariantPublic` for IPC responses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillVariantPublic {
    pub variant_id: String,
    pub skill_name: String,
    pub variant_name: String,
    pub relative_path: String,
    pub status: String,
    pub use_count: u32,
    pub success_count: u32,
    pub failure_count: u32,
    pub context_tags: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Community registry skill entry. Per SKIL-07/D-01.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommunitySkillEntry {
    pub name: String,
    pub description: String,
    pub version: String,
    pub publisher_id: String,
    pub publisher_verified: bool,
    pub success_rate: f64,
    pub use_count: u32,
    pub content_hash: String,
    pub tamux_version: String,
    pub maturity_at_publish: String,
    pub tags: Vec<String>,
    pub published_at: u64,
}

/// Public scan report for IPC responses. Per SKIL-09/D-04.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanReportPublic {
    pub verdict: String,
    pub findings_count: u32,
    pub critical_count: u32,
    pub suspicious_count: u32,
}

/// Public plugin info shared across all crates. Per PLUG-09.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub enabled: bool,
    pub install_source: String,
    pub has_api: bool,
    pub has_auth: bool,
    pub has_commands: bool,
    pub has_skills: bool,
    pub endpoint_count: u32,
    pub settings_count: u32,
    pub installed_at: String,
    pub updated_at: String,
}

/// Info about a registered plugin command. Per PSKL-05.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginCommandInfo {
    pub command: String,
    pub plugin_name: String,
    pub description: String,
    pub api_endpoint: Option<String>,
}

/// Metadata about a running session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub cols: u16,
    pub rows: u16,
    pub created_at: u64, // Unix timestamp
    pub workspace_id: Option<WorkspaceId>,
    pub exit_code: Option<i32>,
    pub is_alive: bool,
    pub active_command: Option<String>,
}

/// Frontend workspace topology snapshot — used by the daemon to include
/// non-session panes (e.g. browser panels) in `list_terminals`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceTopology {
    pub workspaces: Vec<WorkspaceTopologyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceTopologyEntry {
    pub workspace_id: WorkspaceId,
    pub workspace_name: String,
    pub surfaces: Vec<SurfaceTopologyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceTopologyEntry {
    pub surface_id: String,
    pub surface_name: String,
    pub layout_mode: String,
    pub is_active: bool,
    pub panes: Vec<PaneTopologyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneTopologyEntry {
    pub pane_id: String,
    pub pane_name: String,
    pub pane_type: String, // "terminal" | "browser"
    pub is_active: bool,
    pub session_id: Option<String>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub cwd: Option<String>,
}

/// Format a workspace topology into a human-readable string, enriched with
/// session metadata (CWD, active command) where available.
pub fn format_topology(topology: &WorkspaceTopology, sessions: &[SessionInfo]) -> String {
    let session_map: std::collections::HashMap<String, &SessionInfo> =
        sessions.iter().map(|s| (s.id.to_string(), s)).collect();

    let mut lines = Vec::new();
    for ws in &topology.workspaces {
        lines.push(format!("Workspace \"{}\":", ws.workspace_name));
        for sf in &ws.surfaces {
            let active_tag = if sf.is_active { " (active)" } else { "" };
            lines.push(format!(
                "  Surface \"{}\" ({}{}):",
                sf.surface_name, sf.layout_mode, active_tag
            ));
            for pane in &sf.panes {
                let active_tag = if pane.is_active { " (active)" } else { "" };
                let mut parts = vec![format!(
                    "    - {} [{}] type={}",
                    pane.pane_name, pane.pane_id, pane.pane_type
                )];
                if pane.pane_type == "browser" {
                    if let Some(url) = &pane.url {
                        parts.push(format!("url={url}"));
                    }
                    if let Some(title) = &pane.title {
                        parts.push(format!("title={title}"));
                    }
                } else if let Some(sid) = &pane.session_id {
                    parts.push(format!("session={sid}"));
                    // Prefer panel-level CWD (live from shell integration), fall back to session CWD.
                    let cwd = pane
                        .cwd
                        .as_deref()
                        .or_else(|| session_map.get(sid).and_then(|s| s.cwd.as_deref()));
                    if let Some(cwd) = cwd {
                        parts.push(format!("cwd={cwd}"));
                    }
                    if let Some(s) = session_map.get(sid) {
                        if let Some(cmd) = s.active_command.as_deref() {
                            parts.push(format!("cmd={cmd}"));
                        }
                    }
                }
                if !active_tag.is_empty() {
                    parts.push(active_tag.trim().to_string());
                }
                lines.push(parts.join(" "));
            }
        }
    }
    lines.join("\n")
}

/// OSC notification payload (parsed from OSC 9, 99, 777).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OscNotificationPayload {
    pub source: OscSource,
    pub title: String,
    pub body: String,
    pub subtitle: Option<String>,
    pub icon: Option<String>,
    pub progress: Option<u8>, // 0..100
}

/// OSC notification source type.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum OscSource {
    /// OSC 9 — simple notification (iTerm2 Growl).
    Osc9,
    /// OSC 99 — structured notification (kitty).
    Osc99,
    /// OSC 777 — notification (rxvt-unicode).
    Osc777,
}

/// Git repository information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    pub branch: Option<String>,
    pub is_dirty: bool,
    pub ahead: u32,
    pub behind: u32,
    pub untracked: u32,
    pub modified: u32,
    pub staged: u32,
}

/// A single git working tree change entry.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitChangeEntry {
    pub code: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_path: Option<String>,
    pub kind: String,
}

/// Source of a managed command request.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManagedCommandSource {
    Human,
    Agent,
    Replay,
    Gateway,
}

/// Approval decision for daemon-managed commands.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalDecision {
    ApproveOnce,
    ApproveSession,
    Deny,
}

/// Security policy level controlling approval strictness.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SecurityLevel {
    Highest,
    #[default]
    Moderate,
    Lowest,
    Yolo,
}

/// Request describing a managed command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedCommandRequest {
    pub command: String,
    pub rationale: String,
    pub allow_network: bool,
    #[serde(default)]
    pub sandbox_enabled: bool,
    #[serde(default)]
    pub security_level: SecurityLevel,
    pub cwd: Option<String>,
    pub language_hint: Option<String>,
    pub source: ManagedCommandSource,
}

/// Structured approval payload rendered by the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalPayload {
    pub approval_id: String,
    pub execution_id: String,
    pub command: String,
    pub rationale: String,
    pub risk_level: String,
    pub blast_radius: String,
    pub reasons: Vec<String>,
    pub workspace_id: Option<WorkspaceId>,
    pub allow_network: bool,
}

/// Recorded snapshot/checkpoint metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub snapshot_id: String,
    pub workspace_id: Option<WorkspaceId>,
    pub session_id: Option<SessionId>,
    pub command: Option<String>,
    pub kind: String,
    pub label: String,
    pub path: String,
    pub created_at: u64,
    pub status: String,
    pub details: String,
}

/// Search hit from historical command/transcript recall.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistorySearchHit {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub excerpt: String,
    pub path: Option<String>,
    pub timestamp: u64,
    pub score: f64,
}

/// SQLite-backed command log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandLogEntry {
    pub id: String,
    pub command: String,
    pub timestamp: i64,
    pub path: Option<String>,
    pub cwd: Option<String>,
    pub workspace_id: Option<WorkspaceId>,
    pub surface_id: Option<String>,
    pub pane_id: Option<String>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<i64>,
}

/// SQLite-backed agent thread summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDbThread {
    pub id: String,
    pub workspace_id: Option<WorkspaceId>,
    pub surface_id: Option<String>,
    pub pane_id: Option<String>,
    pub agent_name: Option<String>,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: i64,
    pub total_tokens: i64,
    pub last_preview: String,
    pub metadata_json: Option<String>,
}

/// SQLite-backed agent message record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDbMessage {
    pub id: String,
    pub thread_id: String,
    pub created_at: i64,
    pub role: String,
    pub content: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub reasoning: Option<String>,
    pub tool_calls_json: Option<String>,
    pub metadata_json: Option<String>,
}

/// Cached WORM ledger chain tip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WormChainTip {
    pub kind: String,
    pub seq: i64,
    pub hash: String,
}

/// Indexed transcript metadata stored in SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptIndexEntry {
    pub id: String,
    pub pane_id: Option<String>,
    pub workspace_id: Option<WorkspaceId>,
    pub surface_id: Option<String>,
    pub filename: String,
    pub reason: Option<String>,
    pub captured_at: i64,
    pub size_bytes: Option<i64>,
    pub preview: Option<String>,
}

/// Indexed snapshot metadata stored in SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotIndexEntry {
    pub snapshot_id: String,
    pub workspace_id: Option<WorkspaceId>,
    pub session_id: Option<String>,
    pub kind: String,
    pub label: Option<String>,
    pub path: String,
    pub created_at: i64,
    pub details_json: Option<String>,
}

/// Generic agent mission event row stored in SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEventRow {
    pub id: String,
    pub category: String,
    pub kind: String,
    pub pane_id: Option<String>,
    pub workspace_id: Option<WorkspaceId>,
    pub surface_id: Option<String>,
    pub session_id: Option<String>,
    pub payload_json: String,
    pub timestamp: i64,
}

/// Symbol search result emitted by the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolMatch {
    pub path: String,
    pub line: usize,
    pub kind: String,
    pub snippet: String,
}

/// Status of a single WORM telemetry ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryLedgerStatus {
    pub kind: String,
    pub total_entries: usize,
    pub valid: bool,
    pub first_invalid_seq: Option<usize>,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{AmuxCodec, DaemonCodec};
    use bytes::BytesMut;
    use tokio_util::codec::{Decoder, Encoder};

    #[test]
    fn agent_provider_validation_bincode_roundtrip() {
        let msg = DaemonMessage::AgentProviderValidation {
            provider_id: "openrouter".to_string(),
            valid: true,
            error: None,
            models_json: None,
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            DaemonMessage::AgentProviderValidation {
                provider_id,
                valid,
                error,
                models_json,
            } => {
                assert_eq!(provider_id, "openrouter");
                assert!(valid);
                assert!(error.is_none());
                assert!(models_json.is_none());
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn agent_provider_validation_codec_roundtrip() {
        let msg = DaemonMessage::AgentProviderValidation {
            provider_id: "openrouter".to_string(),
            valid: true,
            error: None,
            models_json: None,
        };

        // Encode with DaemonCodec (daemon side), decode with AmuxCodec (client side)
        let mut daemon_codec = DaemonCodec;
        let mut buf = BytesMut::new();
        daemon_codec.encode(msg.clone(), &mut buf).unwrap();

        let mut client_codec = AmuxCodec;
        let decoded = client_codec.decode(&mut buf).unwrap().unwrap();
        match decoded {
            DaemonMessage::AgentProviderValidation {
                provider_id,
                valid,
                error,
                models_json,
            } => {
                assert_eq!(provider_id, "openrouter");
                assert!(valid);
                assert!(error.is_none());
                assert!(models_json.is_none());
            }
            other => panic!("decoded wrong variant: {:?}", other),
        }
    }

    fn sample_skill_variant() -> SkillVariantPublic {
        SkillVariantPublic {
            variant_id: "sv-001".to_string(),
            skill_name: "git_rebase_workflow".to_string(),
            variant_name: "v1".to_string(),
            relative_path: "drafts/git_rebase_workflow/SKILL.md".to_string(),
            status: "active".to_string(),
            use_count: 12,
            success_count: 10,
            failure_count: 2,
            context_tags: vec!["git".to_string(), "rebase".to_string()],
            created_at: 1700000000,
            updated_at: 1700001000,
        }
    }

    fn sample_community_skill_entry() -> CommunitySkillEntry {
        CommunitySkillEntry {
            name: "git-rebase-workflow".to_string(),
            description: "Safely rebase a feature branch".to_string(),
            version: "1.2.0".to_string(),
            publisher_id: "abcd1234efgh5678".to_string(),
            publisher_verified: true,
            success_rate: 0.93,
            use_count: 42,
            content_hash: "d34db33f".to_string(),
            tamux_version: "0.1.10".to_string(),
            maturity_at_publish: "proven".to_string(),
            tags: vec!["git".to_string(), "workflow".to_string()],
            published_at: 1700001234,
        }
    }

    fn sample_scan_report_public() -> ScanReportPublic {
        ScanReportPublic {
            verdict: "warn".to_string(),
            findings_count: 3,
            critical_count: 0,
            suspicious_count: 3,
        }
    }

    // -----------------------------------------------------------------------
    // ClientMessage round-trips
    // -----------------------------------------------------------------------

    #[test]
    fn skill_list_with_status_bincode_roundtrip() {
        let msg = ClientMessage::SkillList {
            status: Some("draft".to_string()),
            limit: 50,
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::SkillList { status, limit } => {
                assert_eq!(status, Some("draft".to_string()));
                assert_eq!(limit, 50);
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn skill_list_without_status_bincode_roundtrip() {
        let msg = ClientMessage::SkillList {
            status: None,
            limit: 10,
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::SkillList { status, limit } => {
                assert_eq!(status, None);
                assert_eq!(limit, 10);
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn skill_inspect_bincode_roundtrip() {
        let msg = ClientMessage::SkillInspect {
            identifier: "test-skill".to_string(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::SkillInspect { identifier } => {
                assert_eq!(identifier, "test-skill");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn skill_reject_bincode_roundtrip() {
        let msg = ClientMessage::SkillReject {
            identifier: "test-skill".to_string(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::SkillReject { identifier } => {
                assert_eq!(identifier, "test-skill");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn skill_promote_bincode_roundtrip() {
        let msg = ClientMessage::SkillPromote {
            identifier: "test-skill".to_string(),
            target_status: "active".to_string(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::SkillPromote {
                identifier,
                target_status,
            } => {
                assert_eq!(identifier, "test-skill");
                assert_eq!(target_status, "active");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn skill_search_bincode_roundtrip() {
        let msg = ClientMessage::SkillSearch {
            query: "git".to_string(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::SkillSearch { query } => {
                assert_eq!(query, "git");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn skill_import_bincode_roundtrip() {
        let msg = ClientMessage::SkillImport {
            source: "https://registry.tamux.dev/skills/git.tar.gz".to_string(),
            force: true,
            publisher_verified: true,
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::SkillImport {
                source,
                force,
                publisher_verified,
            } => {
                assert_eq!(source, "https://registry.tamux.dev/skills/git.tar.gz");
                assert!(force);
                assert!(publisher_verified);
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn skill_export_bincode_roundtrip() {
        let msg = ClientMessage::SkillExport {
            identifier: "test-skill".to_string(),
            format: "agentskills".to_string(),
            output_dir: "/tmp/export".to_string(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::SkillExport {
                identifier,
                format,
                output_dir,
            } => {
                assert_eq!(identifier, "test-skill");
                assert_eq!(format, "agentskills");
                assert_eq!(output_dir, "/tmp/export");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn skill_publish_bincode_roundtrip() {
        let msg = ClientMessage::SkillPublish {
            identifier: "test-skill".to_string(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::SkillPublish { identifier } => {
                assert_eq!(identifier, "test-skill");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // DaemonMessage round-trips
    // -----------------------------------------------------------------------

    #[test]
    fn skill_list_result_with_variants_bincode_roundtrip() {
        let msg = DaemonMessage::SkillListResult {
            variants: vec![sample_skill_variant()],
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            DaemonMessage::SkillListResult { variants } => {
                assert_eq!(variants.len(), 1);
                assert_eq!(variants[0], sample_skill_variant());
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn skill_list_result_empty_bincode_roundtrip() {
        let msg = DaemonMessage::SkillListResult { variants: vec![] };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            DaemonMessage::SkillListResult { variants } => {
                assert!(variants.is_empty());
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn skill_inspect_result_none_bincode_roundtrip() {
        let msg = DaemonMessage::SkillInspectResult {
            variant: None,
            content: None,
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            DaemonMessage::SkillInspectResult { variant, content } => {
                assert!(variant.is_none());
                assert!(content.is_none());
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn skill_action_result_bincode_roundtrip() {
        let msg = DaemonMessage::SkillActionResult {
            success: true,
            message: "ok".to_string(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            DaemonMessage::SkillActionResult { success, message } => {
                assert!(success);
                assert_eq!(message, "ok");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn skill_search_result_bincode_roundtrip() {
        let msg = DaemonMessage::SkillSearchResult {
            entries: vec![sample_community_skill_entry()],
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            DaemonMessage::SkillSearchResult { entries } => {
                assert_eq!(entries, vec![sample_community_skill_entry()]);
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn skill_import_result_bincode_roundtrip() {
        let msg = DaemonMessage::SkillImportResult {
            success: true,
            message: "imported".to_string(),
            variant_id: Some("variant-1".to_string()),
            scan_verdict: Some("warn".to_string()),
            findings_count: 2,
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            DaemonMessage::SkillImportResult {
                success,
                message,
                variant_id,
                scan_verdict,
                findings_count,
            } => {
                assert!(success);
                assert_eq!(message, "imported");
                assert_eq!(variant_id.as_deref(), Some("variant-1"));
                assert_eq!(scan_verdict.as_deref(), Some("warn"));
                assert_eq!(findings_count, 2);
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn skill_export_result_bincode_roundtrip() {
        let msg = DaemonMessage::SkillExportResult {
            success: true,
            message: "exported".to_string(),
            output_path: Some("/tmp/export/skill.md".to_string()),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            DaemonMessage::SkillExportResult {
                success,
                message,
                output_path,
            } => {
                assert!(success);
                assert_eq!(message, "exported");
                assert_eq!(output_path.as_deref(), Some("/tmp/export/skill.md"));
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn skill_publish_result_bincode_roundtrip() {
        let msg = DaemonMessage::SkillPublishResult {
            success: true,
            message: "published".to_string(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            DaemonMessage::SkillPublishResult { success, message } => {
                assert!(success);
                assert_eq!(message, "published");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // SkillVariantPublic serde
    // -----------------------------------------------------------------------

    #[test]
    fn skill_variant_public_json_roundtrip() {
        let variant = sample_skill_variant();
        let json = serde_json::to_string(&variant).unwrap();
        let decoded: SkillVariantPublic = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, variant);
    }

    #[test]
    fn community_skill_entry_json_roundtrip() {
        let entry = sample_community_skill_entry();
        let json = serde_json::to_string(&entry).unwrap();
        let decoded: CommunitySkillEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, entry);
    }

    #[test]
    fn scan_report_public_json_roundtrip() {
        let report = sample_scan_report_public();
        let json = serde_json::to_string(&report).unwrap();
        let decoded: ScanReportPublic = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, report);
    }

    // -----------------------------------------------------------------------
    // Plugin IPC bincode round-trips
    // -----------------------------------------------------------------------

    fn sample_plugin_info() -> PluginInfo {
        PluginInfo {
            name: "test-plugin".into(),
            version: "1.0.0".into(),
            description: Some("A test plugin".into()),
            author: Some("Test Author".into()),
            enabled: true,
            install_source: "local".into(),
            has_api: true,
            has_auth: false,
            has_commands: true,
            has_skills: false,
            endpoint_count: 3,
            settings_count: 2,
            installed_at: "2026-03-24T00:00:00Z".into(),
            updated_at: "2026-03-24T00:00:00Z".into(),
        }
    }

    #[test]
    fn plugin_list_bincode_roundtrip() {
        let msg = ClientMessage::PluginList {};
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::PluginList {} => {}
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_get_bincode_roundtrip() {
        let msg = ClientMessage::PluginGet {
            name: "gmail".into(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::PluginGet { name } => {
                assert_eq!(name, "gmail");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_enable_bincode_roundtrip() {
        let msg = ClientMessage::PluginEnable {
            name: "gmail".into(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::PluginEnable { name } => {
                assert_eq!(name, "gmail");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_disable_bincode_roundtrip() {
        let msg = ClientMessage::PluginDisable {
            name: "gmail".into(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::PluginDisable { name } => {
                assert_eq!(name, "gmail");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_install_bincode_roundtrip() {
        let msg = ClientMessage::PluginInstall {
            dir_name: "gmail-plugin".into(),
            install_source: "npm:tamux-plugin-gmail".into(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::PluginInstall {
                dir_name,
                install_source,
            } => {
                assert_eq!(dir_name, "gmail-plugin");
                assert_eq!(install_source, "npm:tamux-plugin-gmail");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_uninstall_bincode_roundtrip() {
        let msg = ClientMessage::PluginUninstall {
            name: "gmail".into(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::PluginUninstall { name } => {
                assert_eq!(name, "gmail");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_list_result_bincode_roundtrip() {
        let msg = DaemonMessage::PluginListResult {
            plugins: vec![sample_plugin_info()],
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            DaemonMessage::PluginListResult { plugins } => {
                assert_eq!(plugins.len(), 1);
                assert_eq!(plugins[0], sample_plugin_info());
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_get_result_some_bincode_roundtrip() {
        let msg = DaemonMessage::PluginGetResult {
            plugin: Some(sample_plugin_info()),
            settings_schema: Some(r#"{"type":"object"}"#.into()),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            DaemonMessage::PluginGetResult {
                plugin,
                settings_schema,
            } => {
                assert_eq!(plugin, Some(sample_plugin_info()));
                assert_eq!(settings_schema.as_deref(), Some(r#"{"type":"object"}"#));
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_get_result_none_bincode_roundtrip() {
        let msg = DaemonMessage::PluginGetResult {
            plugin: None,
            settings_schema: None,
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            DaemonMessage::PluginGetResult {
                plugin,
                settings_schema,
            } => {
                assert!(plugin.is_none());
                assert!(settings_schema.is_none());
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_action_result_bincode_roundtrip() {
        let msg = DaemonMessage::PluginActionResult {
            success: true,
            message: "ok".into(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            DaemonMessage::PluginActionResult { success, message } => {
                assert!(success);
                assert_eq!(message, "ok");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_get_settings_bincode_roundtrip() {
        let msg = ClientMessage::PluginGetSettings {
            name: "gmail".into(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::PluginGetSettings { name } => {
                assert_eq!(name, "gmail");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_update_settings_bincode_roundtrip() {
        let msg = ClientMessage::PluginUpdateSettings {
            plugin_name: "gmail".into(),
            key: "api_key".into(),
            value: "secret123".into(),
            is_secret: true,
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::PluginUpdateSettings {
                plugin_name,
                key,
                value,
                is_secret,
            } => {
                assert_eq!(plugin_name, "gmail");
                assert_eq!(key, "api_key");
                assert_eq!(value, "secret123");
                assert!(is_secret);
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_test_connection_bincode_roundtrip() {
        let msg = ClientMessage::PluginTestConnection {
            name: "gmail".into(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::PluginTestConnection { name } => {
                assert_eq!(name, "gmail");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_settings_result_bincode_roundtrip() {
        let msg = DaemonMessage::PluginSettingsResult {
            plugin_name: "gmail".into(),
            settings: vec![
                ("api_key".into(), "********".into(), true),
                ("base_url".into(), "https://api.gmail.com".into(), false),
            ],
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            DaemonMessage::PluginSettingsResult {
                plugin_name,
                settings,
            } => {
                assert_eq!(plugin_name, "gmail");
                assert_eq!(settings.len(), 2);
                assert_eq!(settings[0], ("api_key".into(), "********".into(), true));
                assert_eq!(
                    settings[1],
                    ("base_url".into(), "https://api.gmail.com".into(), false)
                );
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_test_connection_result_bincode_roundtrip() {
        let msg = DaemonMessage::PluginTestConnectionResult {
            plugin_name: "gmail".into(),
            success: true,
            message: "Connection successful".into(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            DaemonMessage::PluginTestConnectionResult {
                plugin_name,
                success,
                message,
            } => {
                assert_eq!(plugin_name, "gmail");
                assert!(success);
                assert_eq!(message, "Connection successful");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_api_call_bincode_roundtrip() {
        let msg = ClientMessage::PluginApiCall {
            plugin_name: "gmail".into(),
            endpoint_name: "list_messages".into(),
            params: r#"{"query":"is:unread"}"#.into(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::PluginApiCall {
                plugin_name,
                endpoint_name,
                params,
            } => {
                assert_eq!(plugin_name, "gmail");
                assert_eq!(endpoint_name, "list_messages");
                assert_eq!(params, r#"{"query":"is:unread"}"#);
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_api_call_result_success_bincode_roundtrip() {
        let msg = DaemonMessage::PluginApiCallResult {
            plugin_name: "gmail".into(),
            endpoint_name: "list_messages".into(),
            success: true,
            result: "Subject: Hello\nFrom: alice@example.com".into(),
            error_type: None,
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            DaemonMessage::PluginApiCallResult {
                plugin_name,
                endpoint_name,
                success,
                result,
                error_type,
            } => {
                assert_eq!(plugin_name, "gmail");
                assert_eq!(endpoint_name, "list_messages");
                assert!(success);
                assert!(result.contains("Hello"));
                assert!(error_type.is_none());
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_api_call_result_error_bincode_roundtrip() {
        let msg = DaemonMessage::PluginApiCallResult {
            plugin_name: "gmail".into(),
            endpoint_name: "list_messages".into(),
            success: false,
            result: "Rate limited".into(),
            error_type: Some("rate_limited".into()),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            DaemonMessage::PluginApiCallResult {
                plugin_name,
                endpoint_name,
                success,
                result,
                error_type,
            } => {
                assert_eq!(plugin_name, "gmail");
                assert_eq!(endpoint_name, "list_messages");
                assert!(!success);
                assert_eq!(result, "Rate limited");
                assert_eq!(error_type.as_deref(), Some("rate_limited"));
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_info_json_roundtrip() {
        let info = sample_plugin_info();
        let json = serde_json::to_string(&info).unwrap();
        let decoded: PluginInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, info);
    }

    #[test]
    fn plugin_list_commands_bincode_roundtrip() {
        let msg = ClientMessage::PluginListCommands {};
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::PluginListCommands {} => {}
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn plugin_commands_result_bincode_roundtrip() {
        let msg = DaemonMessage::PluginCommandsResult {
            commands: vec![
                PluginCommandInfo {
                    command: "/gmail.inbox".into(),
                    plugin_name: "gmail".into(),
                    description: "Show inbox".into(),
                    api_endpoint: Some("list_messages".into()),
                },
                PluginCommandInfo {
                    command: "/weather.forecast".into(),
                    plugin_name: "weather".into(),
                    description: "Get forecast".into(),
                    api_endpoint: None,
                },
            ],
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            DaemonMessage::PluginCommandsResult { commands } => {
                assert_eq!(commands.len(), 2);
                assert_eq!(commands[0].command, "/gmail.inbox");
                assert_eq!(commands[0].plugin_name, "gmail");
                assert_eq!(commands[0].description, "Show inbox");
                assert_eq!(commands[0].api_endpoint.as_deref(), Some("list_messages"));
                assert_eq!(commands[1].command, "/weather.forecast");
                assert_eq!(commands[1].api_endpoint, None);
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }
}
