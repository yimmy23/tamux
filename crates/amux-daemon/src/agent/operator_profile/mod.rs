//! Operator profile domain module.
//!
//! Provides the profile field model, the deterministic interview planner, and
//! the in-memory session registry. All components are pure data / logic and
//! contain no async or I/O — persistence is delegated to `HistoryStore`.

pub mod checkins;
pub mod interview;
pub mod model;
pub mod store;
pub(super) mod user_sync;

pub use checkins::{
    build_scheduled_checkin, evaluate_passive_checkin_policy, is_in_critical_goal_execution_window,
    parse_scheduled_metadata, passive_learning_allowed, proactive_suggestions_allowed,
    weekly_checkins_allowed, CheckinKind, ConsentSnapshot, ContextualTrigger,
    PassiveCheckinDecision, PassiveCheckinInput, PassiveSignalKind, ScheduledCheckinMetadata,
};
pub use interview::{is_complete, next_question, progress, InterviewSession};
pub use model::{InputKind, OperatorProfileSnapshot, ProfileFieldSpec, ProfileFieldValue};
pub use store::{default_field_specs, InterviewStore};
