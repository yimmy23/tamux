use super::StreamProgressKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum StalledTurnClass {
    PromiseWithoutAction,
    PostToolResultNoFollowThrough,
    ActiveStreamIdle,
    ToolCallLoop,
    NoProgress,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TurnEvidence {
    pub(super) last_assistant_message: String,
    pub(super) preceded_by_tool_result: bool,
    pub(super) new_tool_call_followed: bool,
    pub(super) new_substantive_assistant_message_followed: bool,
    pub(super) task_or_goal_progressed: bool,
    pub(super) user_replied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ThreadStallObservation {
    pub(crate) thread_id: String,
    pub(crate) last_message_id: String,
    pub(crate) last_message_at: u64,
    pub(crate) last_assistant_message: String,
    pub(crate) class: StalledTurnClass,
    pub(crate) stream_progress_kind: Option<StreamProgressKind>,
    pub(crate) task_id: Option<String>,
    pub(crate) goal_run_id: Option<String>,
}
