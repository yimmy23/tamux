//! TuiModel compositor -- delegates to decomposed state modules.
//!
//! This replaces the old monolithic 3,500-line app.rs with a clean
//! compositor that owns the 8 state sub-modules and bridges between
//! the daemon client events and the UI state.

mod commands;
mod config_io;
mod conversion;
mod events;
mod input_ops;
mod keyboard;
mod modal_handlers;
mod mouse;
mod render_helpers;
mod rendering;
mod settings_handlers;

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
    pub content: String,
    pub size_bytes: usize,
}

/// A recent autonomous action displayed in the sidebar.
#[derive(Debug, Clone)]
pub struct RecentActionVm {
    pub action_type: String,
    pub summary: String,
    pub timestamp: u64,
}

/// Flat representation of a sidebar item for matching selected index to data.
struct SidebarFlatItem {
    target: Option<sidebar::SidebarItemTarget>,
    title: String,
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
    Task(sidebar::SidebarItemTarget),
    WorkContext,
    FilePreview(ChatFilePreviewTarget),
    GoalComposer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsPickerTarget {
    Provider,
    Model,
    SubAgentProvider,
    SubAgentModel,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingChatActionKind {
    Regenerate,
    Delete,
}

#[derive(Clone, Debug)]
struct PendingChatActionConfirm {
    message_index: usize,
    action: PendingChatActionKind,
}

#[derive(Clone, Debug)]
pub(crate) struct QueuedPrompt {
    pub(crate) text: String,
    copied_until_tick: Option<u64>,
}

impl QueuedPrompt {
    pub(crate) fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueuedPromptAction {
    SendNow,
    Copy,
    Delete,
}

impl QueuedPromptAction {
    const ALL: [QueuedPromptAction; 3] = [
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

pub struct TuiModel {
    // State modules
    chat: chat::ChatState,
    input: input::InputState,
    modal: modal::ModalState,
    sidebar: sidebar::SidebarState,
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

    // Error state
    last_error: Option<String>,
    error_active: bool,
    error_tick: u64,

    // Pending ChatGPT subscription login flow
    openai_auth_url: Option<String>,
    openai_auth_status_text: Option<String>,
    settings_picker_target: Option<SettingsPickerTarget>,
    last_attention_surface: Option<String>,

    // Vim motion state
    pending_g: bool,

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
    pending_chat_action_confirm: Option<PendingChatActionConfirm>,
    chat_action_confirm_accept_selected: bool,
    retry_wait_start_selected: bool,
    held_key_modifiers: KeyModifiers,

    // Pending file attachments (prepended to next submitted message)
    attachments: Vec<Attachment>,

    // Queue of prompts submitted while tool execution is still in flight.
    queued_prompts: Vec<QueuedPrompt>,
    queued_prompt_action: QueuedPromptAction,

    operator_profile: OperatorProfileOnboardingState,

    // Thread ID whose stream was cancelled via double-Esc (ignore further events)
    cancelled_thread_id: Option<String>,

    // Selected target agent for the next brand-new thread started from the thread picker.
    pending_new_thread_target_agent: Option<String>,

    // Ignore a stale concierge welcome that arrives after the user navigated away.
    ignore_pending_concierge_welcome: bool,

    // Gateway connection statuses received from daemon
    pub gateway_statuses: Vec<chat::GatewayStatusVm>,

    pub weles_health: Option<crate::client::WelesHealthVm>,

    // Recent autonomous actions from heartbeat digests (shown in sidebar)
    pub recent_actions: Vec<RecentActionVm>,

    // Active mouse drag selection in the chat pane
    chat_drag_anchor: Option<Position>,
    chat_drag_current: Option<Position>,
    chat_drag_anchor_point: Option<widgets::chat::SelectionPoint>,
    chat_drag_current_point: Option<widgets::chat::SelectionPoint>,
    chat_selection_snapshot: Option<widgets::chat::CachedSelectionSnapshot>,

    // Active mouse drag selection in the work-context preview pane
    work_context_drag_anchor: Option<Position>,
    work_context_drag_current: Option<Position>,
    work_context_drag_anchor_point: Option<widgets::chat::SelectionPoint>,
    work_context_drag_current_point: Option<widgets::chat::SelectionPoint>,
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
    }
}

fn sidebar_tab_label(tab: SidebarTab) -> &'static str {
    match tab {
        SidebarTab::Files => "files",
        SidebarTab::Todos => "todos",
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
            provider_id: "openai".to_string(),
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
}
