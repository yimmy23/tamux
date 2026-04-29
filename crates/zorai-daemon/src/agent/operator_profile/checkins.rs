//! Operator profile check-in policy + scheduling helpers.
//!
//! Pure policy logic lives here so threshold math and anti-spam behavior can be
//! unit-tested without daemon I/O.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use zorai_protocol::AgentEventRow;

use crate::agent::types::{GoalRun, GoalRunStatus, TaskPriority};
use crate::history::{
    OperatorProfileCheckinRow, OperatorProfileConsentRow, OperatorProfileFieldRow,
};

pub const CONFIDENCE_DECAY_THRESHOLD: f64 = 0.60;
pub const CONFIDENCE_DECAY_AGE_MS: u64 = 30 * 24 * 60 * 60 * 1000;
pub const BEHAVIOR_WINDOW_7D_MS: u64 = 7 * 24 * 60 * 60 * 1000;
pub const BEHAVIOR_WINDOW_30D_MS: u64 = 30 * 24 * 60 * 60 * 1000;
pub const BEHAVIOR_DELTA_THRESHOLD: f64 = 0.20;
pub const CONTEXTUAL_CHECKIN_COOLDOWN_MS: u64 = 72 * 60 * 60 * 1000;
pub const WEEKLY_CHECKIN_INTERVAL_MS: u64 = 7 * 24 * 60 * 60 * 1000;
pub const MAX_CONTEXTUAL_QUESTIONS_PER_SESSION: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassiveSignalKind {
    OperatorMessage,
    ApprovalRequested,
    ApprovalResolved,
    ToolHesitation,
    AttentionSurface,
}

impl PassiveSignalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OperatorMessage => "operator_message",
            Self::ApprovalRequested => "approval_requested",
            Self::ApprovalResolved => "approval_resolved",
            Self::ToolHesitation => "tool_hesitation",
            Self::AttentionSurface => "attention_surface",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckinKind {
    Weekly,
    Contextual,
}

impl CheckinKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Weekly => "weekly",
            Self::Contextual => "contextual",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextualTrigger {
    MissingCriticalFields,
    ConfidenceDecay,
    BehaviorDelta,
}

impl ContextualTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingCriticalFields => "missing_critical_fields",
            Self::ConfidenceDecay => "confidence_decay",
            Self::BehaviorDelta => "behavior_delta",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsentSnapshot {
    pub passive_learning: bool,
    pub weekly_checkins: bool,
    pub proactive_suggestions: bool,
}

#[derive(Debug, Clone)]
pub struct PassiveCheckinInput<'a> {
    pub now_ms: u64,
    pub signal: PassiveSignalKind,
    pub session_id: Option<&'a str>,
    pub session_kind: Option<&'a str>,
    pub fields: &'a [OperatorProfileFieldRow],
    pub checkins: &'a [OperatorProfileCheckinRow],
    pub events: &'a [AgentEventRow],
    pub goal_runs: &'a VecDeque<GoalRun>,
    pub consents: ConsentSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassiveCheckinDecision {
    pub schedule_weekly: bool,
    pub schedule_contextual: Option<ContextualTrigger>,
    pub contextual_question_index: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduledCheckinMetadata {
    pub version: u8,
    pub source: String,
    pub trigger: String,
    pub signal: String,
    pub session_id: Option<String>,
    pub session_kind: Option<String>,
    pub question_index: u8,
}

pub fn passive_learning_allowed(consent: Option<&OperatorProfileConsentRow>) -> bool {
    consent.map(|row| row.granted).unwrap_or(true)
}

pub fn weekly_checkins_allowed(consent: Option<&OperatorProfileConsentRow>) -> bool {
    consent.map(|row| row.granted).unwrap_or(true)
}

pub fn proactive_suggestions_allowed(consent: Option<&OperatorProfileConsentRow>) -> bool {
    consent.map(|row| row.granted).unwrap_or(true)
}

pub fn has_confidence_decay_trigger(_fields: &[OperatorProfileFieldRow], _now_ms: u64) -> bool {
    _fields.iter().any(|field| {
        if field.confidence >= CONFIDENCE_DECAY_THRESHOLD {
            return false;
        }
        let updated_ms = normalize_timestamp_to_ms(field.updated_at);
        _now_ms.saturating_sub(updated_ms) > CONFIDENCE_DECAY_AGE_MS
    })
}

pub fn has_missing_critical_fields_trigger(_fields: &[OperatorProfileFieldRow]) -> bool {
    let preferred_name_ok = has_meaningful_field(_fields, "preferred_name");
    let primary_goals_ok = has_meaningful_field(_fields, "primary_goals");
    !preferred_name_ok || !primary_goals_ok
}

pub fn has_behavior_delta_trigger(_events: &[AgentEventRow], _now_ms: u64) -> bool {
    let now_i64 = _now_ms as i64;
    let start_30d = now_i64.saturating_sub(BEHAVIOR_WINDOW_30D_MS as i64);
    let start_7d = now_i64.saturating_sub(BEHAVIOR_WINDOW_7D_MS as i64);

    let mut count_30d = 0u64;
    let mut count_7d = 0u64;
    for event in _events {
        if event.category != "behavioral" {
            continue;
        }
        let ts = event.timestamp;
        if ts >= start_30d && ts <= now_i64 {
            count_30d += 1;
            if ts >= start_7d {
                count_7d += 1;
            }
        }
    }
    if count_30d == 0 {
        return false;
    }

    let baseline = count_30d as f64 / 30.0;
    if baseline <= f64::EPSILON {
        return false;
    }
    let rolling = count_7d as f64 / 7.0;
    ((rolling - baseline).abs() / baseline) >= BEHAVIOR_DELTA_THRESHOLD
}

pub fn is_in_critical_goal_execution_window(_goal_runs: &VecDeque<GoalRun>) -> bool {
    _goal_runs.iter().any(|goal| {
        matches!(
            goal.status,
            GoalRunStatus::Planning | GoalRunStatus::Running | GoalRunStatus::AwaitingApproval
        ) && goal.priority == TaskPriority::Urgent
    })
}

pub fn evaluate_passive_checkin_policy(input: &PassiveCheckinInput<'_>) -> PassiveCheckinDecision {
    if !input.consents.passive_learning {
        return PassiveCheckinDecision {
            schedule_weekly: false,
            schedule_contextual: None,
            contextual_question_index: 0,
        };
    }
    if is_in_critical_goal_execution_window(input.goal_runs) {
        return PassiveCheckinDecision {
            schedule_weekly: false,
            schedule_contextual: None,
            contextual_question_index: 0,
        };
    }

    let schedule_weekly = input.consents.weekly_checkins
        && !has_recent_checkin(
            input.checkins,
            CheckinKind::Weekly,
            input.now_ms,
            WEEKLY_CHECKIN_INTERVAL_MS,
        );

    if !input.consents.proactive_suggestions {
        return PassiveCheckinDecision {
            schedule_weekly,
            schedule_contextual: None,
            contextual_question_index: 0,
        };
    }
    if has_recent_checkin(
        input.checkins,
        CheckinKind::Contextual,
        input.now_ms,
        CONTEXTUAL_CHECKIN_COOLDOWN_MS,
    ) {
        return PassiveCheckinDecision {
            schedule_weekly,
            schedule_contextual: None,
            contextual_question_index: 0,
        };
    }

    let current_session_questions = questions_asked_for_session(input.checkins, input.session_id);
    if current_session_questions >= MAX_CONTEXTUAL_QUESTIONS_PER_SESSION {
        return PassiveCheckinDecision {
            schedule_weekly,
            schedule_contextual: None,
            contextual_question_index: 0,
        };
    }

    let contextual_trigger = if has_missing_critical_fields_trigger(input.fields) {
        Some(ContextualTrigger::MissingCriticalFields)
    } else if has_confidence_decay_trigger(input.fields, input.now_ms) {
        Some(ContextualTrigger::ConfidenceDecay)
    } else if has_behavior_delta_trigger(input.events, input.now_ms) {
        Some(ContextualTrigger::BehaviorDelta)
    } else {
        None
    };

    let question_index = if contextual_trigger.is_some() {
        (current_session_questions as u8).saturating_add(1)
    } else {
        0
    };

    PassiveCheckinDecision {
        schedule_weekly,
        schedule_contextual: contextual_trigger,
        contextual_question_index: question_index,
    }
}

pub fn parse_scheduled_metadata(
    checkin: &OperatorProfileCheckinRow,
) -> Option<ScheduledCheckinMetadata> {
    checkin
        .response_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<ScheduledCheckinMetadata>(raw).ok())
}

pub fn build_scheduled_checkin(
    kind: CheckinKind,
    now_ms: u64,
    trigger: &str,
    signal: PassiveSignalKind,
    session_id: Option<&str>,
    session_kind: Option<&str>,
    question_index: u8,
) -> anyhow::Result<OperatorProfileCheckinRow> {
    let metadata = ScheduledCheckinMetadata {
        version: 1,
        source: "passive_hook".to_string(),
        trigger: trigger.to_string(),
        signal: signal.as_str().to_string(),
        session_id: session_id.map(ToOwned::to_owned),
        session_kind: session_kind.map(ToOwned::to_owned),
        question_index,
    };
    let cadence = match kind {
        CheckinKind::Weekly => WEEKLY_CHECKIN_INTERVAL_MS,
        CheckinKind::Contextual => CONTEXTUAL_CHECKIN_COOLDOWN_MS,
    };
    let bucket = now_ms / cadence.max(1);
    let session_component = session_id.unwrap_or("global");
    let id = format!(
        "op_checkin_{}_{}_{}_{}",
        kind.as_str(),
        trigger,
        session_component,
        bucket
    );
    Ok(OperatorProfileCheckinRow {
        id,
        kind: kind.as_str().to_string(),
        scheduled_at: now_ms as i64,
        shown_at: None,
        status: "scheduled".to_string(),
        response_json: Some(serde_json::to_string(&metadata)?),
        created_at: now_ms as i64,
        updated_at: now_ms as i64,
    })
}

fn normalize_timestamp_to_ms(raw: i64) -> u64 {
    if raw <= 0 {
        return 0;
    }
    let value = raw as u64;
    if value < 100_000_000_000 {
        value.saturating_mul(1000)
    } else {
        value
    }
}

fn has_meaningful_field(fields: &[OperatorProfileFieldRow], key: &str) -> bool {
    fields
        .iter()
        .find(|field| field.field_key == key)
        .map(|field| parse_json_has_meaningful_value(&field.field_value_json))
        .unwrap_or(false)
}

fn parse_json_has_meaningful_value(raw: &str) -> bool {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(serde_json::Value::Null) => false,
        Ok(serde_json::Value::String(value)) => !value.trim().is_empty(),
        Ok(serde_json::Value::Array(values)) => !values.is_empty(),
        Ok(serde_json::Value::Object(values)) => !values.is_empty(),
        Ok(_) => true,
        Err(_) => !raw.trim().is_empty(),
    }
}

fn has_recent_checkin(
    checkins: &[OperatorProfileCheckinRow],
    kind: CheckinKind,
    now_ms: u64,
    cooldown_ms: u64,
) -> bool {
    checkins.iter().any(|checkin| {
        if checkin.kind != kind.as_str() || checkin.status == "cancelled" {
            return false;
        }
        let anchor_ms = checkin
            .shown_at
            .map(normalize_timestamp_to_ms)
            .unwrap_or_else(|| normalize_timestamp_to_ms(checkin.scheduled_at));
        now_ms.saturating_sub(anchor_ms) < cooldown_ms
    })
}

fn questions_asked_for_session(
    checkins: &[OperatorProfileCheckinRow],
    session_id: Option<&str>,
) -> usize {
    let Some(session_id) = session_id else {
        return 0;
    };
    checkins
        .iter()
        .filter(|checkin| checkin.kind == CheckinKind::Contextual.as_str())
        .filter(|checkin| checkin.status != "cancelled")
        .filter_map(parse_scheduled_metadata)
        .filter(|meta| meta.session_id.as_deref() == Some(session_id))
        .count()
}

#[cfg(test)]
mod tests;
