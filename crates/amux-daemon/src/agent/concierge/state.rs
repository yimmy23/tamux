use super::*;

pub(super) struct WelcomeContext {
    pub(super) recent_threads: Vec<ThreadSummary>,
    pub(super) pending_task_total: usize,
    pub(super) pending_tasks: Vec<String>,
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

pub(super) struct ThreadSummary {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) updated_at: u64,
    pub(super) message_count: usize,
    pub(super) opening_message: Option<String>,
    pub(super) last_messages: Vec<String>,
}
