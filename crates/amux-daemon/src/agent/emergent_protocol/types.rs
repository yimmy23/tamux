use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolSignalKind {
    RepeatedShorthand,
    RepeatedAffirmation,
    RepeatedContinuationCue,
}

impl ProtocolSignalKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RepeatedShorthand => "repeated_shorthand",
            Self::RepeatedAffirmation => "repeated_affirmation",
            Self::RepeatedContinuationCue => "repeated_continuation_cue",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "repeated_shorthand" => Some(Self::RepeatedShorthand),
            "repeated_affirmation" => Some(Self::RepeatedAffirmation),
            "repeated_continuation_cue" => Some(Self::RepeatedContinuationCue),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolCandidateState {
    Observing,
    Candidate,
    Proposed,
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolObservation {
    pub message_id: String,
    pub role: String,
    pub excerpt: String,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolCandidate {
    pub id: String,
    pub thread_id: String,
    pub kind: ProtocolSignalKind,
    pub trigger_phrase: String,
    pub normalized_pattern: String,
    pub state: ProtocolCandidateState,
    pub confidence: f64,
    pub observation_count: u32,
    pub first_seen_at_ms: u64,
    pub last_seen_at_ms: u64,
    #[serde(default)]
    pub observations: Vec<ProtocolObservation>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmergentProtocolStore {
    #[serde(default)]
    pub candidates: Vec<ProtocolCandidate>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProtocolCandidateStore {
    #[serde(default)]
    pub candidates: Vec<ProtocolCandidate>,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextSignature {
    pub thread_id: String,
    pub normalized_pattern: String,
    pub trigger_phrase: String,
    pub signal_kind: ProtocolSignalKind,
    pub source_role: String,
    pub target_role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProtocolStep {
    pub step_index: u32,
    pub intent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default)]
    pub args_template: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProtocolRegistryEntry {
    pub protocol_id: String,
    pub token: String,
    pub description: String,
    pub agent_a: String,
    pub agent_b: String,
    pub thread_id: String,
    pub normalized_pattern: String,
    pub trigger_phrase: String,
    pub signal_kind: ProtocolSignalKind,
    pub context_signature: ContextSignature,
    #[serde(default)]
    pub steps: Vec<ProtocolStep>,
    pub created_at_ms: u64,
    pub activated_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_ms: Option<u64>,
    pub usage_count: u32,
    pub success_rate: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_candidate_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProtocolUsageRecord {
    pub id: String,
    pub protocol_id: String,
    pub used_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_time_ms: Option<u64>,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolDecodeOutcomeKind {
    Match,
    Fallback,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProtocolDecodeOutcome {
    pub outcome: ProtocolDecodeOutcomeKind,
    pub token: String,
    pub protocol_id: String,
    pub thread_id: String,
    pub context_match: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
    pub entry: ProtocolRegistryEntry,
    #[serde(default)]
    pub expanded_steps: Vec<ProtocolStep>,
}
