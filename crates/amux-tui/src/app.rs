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

#[derive(Clone, Debug)]
enum MainPaneView {
    Conversation,
    Task(sidebar::SidebarItemTarget),
    WorkContext,
    GoalComposer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsPickerTarget {
    Provider,
    Model,
    SubAgentProvider,
    SubAgentModel,
    ConciergeProvider,
    ConciergeModel,
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
    held_key_modifiers: KeyModifiers,

    // Pending file attachments (prepended to next submitted message)
    attachments: Vec<Attachment>,

    // Queue of prompts submitted while streaming (auto-sent after TurnDone)
    queued_prompts: Vec<String>,

    // Thread ID whose stream was cancelled via double-Esc (ignore further events)
    cancelled_thread_id: Option<String>,

    // Ignore a stale concierge welcome that arrives after the user navigated away.
    ignore_pending_concierge_welcome: bool,

    // Gateway connection statuses received from daemon
    pub gateway_statuses: Vec<chat::GatewayStatusVm>,

    // Recent autonomous actions from heartbeat digests (shown in sidebar)
    pub recent_actions: Vec<RecentActionVm>,

    // Active mouse drag selection in the chat pane
    chat_drag_anchor: Option<Position>,
    chat_drag_current: Option<Position>,
    chat_drag_anchor_point: Option<widgets::chat::SelectionPoint>,
    chat_drag_current_point: Option<widgets::chat::SelectionPoint>,

    // Active mouse drag selection in the work-context preview pane
    work_context_drag_anchor: Option<Position>,
    work_context_drag_current: Option<Position>,
    work_context_drag_anchor_point: Option<widgets::chat::SelectionPoint>,
    work_context_drag_current_point: Option<widgets::chat::SelectionPoint>,
}

impl TuiModel {
    pub fn new(
        daemon_events_rx: Receiver<ClientEvent>,
        daemon_cmd_tx: UnboundedSender<DaemonCommand>,
    ) -> Self {
        Self {
            chat: chat::ChatState::new(),
            input: input::InputState::new(),
            modal: modal::ModalState::new(),
            sidebar: sidebar::SidebarState::new(),
            tasks: task::TaskState::new(),
            config: config::ConfigState::new(),
            approval: approval::ApprovalState::new(),
            anticipatory: AnticipatoryState::new(),
            audit: crate::state::audit::AuditState::new(),
            settings: settings::SettingsState::new(),
            plugin_settings: settings::PluginSettingsState::new(),
            auth: AuthState::new(),
            subagents: SubAgentsState::new(),
            concierge: ConciergeState::new(),
            tier: TierState::default(),
            focus: FocusArea::Input,
            theme: ThemeTokens::default(),
            width: 120,
            height: 40,
            daemon_cmd_tx,
            daemon_events_rx,
            connected: false,
            agent_config_loaded: false,
            status_line: "Starting...".to_string(),
            default_session_id: None,
            tick_counter: 0,
            agent_activity: None,
            last_error: None,
            error_active: false,
            error_tick: 0,
            openai_auth_url: None,
            openai_auth_status_text: None,
            settings_picker_target: None,
            last_attention_surface: None,
            pending_g: false,
            show_sidebar_override: None,
            main_pane_view: MainPaneView::Conversation,
            task_view_scroll: 0,
            task_show_live_todos: true,
            task_show_timeline: true,
            task_show_files: true,
            pending_quit: false,
            pending_stop: false,
            pending_stop_tick: 0,
            input_notice: None,
            held_key_modifiers: KeyModifiers::NONE,
            attachments: Vec::new(),
            queued_prompts: Vec::new(),
            cancelled_thread_id: None,
            ignore_pending_concierge_welcome: false,
            gateway_statuses: Vec::new(),
            recent_actions: Vec::new(),
            chat_drag_anchor: None,
            chat_drag_current: None,
            chat_drag_anchor_point: None,
            chat_drag_current_point: None,
            work_context_drag_anchor: None,
            work_context_drag_current: None,
            work_context_drag_anchor_point: None,
            work_context_drag_current_point: None,
        }
    }

    fn send_daemon_command(&self, command: DaemonCommand) {
        let _ = self.daemon_cmd_tx.send(command);
    }

    fn show_input_notice(
        &mut self,
        text: impl Into<String>,
        kind: InputNoticeKind,
        duration_ticks: u64,
        dismiss_on_interaction: bool,
    ) {
        self.input_notice = Some(InputNotice {
            text: text.into(),
            kind,
            expires_at_tick: self.tick_counter.saturating_add(duration_ticks),
            dismiss_on_interaction,
        });
    }

    fn clear_dismissable_input_notice(&mut self) {
        if self
            .input_notice
            .as_ref()
            .is_some_and(|notice| notice.dismiss_on_interaction)
        {
            self.input_notice = None;
        }
    }

    fn clear_pending_stop(&mut self) {
        self.pending_stop = false;
        self.clear_dismissable_input_notice();
    }

    fn pending_stop_active(&self) -> bool {
        self.pending_stop && self.tick_counter.saturating_sub(self.pending_stop_tick) < 100
    }

    fn assistant_busy(&self) -> bool {
        self.chat.is_streaming() || self.agent_activity.is_some()
    }

    fn actions_bar_visible(&self) -> bool {
        if self.should_show_local_landing() {
            return false;
        }

        self.concierge.loading || !self.chat.active_actions().is_empty()
    }

    fn concierge_banner_visible(&self) -> bool {
        self.actions_bar_visible()
    }

    fn should_show_local_landing(&self) -> bool {
        matches!(self.main_pane_view, MainPaneView::Conversation)
            && self.chat.active_thread().is_none()
            && !self.chat.is_streaming()
            && !self.concierge.loading
            && !self.should_show_provider_onboarding()
    }

    fn should_show_concierge_hero_loading(&self) -> bool {
        self.concierge.loading
            && matches!(self.main_pane_view, MainPaneView::Conversation)
            && self.chat.active_thread().is_none()
            && self.chat.streaming_content().is_empty()
            && !self.concierge.has_active_welcome()
    }

    fn concierge_banner_height(&self) -> u16 {
        if self.should_show_concierge_hero_loading() {
            0
        } else if self.actions_bar_visible() {
            1
        } else {
            0
        }
    }

    fn anticipatory_banner_height(&self) -> u16 {
        if self.anticipatory.has_items() {
            8
        } else {
            0
        }
    }

    fn has_configured_provider(&self) -> bool {
        self.auth.entries.iter().any(|entry| entry.authenticated)
    }

    fn should_show_provider_onboarding(&self) -> bool {
        self.connected
            && self.auth.loaded
            && !self.has_configured_provider()
            && matches!(self.main_pane_view, MainPaneView::Conversation)
            && self.chat.active_thread().is_none()
            && self.chat.streaming_content().is_empty()
    }

    fn open_settings_tab(&mut self, tab: SettingsTab) {
        if self.modal.top() != Some(modal::ModalKind::Settings) {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
        }
        self.settings.reduce(SettingsAction::SwitchTab(tab));
        self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
        self.send_daemon_command(DaemonCommand::ListSubAgents);
        self.send_daemon_command(DaemonCommand::GetConciergeConfig);
    }

    fn open_provider_setup(&mut self) {
        self.open_settings_tab(SettingsTab::Provider);
        self.status_line = "Configure provider credentials to start chatting".to_string();
    }

    fn set_input_text(&mut self, text: &str) {
        self.input.reduce(input::InputAction::Clear);
        for ch in text.chars() {
            self.input.reduce(input::InputAction::InsertChar(ch));
        }
        self.input.set_mode(input::InputMode::Insert);
    }

    fn close_top_modal(&mut self) {
        if self.modal.top() == Some(modal::ModalKind::WhatsAppLink) {
            self.send_daemon_command(DaemonCommand::WhatsAppLinkStop);
            self.send_daemon_command(DaemonCommand::WhatsAppLinkUnsubscribe);
            self.modal.reset_whatsapp_link();
        }
        self.modal.reduce(modal::ModalAction::Pop);
    }

    fn cleanup_concierge_on_navigate(&mut self) {
        if !self.concierge.auto_cleanup_on_navigate {
            return;
        }

        self.ignore_pending_concierge_welcome = true;
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeDismissed);
        self.chat.reduce(chat::ChatAction::DismissConciergeWelcome);
        self.send_daemon_command(DaemonCommand::DismissConciergeWelcome);

        if self.chat.active_thread_id() == Some("concierge") && self.assistant_busy() {
            let thread_id = "concierge".to_string();
            self.cancelled_thread_id = Some(thread_id.clone());
            self.chat.reduce(chat::ChatAction::ForceStopStreaming);
            self.agent_activity = None;
            self.send_daemon_command(DaemonCommand::StopStream { thread_id });
        }

        self.clear_pending_stop();
    }

    fn open_thread_conversation(&mut self, thread_id: String) {
        self.cleanup_concierge_on_navigate();
        self.chat
            .reduce(chat::ChatAction::SelectThread(thread_id.clone()));
        self.send_daemon_command(DaemonCommand::RequestThread(thread_id));
        self.main_pane_view = MainPaneView::Conversation;
        self.focus = FocusArea::Chat;
    }

    fn start_new_thread_view(&mut self) {
        self.cleanup_concierge_on_navigate();
        self.chat.reduce(chat::ChatAction::NewThread);
        self.main_pane_view = MainPaneView::Conversation;
        self.focus = FocusArea::Input;
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeLoading(false));
    }

    fn request_concierge_welcome(&mut self) {
        self.ignore_pending_concierge_welcome = false;
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeLoading(true));
        self.send_daemon_command(DaemonCommand::RequestConciergeWelcome);
    }

    fn set_main_pane_conversation(&mut self, focus: FocusArea) {
        self.main_pane_view = MainPaneView::Conversation;
        self.task_view_scroll = 0;
        self.focus = focus;
    }

    fn dismiss_active_main_pane(&mut self, focus: FocusArea) -> bool {
        match &self.main_pane_view {
            MainPaneView::Task(target) => {
                if let Some(thread_id) = self.target_thread_id(target) {
                    if self.tasks.selected_work_path(&thread_id).is_some() {
                        self.tasks.reduce(task::TaskAction::SelectWorkPath {
                            thread_id,
                            path: None,
                        });
                        self.focus = focus;
                        return true;
                    }
                }
                self.set_main_pane_conversation(focus);
                true
            }
            MainPaneView::WorkContext | MainPaneView::GoalComposer => {
                self.set_main_pane_conversation(focus);
                true
            }
            MainPaneView::Conversation => false,
        }
    }

    fn should_toggle_work_context_from_sidebar(&self, thread_id: &str) -> bool {
        if !matches!(self.main_pane_view, MainPaneView::WorkContext) {
            return false;
        }

        match self.sidebar.active_tab() {
            SidebarTab::Files => self
                .tasks
                .work_context_for_thread(thread_id)
                .and_then(|context| context.entries.get(self.sidebar.selected_item()))
                .is_some_and(|entry| {
                    self.tasks.selected_work_path(thread_id) == Some(entry.path.as_str())
                }),
            SidebarTab::Todos => self
                .tasks
                .todos_for_thread(thread_id)
                .get(self.sidebar.selected_item())
                .is_some(),
        }
    }

    fn visible_concierge_action_count(&self) -> usize {
        let active_actions = self.chat.active_actions();
        if !active_actions.is_empty() {
            active_actions.len()
        } else {
            self.concierge.welcome_actions.len()
        }
    }

    fn select_visible_concierge_action(&mut self, action_index: usize) {
        let action_count = self.visible_concierge_action_count();
        self.concierge.selected_action = if action_count == 0 {
            0
        } else {
            action_index.min(action_count - 1)
        };
    }

    fn navigate_visible_concierge_action(&mut self, delta: i32) {
        let action_count = self.visible_concierge_action_count();
        if action_count == 0 {
            self.concierge.selected_action = 0;
        } else if delta > 0 {
            self.concierge.selected_action =
                (self.concierge.selected_action + delta as usize).min(action_count - 1);
        } else {
            self.concierge.selected_action = self
                .concierge
                .selected_action
                .saturating_sub((-delta) as usize);
        }
    }

    fn resolve_visible_concierge_action(
        &self,
        action_index: usize,
    ) -> Option<crate::state::ConciergeActionVm> {
        self.chat
            .active_actions()
            .get(action_index)
            .map(|action| crate::state::ConciergeActionVm {
                label: action.label.clone(),
                action_type: action.action_type.clone(),
                thread_id: action.thread_id.clone(),
            })
            .or_else(|| self.concierge.welcome_actions.get(action_index).cloned())
    }

    fn execute_concierge_action(&mut self, action_index: usize) {
        let Some(action) = self.resolve_visible_concierge_action(action_index) else {
            return;
        };
        self.run_concierge_action(action);
    }

    fn execute_concierge_message_action(&mut self, message_index: usize, action_index: usize) {
        let Some(action) = self
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.get(message_index))
            .and_then(|message| message.actions.get(action_index))
            .cloned()
        else {
            return;
        };
        self.run_concierge_action(crate::state::ConciergeActionVm {
            label: action.label,
            action_type: action.action_type,
            thread_id: action.thread_id,
        });
    }

    fn run_concierge_action(&mut self, action: crate::state::ConciergeActionVm) {
        match action.action_type.as_str() {
            "continue_session" => {
                if let Some(thread_id) = action.thread_id {
                    self.open_thread_conversation(thread_id);
                }
            }
            "start_new" => {
                self.start_new_thread_view();
            }
            "search" => {
                self.ignore_pending_concierge_welcome = true;
                self.concierge
                    .reduce(crate::state::ConciergeAction::WelcomeDismissed);
                self.send_daemon_command(DaemonCommand::DismissConciergeWelcome);
                self.chat
                    .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
                self.send_daemon_command(DaemonCommand::RequestThread("concierge".to_string()));
                self.main_pane_view = MainPaneView::Conversation;
                self.focus = FocusArea::Input;
                self.set_input_text("Search history for: ");
                self.status_line = "Describe what you want to search and press Enter".to_string();
            }
            "dismiss" | "dismiss_welcome" => {
                self.cleanup_concierge_on_navigate();
                if self.chat.active_thread_id() == Some("concierge") {
                    self.chat.reduce(chat::ChatAction::NewThread);
                    self.main_pane_view = MainPaneView::Conversation;
                    self.focus = FocusArea::Input;
                }
            }
            "start_goal_run" => {
                self.cleanup_concierge_on_navigate();
                self.chat
                    .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
                self.send_daemon_command(DaemonCommand::RequestThread("concierge".to_string()));
                self.main_pane_view = MainPaneView::Conversation;
                self.focus = FocusArea::Input;
                self.set_input_text("/goal ");
                self.status_line = "Describe your goal and press Enter".to_string();
            }
            "focus_chat" => {
                self.cleanup_concierge_on_navigate();
                self.chat
                    .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
                self.send_daemon_command(DaemonCommand::RequestThread("concierge".to_string()));
                self.main_pane_view = MainPaneView::Conversation;
                self.focus = FocusArea::Input;
            }
            "open_settings" => {
                self.cleanup_concierge_on_navigate();
                self.open_settings_tab(SettingsTab::Auth);
            }
            _ => {}
        }
    }

    fn update_held_modifier(&mut self, code: KeyCode, pressed: bool) {
        let modifier = match code {
            KeyCode::Modifier(
                ModifierKeyCode::LeftShift
                | ModifierKeyCode::RightShift
                | ModifierKeyCode::IsoLevel3Shift
                | ModifierKeyCode::IsoLevel5Shift,
            ) => Some(KeyModifiers::SHIFT),
            KeyCode::Modifier(ModifierKeyCode::LeftControl | ModifierKeyCode::RightControl) => {
                Some(KeyModifiers::CONTROL)
            }
            KeyCode::Modifier(ModifierKeyCode::LeftAlt | ModifierKeyCode::RightAlt) => {
                Some(KeyModifiers::ALT)
            }
            _ => None,
        };

        if let Some(modifier) = modifier {
            if pressed {
                self.held_key_modifiers.insert(modifier);
            } else {
                self.held_key_modifiers.remove(modifier);
            }
        }
    }

    fn input_notice_style(&self) -> Option<(&str, Style)> {
        self.input_notice.as_ref().map(|notice| {
            let style = match notice.kind {
                InputNoticeKind::Warning => Style::default().fg(Color::Indexed(214)),
                InputNoticeKind::Success => Style::default().fg(Color::Indexed(114)),
            };
            (notice.text.as_str(), style)
        })
    }

    fn sidebar_visible(&self) -> bool {
        if !matches!(
            self.main_pane_view,
            MainPaneView::Conversation | MainPaneView::WorkContext
        ) {
            return false;
        }
        let Some(thread_id) = self.chat.active_thread_id() else {
            return false;
        };
        !self.tasks.todos_for_thread(thread_id).is_empty()
            || self
                .tasks
                .work_context_for_thread(thread_id)
                .is_some_and(|context| !context.entries.is_empty())
    }

    fn current_attention_target(&self) -> (String, Option<String>, Option<String>) {
        if let Some(modal) = self.modal.top() {
            let surface = match modal {
                modal::ModalKind::Settings => {
                    format!(
                        "modal:settings:{}",
                        settings_tab_label(self.settings.active_tab())
                    )
                }
                modal::ModalKind::ApprovalOverlay => "modal:approval".to_string(),
                modal::ModalKind::CommandPalette => "modal:command_palette".to_string(),
                modal::ModalKind::ThreadPicker => "modal:thread_picker".to_string(),
                modal::ModalKind::GoalPicker => "modal:goal_picker".to_string(),
                modal::ModalKind::ProviderPicker => "modal:provider_picker".to_string(),
                modal::ModalKind::ModelPicker => "modal:model_picker".to_string(),
                modal::ModalKind::OpenAIAuth => "modal:openai_auth".to_string(),
                modal::ModalKind::ErrorViewer => "modal:error_viewer".to_string(),
                modal::ModalKind::EffortPicker => "modal:effort_picker".to_string(),
                modal::ModalKind::ToolsPicker => "modal:tools_picker".to_string(),
                modal::ModalKind::ViewPicker => "modal:view_picker".to_string(),
                modal::ModalKind::Help => "modal:help".to_string(),
                modal::ModalKind::WhatsAppLink => "modal:whatsapp_link".to_string(),
            };
            return (
                surface,
                self.chat.active_thread_id().map(str::to_string),
                None,
            );
        }

        match &self.main_pane_view {
            MainPaneView::Conversation => match self.focus {
                FocusArea::Chat => (
                    "conversation:chat".to_string(),
                    self.chat.active_thread_id().map(str::to_string),
                    None,
                ),
                FocusArea::Input => {
                    if self.should_show_provider_onboarding() {
                        ("conversation:onboarding".to_string(), None, None)
                    } else {
                        (
                            "conversation:input".to_string(),
                            self.chat.active_thread_id().map(str::to_string),
                            None,
                        )
                    }
                }
                FocusArea::Sidebar => (
                    format!(
                        "conversation:sidebar:{}",
                        sidebar_tab_label(self.sidebar.active_tab())
                    ),
                    self.chat.active_thread_id().map(str::to_string),
                    None,
                ),
            },
            MainPaneView::Task(target) => (
                "task:detail".to_string(),
                self.target_thread_id(target),
                target_goal_run_id(self, target),
            ),
            MainPaneView::WorkContext => (
                "task:work_context".to_string(),
                self.chat.active_thread_id().map(str::to_string),
                None,
            ),
            MainPaneView::GoalComposer => (
                "task:goal_composer".to_string(),
                self.chat.active_thread_id().map(str::to_string),
                None,
            ),
        }
    }

    fn publish_attention_surface_if_changed(&mut self) {
        if !self.connected {
            return;
        }
        let (surface, thread_id, goal_run_id) = self.current_attention_target();
        let signature = format!(
            "{}|{}|{}",
            surface,
            thread_id.as_deref().unwrap_or_default(),
            goal_run_id.as_deref().unwrap_or_default()
        );
        if self.last_attention_surface.as_deref() == Some(signature.as_str()) {
            return;
        }
        self.last_attention_surface = Some(signature);
        self.send_daemon_command(DaemonCommand::RecordAttention {
            surface,
            thread_id,
            goal_run_id,
        });
    }
}

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

    #[test]
    fn provider_onboarding_requires_loaded_auth_state() {
        let mut model = build_model();
        model.connected = true;
        model.auth.entries = vec![unauthenticated_entry()];

        assert!(!model.should_show_provider_onboarding());
    }

    #[test]
    fn provider_onboarding_shows_when_no_provider_is_configured() {
        let mut model = build_model();
        model.connected = true;
        model.auth.loaded = true;
        model.auth.entries = vec![unauthenticated_entry()];

        assert!(model.should_show_provider_onboarding());
    }

    #[test]
    fn provider_onboarding_hides_when_provider_is_configured() {
        let mut model = build_model();
        model.connected = true;
        model.auth.loaded = true;
        let mut entry = unauthenticated_entry();
        entry.authenticated = true;
        model.auth.entries = vec![entry];

        assert!(!model.should_show_provider_onboarding());
    }

    #[test]
    fn local_landing_shows_only_for_empty_conversation_state() {
        let mut model = build_model();

        assert!(model.should_show_local_landing());

        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "thread-1".to_string(),
            title: "Thread".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
        assert!(!model.should_show_local_landing());

        model
            .chat
            .reduce(chat::ChatAction::SelectThread(String::new()));
        model.chat.reduce(chat::ChatAction::Delta {
            thread_id: "stream".to_string(),
            content: "hello".to_string(),
        });
        assert!(!model.should_show_local_landing());

        model.chat.reduce(chat::ChatAction::ResetStreaming);
        model
            .chat
            .reduce(chat::ChatAction::ThreadListReceived(Vec::new()));
        model.connected = true;
        model.auth.loaded = true;
        model.auth.entries = vec![unauthenticated_entry()];
        assert!(!model.should_show_local_landing());
    }

    #[test]
    fn local_landing_yields_to_concierge_loading() {
        let mut model = build_model();
        model.concierge.loading = true;

        assert!(model.should_show_concierge_hero_loading());
        assert!(
            !model.should_show_local_landing(),
            "local landing should not hide concierge loading animation"
        );
    }

    #[test]
    fn cleanup_concierge_on_navigate_hides_local_welcome_message() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "concierge".to_string(),
            title: "Concierge".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
        model.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id: "concierge".to_string(),
            message: chat::AgentMessage {
                role: chat::MessageRole::Assistant,
                content: "Welcome".to_string(),
                is_concierge_welcome: true,
                ..Default::default()
            },
        });
        model
            .concierge
            .reduce(crate::state::ConciergeAction::WelcomeReceived {
                content: "Welcome".to_string(),
                actions: vec![crate::state::ConciergeActionVm {
                    label: "Dismiss".to_string(),
                    action_type: "dismiss".to_string(),
                    thread_id: None,
                }],
            });

        model.cleanup_concierge_on_navigate();

        assert!(!model.concierge.welcome_visible);
        assert!(model.ignore_pending_concierge_welcome);
        assert!(
            model.chat.active_actions().is_empty(),
            "dismissed concierge welcome should not leave actionable buttons behind"
        );
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            other => panic!("expected dismiss command, got {:?}", other),
        }
    }

    #[test]
    fn submit_prompt_dismisses_concierge_and_avoids_session_binding() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.connected = true;
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "concierge".to_string(),
            title: "Concierge".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
        model.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id: "concierge".to_string(),
            message: chat::AgentMessage {
                role: chat::MessageRole::Assistant,
                content: "Welcome".to_string(),
                actions: vec![chat::MessageAction {
                    label: "Dismiss".to_string(),
                    action_type: "dismiss".to_string(),
                    thread_id: None,
                }],
                is_concierge_welcome: true,
                ..Default::default()
            },
        });
        model
            .concierge
            .reduce(crate::state::ConciergeAction::WelcomeReceived {
                content: "Welcome".to_string(),
                actions: vec![crate::state::ConciergeActionVm {
                    label: "Dismiss".to_string(),
                    action_type: "dismiss".to_string(),
                    thread_id: None,
                }],
            });
        model.default_session_id = Some("stale-session".to_string());

        model.submit_prompt("hello".to_string());

        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            other => panic!("expected dismiss command first, got {:?}", other),
        }
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::SendMessage {
                thread_id,
                content,
                session_id,
            }) => {
                assert_eq!(thread_id.as_deref(), Some("concierge"));
                assert_eq!(content, "hello");
                assert_eq!(session_id, None);
            }
            other => panic!("expected send-message command, got {:?}", other),
        }
        assert!(
            model.chat.active_actions().is_empty(),
            "submitting a prompt should hide concierge welcome actions"
        );
    }

    #[test]
    fn start_goal_run_dismisses_concierge_and_avoids_session_binding() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.connected = true;
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "concierge".to_string(),
            title: "Concierge".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
        model.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id: "concierge".to_string(),
            message: chat::AgentMessage {
                role: chat::MessageRole::Assistant,
                content: "Welcome".to_string(),
                actions: vec![chat::MessageAction {
                    label: "Goal".to_string(),
                    action_type: "start_goal_run".to_string(),
                    thread_id: None,
                }],
                is_concierge_welcome: true,
                ..Default::default()
            },
        });
        model
            .concierge
            .reduce(crate::state::ConciergeAction::WelcomeReceived {
                content: "Welcome".to_string(),
                actions: vec![crate::state::ConciergeActionVm {
                    label: "Goal".to_string(),
                    action_type: "start_goal_run".to_string(),
                    thread_id: None,
                }],
            });
        model.default_session_id = Some("stale-session".to_string());

        model.start_goal_run_from_prompt("ship it".to_string());

        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            other => panic!("expected dismiss command first, got {:?}", other),
        }
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::StartGoalRun {
                goal,
                thread_id,
                session_id,
            }) => {
                assert_eq!(goal, "ship it");
                assert_eq!(thread_id, None);
                assert_eq!(session_id, None);
            }
            other => panic!("expected start-goal-run command, got {:?}", other),
        }
    }

    #[test]
    fn start_new_thread_shows_local_landing_and_does_not_request_concierge() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.concierge.loading = false;
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "concierge".to_string(),
            title: "Concierge".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
        model
            .concierge
            .reduce(crate::state::ConciergeAction::WelcomeReceived {
                content: "Welcome".to_string(),
                actions: vec![crate::state::ConciergeActionVm {
                    label: "Search".to_string(),
                    action_type: "search".to_string(),
                    thread_id: None,
                }],
            });

        model.start_new_thread_view();

        assert!(model.should_show_local_landing());
        assert_eq!(model.chat.active_thread_id(), None);
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            other => panic!("expected dismiss command first, got {:?}", other),
        }
        assert!(
            cmd_rx.try_recv().is_err(),
            "unexpected daemon command after /new"
        );
    }

    #[test]
    fn concierge_arrow_keys_navigate_visible_actions() {
        let mut model = build_model();
        model.focus = FocusArea::Chat;
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "concierge".to_string(),
            title: "Concierge".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
        model.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id: "concierge".to_string(),
            message: chat::AgentMessage {
                role: chat::MessageRole::Assistant,
                content: "Welcome".to_string(),
                actions: vec![
                    chat::MessageAction {
                        label: "One".to_string(),
                        action_type: "dismiss".to_string(),
                        thread_id: None,
                    },
                    chat::MessageAction {
                        label: "Two".to_string(),
                        action_type: "dismiss".to_string(),
                        thread_id: None,
                    },
                ],
                is_concierge_welcome: true,
                ..Default::default()
            },
        });

        let handled = model.handle_key(KeyCode::Right, KeyModifiers::NONE);

        assert!(!handled);
        assert_eq!(model.concierge.selected_action, 1);
    }

    #[test]
    fn concierge_mouse_click_executes_visible_action() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.focus = FocusArea::Chat;
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "concierge".to_string(),
            title: "Concierge".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
        model.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id: "concierge".to_string(),
            message: chat::AgentMessage {
                role: chat::MessageRole::Assistant,
                content: "Welcome".to_string(),
                actions: vec![
                    chat::MessageAction {
                        label: "One".to_string(),
                        action_type: "dismiss".to_string(),
                        thread_id: None,
                    },
                    chat::MessageAction {
                        label: "Two".to_string(),
                        action_type: "start_goal_run".to_string(),
                        thread_id: None,
                    },
                ],
                is_concierge_welcome: true,
                ..Default::default()
            },
        });

        model.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 8,
            row: 35,
            modifiers: KeyModifiers::NONE,
        });

        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            other => panic!("expected dismiss command first, got {:?}", other),
        }
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::RequestThread(thread_id)) => {
                assert_eq!(thread_id, "concierge");
            }
            other => panic!("expected thread request command, got {:?}", other),
        }
        assert_eq!(model.focus, FocusArea::Input);
        assert_eq!(model.input.buffer(), "/goal ");
        assert!(model.chat.active_actions().is_empty());
    }

    #[test]
    fn dismissing_concierge_welcome_returns_to_local_landing() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.focus = FocusArea::Chat;
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "concierge".to_string(),
            title: "Concierge".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
        model.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id: "concierge".to_string(),
            message: chat::AgentMessage {
                role: chat::MessageRole::Assistant,
                content: "Welcome".to_string(),
                actions: vec![chat::MessageAction {
                    label: "Dismiss".to_string(),
                    action_type: "dismiss".to_string(),
                    thread_id: None,
                }],
                is_concierge_welcome: true,
                ..Default::default()
            },
        });
        model
            .concierge
            .reduce(crate::state::ConciergeAction::WelcomeReceived {
                content: "Welcome".to_string(),
                actions: vec![crate::state::ConciergeActionVm {
                    label: "Dismiss".to_string(),
                    action_type: "dismiss".to_string(),
                    thread_id: None,
                }],
            });

        model.run_concierge_action(crate::state::ConciergeActionVm {
            label: "Dismiss".to_string(),
            action_type: "dismiss".to_string(),
            thread_id: None,
        });

        assert_eq!(model.chat.active_thread_id(), None);
        assert!(model.should_show_local_landing());
        assert_eq!(model.focus, FocusArea::Input);
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            other => panic!("expected dismiss command, got {:?}", other),
        }
        assert!(cmd_rx.try_recv().is_err(), "unexpected follow-up command");
    }

    #[test]
    fn drag_selection_keeps_original_anchor_point_when_chat_scrolls() {
        let mut model = build_model();
        model.show_sidebar_override = Some(false);
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "thread-1".to_string(),
            title: "Thread".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
        model.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id: "thread-1".to_string(),
            message: chat::AgentMessage {
                role: chat::MessageRole::User,
                content: (1..=80)
                    .map(|idx| format!("line {idx}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                ..Default::default()
            },
        });

        let input_start_row = model.height.saturating_sub(model.input_height() + 1);
        let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
        let preferred_row = chat_area.y.saturating_add(chat_area.height / 2);
        let start_row = (preferred_row..chat_area.y.saturating_add(chat_area.height))
            .chain(chat_area.y..preferred_row)
            .find(|row| {
                widgets::chat::selection_point_from_mouse(
                    chat_area,
                    &model.chat,
                    &model.theme,
                    model.tick_counter,
                    Position::new(3, *row),
                )
                .is_some()
            })
            .expect("chat transcript should expose at least one selectable row");

        model.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: start_row,
            modifiers: KeyModifiers::NONE,
        });
        let anchor_point = model
            .chat_drag_anchor_point
            .expect("mouse down should capture a document anchor point");

        for _ in 0..4 {
            model.handle_mouse(MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: 3,
                row: start_row,
                modifiers: KeyModifiers::NONE,
            });
        }

        let current_point = model
            .chat_drag_current_point
            .expect("dragging should keep updating the current document point");
        assert_eq!(
            model.chat_drag_anchor_point,
            Some(anchor_point),
            "autoscroll should not rewrite the original selection anchor"
        );
        assert!(
            current_point.row < anchor_point.row,
            "dragging upward with autoscroll should extend the selection into older transcript rows: anchor={anchor_point:?} current={current_point:?}"
        );
    }

    #[test]
    fn esc_closes_work_context_even_from_input_focus() {
        let mut model = build_model();
        model.focus = FocusArea::Input;
        model.main_pane_view = MainPaneView::WorkContext;

        let handled = model.handle_key(KeyCode::Esc, KeyModifiers::NONE);

        assert!(!handled);
        assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
        assert_eq!(model.focus, FocusArea::Chat);
    }

    #[test]
    fn reselecting_same_sidebar_file_closes_work_context() {
        let mut model = build_model();
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "thread-1".to_string(),
            title: "Thread".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
        model.tasks.reduce(task::TaskAction::WorkContextReceived(
            task::ThreadWorkContext {
                thread_id: "thread-1".to_string(),
                entries: vec![task::WorkContextEntry {
                    path: "/tmp/demo.txt".to_string(),
                    is_text: true,
                    ..Default::default()
                }],
            },
        ));
        model.tasks.reduce(task::TaskAction::SelectWorkPath {
            thread_id: "thread-1".to_string(),
            path: Some("/tmp/demo.txt".to_string()),
        });
        model
            .sidebar
            .reduce(SidebarAction::SwitchTab(SidebarTab::Files));
        model.main_pane_view = MainPaneView::WorkContext;
        model.focus = FocusArea::Sidebar;

        model.handle_sidebar_enter();

        assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
        assert_eq!(model.focus, FocusArea::Sidebar);
    }

    #[test]
    fn reselecting_same_sidebar_todo_closes_work_context() {
        let mut model = build_model();
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "thread-1".to_string(),
            title: "Thread".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
        model.tasks.reduce(task::TaskAction::ThreadTodosReceived {
            thread_id: "thread-1".to_string(),
            items: vec![task::TodoItem {
                id: "todo-1".to_string(),
                content: "todo".to_string(),
                status: Some(task::TodoStatus::Pending),
                position: 0,
                step_index: None,
                created_at: 0,
                updated_at: 0,
            }],
        });
        model
            .sidebar
            .reduce(SidebarAction::SwitchTab(SidebarTab::Todos));
        model.main_pane_view = MainPaneView::WorkContext;
        model.focus = FocusArea::Sidebar;

        model.handle_sidebar_enter();

        assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
        assert_eq!(model.focus, FocusArea::Sidebar);
    }

    #[test]
    fn attention_surface_uses_settings_tab_when_modal_open() {
        let mut model = build_model();
        model
            .modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
        model
            .settings
            .reduce(SettingsAction::SwitchTab(SettingsTab::SubAgents));

        let (surface, thread_id, goal_run_id) = model.current_attention_target();
        assert_eq!(surface, "modal:settings:subagents");
        assert_eq!(thread_id, None);
        assert_eq!(goal_run_id, None);
    }

    #[test]
    fn attention_surface_uses_sidebar_tab_for_sidebar_focus() {
        let mut model = build_model();
        model.connected = true;
        model.auth.loaded = true;
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "thread_1".to_string(),
            title: "Test".to_string(),
        });
        model.tasks.reduce(task::TaskAction::ThreadTodosReceived {
            thread_id: "thread_1".to_string(),
            items: vec![task::TodoItem {
                id: "todo_1".to_string(),
                content: "todo".to_string(),
                status: Some(task::TodoStatus::Pending),
                position: 0,
                step_index: None,
                created_at: 0,
                updated_at: 0,
            }],
        });
        model.focus = FocusArea::Sidebar;
        model
            .sidebar
            .reduce(SidebarAction::SwitchTab(SidebarTab::Todos));

        let (surface, thread_id, goal_run_id) = model.current_attention_target();
        assert_eq!(surface, "conversation:sidebar:todos");
        assert_eq!(thread_id.as_deref(), Some("thread_1"));
        assert_eq!(goal_run_id, None);
    }
}
