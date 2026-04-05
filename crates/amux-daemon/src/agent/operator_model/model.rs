use super::*;

pub(crate) const OPERATOR_MODEL_VERSION: &str = "1.0";
pub(crate) const OPERATOR_PROFILE_VERSION: &str = "1.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OperatorProfileInputKind {
    Boolean,
}

impl OperatorProfileInputKind {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OperatorProfileQuestion {
    pub id: String,
    pub field_key: String,
    pub prompt: String,
    pub input_kind: OperatorProfileInputKind,
    pub optional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct OperatorProfileQuestionState {
    pub answer_json: Option<String>,
    pub skipped: bool,
    pub skip_reason: Option<String>,
    pub deferred_until_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OperatorProfileSessionState {
    pub version: String,
    pub session_id: String,
    pub kind: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub questions: Vec<OperatorProfileQuestion>,
    pub answers: HashMap<String, OperatorProfileQuestionState>,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OperatorProfileQuestionPayload {
    pub session_id: String,
    pub question_id: String,
    pub field_key: String,
    pub prompt: String,
    pub input_kind: String,
    pub optional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OperatorProfileProgressPayload {
    pub session_id: String,
    pub answered: u32,
    pub remaining: u32,
    pub completion_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OperatorProfileCompletionPayload {
    pub session_id: String,
    pub updated_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OperatorProfileSessionStarted {
    pub session_id: String,
    pub kind: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VerbosityPreference {
    Terse,
    #[default]
    Moderate,
    Verbose,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReadingDepth {
    Skim,
    #[default]
    Standard,
    Deep,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RiskTolerance {
    Conservative,
    #[default]
    Moderate,
    Aggressive,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub(crate) struct CognitiveStyle {
    pub avg_message_length: f64,
    pub question_frequency: f64,
    pub confirmation_seeking: f64,
    pub verbosity_preference: VerbosityPreference,
    pub reading_depth: ReadingDepth,
    pub prefers_summaries: bool,
    pub skips_reasoning: bool,
    pub message_count: u64,
    pub question_count: u64,
    pub confirmation_count: u64,
    pub summary_request_count: u64,
    pub reasoning_skip_request_count: u64,
    pub detail_request_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub(crate) struct RiskFingerprint {
    pub approval_rate_by_category: HashMap<String, f64>,
    pub avg_response_time_secs: f64,
    pub risk_tolerance: RiskTolerance,
    pub approval_requests: u64,
    pub approvals: u64,
    pub denials: u64,
    pub category_requests: HashMap<String, u64>,
    pub category_approvals: HashMap<String, u64>,
    pub fast_denials_by_category: HashMap<String, u64>,
    pub auto_approve_categories: Vec<String>,
    pub auto_deny_categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct SessionRhythm {
    pub typical_start_hour_utc: Option<u8>,
    pub session_duration_avg_minutes: f64,
    pub peak_activity_hours_utc: Vec<u8>,
    pub session_count: u64,
    pub start_hour_histogram: HashMap<u8, u64>,
    pub activity_hour_histogram: HashMap<u8, u64>,
    pub total_observed_session_minutes: u64,
    #[serde(default)]
    pub smoothed_activity_histogram: HashMap<u8, f64>,
}

/// Exponential moving average update: `alpha * sample + (1 - alpha) * current`.
pub(crate) fn ema_update(current: f64, sample: f64, alpha: f64) -> f64 {
    alpha * sample + (1.0 - alpha) * current
}

/// Apply EMA smoothing to an activity histogram across all 24 hours.
pub(crate) fn smooth_activity_histogram(
    current: &HashMap<u8, f64>,
    observation: &HashMap<u8, u64>,
    alpha: f64,
) -> HashMap<u8, f64> {
    let mut smoothed = current.clone();
    for hour in 0..24u8 {
        let observed = observation.get(&hour).copied().unwrap_or(0) as f64;
        let old = smoothed.get(&hour).copied().unwrap_or(0.0);
        smoothed.insert(hour, ema_update(old, observed, alpha));
    }
    smoothed
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct AttentionTopology {
    pub dominant_surface: Option<String>,
    pub common_surfaces: Vec<String>,
    pub top_transitions: Vec<String>,
    pub surface_histogram: HashMap<String, u64>,
    pub transition_histogram: HashMap<String, u64>,
    pub dwell_histogram: HashMap<String, u64>,
    pub focus_event_count: u64,
    pub avg_surface_dwell_secs: f64,
    pub rapid_switch_count: u64,
    pub deep_focus_surface: Option<String>,
    pub last_surface: Option<String>,
    pub last_surface_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct ImplicitFeedback {
    pub tool_hesitation_count: u64,
    pub revision_message_count: u64,
    pub correction_message_count: u64,
    pub fast_denial_count: u64,
    pub fallback_histogram: HashMap<String, u64>,
    pub top_tool_fallbacks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct OperatorModel {
    pub version: String,
    pub last_updated: u64,
    pub session_count: u64,
    pub cognitive_style: CognitiveStyle,
    pub risk_fingerprint: RiskFingerprint,
    pub session_rhythm: SessionRhythm,
    pub attention_topology: AttentionTopology,
    pub implicit_feedback: ImplicitFeedback,
    #[serde(default)]
    pub unique_tools_seen: Vec<String>,
    #[serde(default)]
    pub goal_runs_completed: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingApprovalObservation {
    pub requested_at: u64,
    pub category: String,
}

impl Default for OperatorModel {
    fn default() -> Self {
        Self {
            version: OPERATOR_MODEL_VERSION.to_string(),
            last_updated: 0,
            session_count: 0,
            cognitive_style: CognitiveStyle::default(),
            risk_fingerprint: RiskFingerprint::default(),
            session_rhythm: SessionRhythm::default(),
            attention_topology: AttentionTopology::default(),
            implicit_feedback: ImplicitFeedback::default(),
            unique_tools_seen: Vec::new(),
            goal_runs_completed: 0,
        }
    }
}
