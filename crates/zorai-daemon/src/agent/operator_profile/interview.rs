//! Deterministic interview planner for the operator profile questionnaire.
//!
//! All logic is pure: no async, no I/O, fully unit-testable. The planner
//! selects exactly one question per call and prioritises required fields over
//! optional ones.

use std::collections::{HashMap, HashSet};

use super::model::{ProfileFieldSpec, ProfileFieldValue};

// ---------------------------------------------------------------------------
// Session state
// ---------------------------------------------------------------------------

/// In-memory state for a single operator profile interview session.
///
/// Tracks which questions have been skipped or deferred so that the planner
/// can exclude them from selection without mutating the global field specs.
#[derive(Debug)]
pub struct InterviewSession {
    pub session_id: String,
    pub kind: String,
    /// Fields the operator explicitly skipped this session.
    skipped: HashSet<String>,
    /// Fields deferred: `None` means indefinite; `Some(ms)` means retry after that timestamp.
    deferred: HashMap<String, Option<u64>>,
}

impl InterviewSession {
    pub fn new(session_id: &str, kind: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            kind: kind.to_string(),
            skipped: HashSet::new(),
            deferred: HashMap::new(),
        }
    }

    /// Mark a field as skipped for the remainder of this session.
    pub fn mark_skipped(&mut self, field_key: &str) {
        self.skipped.insert(field_key.to_string());
    }

    /// Defer a field until `defer_until_ms` (or indefinitely when `None`).
    pub fn defer(&mut self, field_key: &str, defer_until_ms: Option<u64>) {
        self.deferred.insert(field_key.to_string(), defer_until_ms);
    }

    /// Whether a field is currently excluded from selection.
    pub fn is_excluded(&self, field_key: &str, now_ms: u64) -> bool {
        if self.skipped.contains(field_key) {
            return true;
        }
        if let Some(until_opt) = self.deferred.get(field_key) {
            match until_opt {
                None => return true,
                Some(until) => {
                    if now_ms < *until {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// List fields currently deferred (including indefinite deferrals).
    pub fn deferred_fields(&self) -> &HashMap<String, Option<u64>> {
        &self.deferred
    }

    /// List fields skipped this session.
    pub fn skipped_fields(&self) -> &HashSet<String> {
        &self.skipped
    }
}

// ---------------------------------------------------------------------------
// Question selector
// ---------------------------------------------------------------------------

/// Return the next question to present, or `None` when all specs have been
/// answered or excluded.
///
/// Selection order:
/// 1. Required fields that are unanswered and not excluded (in spec order).
/// 2. Optional fields that are unanswered and not excluded (in spec order).
///
/// Exactly one spec is returned per call — one-question-at-a-time semantics.
pub fn next_question<'a>(
    specs: &'a [ProfileFieldSpec],
    answered: &HashMap<String, ProfileFieldValue>,
    session: &InterviewSession,
    now_ms: u64,
) -> Option<&'a ProfileFieldSpec> {
    // Pass 1: required fields only.
    for spec in specs.iter().filter(|s| s.required) {
        if !answered.contains_key(&spec.field_key) && !session.is_excluded(&spec.field_key, now_ms)
        {
            return Some(spec);
        }
    }
    // Pass 2: optional fields.
    for spec in specs.iter().filter(|s| !s.required) {
        if !answered.contains_key(&spec.field_key) && !session.is_excluded(&spec.field_key, now_ms)
        {
            return Some(spec);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Completion check
// ---------------------------------------------------------------------------

/// Returns `true` when every required field has been answered.
///
/// Optional fields and skipped/deferred items do **not** block completion.
pub fn is_complete(
    specs: &[ProfileFieldSpec],
    answered: &HashMap<String, ProfileFieldValue>,
) -> bool {
    specs
        .iter()
        .filter(|s| s.required)
        .all(|s| answered.contains_key(&s.field_key))
}

// ---------------------------------------------------------------------------
// Progress
// ---------------------------------------------------------------------------

/// Returns `(answered_count, remaining_count, completion_ratio)`.
///
/// * `answered_count` — specs with a value in `answered`.
/// * `remaining_count` — specs not yet answered and not currently excluded.
/// * `completion_ratio` — `answered / total` clamped to `[0.0, 1.0]`.
pub fn progress(
    specs: &[ProfileFieldSpec],
    answered: &HashMap<String, ProfileFieldValue>,
    session: &InterviewSession,
    now_ms: u64,
) -> (u32, u32, f64) {
    let answered_count = specs
        .iter()
        .filter(|s| answered.contains_key(&s.field_key))
        .count() as u32;

    let remaining_count = specs
        .iter()
        .filter(|s| {
            !answered.contains_key(&s.field_key) && !session.is_excluded(&s.field_key, now_ms)
        })
        .count() as u32;

    let total = specs.len() as f64;
    let ratio = if total == 0.0 {
        1.0
    } else {
        (answered_count as f64 / total).clamp(0.0, 1.0)
    };

    (answered_count, remaining_count, ratio)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_spec(key: &str, required: bool) -> ProfileFieldSpec {
        ProfileFieldSpec {
            field_key: key.to_string(),
            prompt: format!("Question for {key}"),
            input_kind: "text".to_string(),
            required,
        }
    }

    fn make_value(v: &str) -> ProfileFieldValue {
        ProfileFieldValue {
            value_json: v.to_string(),
            confidence: 1.0,
            source: "test".to_string(),
            updated_at: 0,
        }
    }

    // ── Question selection ───────────────────────────────────────────────

    #[test]
    fn first_run_asks_required_field_before_optional() {
        let specs = vec![make_spec("opt_field", false), make_spec("req_field", true)];
        let answered = HashMap::new();
        let session = InterviewSession::new("s1", "onboarding");

        let next = next_question(&specs, &answered, &session, 0);
        assert_eq!(next.unwrap().field_key, "req_field");
    }

    #[test]
    fn one_question_at_a_time_returns_single_spec() {
        let specs = vec![make_spec("field_a", true), make_spec("field_b", true)];
        let answered = HashMap::new();
        let session = InterviewSession::new("s1", "onboarding");

        // Should only return the first spec, not a list.
        let next = next_question(&specs, &answered, &session, 0);
        assert_eq!(next.unwrap().field_key, "field_a");
    }

    #[test]
    fn advances_to_next_required_after_answer() {
        let specs = vec![make_spec("field_a", true), make_spec("field_b", true)];
        let mut answered = HashMap::new();
        answered.insert("field_a".to_string(), make_value("\"a\""));
        let session = InterviewSession::new("s1", "onboarding");

        let next = next_question(&specs, &answered, &session, 0);
        assert_eq!(next.unwrap().field_key, "field_b");
    }

    #[test]
    fn falls_through_to_optional_when_required_exhausted() {
        let specs = vec![make_spec("req", true), make_spec("opt", false)];
        let mut answered = HashMap::new();
        answered.insert("req".to_string(), make_value("\"done\""));
        let session = InterviewSession::new("s1", "onboarding");

        let next = next_question(&specs, &answered, &session, 0);
        assert_eq!(next.unwrap().field_key, "opt");
    }

    #[test]
    fn returns_none_when_all_specs_answered() {
        let specs = vec![make_spec("field_a", true)];
        let mut answered = HashMap::new();
        answered.insert("field_a".to_string(), make_value("\"val\""));
        let session = InterviewSession::new("s1", "onboarding");

        let next = next_question(&specs, &answered, &session, 0);
        assert!(next.is_none());
    }

    #[test]
    fn returns_none_for_empty_specs() {
        let specs: Vec<ProfileFieldSpec> = vec![];
        let answered = HashMap::new();
        let session = InterviewSession::new("s1", "onboarding");

        assert!(next_question(&specs, &answered, &session, 0).is_none());
    }

    // ── Skip behaviour ───────────────────────────────────────────────────

    #[test]
    fn skip_excludes_field_from_selection() {
        let specs = vec![make_spec("field_a", true), make_spec("field_b", true)];
        let answered = HashMap::new();
        let mut session = InterviewSession::new("s1", "onboarding");
        session.mark_skipped("field_a");

        let next = next_question(&specs, &answered, &session, 0);
        assert_eq!(next.unwrap().field_key, "field_b");
    }

    #[test]
    fn skip_all_required_surfaces_optional() {
        let specs = vec![make_spec("req", true), make_spec("opt", false)];
        let answered = HashMap::new();
        let mut session = InterviewSession::new("s1", "onboarding");
        session.mark_skipped("req");

        let next = next_question(&specs, &answered, &session, 0);
        assert_eq!(next.unwrap().field_key, "opt");
    }

    // ── Defer behaviour ──────────────────────────────────────────────────

    #[test]
    fn defer_excludes_field_before_expiry() {
        let specs = vec![make_spec("field_a", true), make_spec("field_b", true)];
        let answered = HashMap::new();
        let mut session = InterviewSession::new("s1", "onboarding");
        session.defer("field_a", Some(1000));

        let next = next_question(&specs, &answered, &session, 500);
        assert_eq!(next.unwrap().field_key, "field_b");
    }

    #[test]
    fn defer_re_includes_field_after_expiry() {
        let specs = vec![make_spec("field_a", true), make_spec("field_b", true)];
        let answered = HashMap::new();
        let mut session = InterviewSession::new("s1", "onboarding");
        session.defer("field_a", Some(1000));

        // At exact expiry timestamp field_a is still deferred (now_ms < until → excluded when now_ms < 1000).
        // now_ms == 1000: 1000 < 1000 is false → field_a eligible.
        let next = next_question(&specs, &answered, &session, 1000);
        assert_eq!(next.unwrap().field_key, "field_a");
    }

    #[test]
    fn indefinite_defer_always_excludes_regardless_of_time() {
        let specs = vec![make_spec("field_a", true), make_spec("field_b", true)];
        let answered = HashMap::new();
        let mut session = InterviewSession::new("s1", "onboarding");
        session.defer("field_a", None);

        let next = next_question(&specs, &answered, &session, u64::MAX);
        assert_eq!(next.unwrap().field_key, "field_b");
    }

    #[test]
    fn indefinite_defer_returns_none_when_only_field() {
        let specs = vec![make_spec("field_a", true)];
        let answered = HashMap::new();
        let mut session = InterviewSession::new("s1", "onboarding");
        session.defer("field_a", None);

        let next = next_question(&specs, &answered, &session, u64::MAX);
        assert!(next.is_none());
    }

    // ── Completion threshold ─────────────────────────────────────────────

    #[test]
    fn is_complete_true_when_all_required_answered() {
        let specs = vec![
            make_spec("req_a", true),
            make_spec("req_b", true),
            make_spec("opt_c", false),
        ];
        let mut answered = HashMap::new();
        answered.insert("req_a".to_string(), make_value("\"a\""));
        answered.insert("req_b".to_string(), make_value("\"b\""));

        assert!(is_complete(&specs, &answered));
    }

    #[test]
    fn is_complete_false_when_required_missing() {
        let specs = vec![make_spec("req_a", true), make_spec("req_b", true)];
        let mut answered = HashMap::new();
        answered.insert("req_a".to_string(), make_value("\"a\""));

        assert!(!is_complete(&specs, &answered));
    }

    #[test]
    fn is_complete_true_with_no_required_specs() {
        let specs = vec![make_spec("opt", false)];
        let answered = HashMap::new();

        assert!(is_complete(&specs, &answered));
    }

    #[test]
    fn is_complete_true_for_empty_specs() {
        let specs: Vec<ProfileFieldSpec> = vec![];
        let answered = HashMap::new();
        assert!(is_complete(&specs, &answered));
    }

    #[test]
    fn completion_does_not_require_optional_fields() {
        let specs = vec![make_spec("req", true), make_spec("opt", false)];
        let mut answered = HashMap::new();
        answered.insert("req".to_string(), make_value("\"done\""));

        // Complete even though optional field is not answered.
        assert!(is_complete(&specs, &answered));
        // next_question still returns the optional field.
        let session = InterviewSession::new("s1", "onboarding");
        let next = next_question(&specs, &answered, &session, 0);
        assert_eq!(next.unwrap().field_key, "opt");
    }

    // ── Progress counters ────────────────────────────────────────────────

    #[test]
    fn progress_returns_correct_counts() {
        let specs = vec![
            make_spec("a", true),
            make_spec("b", true),
            make_spec("c", false),
        ];
        let mut answered = HashMap::new();
        answered.insert("a".to_string(), make_value("\"v\""));
        let session = InterviewSession::new("s1", "onboarding");

        let (ans, rem, ratio) = progress(&specs, &answered, &session, 0);
        assert_eq!(ans, 1);
        assert_eq!(rem, 2);
        assert!((ratio - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn progress_excluded_fields_not_counted_as_remaining() {
        let specs = vec![make_spec("a", true), make_spec("b", true)];
        let answered = HashMap::new();
        let mut session = InterviewSession::new("s1", "onboarding");
        session.mark_skipped("b");

        let (ans, rem, _) = progress(&specs, &answered, &session, 0);
        assert_eq!(ans, 0);
        assert_eq!(rem, 1); // only "a" is remaining
    }

    #[test]
    fn progress_ratio_one_when_all_answered() {
        let specs = vec![make_spec("a", true)];
        let mut answered = HashMap::new();
        answered.insert("a".to_string(), make_value("\"v\""));
        let session = InterviewSession::new("s1", "onboarding");

        let (_, _, ratio) = progress(&specs, &answered, &session, 0);
        assert!((ratio - 1.0).abs() < 1e-9);
    }

    #[test]
    fn progress_ratio_one_for_empty_specs() {
        let specs: Vec<ProfileFieldSpec> = vec![];
        let answered = HashMap::new();
        let session = InterviewSession::new("s1", "onboarding");

        let (_, _, ratio) = progress(&specs, &answered, &session, 0);
        assert!((ratio - 1.0).abs() < 1e-9);
    }
}
