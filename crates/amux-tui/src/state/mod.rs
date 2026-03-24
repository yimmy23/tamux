// Sub-module declarations — uncomment as modules are implemented
pub mod anticipatory;
pub mod approval;
pub mod audit;
pub mod auth;
pub mod chat;
pub mod concierge;
pub mod config;
pub mod input;
pub mod modal;
pub mod settings;
pub mod sidebar;
pub mod subagents;
pub mod task;
pub mod tier;

// ── Focus ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusArea {
    Chat,
    Sidebar,
    Input,
}

// ── Daemon commands ───────────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum DaemonCommand {
    Refresh,
    RefreshServices,
    RequestThread(String),
    RequestThreadTodos(String),
    RequestThreadWorkContext(String),
    RequestGoalRunDetail(String),
    RequestGoalRunCheckpoints(String),
    StartGoalRun {
        goal: String,
        thread_id: Option<String>,
        session_id: Option<String>,
    },
    RequestGitDiff {
        repo_path: String,
        file_path: Option<String>,
    },
    RequestFilePreview {
        path: String,
        max_bytes: Option<usize>,
    },
    SendMessage {
        thread_id: Option<String>,
        content: String,
        session_id: Option<String>,
    },
    StopStream {
        thread_id: String,
    },
    FetchModels {
        provider_id: String,
        base_url: String,
        api_key: String,
    },
    SetConfigItem {
        key_path: String,
        value_json: String,
    },
    ControlGoalRun {
        goal_run_id: String,
        action: String,
    },
    ResolveTaskApproval {
        approval_id: String,
        decision: String,
    },
    SpawnSession {
        shell: Option<String>,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
    },
    GetProviderAuthStates,
    ValidateProvider {
        provider_id: String,
        base_url: String,
        api_key: String,
        auth_source: String,
    },
    SetSubAgent(String),    // sub_agent_json
    RemoveSubAgent(String), // sub_agent_id
    ListSubAgents,
    GetConciergeConfig,
    SetConciergeConfig(String), // config_json
    RequestConciergeWelcome,
    DismissConciergeWelcome,
    DeleteMessages {
        thread_id: String,
        message_ids: Vec<String>,
    },
    RecordAttention {
        surface: String,
        thread_id: Option<String>,
        goal_run_id: Option<String>,
    },
    AuditDismiss {
        entry_id: String,
    },
    // Plugin commands (Plan 16-03)
    PluginList,
    PluginGet(String),
    PluginEnable(String),
    PluginDisable(String),
    PluginGetSettings(String),
    PluginUpdateSetting {
        plugin_name: String,
        key: String,
        value: String,
        is_secret: bool,
    },
    PluginTestConnection(String),
}

// ── Placeholder sub-action enums ──────────────────────────────────────────────
// These will be filled in by later tasks.

#[allow(unused_imports)]
pub use anticipatory::{AnticipatoryAction, AnticipatoryState};
#[allow(unused_imports)]
pub use approval::{ApprovalAction, ApprovalState, PendingApproval, RiskLevel};
#[allow(unused_imports)]
pub use audit::{AuditAction, AuditEntryVm, AuditState, EscalationVm, TimeRange};
#[allow(unused_imports)]
pub use auth::{AuthAction, AuthState, ProviderAuthEntry};
#[allow(unused_imports)]
pub use chat::{ChatAction, ChatState, ToolCallStatus, ToolCallVm, TranscriptMode};
#[allow(unused_imports)]
pub use concierge::{ConciergeAction, ConciergeActionVm, ConciergeState};
#[allow(unused_imports)]
pub use config::{ConfigAction, ConfigState};
#[allow(unused_imports)]
pub use input::{InputAction, InputMode, InputState};
#[allow(unused_imports)]
pub use modal::{CommandItem, ModalAction, ModalKind, ModalState};
#[allow(unused_imports)]
pub use settings::{SettingsAction, SettingsState, SettingsTab};
#[allow(unused_imports)]
pub use sidebar::{SidebarAction, SidebarItemTarget, SidebarState, SidebarTab};
#[allow(unused_imports)]
pub use subagents::{SubAgentEntry, SubAgentsAction, SubAgentsState};
#[allow(unused_imports)]
pub use task::{TaskAction, TaskState};
#[allow(unused_imports)]
pub use tier::TierState;

// ── Top-level app action ──────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug)]
pub enum AppAction {
    Chat(ChatAction),
    Task(TaskAction),
    Audit(AuditAction),
    Sidebar(SidebarAction),
    Input(InputAction),
    Modal(ModalAction),
    Config(ConfigAction),
    Approval(ApprovalAction),
    Settings(SettingsAction),
    Status(String),
    Focus(FocusArea),
    Resize { width: u16, height: u16 },
    Tick,
    Connected,
    Disconnected,
    Quit,
}

// ── Re-exports ────────────────────────────────────────────────────────────────

#[allow(unused_imports)]
pub use AppAction as Action;
