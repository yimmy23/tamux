// Sub-module declarations — uncomment as modules are implemented
pub mod anticipatory;
pub mod approval;
pub mod audit;
pub mod auth;
pub mod chat;
pub mod collaboration;
pub mod concierge;
pub mod config;
pub mod input;
pub mod input_refs;
pub mod modal;
pub mod notifications;
pub mod settings;
pub mod sidebar;
pub mod statistics;
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
    RequestThread {
        thread_id: String,
        message_limit: Option<usize>,
        message_offset: Option<usize>,
    },
    RequestThreadTodos(String),
    RequestThreadWorkContext(String),
    RequestGoalRunDetail(String),
    RequestGoalRunCheckpoints(String),
    StartGoalRun {
        goal: String,
        thread_id: Option<String>,
        session_id: Option<String>,
    },
    ExplainAction {
        action_id: String,
        step_index: Option<usize>,
    },
    StartDivergentSession {
        problem_statement: String,
        thread_id: String,
        goal_run_id: Option<String>,
    },
    GetDivergentSession {
        session_id: String,
    },
    RequestGitDiff {
        repo_path: String,
        file_path: Option<String>,
    },
    RequestFilePreview {
        path: String,
        max_bytes: Option<usize>,
    },
    RequestAgentStatus,
    RequestAgentStatistics {
        window: amux_protocol::AgentStatisticsWindow,
    },
    RequestPromptInspection {
        agent_id: Option<String>,
    },
    SendMessage {
        thread_id: Option<String>,
        content: String,
        session_id: Option<String>,
        target_agent_id: Option<String>,
    },
    InternalDelegate {
        thread_id: Option<String>,
        target_agent_id: String,
        content: String,
        session_id: Option<String>,
    },
    ThreadParticipantCommand {
        thread_id: String,
        target_agent_id: String,
        action: String,
        instruction: Option<String>,
        session_id: Option<String>,
    },
    SendParticipantSuggestion {
        thread_id: String,
        suggestion_id: String,
    },
    DismissParticipantSuggestion {
        thread_id: String,
        suggestion_id: String,
    },
    StopStream {
        thread_id: String,
    },
    ForceCompact {
        thread_id: String,
    },
    RetryStreamNow {
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
    SetProviderModel {
        provider_id: String,
        model: String,
    },
    SetTargetAgentProviderModel {
        target_agent_id: String,
        provider_id: String,
        model: String,
    },
    ControlGoalRun {
        goal_run_id: String,
        action: String,
    },
    ListTaskApprovalRules,
    CreateTaskApprovalRule {
        approval_id: String,
    },
    RevokeTaskApprovalRule {
        rule_id: String,
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
    GetOpenAICodexAuthStatus,
    LoginOpenAICodex,
    LogoutOpenAICodex,
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
    RetryOperatorProfile,
    StartOperatorProfileSession {
        kind: String,
    },
    NextOperatorProfileQuestion {
        session_id: String,
    },
    SubmitOperatorProfileAnswer {
        session_id: String,
        question_id: String,
        answer_json: String,
    },
    SkipOperatorProfileQuestion {
        session_id: String,
        question_id: String,
        reason: Option<String>,
    },
    DeferOperatorProfileQuestion {
        session_id: String,
        question_id: String,
        defer_until_unix_ms: Option<u64>,
    },
    AnswerOperatorQuestion {
        question_id: String,
        answer: String,
    },
    GetOperatorProfileSummary,
    GetOperatorModel,
    ResetOperatorModel,
    GetCollaborationSessions,
    VoteOnCollaborationDisagreement {
        parent_task_id: String,
        disagreement_id: String,
        task_id: String,
        position: String,
        confidence: Option<f64>,
    },
    GetGeneratedTools,
    SetOperatorProfileConsent {
        consent_key: String,
        granted: bool,
    },
    DismissConciergeWelcome,
    WhatsAppLinkStart,
    WhatsAppLinkStop,
    WhatsAppLinkStatus,
    WhatsAppLinkSubscribe,
    WhatsAppLinkUnsubscribe,
    WhatsAppLinkReset,
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
    PluginListCommands,
    // OAuth (Plan 18-03)
    PluginOAuthStart(String),
    ListNotifications,
    UpsertNotification(amux_protocol::InboxNotification),
}

// ── Placeholder sub-action enums ──────────────────────────────────────────────
// These will be filled in by later tasks.

#[allow(unused_imports)]
pub use anticipatory::{AnticipatoryAction, AnticipatoryState};
#[allow(unused_imports)]
pub use approval::{ApprovalAction, ApprovalFilter, ApprovalState, PendingApproval, RiskLevel};
#[allow(unused_imports)]
pub use audit::{AuditAction, AuditEntryVm, AuditState, EscalationVm, TimeRange};
#[allow(unused_imports)]
pub use auth::{AuthAction, AuthState, ProviderAuthEntry};
#[allow(unused_imports)]
pub use chat::{ChatAction, ChatState, ToolCallStatus, ToolCallVm, TranscriptMode};
#[allow(unused_imports)]
pub use collaboration::{
    CollaborationAction, CollaborationDisagreementVm, CollaborationEscalationVm,
    CollaborationPaneFocus, CollaborationRowVm, CollaborationSessionVm, CollaborationState,
};
#[allow(unused_imports)]
pub use concierge::{ConciergeAction, ConciergeActionVm, ConciergeState};
#[allow(unused_imports)]
pub use config::{ConfigAction, ConfigState};
#[allow(unused_imports)]
pub use input::{InputAction, InputMode, InputState};
#[allow(unused_imports)]
pub use modal::{CommandItem, ModalAction, ModalKind, ModalState};
#[allow(unused_imports)]
pub use notifications::{NotificationsAction, NotificationsHeaderAction, NotificationsState};
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

#[cfg(test)]
mod tests {
    use super::DaemonCommand;

    #[test]
    fn daemon_command_openai_codex_auth_variants_are_constructible() {
        let commands = [
            DaemonCommand::GetOpenAICodexAuthStatus,
            DaemonCommand::LoginOpenAICodex,
            DaemonCommand::LogoutOpenAICodex,
        ];

        assert_eq!(commands.len(), 3);
    }
}
