use amux_protocol::{
    AmuxCodec, ApprovalDecision, ApprovalPayload, ClientMessage, DaemonMessage, HistorySearchHit,
    ManagedCommandRequest, ManagedCommandSource, OscNotificationPayload, SecurityLevel,
    SessionInfo, SnapshotInfo, SymbolMatch,
};
use anyhow::{Context, Result};
use base64::Engine;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::codec::Framed;

const BASE64_ENGINE: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum BridgeCommand {
    Input {
        data: String,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    ExecuteManaged {
        command: String,
        rationale: String,
        allow_network: bool,
        sandbox_enabled: Option<bool>,
        security_level: Option<String>,
        cwd: Option<String>,
        language_hint: Option<String>,
        source: Option<String>,
    },
    ApprovalDecision {
        approval_id: String,
        decision: String,
    },
    SearchHistory {
        query: String,
        limit: Option<usize>,
    },
    GenerateSkill {
        query: Option<String>,
        title: Option<String>,
    },
    FindSymbol {
        workspace_root: String,
        symbol: String,
        limit: Option<usize>,
    },
    ListSnapshots {
        workspace_id: Option<String>,
    },
    RestoreSnapshot {
        snapshot_id: String,
    },
    Shutdown,
    KillSession,
}

/// Gracefully deserialize context_messages — if the array contains malformed entries, drop them rather than failing.
fn deserialize_context_messages<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<amux_protocol::AgentDbMessage>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let raw: Option<Vec<serde_json::Value>> = Option::deserialize(deserializer)?;
    match raw {
        None => Ok(None),
        Some(arr) => {
            let parsed: Vec<amux_protocol::AgentDbMessage> = arr
                .into_iter()
                .filter_map(|v| serde_json::from_value(v).ok())
                .collect();
            if parsed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(parsed))
            }
        }
    }
}

/// Commands for the agent bridge (JSON over stdin).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum AgentBridgeCommand {
    SendMessage {
        thread_id: Option<String>,
        content: String,
        session_id: Option<String>,
        #[serde(default, deserialize_with = "deserialize_context_messages")]
        context_messages: Option<Vec<amux_protocol::AgentDbMessage>>,
    },
    StopStream {
        thread_id: String,
    },
    ListThreads,
    GetThread {
        thread_id: String,
    },
    DeleteThread {
        thread_id: String,
    },
    AddTask {
        title: String,
        description: String,
        priority: Option<String>,
        command: Option<String>,
        session_id: Option<String>,
        scheduled_at: Option<u64>,
        #[serde(default)]
        dependencies: Vec<String>,
    },
    CancelTask {
        task_id: String,
    },
    ListTasks,
    ListRuns,
    GetRun {
        run_id: String,
    },
    StartGoalRun {
        goal: String,
        title: Option<String>,
        thread_id: Option<String>,
        session_id: Option<String>,
        priority: Option<String>,
        client_request_id: Option<String>,
    },
    ListGoalRuns,
    GetGoalRun {
        goal_run_id: String,
    },
    ControlGoalRun {
        goal_run_id: String,
        action: String,
        step_index: Option<usize>,
    },
    ListTodos,
    GetTodos {
        thread_id: String,
    },
    GetWorkContext {
        thread_id: String,
    },
    GetGitDiff {
        repo_path: String,
        file_path: Option<String>,
    },
    GetFilePreview {
        path: String,
        max_bytes: Option<usize>,
    },
    GetConfig,
    SetConfigItem {
        key_path: String,
        value_json: String,
    },
    HeartbeatGetItems,
    HeartbeatSetItems {
        items_json: String,
    },
    ResolveTaskApproval {
        approval_id: String,
        decision: String,
    },
    ValidateProvider {
        provider_id: String,
        base_url: String,
        api_key: String,
        auth_source: String,
    },
    GetProviderAuthStates,
    LoginProvider {
        provider_id: String,
        api_key: String,
        #[serde(default)]
        base_url: String,
    },
    LogoutProvider {
        provider_id: String,
    },
    SetSubAgent {
        sub_agent_json: String,
    },
    RemoveSubAgent {
        sub_agent_id: String,
    },
    ListSubAgents,
    GetConciergeConfig,
    SetConciergeConfig {
        config_json: String,
    },
    DismissConciergeWelcome,
    RequestConciergeWelcome,
    AuditDismiss { entry_id: String },
    GetStatus,
    SetTierOverride {
        tier: Option<String>,
    },
    #[serde(rename = "plugin-list")]
    PluginList,
    #[serde(rename = "plugin-get")]
    PluginGetDetail { name: String },
    #[serde(rename = "plugin-enable")]
    PluginEnableCmd { name: String },
    #[serde(rename = "plugin-disable")]
    PluginDisableCmd { name: String },
    #[serde(rename = "plugin-get-settings")]
    PluginGetSettings { name: String },
    #[serde(rename = "plugin-update-settings")]
    PluginUpdateSettings {
        plugin_name: String,
        key: String,
        value: String,
        is_secret: bool,
    },
    #[serde(rename = "plugin-test-connection")]
    PluginTestConnection { name: String },
    Shutdown,
}

/// Commands for the database bridge (JSON over stdin).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum DbBridgeCommand {
    AppendCommandLog {
        entry_json: String,
    },
    CompleteCommandLog {
        id: String,
        exit_code: Option<i32>,
        duration_ms: Option<i64>,
    },
    QueryCommandLog {
        workspace_id: Option<String>,
        pane_id: Option<String>,
        limit: Option<usize>,
    },
    ClearCommandLog,
    CreateAgentThread {
        thread_json: String,
    },
    DeleteAgentThread {
        thread_id: String,
    },
    ListAgentThreads,
    GetAgentThread {
        thread_id: String,
    },
    AddAgentMessage {
        message_json: String,
    },
    DeleteAgentMessages {
        thread_id: String,
        message_ids: Vec<String>,
    },
    ListAgentMessages {
        thread_id: String,
        limit: Option<usize>,
    },
    UpsertTranscriptIndex {
        entry_json: String,
    },
    ListTranscriptIndex {
        workspace_id: Option<String>,
    },
    UpsertSnapshotIndex {
        entry_json: String,
    },
    ListSnapshotIndex {
        workspace_id: Option<String>,
    },
    UpsertAgentEvent {
        event_json: String,
    },
    ListAgentEvents {
        category: Option<String>,
        pane_id: Option<String>,
        limit: Option<usize>,
    },
    Shutdown,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum BridgeEvent {
    Ready {
        session_id: String,
    },
    Output {
        session_id: String,
        data: String,
    },
    CommandStarted {
        session_id: String,
        command_b64: String,
    },
    CommandFinished {
        session_id: String,
        exit_code: Option<i32>,
    },
    CwdChanged {
        session_id: String,
        cwd: String,
    },
    ManagedQueued {
        session_id: String,
        execution_id: String,
        position: usize,
        snapshot: Option<SnapshotInfo>,
    },
    ApprovalRequired {
        session_id: String,
        approval: ApprovalPayload,
    },
    ApprovalResolved {
        session_id: String,
        approval_id: String,
        decision: ApprovalDecision,
    },
    ManagedStarted {
        session_id: String,
        execution_id: String,
        command: String,
        source: ManagedCommandSource,
    },
    ManagedFinished {
        session_id: String,
        execution_id: String,
        command: String,
        exit_code: Option<i32>,
        duration_ms: Option<u64>,
        snapshot: Option<SnapshotInfo>,
    },
    ManagedRejected {
        session_id: String,
        execution_id: Option<String>,
        message: String,
    },
    HistorySearchResult {
        query: String,
        summary: String,
        hits: Vec<HistorySearchHit>,
    },
    SkillGenerated {
        title: String,
        path: String,
    },
    SymbolSearchResult {
        symbol: String,
        matches: Vec<SymbolMatch>,
    },
    SnapshotList {
        snapshots: Vec<SnapshotInfo>,
    },
    SnapshotRestored {
        snapshot_id: String,
        ok: bool,
        message: String,
    },
    OscNotification {
        session_id: String,
        notification: OscNotificationPayload,
    },
    SessionExited {
        session_id: String,
        exit_code: Option<i32>,
    },
    Error {
        message: String,
    },
}

fn emit_bridge_event(event: BridgeEvent) -> Result<()> {
    println!("{}", serde_json::to_string(&event)?);
    Ok(())
}

/// Connect to the daemon and return a framed stream.
async fn connect(
) -> Result<Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>> {
    #[cfg(unix)]
    {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
        let path = std::path::PathBuf::from(runtime_dir).join("tamux-daemon.sock");
        let stream = tokio::net::UnixStream::connect(&path)
            .await
            .with_context(|| format!("cannot connect to daemon at {}", path.display()))?;
        Ok(Framed::new(stream, AmuxCodec))
    }

    #[cfg(windows)]
    {
        let addr = amux_protocol::default_tcp_addr();
        let stream = tokio::net::TcpStream::connect(&addr)
            .await
            .with_context(|| format!("cannot connect to daemon on {addr}"))?;
        Ok(Framed::new(stream, AmuxCodec))
    }
}

/// Send a message and receive exactly one response.
async fn roundtrip(msg: ClientMessage) -> Result<DaemonMessage> {
    let mut framed = connect().await?;
    framed.send(msg).await?;
    let resp = framed
        .next()
        .await
        .ok_or_else(|| anyhow::anyhow!("daemon closed connection"))??;
    Ok(resp)
}

pub async fn ping() -> Result<()> {
    match roundtrip(ClientMessage::Ping).await? {
        DaemonMessage::Pong => Ok(()),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn list_sessions() -> Result<Vec<SessionInfo>> {
    match roundtrip(ClientMessage::ListSessions).await? {
        DaemonMessage::SessionList { sessions } => Ok(sessions),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn spawn_session(
    shell: Option<String>,
    cwd: Option<String>,
    workspace_id: Option<String>,
) -> Result<String> {
    match roundtrip(ClientMessage::SpawnSession {
        shell,
        cwd,
        env: None,
        workspace_id,
        cols: 80,
        rows: 24,
    })
    .await?
    {
        DaemonMessage::SessionSpawned { id } => Ok(id.to_string()),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn clone_session(
    source_id: &str,
    workspace_id: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
    cwd: Option<String>,
) -> Result<(String, Option<String>)> {
    let source_uuid = source_id.parse().context("invalid source session ID")?;
    match roundtrip(ClientMessage::CloneSession {
        source_id: source_uuid,
        workspace_id,
        cols,
        rows,
        replay_scrollback: false,
        cwd,
    })
    .await?
    {
        DaemonMessage::SessionCloned {
            id, active_command, ..
        } => Ok((id.to_string(), active_command)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn kill_session(id: &str) -> Result<()> {
    let uuid = id.parse().context("invalid session ID")?;
    match roundtrip(ClientMessage::KillSession { id: uuid }).await? {
        DaemonMessage::SessionKilled { .. } => Ok(()),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn attach_session(id: &str) -> Result<()> {
    let uuid: uuid::Uuid = id.parse().context("invalid session ID")?;
    let mut framed = connect().await?;

    // Attach.
    framed
        .send(ClientMessage::AttachSession { id: uuid })
        .await?;

    // Stream output to stdout.
    while let Some(msg) = framed.next().await {
        match msg? {
            DaemonMessage::Output { data, .. } => {
                use std::io::Write;
                std::io::stdout().write_all(&data)?;
                std::io::stdout().flush()?;
            }
            DaemonMessage::SessionExited { exit_code, .. } => {
                println!(
                    "\r\nSession exited (code: {})",
                    exit_code.map_or("unknown".to_string(), |c| c.to_string())
                );
                break;
            }
            DaemonMessage::Error { message } => {
                eprintln!("Error: {message}");
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

pub async fn get_git_status(path: String) -> Result<amux_protocol::GitInfo> {
    match roundtrip(ClientMessage::GetGitStatus { path }).await? {
        DaemonMessage::GitStatus { info, .. } => Ok(info),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn get_git_diff(repo_path: String, file_path: Option<String>) -> Result<String> {
    match roundtrip(ClientMessage::GetGitDiff {
        repo_path,
        file_path,
    })
    .await?
    {
        DaemonMessage::GitDiff { diff, .. } => Ok(diff),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn get_file_preview(path: String, max_bytes: Option<usize>) -> Result<serde_json::Value> {
    match roundtrip(ClientMessage::GetFilePreview { path, max_bytes }).await? {
        DaemonMessage::FilePreview {
            path,
            content,
            truncated,
            is_text,
        } => Ok(serde_json::json!({
            "path": path,
            "content": content,
            "truncated": truncated,
            "is_text": is_text,
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn scrub_text(text: String) -> Result<String> {
    match roundtrip(ClientMessage::ScrubSensitive { text }).await? {
        DaemonMessage::ScrubResult { text } => Ok(text),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

/// Status response fields from the daemon.
pub struct AgentStatusSnapshot {
    pub tier: String,
    pub activity: String,
    pub active_thread_id: Option<String>,
    pub active_goal_run_id: Option<String>,
    pub active_goal_run_title: Option<String>,
    pub provider_health_json: String,
    pub gateway_statuses_json: String,
    pub recent_actions_json: String,
}

pub async fn send_status_query() -> Result<AgentStatusSnapshot> {
    match roundtrip(ClientMessage::AgentStatusQuery).await? {
        DaemonMessage::AgentStatusResponse {
            tier,
            activity,
            active_thread_id,
            active_goal_run_id,
            active_goal_run_title,
            provider_health_json,
            gateway_statuses_json,
            recent_actions_json,
            ..
        } => Ok(AgentStatusSnapshot {
            tier,
            activity,
            active_thread_id,
            active_goal_run_id,
            active_goal_run_title,
            provider_health_json,
            gateway_statuses_json,
            recent_actions_json,
        }),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_config_get() -> Result<serde_json::Value> {
    match roundtrip(ClientMessage::AgentGetConfig).await? {
        DaemonMessage::AgentConfigResponse { config_json } => {
            let config: serde_json::Value = serde_json::from_str(&config_json)?;
            Ok(config)
        }
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_config_set(key_path: String, value_json: String) -> Result<()> {
    match roundtrip(ClientMessage::AgentSetConfigItem { key_path, value_json }).await? {
        DaemonMessage::AgentConfigResponse { .. } => Ok(()),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_audit_query(
    action_types: Option<Vec<String>>,
    since: Option<u64>,
    limit: Option<usize>,
) -> Result<Vec<amux_protocol::AuditEntryPublic>> {
    match roundtrip(ClientMessage::AuditQuery {
        action_types,
        since,
        limit,
    })
    .await?
    {
        DaemonMessage::AuditList { entries_json } => {
            let entries: Vec<amux_protocol::AuditEntryPublic> =
                serde_json::from_str(&entries_json)?;
            Ok(entries)
        }
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_skill_list(
    status: Option<String>,
    limit: usize,
) -> Result<Vec<amux_protocol::SkillVariantPublic>> {
    match roundtrip(ClientMessage::SkillList { status, limit }).await? {
        DaemonMessage::SkillListResult { variants } => Ok(variants),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_skill_inspect(
    identifier: &str,
) -> Result<(Option<amux_protocol::SkillVariantPublic>, Option<String>)> {
    match roundtrip(ClientMessage::SkillInspect {
        identifier: identifier.to_string(),
    })
    .await?
    {
        DaemonMessage::SkillInspectResult { variant, content } => Ok((variant, content)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_skill_reject(identifier: &str) -> Result<(bool, String)> {
    match roundtrip(ClientMessage::SkillReject {
        identifier: identifier.to_string(),
    })
    .await?
    {
        DaemonMessage::SkillActionResult { success, message } => Ok((success, message)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_skill_promote(identifier: &str, target_status: &str) -> Result<(bool, String)> {
    match roundtrip(ClientMessage::SkillPromote {
        identifier: identifier.to_string(),
        target_status: target_status.to_string(),
    })
    .await?
    {
        DaemonMessage::SkillActionResult { success, message } => Ok((success, message)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_skill_search(query: &str) -> Result<Vec<amux_protocol::CommunitySkillEntry>> {
    match roundtrip(ClientMessage::SkillSearch {
        query: query.to_string(),
    })
    .await?
    {
        DaemonMessage::SkillSearchResult { entries } => Ok(entries),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_skill_import(
    source: &str,
    force: bool,
) -> Result<(bool, String, Option<String>, Option<String>, u32)> {
    let publisher_verified = if source.starts_with("http://") || source.starts_with("https://") {
        false
    } else {
        send_skill_search(source)
            .await?
            .into_iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(source))
            .map(|entry| entry.publisher_verified)
            .unwrap_or(false)
    };

    match roundtrip(ClientMessage::SkillImport {
        source: source.to_string(),
        force,
        publisher_verified,
    })
    .await?
    {
        DaemonMessage::SkillImportResult {
            success,
            message,
            variant_id,
            scan_verdict,
            findings_count,
        } => Ok((success, message, variant_id, scan_verdict, findings_count)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_skill_export(
    identifier: &str,
    format: &str,
    output_dir: &str,
) -> Result<(bool, String, Option<String>)> {
    match roundtrip(ClientMessage::SkillExport {
        identifier: identifier.to_string(),
        format: format.to_string(),
        output_dir: output_dir.to_string(),
    })
    .await?
    {
        DaemonMessage::SkillExportResult {
            success,
            message,
            output_path,
        } => Ok((success, message, output_path)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_skill_publish(identifier: &str) -> Result<(bool, String)> {
    match roundtrip(ClientMessage::SkillPublish {
        identifier: identifier.to_string(),
    })
    .await?
    {
        DaemonMessage::SkillPublishResult { success, message } => Ok((success, message)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Plugin IPC helpers (v2 manifest format)
// ---------------------------------------------------------------------------

/// Send PluginInstall to daemon for registration. Returns (success, message).
pub async fn send_plugin_install(dir_name: &str, install_source: &str) -> Result<(bool, String)> {
    match roundtrip(ClientMessage::PluginInstall {
        dir_name: dir_name.to_string(),
        install_source: install_source.to_string(),
    })
    .await?
    {
        DaemonMessage::PluginActionResult { success, message } => Ok((success, message)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

/// Send PluginUninstall to daemon for deregistration. Returns (success, message).
pub async fn send_plugin_uninstall(name: &str) -> Result<(bool, String)> {
    match roundtrip(ClientMessage::PluginUninstall {
        name: name.to_string(),
    })
    .await?
    {
        DaemonMessage::PluginActionResult { success, message } => Ok((success, message)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

/// Send PluginList to daemon. Returns list of PluginInfo. Per INST-05.
pub async fn send_plugin_list() -> Result<Vec<amux_protocol::PluginInfo>> {
    match roundtrip(ClientMessage::PluginList {}).await? {
        DaemonMessage::PluginListResult { plugins } => Ok(plugins),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

/// Send PluginEnable to daemon. Returns (success, message). Per INST-06.
pub async fn send_plugin_enable(name: &str) -> Result<(bool, String)> {
    match roundtrip(ClientMessage::PluginEnable {
        name: name.to_string(),
    })
    .await?
    {
        DaemonMessage::PluginActionResult { success, message } => Ok((success, message)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

/// Send PluginDisable to daemon. Returns (success, message). Per INST-06.
pub async fn send_plugin_disable(name: &str) -> Result<(bool, String)> {
    match roundtrip(ClientMessage::PluginDisable {
        name: name.to_string(),
    })
    .await?
    {
        DaemonMessage::PluginActionResult { success, message } => Ok((success, message)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn run_bridge(
    session: Option<String>,
    shell: Option<String>,
    cwd: Option<String>,
    workspace: Option<String>,
    cols: u16,
    rows: u16,
) -> Result<()> {
    let mut framed = connect().await?;
    let session_id = if let Some(session) = session {
        let id = session.parse().context("invalid session ID")?;
        match attach_bridge_session(&mut framed, id).await {
            Ok(attached_id) => attached_id,
            Err(error) if is_missing_session_error(&error) => {
                tracing::warn!(requested_session = %id, error = %error, "saved session missing; spawning replacement session");
                spawn_bridge_session(
                    &mut framed,
                    shell.clone(),
                    cwd.clone(),
                    workspace.clone(),
                    cols,
                    rows,
                )
                .await?
            }
            Err(error) => return Err(error),
        }
    } else {
        spawn_bridge_session(
            &mut framed,
            shell.clone(),
            cwd.clone(),
            workspace.clone(),
            cols,
            rows,
        )
        .await?
    };

    emit_bridge_event(BridgeEvent::Ready {
        session_id: session_id.to_string(),
    })?;

    // Replay recent daemon scrollback so renderer reloads can reconstruct terminal state
    // even when the Electron-side bridge process is recreated.
    framed
        .send(ClientMessage::GetScrollback {
            id: session_id,
            max_lines: None,
        })
        .await
        .ok();

    // Nudge the PTY with a resize so that shells (especially wsl.exe on Windows)
    // redraw their prompt even if the initial output was produced before we subscribed.
    framed
        .send(ClientMessage::Resize {
            id: session_id,
            cols,
            rows,
        })
        .await
        .ok();

    let mut stdin_lines = BufReader::new(tokio::io::stdin()).lines();

    loop {
        tokio::select! {
            maybe_line = stdin_lines.next_line() => {
                match maybe_line? {
                    Some(line) => {
                        let command: BridgeCommand = match serde_json::from_str(&line) {
                            Ok(command) => command,
                            Err(error) => {
                                emit_bridge_event(BridgeEvent::Error {
                                    message: format!("invalid bridge command: {error}"),
                                })?;
                                continue;
                            }
                        };

                        match command {
                            BridgeCommand::Input { data } => {
                                let bytes = BASE64_ENGINE
                                    .decode(data)
                                    .context("invalid input payload")?;
                                framed
                                    .send(ClientMessage::Input { id: session_id, data: bytes })
                                    .await?;
                            }
                            BridgeCommand::Resize { cols, rows } => {
                                framed
                                    .send(ClientMessage::Resize { id: session_id, cols, rows })
                                    .await?;
                            }
                            BridgeCommand::ExecuteManaged {
                                command,
                                rationale,
                                allow_network,
                                sandbox_enabled,
                                security_level,
                                cwd,
                                language_hint,
                                source,
                            } => {
                                let source = match source.as_deref() {
                                    Some("human") => ManagedCommandSource::Human,
                                    Some("replay") => ManagedCommandSource::Replay,
                                    Some("gateway") => ManagedCommandSource::Gateway,
                                    _ => ManagedCommandSource::Agent,
                                };
                                let security_level = match security_level.as_deref() {
                                    Some("highest") => SecurityLevel::Highest,
                                    Some("lowest") => SecurityLevel::Lowest,
                                    Some("yolo") => SecurityLevel::Yolo,
                                    _ => SecurityLevel::Moderate,
                                };
                                framed
                                    .send(ClientMessage::ExecuteManagedCommand {
                                        id: session_id,
                                        request: ManagedCommandRequest {
                                            command,
                                            rationale,
                                            allow_network,
                                            sandbox_enabled: sandbox_enabled.unwrap_or(false),
                                            security_level,
                                            cwd,
                                            language_hint,
                                            source,
                                        },
                                    })
                                    .await?;
                            }
                            BridgeCommand::ApprovalDecision { approval_id, decision } => {
                                let decision = match decision.as_str() {
                                    "approve-session" => ApprovalDecision::ApproveSession,
                                    "deny" => ApprovalDecision::Deny,
                                    _ => ApprovalDecision::ApproveOnce,
                                };
                                framed
                                    .send(ClientMessage::ResolveApproval {
                                        id: session_id,
                                        approval_id,
                                        decision,
                                    })
                                    .await?;
                            }
                            BridgeCommand::SearchHistory { query, limit } => {
                                framed
                                    .send(ClientMessage::SearchHistory { query, limit })
                                    .await?;
                            }
                            BridgeCommand::GenerateSkill { query, title } => {
                                framed
                                    .send(ClientMessage::GenerateSkill { query, title })
                                    .await?;
                            }
                            BridgeCommand::FindSymbol { workspace_root, symbol, limit } => {
                                framed
                                    .send(ClientMessage::FindSymbol {
                                        workspace_root,
                                        symbol,
                                        limit,
                                    })
                                    .await?;
                            }
                            BridgeCommand::ListSnapshots { workspace_id } => {
                                framed
                                    .send(ClientMessage::ListSnapshots { workspace_id })
                                    .await?;
                            }
                            BridgeCommand::RestoreSnapshot { snapshot_id } => {
                                framed
                                    .send(ClientMessage::RestoreSnapshot { snapshot_id })
                                    .await?;
                            }
                            BridgeCommand::Shutdown => {
                                break;
                            }
                            BridgeCommand::KillSession => {
                                framed
                                    .send(ClientMessage::KillSession { id: session_id })
                                    .await?;
                                break;
                            }
                        }
                    }
                    None => break,
                }
            }
            maybe_message = framed.next() => {
                match maybe_message {
                    Some(Ok(DaemonMessage::Output { id, data })) if id == session_id => {
                        if !data.is_empty() {
                            emit_bridge_event(BridgeEvent::Output {
                                session_id: id.to_string(),
                                data: BASE64_ENGINE.encode(data),
                            })?;
                        }
                    }
                    Some(Ok(DaemonMessage::CommandStarted { id, command })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::CommandStarted {
                            session_id: id.to_string(),
                            command_b64: BASE64_ENGINE.encode(command),
                        })?;
                    }
                    Some(Ok(DaemonMessage::CommandFinished { id, exit_code })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::CommandFinished {
                            session_id: id.to_string(),
                            exit_code,
                        })?;
                    }
                    Some(Ok(DaemonMessage::CwdChanged { id, cwd })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::CwdChanged {
                            session_id: id.to_string(),
                            cwd,
                        })?;
                    }
                    Some(Ok(DaemonMessage::ManagedCommandQueued { id, execution_id, position, snapshot })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::ManagedQueued {
                            session_id: id.to_string(),
                            execution_id,
                            position,
                            snapshot,
                        })?;
                    }
                    Some(Ok(DaemonMessage::ApprovalRequired { id, approval })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::ApprovalRequired {
                            session_id: id.to_string(),
                            approval,
                        })?;
                    }
                    Some(Ok(DaemonMessage::ApprovalResolved { id, approval_id, decision })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::ApprovalResolved {
                            session_id: id.to_string(),
                            approval_id,
                            decision,
                        })?;
                    }
                    Some(Ok(DaemonMessage::ManagedCommandStarted { id, execution_id, command, source })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::ManagedStarted {
                            session_id: id.to_string(),
                            execution_id,
                            command,
                            source,
                        })?;
                    }
                    Some(Ok(DaemonMessage::ManagedCommandFinished { id, execution_id, command, exit_code, duration_ms, snapshot })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::ManagedFinished {
                            session_id: id.to_string(),
                            execution_id,
                            command,
                            exit_code,
                            duration_ms,
                            snapshot,
                        })?;
                    }
                    Some(Ok(DaemonMessage::ManagedCommandRejected { id, execution_id, message })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::ManagedRejected {
                            session_id: id.to_string(),
                            execution_id,
                            message,
                        })?;
                    }
                    Some(Ok(DaemonMessage::HistorySearchResult { query, summary, hits })) => {
                        emit_bridge_event(BridgeEvent::HistorySearchResult { query, summary, hits })?;
                    }
                    Some(Ok(DaemonMessage::SkillGenerated { title, path })) => {
                        emit_bridge_event(BridgeEvent::SkillGenerated { title, path })?;
                    }
                    Some(Ok(DaemonMessage::SymbolSearchResult { symbol, matches })) => {
                        emit_bridge_event(BridgeEvent::SymbolSearchResult { symbol, matches })?;
                    }
                    Some(Ok(DaemonMessage::SnapshotList { snapshots })) => {
                        emit_bridge_event(BridgeEvent::SnapshotList { snapshots })?;
                    }
                    Some(Ok(DaemonMessage::SnapshotRestored { snapshot_id, ok, message })) => {
                        emit_bridge_event(BridgeEvent::SnapshotRestored { snapshot_id, ok, message })?;
                    }
                    Some(Ok(DaemonMessage::OscNotification { id, notification })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::OscNotification {
                            session_id: id.to_string(),
                            notification,
                        })?;
                    }
                    Some(Ok(DaemonMessage::Scrollback { id, data })) if id == session_id => {
                        if !data.is_empty() {
                            emit_bridge_event(BridgeEvent::Output {
                                session_id: id.to_string(),
                                data: BASE64_ENGINE.encode(data),
                            })?;
                        }
                    }
                    Some(Ok(DaemonMessage::SessionExited { id, exit_code })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::SessionExited {
                            session_id: id.to_string(),
                            exit_code,
                        })?;
                        break;
                    }
                    Some(Ok(DaemonMessage::SessionKilled { id })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::SessionExited {
                            session_id: id.to_string(),
                            exit_code: None,
                        })?;
                        break;
                    }
                    Some(Ok(DaemonMessage::Error { message })) => {
                        emit_bridge_event(BridgeEvent::Error { message })?;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => return Err(error.into()),
                    None => {
                        emit_bridge_event(BridgeEvent::Error {
                            message: "daemon connection closed".to_string(),
                        })?;
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn attach_bridge_session(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
    id: uuid::Uuid,
) -> Result<uuid::Uuid> {
    framed
        .send(ClientMessage::AttachSession { id })
        .await
        .context("failed to attach to daemon session")?;

    match framed.next().await {
        Some(Ok(DaemonMessage::SessionAttached { id })) => Ok(id),
        Some(Ok(DaemonMessage::Error { message })) => anyhow::bail!(message),
        Some(Ok(other)) => anyhow::bail!("unexpected response: {other:?}"),
        Some(Err(error)) => Err(error.into()),
        None => anyhow::bail!("daemon closed connection while attaching"),
    }
}

async fn spawn_bridge_session(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
    shell: Option<String>,
    cwd: Option<String>,
    workspace: Option<String>,
    cols: u16,
    rows: u16,
) -> Result<uuid::Uuid> {
    framed
        .send(ClientMessage::SpawnSession {
            shell,
            cwd,
            env: None,
            workspace_id: workspace,
            cols,
            rows,
        })
        .await
        .context("failed to request session spawn")?;

    match framed.next().await {
        Some(Ok(DaemonMessage::SessionSpawned { id })) => Ok(id),
        Some(Ok(DaemonMessage::Error { message })) => anyhow::bail!(message),
        Some(Ok(other)) => anyhow::bail!("unexpected response: {other:?}"),
        Some(Err(error)) => Err(error.into()),
        None => anyhow::bail!("daemon closed connection while spawning session"),
    }
}

fn is_missing_session_error(error: &anyhow::Error) -> bool {
    error
        .to_string()
        .to_ascii_lowercase()
        .contains("session not found")
}

// ---------------------------------------------------------------------------
// Agent bridge — persistent connection for agent engine IPC
// ---------------------------------------------------------------------------

fn emit_agent_event(json: &str) -> Result<()> {
    println!("{json}");
    Ok(())
}

fn emit_db_event(json: &str) -> Result<()> {
    println!("{json}");
    Ok(())
}

pub async fn run_agent_bridge() -> Result<()> {
    let mut framed = connect().await?;

    // Subscribe to agent events
    framed.send(ClientMessage::AgentSubscribe).await?;

    // Emit ready signal
    println!(r#"{{"type":"ready"}}"#);

    let mut stdin_lines = BufReader::new(tokio::io::stdin()).lines();

    loop {
        tokio::select! {
            maybe_line = stdin_lines.next_line() => {
                match maybe_line? {
                    Some(line) => {
                        let command: AgentBridgeCommand = match serde_json::from_str(&line) {
                            Ok(cmd) => cmd,
                            Err(error) => {
                                let err_json = serde_json::json!({"type":"error","message":format!("invalid command: {error}")});
                                emit_agent_event(&err_json.to_string())?;
                                continue;
                            }
                        };

                        match command {
                            AgentBridgeCommand::SendMessage { thread_id, content, session_id, context_messages } => {
                                let context_messages_json = context_messages
                                    .and_then(|msgs| serde_json::to_string(&msgs).ok());
                                framed.send(ClientMessage::AgentSendMessage { thread_id, content, session_id, context_messages_json }).await?;
                            }
                            AgentBridgeCommand::StopStream { thread_id } => {
                                framed.send(ClientMessage::AgentStopStream { thread_id }).await?;
                            }
                            AgentBridgeCommand::ListThreads => {
                                framed.send(ClientMessage::AgentListThreads).await?;
                            }
                            AgentBridgeCommand::GetThread { thread_id } => {
                                framed.send(ClientMessage::AgentGetThread { thread_id }).await?;
                            }
                            AgentBridgeCommand::DeleteThread { thread_id } => {
                                framed.send(ClientMessage::AgentDeleteThread { thread_id }).await?;
                            }
                            AgentBridgeCommand::AddTask {
                                title,
                                description,
                                priority,
                                command,
                                session_id,
                                scheduled_at,
                                dependencies,
                            } => {
                                framed.send(ClientMessage::AgentAddTask {
                                    title,
                                    description,
                                    priority: priority.unwrap_or_else(|| "normal".into()),
                                    command,
                                    session_id,
                                    scheduled_at,
                                    dependencies,
                                }).await?;
                            }
                            AgentBridgeCommand::CancelTask { task_id } => {
                                framed.send(ClientMessage::AgentCancelTask { task_id }).await?;
                            }
                            AgentBridgeCommand::ListTasks => {
                                framed.send(ClientMessage::AgentListTasks).await?;
                            }
                            AgentBridgeCommand::ListRuns => {
                                framed.send(ClientMessage::AgentListRuns).await?;
                            }
                            AgentBridgeCommand::GetRun { run_id } => {
                                framed.send(ClientMessage::AgentGetRun { run_id }).await?;
                            }
                            AgentBridgeCommand::StartGoalRun {
                                goal,
                                title,
                                thread_id,
                                session_id,
                                priority,
                                client_request_id,
                            } => {
                                framed.send(ClientMessage::AgentStartGoalRun {
                                    goal,
                                    title,
                                    thread_id,
                                    session_id,
                                    priority,
                                    client_request_id,
                                }).await?;
                            }
                            AgentBridgeCommand::ListGoalRuns => {
                                framed.send(ClientMessage::AgentListGoalRuns).await?;
                            }
                            AgentBridgeCommand::GetGoalRun { goal_run_id } => {
                                framed.send(ClientMessage::AgentGetGoalRun { goal_run_id }).await?;
                            }
                            AgentBridgeCommand::ControlGoalRun {
                                goal_run_id,
                                action,
                                step_index,
                            } => {
                                framed
                                    .send(ClientMessage::AgentControlGoalRun {
                                        goal_run_id,
                                        action,
                                        step_index,
                                    })
                                    .await?;
                            }
                            AgentBridgeCommand::ListTodos => {
                                framed.send(ClientMessage::AgentListTodos).await?;
                            }
                            AgentBridgeCommand::GetTodos { thread_id } => {
                                framed.send(ClientMessage::AgentGetTodos { thread_id }).await?;
                            }
                            AgentBridgeCommand::GetWorkContext { thread_id } => {
                                framed
                                    .send(ClientMessage::AgentGetWorkContext { thread_id })
                                    .await?;
                            }
                            AgentBridgeCommand::GetGitDiff { repo_path, file_path } => {
                                framed
                                    .send(ClientMessage::GetGitDiff { repo_path, file_path })
                                    .await?;
                            }
                            AgentBridgeCommand::GetFilePreview { path, max_bytes } => {
                                framed
                                    .send(ClientMessage::GetFilePreview { path, max_bytes })
                                    .await?;
                            }
                            AgentBridgeCommand::GetConfig => {
                                framed.send(ClientMessage::AgentGetConfig).await?;
                            }
                            AgentBridgeCommand::SetConfigItem {
                                key_path,
                                value_json,
                            } => {
                                framed
                                    .send(ClientMessage::AgentSetConfigItem {
                                        key_path,
                                        value_json,
                                    })
                                    .await?;
                            }
                            AgentBridgeCommand::HeartbeatGetItems => {
                                framed.send(ClientMessage::AgentHeartbeatGetItems).await?;
                            }
                            AgentBridgeCommand::HeartbeatSetItems { items_json } => {
                                framed.send(ClientMessage::AgentHeartbeatSetItems { items_json }).await?;
                            }
                            AgentBridgeCommand::ResolveTaskApproval { approval_id, decision } => {
                                framed.send(ClientMessage::AgentResolveTaskApproval {
                                    approval_id,
                                    decision,
                                }).await?;
                            }
                            AgentBridgeCommand::ValidateProvider { provider_id, base_url, api_key, auth_source } => {
                                framed.send(ClientMessage::AgentValidateProvider {
                                    provider_id,
                                    base_url,
                                    api_key,
                                    auth_source,
                                }).await?;
                            }
                            AgentBridgeCommand::LoginProvider { provider_id, api_key, base_url } => {
                                framed.send(ClientMessage::AgentLoginProvider {
                                    provider_id,
                                    api_key,
                                    base_url,
                                }).await?;
                            }
                            AgentBridgeCommand::LogoutProvider { provider_id } => {
                                framed.send(ClientMessage::AgentLogoutProvider { provider_id }).await?;
                            }
                            AgentBridgeCommand::GetProviderAuthStates => {
                                framed.send(ClientMessage::AgentGetProviderAuthStates).await?;
                            }
                            AgentBridgeCommand::SetSubAgent { sub_agent_json } => {
                                framed.send(ClientMessage::AgentSetSubAgent { sub_agent_json }).await?;
                            }
                            AgentBridgeCommand::RemoveSubAgent { sub_agent_id } => {
                                framed.send(ClientMessage::AgentRemoveSubAgent { sub_agent_id }).await?;
                            }
                            AgentBridgeCommand::ListSubAgents => {
                                framed.send(ClientMessage::AgentListSubAgents).await?;
                            }
                            AgentBridgeCommand::GetConciergeConfig => {
                                framed.send(ClientMessage::AgentGetConciergeConfig).await?;
                            }
                            AgentBridgeCommand::SetConciergeConfig { config_json } => {
                                framed.send(ClientMessage::AgentSetConciergeConfig { config_json }).await?;
                            }
                            AgentBridgeCommand::DismissConciergeWelcome => {
                                framed.send(ClientMessage::AgentDismissConciergeWelcome).await?;
                            }
                            AgentBridgeCommand::RequestConciergeWelcome => {
                                framed.send(ClientMessage::AgentRequestConciergeWelcome).await?;
                            }
                            AgentBridgeCommand::AuditDismiss { entry_id } => {
                                framed.send(ClientMessage::AuditDismiss { entry_id }).await?;
                            }
                            AgentBridgeCommand::GetStatus => {
                                framed.send(ClientMessage::AgentStatusQuery).await?;
                            }
                            AgentBridgeCommand::SetTierOverride { tier } => {
                                framed.send(ClientMessage::AgentSetTierOverride { tier }).await?;
                            }
                            AgentBridgeCommand::PluginList => {
                                framed.send(ClientMessage::PluginList {}).await?;
                            }
                            AgentBridgeCommand::PluginGetDetail { name } => {
                                framed.send(ClientMessage::PluginGet { name }).await?;
                            }
                            AgentBridgeCommand::PluginEnableCmd { name } => {
                                framed.send(ClientMessage::PluginEnable { name }).await?;
                            }
                            AgentBridgeCommand::PluginDisableCmd { name } => {
                                framed.send(ClientMessage::PluginDisable { name }).await?;
                            }
                            AgentBridgeCommand::PluginGetSettings { name } => {
                                framed.send(ClientMessage::PluginGetSettings { name }).await?;
                            }
                            AgentBridgeCommand::PluginUpdateSettings { plugin_name, key, value, is_secret } => {
                                framed.send(ClientMessage::PluginUpdateSettings { plugin_name, key, value, is_secret }).await?;
                            }
                            AgentBridgeCommand::PluginTestConnection { name } => {
                                framed.send(ClientMessage::PluginTestConnection { name }).await?;
                            }
                            AgentBridgeCommand::Shutdown => {
                                framed.send(ClientMessage::AgentUnsubscribe).await?;
                                break;
                            }
                        }
                    }
                    None => break,
                }
            }
            maybe_message = framed.next() => {
                match maybe_message {
                    Some(Ok(DaemonMessage::AgentEvent { event_json })) => {
                        emit_agent_event(&event_json)?;
                    }
                    Some(Ok(DaemonMessage::AgentThreadList { threads_json })) => {
                        let msg = serde_json::json!({"type":"thread-list","data":serde_json::from_str::<serde_json::Value>(&threads_json).unwrap_or_default()});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentThreadDetail { thread_json })) => {
                        let msg = serde_json::json!({"type":"thread-detail","data":serde_json::from_str::<serde_json::Value>(&thread_json).unwrap_or_default()});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentTaskList { tasks_json })) => {
                        let msg = serde_json::json!({"type":"task-list","data":serde_json::from_str::<serde_json::Value>(&tasks_json).unwrap_or_default()});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentRunList { runs_json })) => {
                        let msg = serde_json::json!({"type":"run-list","data":serde_json::from_str::<serde_json::Value>(&runs_json).unwrap_or_default()});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentRunDetail { run_json })) => {
                        let msg = serde_json::json!({"type":"run-detail","data":serde_json::from_str::<serde_json::Value>(&run_json).unwrap_or_default()});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentTaskEnqueued { task_json })) => {
                        let msg = serde_json::json!({"type":"task-enqueued","data":serde_json::from_str::<serde_json::Value>(&task_json).unwrap_or_default()});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentTaskCancelled { task_id, cancelled })) => {
                        let msg = serde_json::json!({"type":"task-cancelled","task_id":task_id,"cancelled":cancelled});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentGoalRunStarted { goal_run_json })) => {
                        let msg = serde_json::json!({"type":"goal-run-started","data":serde_json::from_str::<serde_json::Value>(&goal_run_json).unwrap_or_default()});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentGoalRunList { goal_runs_json })) => {
                        let msg = serde_json::json!({"type":"goal-run-list","data":serde_json::from_str::<serde_json::Value>(&goal_runs_json).unwrap_or_default()});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentGoalRunDetail { goal_run_json })) => {
                        let msg = serde_json::json!({"type":"goal-run-detail","data":serde_json::from_str::<serde_json::Value>(&goal_run_json).unwrap_or_default()});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentGoalRunControlled { goal_run_id, ok })) => {
                        let msg = serde_json::json!({"type":"goal-run-controlled","data":{"goal_run_id":goal_run_id,"ok":ok}});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentTodoList { todos_json })) => {
                        let msg = serde_json::json!({"type":"todo-list","data":serde_json::from_str::<serde_json::Value>(&todos_json).unwrap_or_default()});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentTodoDetail { thread_id, todos_json })) => {
                        let msg = serde_json::json!({"type":"todo-detail","data":{"thread_id":thread_id,"items":serde_json::from_str::<serde_json::Value>(&todos_json).unwrap_or_default()}});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentWorkContextDetail { thread_id, context_json })) => {
                        let msg = serde_json::json!({"type":"work-context-detail","data":{"thread_id":thread_id,"context":serde_json::from_str::<serde_json::Value>(&context_json).unwrap_or_default()}});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::GitDiff { repo_path, file_path, diff })) => {
                        let msg = serde_json::json!({"type":"git-diff","data":{"repo_path":repo_path,"file_path":file_path,"diff":diff}});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::FilePreview { path, content, truncated, is_text })) => {
                        let msg = serde_json::json!({"type":"file-preview","data":{"path":path,"content":content,"truncated":truncated,"is_text":is_text}});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentConfigResponse { config_json })) => {
                        let msg = serde_json::json!({"type":"config","data":serde_json::from_str::<serde_json::Value>(&config_json).unwrap_or_default()});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentHeartbeatItems { items_json })) => {
                        let msg = serde_json::json!({"type":"heartbeat-items","data":serde_json::from_str::<serde_json::Value>(&items_json).unwrap_or_default()});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentProviderValidation { provider_id, valid, error, models_json })) => {
                        let msg = serde_json::json!({
                            "type": "provider-validation",
                            "data": {
                                "provider_id": provider_id,
                                "valid": valid,
                                "error": error,
                                "models": models_json.and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok()),
                            }
                        });
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentProviderAuthStates { states_json })) => {
                        let msg = serde_json::json!({"type":"provider-auth-states","data":serde_json::from_str::<serde_json::Value>(&states_json).unwrap_or_default()});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentSubAgentList { sub_agents_json })) => {
                        let msg = serde_json::json!({"type":"sub-agent-list","data":serde_json::from_str::<serde_json::Value>(&sub_agents_json).unwrap_or_default()});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentSubAgentUpdated { sub_agent_json })) => {
                        let msg = serde_json::json!({"type":"sub-agent-updated","data":serde_json::from_str::<serde_json::Value>(&sub_agent_json).unwrap_or_default()});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentSubAgentRemoved { sub_agent_id })) => {
                        let msg = serde_json::json!({"type":"sub-agent-removed","data":{"sub_agent_id":sub_agent_id}});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentConciergeConfig { config_json })) => {
                        let msg = serde_json::json!({"type":"concierge-config","data":serde_json::from_str::<serde_json::Value>(&config_json).unwrap_or_default()});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentConciergeWelcomeDismissed)) => {
                        let msg = serde_json::json!({"type":"concierge-welcome-dismissed"});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentTierChanged { previous_tier, new_tier, reason })) => {
                        let msg = serde_json::json!({
                            "type": "tier-changed",
                            "data": {
                                "previous_tier": previous_tier,
                                "new_tier": new_tier,
                                "reason": reason,
                            }
                        });
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentStatusResponse {
                        tier, feature_flags_json, activity,
                        active_thread_id, active_goal_run_id, active_goal_run_title,
                        provider_health_json, gateway_statuses_json, recent_actions_json,
                    })) => {
                        let msg = serde_json::json!({
                            "type": "status-response",
                            "data": {
                                "tier": tier,
                                "feature_flags": serde_json::from_str::<serde_json::Value>(&feature_flags_json).unwrap_or_default(),
                                "activity": activity,
                                "active_thread_id": active_thread_id,
                                "active_goal_run_id": active_goal_run_id,
                                "active_goal_run_title": active_goal_run_title,
                                "provider_health": serde_json::from_str::<serde_json::Value>(&provider_health_json).unwrap_or_default(),
                                "gateway_statuses": serde_json::from_str::<serde_json::Value>(&gateway_statuses_json).unwrap_or_default(),
                                "recent_actions": serde_json::from_str::<serde_json::Value>(&recent_actions_json).unwrap_or_default(),
                            }
                        });
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::PluginListResult { plugins })) => {
                        let msg = serde_json::json!({
                            "type": "plugin-list-result",
                            "plugins": plugins.iter().map(|p| serde_json::json!({
                                "name": p.name, "version": p.version, "description": p.description,
                                "author": p.author, "enabled": p.enabled, "install_source": p.install_source,
                                "has_api": p.has_api, "has_auth": p.has_auth, "has_commands": p.has_commands,
                                "has_skills": p.has_skills, "endpoint_count": p.endpoint_count,
                                "settings_count": p.settings_count, "installed_at": p.installed_at,
                                "updated_at": p.updated_at,
                            })).collect::<Vec<_>>(),
                        });
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::PluginGetResult { plugin, settings_schema })) => {
                        let msg = serde_json::json!({
                            "type": "plugin-get-result",
                            "plugin": plugin.as_ref().map(|p| serde_json::json!({
                                "name": p.name, "version": p.version, "description": p.description,
                                "author": p.author, "enabled": p.enabled, "install_source": p.install_source,
                                "has_api": p.has_api, "has_auth": p.has_auth, "has_commands": p.has_commands,
                                "has_skills": p.has_skills, "endpoint_count": p.endpoint_count,
                                "settings_count": p.settings_count, "installed_at": p.installed_at,
                                "updated_at": p.updated_at,
                            })),
                            "settings_schema": settings_schema,
                        });
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::PluginActionResult { success, message })) => {
                        let msg = serde_json::json!({
                            "type": "plugin-action-result",
                            "success": success,
                            "message": message,
                        });
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::PluginSettingsResult { plugin_name, settings })) => {
                        let msg = serde_json::json!({
                            "type": "plugin-settings",
                            "plugin_name": plugin_name,
                            "settings": settings.iter().map(|(k, v, s)| serde_json::json!({"key": k, "value": v, "is_secret": s})).collect::<Vec<_>>(),
                        });
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::PluginTestConnectionResult { plugin_name, success, message })) => {
                        let msg = serde_json::json!({
                            "type": "plugin-test-connection-result",
                            "plugin_name": plugin_name,
                            "success": success,
                            "message": message,
                        });
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::Error { message })) => {
                        let msg = serde_json::json!({"type":"error","message":message});
                        emit_agent_event(&msg.to_string())?;
                    }
                    Some(Ok(_)) => {} // Ignore non-agent messages
                    Some(Err(error)) => return Err(error.into()),
                    None => {
                        let msg = serde_json::json!({"type":"error","message":"daemon connection closed"});
                        emit_agent_event(&msg.to_string())?;
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

pub async fn run_db_bridge() -> Result<()> {
    let mut framed = connect().await?;

    println!("{{\"type\":\"ready\"}}");

    let mut stdin_lines = BufReader::new(tokio::io::stdin()).lines();

    loop {
        tokio::select! {
            maybe_line = stdin_lines.next_line() => {
                match maybe_line? {
                    Some(line) => {
                        let command: DbBridgeCommand = match serde_json::from_str(&line) {
                            Ok(cmd) => cmd,
                            Err(error) => {
                                let err_json = serde_json::json!({"type":"error","message":format!("invalid command: {error}")});
                                emit_db_event(&err_json.to_string())?;
                                continue;
                            }
                        };

                        match command {
                            DbBridgeCommand::AppendCommandLog { entry_json } => {
                                framed.send(ClientMessage::AppendCommandLog { entry_json }).await?;
                            }
                            DbBridgeCommand::CompleteCommandLog { id, exit_code, duration_ms } => {
                                framed.send(ClientMessage::CompleteCommandLog { id, exit_code, duration_ms }).await?;
                            }
                            DbBridgeCommand::QueryCommandLog { workspace_id, pane_id, limit } => {
                                framed.send(ClientMessage::QueryCommandLog { workspace_id, pane_id, limit }).await?;
                            }
                            DbBridgeCommand::ClearCommandLog => {
                                framed.send(ClientMessage::ClearCommandLog).await?;
                            }
                            DbBridgeCommand::CreateAgentThread { thread_json } => {
                                framed.send(ClientMessage::CreateAgentThread { thread_json }).await?;
                            }
                            DbBridgeCommand::DeleteAgentThread { thread_id } => {
                                framed.send(ClientMessage::DeleteAgentThread { thread_id }).await?;
                            }
                            DbBridgeCommand::ListAgentThreads => {
                                framed.send(ClientMessage::ListAgentThreads).await?;
                            }
                            DbBridgeCommand::GetAgentThread { thread_id } => {
                                framed.send(ClientMessage::GetAgentThread { thread_id }).await?;
                            }
                            DbBridgeCommand::AddAgentMessage { message_json } => {
                                framed.send(ClientMessage::AddAgentMessage { message_json }).await?;
                            }
                            DbBridgeCommand::DeleteAgentMessages { thread_id, message_ids } => {
                                framed.send(ClientMessage::DeleteAgentMessages { thread_id, message_ids }).await?;
                            }
                            DbBridgeCommand::ListAgentMessages { thread_id, limit } => {
                                framed.send(ClientMessage::ListAgentMessages { thread_id, limit }).await?;
                            }
                            DbBridgeCommand::UpsertTranscriptIndex { entry_json } => {
                                framed.send(ClientMessage::UpsertTranscriptIndex { entry_json }).await?;
                            }
                            DbBridgeCommand::ListTranscriptIndex { workspace_id } => {
                                framed.send(ClientMessage::ListTranscriptIndex { workspace_id }).await?;
                            }
                            DbBridgeCommand::UpsertSnapshotIndex { entry_json } => {
                                framed.send(ClientMessage::UpsertSnapshotIndex { entry_json }).await?;
                            }
                            DbBridgeCommand::ListSnapshotIndex { workspace_id } => {
                                framed.send(ClientMessage::ListSnapshotIndex { workspace_id }).await?;
                            }
                            DbBridgeCommand::UpsertAgentEvent { event_json } => {
                                framed.send(ClientMessage::UpsertAgentEvent { event_json }).await?;
                            }
                            DbBridgeCommand::ListAgentEvents { category, pane_id, limit } => {
                                framed.send(ClientMessage::ListAgentEvents { category, pane_id, limit }).await?;
                            }
                            DbBridgeCommand::Shutdown => {
                                break;
                            }
                        }
                    }
                    None => break,
                }
            }
            maybe_message = framed.next() => {
                match maybe_message {
                    Some(Ok(DaemonMessage::CommandLogEntries { entries_json })) => {
                        let msg = serde_json::json!({"type":"command-log-entries","data":serde_json::from_str::<serde_json::Value>(&entries_json).unwrap_or_default()});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::CommandLogAck)) => {
                        let msg = serde_json::json!({"type":"ack"});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentDbThreadList { threads_json })) => {
                        let msg = serde_json::json!({"type":"agent-thread-list","data":serde_json::from_str::<serde_json::Value>(&threads_json).unwrap_or_default()});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentDbThreadDetail { thread_json, messages_json })) => {
                        let msg = serde_json::json!({
                            "type":"agent-thread-detail",
                            "thread": serde_json::from_str::<serde_json::Value>(&thread_json).unwrap_or(serde_json::Value::Null),
                            "messages": serde_json::from_str::<serde_json::Value>(&messages_json).unwrap_or_default(),
                        });
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentDbMessageAck)) => {
                        let msg = serde_json::json!({"type":"ack"});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::TranscriptIndexEntries { entries_json })) => {
                        let msg = serde_json::json!({"type":"transcript-index-entries","data":serde_json::from_str::<serde_json::Value>(&entries_json).unwrap_or_default()});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::SnapshotIndexEntries { entries_json })) => {
                        let msg = serde_json::json!({"type":"snapshot-index-entries","data":serde_json::from_str::<serde_json::Value>(&entries_json).unwrap_or_default()});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentEventRows { events_json })) => {
                        let msg = serde_json::json!({"type":"agent-event-rows","data":serde_json::from_str::<serde_json::Value>(&events_json).unwrap_or_default()});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::Error { message })) => {
                        let msg = serde_json::json!({"type":"error","message":message});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => return Err(error.into()),
                    None => {
                        let msg = serde_json::json!({"type":"error","message":"daemon connection closed"});
                        emit_db_event(&msg.to_string())?;
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
