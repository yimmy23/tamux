use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    InProgress,
    Resolved,
    Deferred,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Advocate,
    Critic,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Proceed,
    ProceedWithModifications,
    Defer,
    Reject,
}

impl Decision {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Proceed => "proceed",
            Self::ProceedWithModifications => "proceed_with_modifications",
            Self::Defer => "defer",
            Self::Reject => "reject",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CritiqueDirective {
    ScheduleForOperatorWindow,
    LimitSubagentToolCalls,
    LimitSubagentWallTime,
    DisableNetwork,
    EnableSandbox,
    DowngradeSecurityLevel,
    StripExplicitMessagingTargets,
    StripBroadcastMentions,
    NarrowSensitiveFilePath,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArgumentPoint {
    pub claim: String,
    pub weight: f64,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Argument {
    pub role: Role,
    #[serde(default)]
    pub points: Vec<ArgumentPoint>,
    pub overall_confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Resolution {
    pub decision: Decision,
    pub synthesis: String,
    pub risk_score: f64,
    pub confidence: f64,
    #[serde(default)]
    pub modifications: Vec<String>,
    #[serde(default)]
    pub directives: Vec<CritiqueDirective>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CritiqueSession {
    pub id: String,
    pub action_id: String,
    pub tool_name: String,
    pub proposed_action_summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub advocate_id: String,
    pub critic_id: String,
    pub arbiter_id: String,
    pub status: SessionStatus,
    pub advocate_argument: Argument,
    pub critic_argument: Argument,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<Resolution>,
    pub created_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at_ms: Option<u64>,
}
