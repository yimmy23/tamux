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
const SPAWNED_SIDEBAR_TASK_REFRESH_TICKS: u64 = 20;

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
    Workspace,
    Task(sidebar::SidebarItemTarget),
    WorkContext,
    FilePreview(ChatFilePreviewTarget),
    GoalComposer,
}

#[derive(Clone, Debug, Default)]
struct MissionControlNavigationState {
    source_goal_target: Option<sidebar::SidebarItemTarget>,
    return_to_goal_target: Option<sidebar::SidebarItemTarget>,
    return_to_thread_id: Option<String>,
    return_to_workspace: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum GoalSidebarSelectionAnchor {
    Step(String),
    Checkpoint(String),
    Task(String),
    File { thread_id: String, path: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AutoRefreshTarget {
    Goal(String),
    Workspace(String),
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
    EmbeddingProvider,
    EmbeddingModel,
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
    TargetAgentProvider,
    TargetAgentModel,
    TargetAgentReasoningEffort,
    ConciergeProvider,
    ConciergeModel,
    ConciergeReasoningEffort,
    CompactionWelesReasoningEffort,
    CompactionCustomReasoningEffort,
    OpenRouterPreferredProviders,
    OpenRouterExcludedProviders,
    SubAgentOpenRouterPreferredProviders,
    SubAgentOpenRouterExcludedProviders,
    ConciergeOpenRouterPreferredProviders,
    ConciergeOpenRouterExcludedProviders,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputNoticeKind {
    Warning,
    Success,
    Error,
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
    RetryGoalPrompt {
        goal_run_id: String,
        goal_title: String,
    },
    RerunGoalFromStep {
        goal_run_id: String,
        goal_title: String,
        step_index: usize,
        step_title: String,
    },
    RerunGoalPrompt {
        goal_run_id: String,
        goal_title: String,
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
            PendingConfirmAction::RetryGoalPrompt { goal_title, .. } => {
                format!("Retry goal \"{goal_title}\" from the current prompt?")
            }
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
            PendingConfirmAction::RerunGoalPrompt { goal_title, .. } => {
                format!("Rerun goal \"{goal_title}\" from the current prompt?")
            }
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
    continuation: PendingBuiltinPersonaSetupContinuation,
    config_snapshot: BuiltinPersonaSetupConfigSnapshot,
}

#[derive(Clone, Debug)]
struct PendingTargetAgentConfig {
    target_agent_id: String,
    target_agent_name: String,
    provider_id: String,
    model: String,
    reasoning_effort: Option<String>,
}

#[derive(Clone, Debug)]
enum PendingBuiltinPersonaSetupContinuation {
    SubmitPrompt(String),
    SelectWorkspaceActor {
        pending: PendingWorkspaceActorPicker,
        actor: zorai_protocol::WorkspaceActor,
    },
}

#[derive(Clone, Debug)]
struct ParticipantPlaygroundActivity {
    visible_thread_id: String,
    participant_agent_id: String,
    participant_agent_name: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingPinnedBudgetExceeded {
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

    fn clear_expired_copy_feedback(&mut self, current_tick: u64) -> bool {
        if self
            .copied_until_tick
            .is_some_and(|expires_at| current_tick >= expires_at)
        {
            self.copied_until_tick = None;
            return true;
        }
        false
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
    bool_answer: Option<bool>,
    deferred_session_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingReconnectRestore {
    thread_id: String,
    should_resume: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GoalApprovalContext {
    approval_id: String,
    goal_run_id: String,
    thread_id: Option<String>,
    goal_title: String,
    step_title: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PendingWorkspaceActorPickerTarget {
    Task { task_id: String },
    CreateForm,
    EditForm,
}
