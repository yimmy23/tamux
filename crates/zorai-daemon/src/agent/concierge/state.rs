use super::*;

pub(super) struct WelcomeContext {
    pub(super) recent_threads: Vec<ThreadSummary>,
    pub(super) latest_goal_run: Option<GoalRunSummary>,
    pub(super) running_goal_total: usize,
    pub(super) paused_goal_total: usize,
}

pub(super) const WELCOME_CACHE_TTL: std::time::Duration =
    std::time::Duration::from_secs(2 * 60 * 60);

#[derive(Clone)]
pub(super) struct WelcomeCacheEntry {
    pub(super) signature: String,
    pub(super) content: String,
    pub(super) actions: Vec<ConciergeAction>,
    pub(super) created_at: std::time::Instant,
}

#[derive(Clone)]
pub(crate) struct ThreadSummary {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) updated_at: u64,
    pub(super) message_count: usize,
    pub(super) opening_message: Option<String>,
    pub(super) last_messages: Vec<String>,
}

#[derive(Clone)]
pub(super) struct GoalRunSummary {
    pub(super) label: String,
    pub(super) prompt: Option<String>,
    pub(super) status: GoalRunStatus,
    pub(super) updated_at: u64,
    pub(super) summary: Option<String>,
    pub(super) latest_step_result: Option<String>,
}
