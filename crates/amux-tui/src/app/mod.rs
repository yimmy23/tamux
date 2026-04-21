//! TuiModel compositor -- delegates to decomposed state modules.
//!
//! This replaces the old monolithic 3,500-line app.rs with a clean
//! compositor that owns the 8 state sub-modules and bridges between
//! the daemon client events and the UI state.

mod commands;
mod config_io;
pub(crate) mod conversion;
mod events;
mod input_ops;
mod keyboard;
mod modal_handlers;
mod mouse;
mod render_helpers;
mod rendering;
mod settings_handlers;

use std::process::Child;
use std::sync::mpsc::Receiver;

use crossterm::event::{
    KeyCode, KeyModifiers, ModifierKeyCode, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear};
use tokio::sync::mpsc::UnboundedSender;

use crate::client::ClientEvent;
use crate::providers;
use crate::state::*;
use crate::theme::ThemeTokens;
use crate::widgets;

pub(crate) const TUI_TICK_RATE_MS: u64 = 50;

/// A file attached to the next outgoing message.
#[derive(Debug, Clone)]
pub struct Attachment {
    pub filename: String,
    pub size_bytes: usize,
    pub payload: AttachmentPayload,
}

#[derive(Debug, Clone)]
pub enum AttachmentPayload {
    Text(String),
    ContentBlock(serde_json::Value),
}

/// A recent autonomous action displayed in the sidebar.
#[derive(Debug, Clone)]
pub struct RecentActionVm {
    pub action_type: String,
    pub summary: String,
    pub timestamp: u64,
}

#[derive(Clone, Copy, Debug)]
struct PaneLayout {
    chat: Rect,
    sidebar: Option<Rect>,
    concierge: Rect,
    input: Rect,
}

#[derive(Clone, Debug)]
pub(crate) struct ChatFilePreviewTarget {
    pub(crate) path: String,
    pub(crate) repo_root: Option<String>,
    pub(crate) repo_relative_path: Option<String>,
}

#[derive(Clone, Debug)]
enum MainPaneView {
    Conversation,
    Collaboration,
    Task(sidebar::SidebarItemTarget),
    WorkContext,
    FilePreview(ChatFilePreviewTarget),
    GoalComposer,
}

#[derive(Clone, Debug, Default)]
struct MissionControlNavigationState {
    source_goal_target: Option<sidebar::SidebarItemTarget>,
    return_to_goal_target: Option<sidebar::SidebarItemTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum GoalSidebarSelectionAnchor {
    Step(String),
    Checkpoint(String),
    Task(String),
    File { thread_id: String, path: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsPickerTarget {
    Provider,
    Model,
    AudioSttProvider,
    AudioSttModel,
    AudioTtsProvider,
    AudioTtsModel,
    ImageGenerationProvider,
    ImageGenerationModel,
    BuiltinPersonaProvider,
    BuiltinPersonaModel,
    CompactionWelesProvider,
    CompactionWelesModel,
    CompactionCustomProvider,
    CompactionCustomModel,
    SubAgentProvider,
    SubAgentModel,
    SubAgentRole,
    SubAgentReasoningEffort,
    ConciergeProvider,
    ConciergeModel,
    ConciergeReasoningEffort,
    CompactionWelesReasoningEffort,
    CompactionCustomReasoningEffort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputNoticeKind {
    Warning,
    Success,
}

#[derive(Clone, Debug)]
struct InputNotice {
    text: String,
    kind: InputNoticeKind,
    expires_at_tick: u64,
    dismiss_on_interaction: bool,
}

#[derive(Clone, Debug)]
enum PendingConfirmAction {
    RegenerateMessage {
        message_index: usize,
    },
    DeleteMessage {
        message_index: usize,
    },
    DeleteThread {
        thread_id: String,
        title: String,
    },
    StopThread {
        thread_id: String,
        title: String,
    },
    ResumeThread {
        thread_id: String,
        title: String,
    },
    DeleteGoalRun {
        goal_run_id: String,
        title: String,
    },
    PauseGoalRun {
        goal_run_id: String,
        title: String,
    },
    StopGoalRun {
        goal_run_id: String,
        title: String,
    },
    ResumeGoalRun {
        goal_run_id: String,
        title: String,
    },
    RetryGoalStep {
        goal_run_id: String,
        goal_title: String,
        step_index: usize,
        step_title: String,
    },
    RerunGoalFromStep {
        goal_run_id: String,
        goal_title: String,
        step_index: usize,
        step_title: String,
    },
    ReuseModelAsStt {
        model_id: String,
    },
}

impl PendingConfirmAction {
    fn modal_body(&self) -> String {
        match self {
            PendingConfirmAction::RegenerateMessage { message_index } => {
                format!("Proceed with regenerate for message {}?", message_index + 1)
            }
            PendingConfirmAction::DeleteMessage { message_index } => {
                format!("Proceed with delete for message {}?", message_index + 1)
            }
            PendingConfirmAction::DeleteThread { title, .. } => {
                format!("Delete thread \"{title}\"?")
            }
            PendingConfirmAction::StopThread { title, .. } => {
                format!("Stop thread \"{title}\"?")
            }
            PendingConfirmAction::ResumeThread { title, .. } => {
                format!("Resume thread \"{title}\"?")
            }
            PendingConfirmAction::DeleteGoalRun { title, .. } => {
                format!("Delete goal run \"{title}\"?")
            }
            PendingConfirmAction::PauseGoalRun { title, .. } => {
                format!("Pause goal run \"{title}\"?")
            }
            PendingConfirmAction::StopGoalRun { title, .. } => {
                format!("Stop goal run \"{title}\"?")
            }
            PendingConfirmAction::ResumeGoalRun { title, .. } => {
                format!("Resume goal run \"{title}\"?")
            }
            PendingConfirmAction::RetryGoalStep {
                goal_title,
                step_index,
                step_title,
                ..
            } => format!(
                "Retry step {} \"{}\" in goal \"{}\"?",
                step_index + 1,
                step_title,
                goal_title
            ),
            PendingConfirmAction::RerunGoalFromStep {
                goal_title,
                step_index,
                step_title,
                ..
            } => format!(
                "Rerun from step {} \"{}\" in goal \"{}\"?",
                step_index + 1,
                step_title,
                goal_title
            ),
            PendingConfirmAction::ReuseModelAsStt { model_id } => {
                if model_id == "__mission_control__:next_turn" {
                    "Apply the pending Mission Control roster change on the next turn?".to_string()
                } else if model_id == "__mission_control__:reassign_active_step" {
                    "Reassign the active step with the pending Mission Control roster change?"
                        .to_string()
                } else if model_id == "__mission_control__:restart_active_step" {
                    "Restart the active step with the pending Mission Control roster change?"
                        .to_string()
                } else {
                    "Selected model supports audio. Use it as the STT model too?".to_string()
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
struct BuiltinPersonaSetupConfigSnapshot {
    provider: String,
    base_url: String,
    model: String,
    custom_model_name: String,
    api_key: String,
    assistant_id: String,
    auth_source: String,
    api_transport: String,
    custom_context_window_tokens: Option<u32>,
    context_window_tokens: u32,
    fetched_models: Vec<config::FetchedModel>,
}

#[derive(Clone, Debug)]
struct PendingBuiltinPersonaSetup {
    target_agent_id: String,
    target_agent_name: String,
    prompt: String,
    config_snapshot: BuiltinPersonaSetupConfigSnapshot,
}

#[derive(Clone, Debug)]
struct ParticipantPlaygroundActivity {
    visible_thread_id: String,
    participant_agent_id: String,
    participant_agent_name: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingPinnedBudgetExceeded {
    thread_id: String,
    message_id: String,
    current_pinned_chars: usize,
    pinned_budget_chars: usize,
    candidate_pinned_chars: usize,
}

#[derive(Clone, Debug)]
struct PendingPinnedJump {
    thread_id: String,
    message_id: String,
    absolute_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingPinnedShortcutLeader {
    Active,
}

#[derive(Clone, Debug)]
pub(crate) struct QueuedPrompt {
    pub(crate) text: String,
    pub(crate) thread_id: Option<String>,
    pub(crate) suggestion_id: Option<String>,
    pub(crate) participant_agent_id: Option<String>,
    pub(crate) participant_agent_name: Option<String>,
    pub(crate) force_send: bool,
    copied_until_tick: Option<u64>,
}

impl QueuedPrompt {
    pub(crate) fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            thread_id: None,
            suggestion_id: None,
            participant_agent_id: None,
            participant_agent_name: None,
            force_send: false,
            copied_until_tick: None,
        }
    }

    pub(crate) fn new_with_agent(
        text: impl Into<String>,
        thread_id: impl Into<String>,
        suggestion_id: impl Into<String>,
        participant_agent_id: impl Into<String>,
        participant_agent_name: impl Into<String>,
        force_send: bool,
    ) -> Self {
        Self {
            text: text.into(),
            thread_id: Some(thread_id.into()),
            suggestion_id: Some(suggestion_id.into()),
            participant_agent_id: Some(participant_agent_id.into()),
            participant_agent_name: Some(participant_agent_name.into()),
            force_send,
            copied_until_tick: None,
        }
    }

    pub(crate) fn is_copied(&self, current_tick: u64) -> bool {
        self.copied_until_tick
            .is_some_and(|expires_at| current_tick < expires_at)
    }

    fn mark_copied(&mut self, expires_at_tick: u64) {
        self.copied_until_tick = Some(expires_at_tick);
    }

    fn clear_expired_copy_feedback(&mut self, current_tick: u64) {
        if self
            .copied_until_tick
            .is_some_and(|expires_at| current_tick >= expires_at)
        {
            self.copied_until_tick = None;
        }
    }

    pub(crate) fn display_text(&self) -> String {
        match self.participant_agent_name.as_deref() {
            Some(agent_name) => format!("{agent_name}: {}", self.text),
            None => self.text.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueuedPromptAction {
    Expand,
    SendNow,
    Copy,
    Delete,
}

impl QueuedPromptAction {
    const ALL: [QueuedPromptAction; 4] = [
        QueuedPromptAction::Expand,
        QueuedPromptAction::SendNow,
        QueuedPromptAction::Copy,
        QueuedPromptAction::Delete,
    ];

    fn step(self, delta: i32) -> Self {
        let current = Self::ALL
            .iter()
            .position(|action| *action == self)
            .unwrap_or(0) as i32;
        let next = (current + delta).clamp(0, (Self::ALL.len() - 1) as i32) as usize;
        Self::ALL[next]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoResponseActionSelection {
    Yes,
    No,
    Always,
}

#[derive(Clone, Debug)]
struct OperatorProfileQuestionVm {
    session_id: String,
    question_id: String,
    field_key: String,
    prompt: String,
    input_kind: String,
    optional: bool,
}

#[derive(Clone, Debug)]
struct OperatorProfileProgressVm {
    answered: u32,
    remaining: u32,
    completion_ratio: f64,
}

#[derive(Clone, Debug, Default)]
struct OperatorProfileOnboardingState {
    visible: bool,
    loading: bool,
    session_id: Option<String>,
    session_kind: Option<String>,
    question: Option<OperatorProfileQuestionVm>,
    progress: Option<OperatorProfileProgressVm>,
    summary_json: Option<String>,
    warning: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingReconnectRestore {
    thread_id: String,
    should_resume: bool,
}

pub struct TuiModel {
    // State modules
    chat: chat::ChatState,
    input: input::InputState,
    modal: modal::ModalState,
    sidebar: sidebar::SidebarState,
    goal_sidebar: goal_sidebar::GoalSidebarState,
    goal_mission_control: goal_mission_control::GoalMissionControlState,
    goal_workspace: goal_workspace::GoalWorkspaceState,
    mission_control_navigation: MissionControlNavigationState,
    goal_sidebar_selection_anchor: Option<GoalSidebarSelectionAnchor>,
    tasks: task::TaskState,
    config: config::ConfigState,
    approval: approval::ApprovalState,
    anticipatory: AnticipatoryState,
    pub audit: crate::state::audit::AuditState,
    notifications: notifications::NotificationsState,
    settings: settings::SettingsState,
    pub plugin_settings: settings::PluginSettingsState,
    pub auth: AuthState,
    pub subagents: SubAgentsState,
    pub collaboration: CollaborationState,
    pub concierge: ConciergeState,
    pub tier: TierState,

    // UI chrome
    focus: FocusArea,
    theme: ThemeTokens,
    width: u16,
    height: u16,

    // Infrastructure
    daemon_cmd_tx: UnboundedSender<DaemonCommand>,
    daemon_events_rx: Receiver<ClientEvent>,

    // Status
    connected: bool,
    agent_config_loaded: bool,
    status_line: String,
    default_session_id: Option<String>,
    tick_counter: u64,

    // Agent activity state (from daemon events, not local buffers)
    agent_activity: Option<String>,
    thread_agent_activity: std::collections::HashMap<String, String>,
    bootstrap_pending_activity_threads: std::collections::HashSet<String>,
    participant_playground_activity:
        std::collections::HashMap<String, ParticipantPlaygroundActivity>,

    // Error state
    last_error: Option<String>,
    error_active: bool,
    error_tick: u64,

    // Pending ChatGPT subscription login flow
    openai_auth_url: Option<String>,
    openai_auth_status_text: Option<String>,
    settings_picker_target: Option<SettingsPickerTarget>,
    last_attention_surface: Option<String>,

    // Responsive layout override: when Some, overrides breakpoint-based sidebar visibility
    show_sidebar_override: Option<bool>,
    main_pane_view: MainPaneView,
    task_view_scroll: usize,
    task_show_live_todos: bool,
    task_show_timeline: bool,
    task_show_files: bool,

    // Set by /quit command; checked after modal enter to issue quit
    pending_quit: bool,

    // Double-Esc stream stop state
    pending_stop: bool,
    pending_stop_tick: u64,
    input_notice: Option<InputNotice>,
    pending_chat_action_confirm: Option<PendingConfirmAction>,
    pending_pinned_budget_exceeded: Option<PendingPinnedBudgetExceeded>,
    pending_pinned_jump: Option<PendingPinnedJump>,
    pending_pinned_shortcut_leader: Option<PendingPinnedShortcutLeader>,
    chat_action_confirm_accept_selected: bool,
    retry_wait_start_selected: bool,
    auto_response_selection: AutoResponseActionSelection,
    held_key_modifiers: KeyModifiers,

    // Pending file attachments (prepended to next submitted message)
    attachments: Vec<Attachment>,

    // Voice capture / playback state
    voice_recording: bool,
    voice_capture_path: Option<String>,
    voice_capture_stderr_path: Option<String>,
    voice_capture_backend_label: Option<String>,
    voice_recorder: Option<Child>,
    voice_player: Option<Child>,

    // Queue of prompts submitted while tool execution is still in flight.
    queued_prompts: Vec<QueuedPrompt>,
    queued_prompt_action: QueuedPromptAction,
    hidden_auto_response_suggestion_ids: std::collections::HashSet<String>,

    operator_profile: OperatorProfileOnboardingState,

    // Thread ID whose stream was cancelled via double-Esc (ignore further events)
    cancelled_thread_id: Option<String>,

    // Selected target agent for the next brand-new thread started from the thread picker.
    pending_new_thread_target_agent: Option<String>,

    // Builtin persona setup flow launched from @agent / !agent commands.
    pending_builtin_persona_setup: Option<PendingBuiltinPersonaSetup>,

    // Thread currently awaiting full detail from the daemon.
    thread_loading_id: Option<String>,
    pending_reconnect_restore: Option<PendingReconnectRestore>,
    pending_goal_hydration_refreshes: std::collections::HashSet<String>,

    // Ignore a stale concierge welcome that arrives after the user navigated away.
    ignore_pending_concierge_welcome: bool,

    // Gateway connection statuses received from daemon
    pub gateway_statuses: Vec<chat::GatewayStatusVm>,

    pub weles_health: Option<crate::client::WelesHealthVm>,

    // Recent autonomous actions from heartbeat digests (shown in sidebar)
    pub recent_actions: Vec<RecentActionVm>,
    status_modal_snapshot: Option<crate::client::AgentStatusSnapshotVm>,
    status_modal_diagnostics_json: Option<String>,
    status_modal_loading: bool,
    status_modal_error: Option<String>,
    status_modal_scroll: usize,
    statistics_modal_snapshot: Option<amux_protocol::AgentStatisticsSnapshot>,
    statistics_modal_loading: bool,
    statistics_modal_error: Option<String>,
    statistics_modal_tab: crate::state::statistics::StatisticsTab,
    statistics_modal_window: amux_protocol::AgentStatisticsWindow,
    statistics_modal_scroll: usize,
    prompt_modal_snapshot: Option<crate::client::AgentPromptInspectionVm>,
    prompt_modal_loading: bool,
    prompt_modal_error: Option<String>,
    prompt_modal_scroll: usize,
    prompt_modal_title_override: Option<String>,
    prompt_modal_body_override: Option<String>,
    settings_modal_scroll: usize,
    thread_participants_modal_scroll: usize,
    help_modal_scroll: usize,

    // Active mouse drag selection in the chat pane
    chat_drag_anchor: Option<Position>,
    chat_drag_current: Option<Position>,
    chat_drag_anchor_point: Option<widgets::chat::SelectionPoint>,
    chat_drag_current_point: Option<widgets::chat::SelectionPoint>,
    chat_selection_snapshot: Option<widgets::chat::CachedSelectionSnapshot>,
    chat_scrollbar_drag_grab_offset: Option<u16>,

    // Active mouse drag selection in the work-context preview pane
    work_context_drag_anchor: Option<Position>,
    work_context_drag_current: Option<Position>,
    work_context_drag_anchor_point: Option<widgets::chat::SelectionPoint>,
    work_context_drag_current_point: Option<widgets::chat::SelectionPoint>,

    // Active mouse drag selection in the goal/task detail pane
    task_view_drag_anchor: Option<Position>,
    task_view_drag_current: Option<Position>,
    task_view_drag_anchor_point: Option<widgets::chat::SelectionPoint>,
    task_view_drag_current_point: Option<widgets::chat::SelectionPoint>,
}

include!("model_impl_part1.rs");
include!("model_impl_part2.rs");
include!("model_impl_part3.rs");

fn settings_tab_label(tab: SettingsTab) -> &'static str {
    match tab {
        SettingsTab::Provider => "provider",
        SettingsTab::Tools => "tools",
        SettingsTab::WebSearch => "web_search",
        SettingsTab::Chat => "chat",
        SettingsTab::Gateway => "gateway",
        SettingsTab::Auth => "auth",
        SettingsTab::Agent => "agent",
        SettingsTab::SubAgents => "subagents",
        SettingsTab::Concierge => "concierge",
        SettingsTab::Features => "features",
        SettingsTab::Advanced => "advanced",
        SettingsTab::Plugins => "plugins",
        SettingsTab::About => "about",
    }
}

fn sidebar_tab_label(tab: SidebarTab) -> &'static str {
    match tab {
        SidebarTab::Files => "files",
        SidebarTab::Todos => "todos",
        SidebarTab::Spawned => "spawned",
        SidebarTab::Pinned => "pinned",
    }
}

fn target_goal_run_id(model: &TuiModel, target: &SidebarItemTarget) -> Option<String> {
    match target {
        SidebarItemTarget::GoalRun { goal_run_id, .. } => Some(goal_run_id.clone()),
        SidebarItemTarget::Task { task_id } => model
            .tasks
            .task_by_id(task_id)
            .and_then(|task| task.goal_run_id.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::conversion;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::sync::mpsc;
    use tokio::sync::mpsc::unbounded_channel;

    fn build_model() -> TuiModel {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, _cmd_rx) = unbounded_channel();
        TuiModel::new(daemon_rx, cmd_tx)
    }

    fn unauthenticated_entry() -> ProviderAuthEntry {
        ProviderAuthEntry {
            provider_id: amux_shared::providers::PROVIDER_ID_OPENAI.to_string(),
            provider_name: "OpenAI".to_string(),
            authenticated: false,
            auth_source: "api_key".to_string(),
            model: "gpt-5.4".to_string(),
        }
    }

    fn rendered_chat_area(model: &TuiModel) -> Rect {
        let area = Rect::new(0, 0, model.width, model.height);
        let input_height = model.input_height();
        let anticipatory_height = model.anticipatory_banner_height();
        let concierge_height = model.concierge_banner_height();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(anticipatory_height),
                Constraint::Length(concierge_height),
                Constraint::Length(input_height),
                Constraint::Length(1),
            ])
            .split(area);

        if model.sidebar_visible() {
            let sidebar_pct = if model.width >= 120 { 33 } else { 28 };
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(100 - sidebar_pct),
                    Constraint::Percentage(sidebar_pct),
                ])
                .split(chunks[1])[0]
        } else {
            chunks[1]
        }
    }

    include!("tests/tests_part1.rs");
    include!("tests/tests_part2.rs");
    include!("tests/tests_part3.rs");
    include!("tests/tests_part4.rs");
    include!("tests/tests_part5.rs");
    include!("tests/tests_part6.rs");
    include!("tests/tests_part7_multithread_events.rs");
    include!("tests/tests_part8_goal_mission_control.rs");
}
