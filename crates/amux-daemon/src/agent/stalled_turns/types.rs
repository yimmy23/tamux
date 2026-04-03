#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StalledTurnClass {
    PromiseWithoutAction,
    PostToolResultNoFollowThrough,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ThreadStallObservation {
    pub(super) thread_id: String,
    pub(super) last_message_id: String,
    pub(super) last_message_at: u64,
    pub(super) last_assistant_message: String,
    pub(super) class: StalledTurnClass,
    pub(super) task_id: Option<String>,
    pub(super) goal_run_id: Option<String>,
}
