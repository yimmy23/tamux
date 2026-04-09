use serde::{Deserialize, Serialize};

mod json_string_or_value {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(value: &str, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            let json = serde_json::from_str::<serde_json::Value>(value)
                .map_err(serde::ser::Error::custom)?;
            json.serialize(serializer)
        } else {
            serializer.serialize_str(value)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let json = serde_json::Value::deserialize(deserializer)?;
            serde_json::to_string(&json).map_err(serde::de::Error::custom)
        } else {
            String::deserialize(deserializer)
        }
    }
}

use super::{SessionId, WorkspaceId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsyncCommandCapability {
    pub version: u32,
    pub supports_operation_acceptance: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientSurface {
    Tui,
    Electron,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationLifecycleState {
    Accepted,
    Started,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationStatusSnapshot {
    pub operation_id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedup: Option<String>,
    pub state: OperationLifecycleState,
    pub revision: u64,
}

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillDiscoveryCandidatePublic {
    #[serde(default)]
    pub variant_id: String,
    #[serde(default)]
    pub skill_name: String,
    #[serde(default)]
    pub variant_name: String,
    #[serde(default)]
    pub relative_path: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub confidence_tier: String,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub context_tags: Vec<String>,
    #[serde(default)]
    pub use_count: u32,
    #[serde(default)]
    pub success_count: u32,
    #[serde(default)]
    pub failure_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillDiscoveryResultPublic {
    pub query: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub confidence_tier: String,
    #[serde(default)]
    pub recommended_action: String,
    #[serde(default)]
    pub explicit_rationale_required: bool,
    #[serde(default)]
    pub workspace_tags: Vec<String>,
    #[serde(default)]
    pub candidates: Vec<SkillDiscoveryCandidatePublic>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDescriptorPublic {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(with = "json_string_or_value")]
    pub parameters: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolListResultPublic {
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    #[serde(default)]
    pub items: Vec<ToolDescriptorPublic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSearchMatchPublic {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(with = "json_string_or_value")]
    pub parameters: String,
    pub score: u32,
    #[serde(default)]
    pub matched_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSearchResultPublic {
    pub query: String,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    #[serde(default)]
    pub items: Vec<ToolSearchMatchPublic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanReportPublic {
    pub verdict: String,
    pub findings_count: u32,
    pub critical_count: u32,
    pub suspicious_count: u32,
}

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
    #[serde(default)]
    pub auth_status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginCommandInfo {
    pub command: String,
    pub plugin_name: String,
    pub description: String,
    pub api_endpoint: Option<String>,
}

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
    pub pane_type: String,
    pub is_active: bool,
    pub session_id: Option<String>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub cwd: Option<String>,
}

pub fn format_topology(topology: &WorkspaceTopology, sessions: &[super::SessionInfo]) -> String {
    let session_map: std::collections::HashMap<String, &super::SessionInfo> =
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OscNotificationPayload {
    pub source: OscSource,
    pub title: String,
    pub body: String,
    pub subtitle: Option<String>,
    pub icon: Option<String>,
    pub progress: Option<u8>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum OscSource {
    Osc9,
    Osc99,
    Osc777,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitChangeEntry {
    pub code: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_path: Option<String>,
    pub kind: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManagedCommandSource {
    Human,
    Agent,
    Replay,
    Gateway,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalDecision {
    ApproveOnce,
    ApproveSession,
    Deny,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SecurityLevel {
    Highest,
    #[default]
    Moderate,
    Lowest,
    Yolo,
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_summary: Option<String>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WormChainTip {
    pub kind: String,
    pub seq: i64,
    pub hash: String,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboxNotificationAction {
    pub id: String,
    pub label: String,
    pub action_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboxNotification {
    pub id: String,
    pub source: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    pub severity: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<i64>,
    #[serde(default)]
    pub actions: Vec<InboxNotificationAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolMatch {
    pub path: String,
    pub line: usize,
    pub kind: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryLedgerStatus {
    pub kind: String,
    pub total_entries: usize,
    pub valid: bool,
    pub first_invalid_seq: Option<usize>,
    pub message: String,
}
