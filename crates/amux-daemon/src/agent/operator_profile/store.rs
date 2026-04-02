//! In-memory registry of active operator profile interview sessions.
//!
//! Each session is short-lived and keyed by session ID. Completed or abandoned
//! sessions should be removed via [`InterviewStore::remove_session`] to keep
//! the registry bounded.

use std::collections::HashMap;

use super::interview::InterviewSession;
use super::model::ProfileFieldSpec;

// ---------------------------------------------------------------------------
// Session registry
// ---------------------------------------------------------------------------

/// Bounded in-memory map of active interview sessions.
///
/// This is the runtime state stored on `AgentEngine`. It is **not** a
/// persistence layer — history is written through `HistoryStore` by the
/// message handler (next task).
#[derive(Default)]
pub struct InterviewStore {
    sessions: HashMap<String, InterviewSession>,
}

impl InterviewStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new session and insert it into the registry.
    ///
    /// If a session with the same ID already exists it is replaced.
    pub fn create_session(&mut self, session_id: String, kind: &str) -> &InterviewSession {
        self.sessions
            .insert(session_id.clone(), InterviewSession::new(&session_id, kind));
        self.sessions.get(&session_id).expect("just inserted")
    }

    /// Look up a session by ID.
    pub fn get_session(&self, session_id: &str) -> Option<&InterviewSession> {
        self.sessions.get(session_id)
    }

    /// Look up a session mutably (for skip/defer operations).
    pub fn get_session_mut(&mut self, session_id: &str) -> Option<&mut InterviewSession> {
        self.sessions.get_mut(session_id)
    }

    /// Remove and return a session. Returns `None` when not found.
    pub fn remove_session(&mut self, session_id: &str) -> Option<InterviewSession> {
        self.sessions.remove(session_id)
    }

    /// Number of currently active sessions.
    pub fn active_count(&self) -> usize {
        self.sessions.len()
    }

    /// Number of active sessions that match a specific kind.
    pub fn active_count_for_kind(&self, kind: &str) -> usize {
        self.sessions
            .values()
            .filter(|session| session.kind == kind)
            .count()
    }
}

// ---------------------------------------------------------------------------
// Default field specifications
// ---------------------------------------------------------------------------

/// Canonical ordered list of profile questions used for every interview.
///
/// Required fields appear first and are presented before optional ones by
/// the planner regardless of their position here. Ordering within each tier
/// follows the order of this list.
pub fn default_field_specs() -> Vec<ProfileFieldSpec> {
    vec![
        ProfileFieldSpec {
            field_key: "name".to_string(),
            prompt: "What should I call you?".to_string(),
            input_kind: "text".to_string(),
            required: true,
        },
        ProfileFieldSpec {
            field_key: "role".to_string(),
            prompt: "What best describes your role? (e.g. backend engineer, data scientist, DevOps)".to_string(),
            input_kind: "text".to_string(),
            required: true,
        },
        ProfileFieldSpec {
            field_key: "primary_language".to_string(),
            prompt: "What is your primary programming language?".to_string(),
            input_kind: "text".to_string(),
            required: true,
        },
        ProfileFieldSpec {
            field_key: "preferred_editor".to_string(),
            prompt: "Which editor or IDE do you use most?".to_string(),
            input_kind: "text".to_string(),
            required: false,
        },
        ProfileFieldSpec {
            field_key: "os".to_string(),
            prompt: "Which operating system do you work on primarily?".to_string(),
            input_kind: "text".to_string(),
            required: false,
        },
        ProfileFieldSpec {
            field_key: "work_style".to_string(),
            prompt: "How would you describe your work style? (e.g. focused, exploratory, collaborative)".to_string(),
            input_kind: "text".to_string(),
            required: false,
        },
        ProfileFieldSpec {
            field_key: "notification_preference".to_string(),
            prompt: "How much should I interrupt you with suggestions and check-ins? (minimal / balanced / proactive)".to_string(),
            input_kind: "select".to_string(),
            required: false,
        },
    ]
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_get_session() {
        let mut store = InterviewStore::new();
        store.create_session("sess-1".to_string(), "onboarding");
        let session = store.get_session("sess-1");
        assert!(session.is_some());
        assert_eq!(session.unwrap().session_id, "sess-1");
        assert_eq!(session.unwrap().kind, "onboarding");
    }

    #[test]
    fn get_nonexistent_session_returns_none() {
        let store = InterviewStore::new();
        assert!(store.get_session("missing").is_none());
    }

    #[test]
    fn create_session_replaces_existing() {
        let mut store = InterviewStore::new();
        store.create_session("sess-1".to_string(), "onboarding");
        store.create_session("sess-1".to_string(), "refresh");
        let session = store.get_session("sess-1").unwrap();
        assert_eq!(session.kind, "refresh");
    }

    #[test]
    fn remove_session_returns_it_and_decrements_count() {
        let mut store = InterviewStore::new();
        store.create_session("sess-1".to_string(), "onboarding");
        assert_eq!(store.active_count(), 1);
        let removed = store.remove_session("sess-1");
        assert!(removed.is_some());
        assert_eq!(store.active_count(), 0);
    }

    #[test]
    fn remove_nonexistent_returns_none() {
        let mut store = InterviewStore::new();
        assert!(store.remove_session("nope").is_none());
    }

    #[test]
    fn get_session_mut_allows_skip() {
        let mut store = InterviewStore::new();
        store.create_session("sess-1".to_string(), "onboarding");
        {
            let sess = store.get_session_mut("sess-1").unwrap();
            sess.mark_skipped("some_field");
        }
        let sess = store.get_session("sess-1").unwrap();
        assert!(sess.skipped_fields().contains("some_field"));
    }

    #[test]
    fn default_field_specs_has_required_and_optional() {
        let specs = default_field_specs();
        let required: Vec<_> = specs.iter().filter(|s| s.required).collect();
        let optional: Vec<_> = specs.iter().filter(|s| !s.required).collect();
        assert!(!required.is_empty(), "should have required fields");
        assert!(!optional.is_empty(), "should have optional fields");
    }

    #[test]
    fn default_field_specs_keys_are_unique() {
        let specs = default_field_specs();
        let keys: std::collections::HashSet<_> = specs.iter().map(|s| &s.field_key).collect();
        assert_eq!(keys.len(), specs.len(), "field keys must be unique");
    }
}
