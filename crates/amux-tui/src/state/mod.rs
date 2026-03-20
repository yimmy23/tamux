// Sub-module declarations — uncomment as modules are implemented
pub mod approval;
pub mod chat;
pub mod config;
pub mod input;
pub mod modal;
pub mod settings;
pub mod sidebar;
pub mod task;

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
    RequestGoalRunDetail(String),
    SendMessage {
        thread_id: Option<String>,
        content: String,
        session_id: Option<String>,
    },
    FetchModels {
        provider_id: String,
        base_url: String,
        api_key: String,
    },
    SetConfigJson(String),
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
}

// ── Placeholder sub-action enums ──────────────────────────────────────────────
// These will be filled in by later tasks.

#[allow(unused_imports)]
pub use approval::{ApprovalAction, ApprovalState, PendingApproval, RiskLevel};
#[allow(unused_imports)]
pub use chat::{ChatAction, ChatState, ToolCallStatus, ToolCallVm, TranscriptMode};
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
pub use task::{TaskAction, TaskState};

// ── Top-level app action ──────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug)]
pub enum AppAction {
    Chat(ChatAction),
    Task(TaskAction),
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
