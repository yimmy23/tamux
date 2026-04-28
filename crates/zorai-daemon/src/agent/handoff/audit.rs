#![allow(dead_code)]

//! WORM audit trail and SQLite logging for handoff events.
//!
//! Records every handoff to the WORM telemetry ledger (immutable) and
//! to the handoff_log SQLite table (queryable detail).

use anyhow::Result;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::agent::engine::AgentEngine;
use crate::agent::morphogenesis::types::MorphogenesisOutcome;

/// Current Unix timestamp in seconds.
fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn is_terminal_handoff_outcome(outcome: &str) -> bool {
    matches!(outcome, "accepted" | "rejected" | "completed" | "failed")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct CapabilityScoreRow {
    pub agent_id: String,
    pub capability_tag: String,
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
    pub partials: u64,
    pub last_attempt_ms: Option<u64>,
    pub avg_confidence_score: f64,
    pub total_tokens_used: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct HandoffLearningContext {
    pub handoff_log_id: String,
    pub to_specialist_id: String,
    pub capability_tags: Vec<String>,
    pub routing_score: f64,
}

fn clamp_unit_interval(value: f64) -> f64 {
    if !value.is_finite() {
        0.5
    } else {
        value.clamp(0.0, 1.0)
    }
}

/// Format the JSON payload for a WORM handoff audit entry.
///
/// This is a pure function suitable for unit testing without I/O.
pub fn format_handoff_audit_payload(
    from_task_id: &str,
    to_specialist: &str,
    to_task_id: &str,
    task_description: &str,
    outcome: &str,
    duration_ms: Option<u64>,
    confidence_band: Option<&str>,
    handoff_log_id: &str,
    routing_method: &str,
    capability_tags_json: &str,
    routing_score: f64,
    fallback_used: bool,
) -> Value {
    json!({
        "kind": "handoff",
        "timestamp": now_ts() as i64,
        "from_task_id": from_task_id,
        "to_specialist": to_specialist,
        "to_task_id": to_task_id,
        "task_description": task_description,
        "outcome": outcome,
        "duration_ms": duration_ms,
        "confidence_band": confidence_band,
        "handoff_log_id": handoff_log_id,
        "routing_method": routing_method,
        "capability_tags_json": capability_tags_json,
        "routing_score": routing_score,
        "fallback_used": fallback_used,
    })
}

impl AgentEngine {
    /// Record a handoff event to the WORM telemetry ledger.
    pub(super) async fn record_handoff_audit(
        &self,
        from_task_id: &str,
        to_specialist: &str,
        to_task_id: &str,
        task_description: &str,
        outcome: &str,
        duration_ms: Option<u64>,
        confidence_band: Option<&str>,
        handoff_log_id: &str,
        routing_method: &str,
        capability_tags_json: &str,
        routing_score: f64,
        fallback_used: bool,
    ) -> Result<()> {
        let payload = format_handoff_audit_payload(
            from_task_id,
            to_specialist,
            to_task_id,
            task_description,
            outcome,
            duration_ms,
            confidence_band,
            handoff_log_id,
            routing_method,
            capability_tags_json,
            routing_score,
            fallback_used,
        );
        self.history.append_telemetry("handoff", payload).await
    }

    /// Insert a detailed handoff log entry into SQLite.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn log_handoff_detail(
        &self,
        handoff_log_id: &str,
        from_task_id: &str,
        to_specialist_id: &str,
        to_task_id: Option<&str>,
        task_description: &str,
        acceptance_criteria_json: &str,
        context_bundle_json: &str,
        capability_tags_json: &str,
        handoff_depth: u8,
        outcome: &str,
        confidence_band: Option<&str>,
        routing_method: &str,
        routing_score: f64,
        fallback_used: bool,
    ) -> Result<()> {
        let handoff_log_id = handoff_log_id.to_string();
        let from_task_id = from_task_id.to_string();
        let to_specialist_id = to_specialist_id.to_string();
        let to_task_id = to_task_id.map(str::to_string);
        let task_description = task_description.to_string();
        let acceptance_criteria_json = acceptance_criteria_json.to_string();
        let context_bundle_json = context_bundle_json.to_string();
        let capability_tags_json = capability_tags_json.to_string();
        let outcome = outcome.to_string();
        let confidence_band = confidence_band.map(str::to_string);
        let routing_method = routing_method.to_string();
        let now = now_ts() as i64;

        self.history
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO handoff_log (
                        id, from_task_id, to_specialist_id, to_task_id,
                        task_description, acceptance_criteria_json, context_bundle_json,
                        capability_tags_json, handoff_depth, outcome, confidence_band,
                        routing_method, routing_score, fallback_used, created_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                    params![
                        handoff_log_id,
                        from_task_id,
                        to_specialist_id,
                        to_task_id,
                        task_description,
                        acceptance_criteria_json,
                        context_bundle_json,
                        capability_tags_json,
                        handoff_depth as i64,
                        outcome,
                        confidence_band,
                        routing_method,
                        routing_score,
                        fallback_used as i64,
                        now,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Update the outcome of an existing handoff log entry.
    pub(crate) async fn update_handoff_outcome(
        &self,
        handoff_log_id: &str,
        outcome: &str,
        duration_ms: Option<u64>,
        error_message: Option<&str>,
    ) -> Result<()> {
        let handoff_log_id = handoff_log_id.to_string();
        let outcome = outcome.to_string();
        let error_message = error_message.map(str::to_string);
        let completed_at = is_terminal_handoff_outcome(&outcome).then(|| now_ts() as i64);

        self.history
            .conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE handoff_log SET outcome = ?1, duration_ms = ?2, \
                     completed_at = ?3, error_message = ?4 WHERE id = ?5",
                    params![
                        outcome,
                        duration_ms.map(|d| d as i64),
                        completed_at,
                        error_message,
                        handoff_log_id,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Bind a dispatched specialist task ID to an existing handoff log entry.
    pub(crate) async fn bind_handoff_task_id(
        &self,
        handoff_log_id: &str,
        to_task_id: &str,
    ) -> Result<()> {
        let handoff_log_id = handoff_log_id.to_string();
        let to_task_id = to_task_id.to_string();

        self.history
            .conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE handoff_log SET to_task_id = ?1 WHERE id = ?2",
                    params![to_task_id, handoff_log_id],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Resolve handoff log ID from persisted specialist task linkage.
    pub(crate) async fn resolve_handoff_log_id_by_task_id(
        &self,
        task_id: &str,
    ) -> Result<Option<String>> {
        let task_id = task_id.to_string();
        self.history
            .conn
            .call(move |conn| {
                let mut stmt =
                    conn.prepare("SELECT id FROM handoff_log WHERE to_task_id = ?1 LIMIT 1")?;
                let mut rows = stmt.query(params![task_id])?;
                if let Some(row) = rows.next()? {
                    let id: String = row.get(0)?;
                    Ok(Some(id))
                } else {
                    Ok(None)
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn get_handoff_learning_context_by_task_id(
        &self,
        task_id: &str,
    ) -> Result<Option<HandoffLearningContext>> {
        let task_id = task_id.to_string();
        self.history
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, to_specialist_id, capability_tags_json, routing_score \
                     FROM handoff_log WHERE to_task_id = ?1 LIMIT 1",
                )?;
                let mut rows = stmt.query(params![task_id])?;
                if let Some(row) = rows.next()? {
                    let handoff_log_id: String = row.get(0)?;
                    let to_specialist_id: String = row.get(1)?;
                    let capability_tags_json: Option<String> = row.get(2)?;
                    let routing_score: f64 = row.get(3)?;
                    let capability_tags = capability_tags_json
                        .as_deref()
                        .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
                        .unwrap_or_default();
                    Ok(Some(HandoffLearningContext {
                        handoff_log_id,
                        to_specialist_id,
                        capability_tags,
                        routing_score,
                    }))
                } else {
                    Ok(None)
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn load_capability_score_rows(
        &self,
        capability_tags: &[String],
    ) -> Result<Vec<CapabilityScoreRow>> {
        let capability_tags = capability_tags.to_vec();
        self.history
            .conn
            .call(move |conn| {
                let mut rows_out = Vec::new();
                let mut stmt = conn.prepare(
                    "SELECT agent_id, capability_tag, attempts, successes, failures, partials, \
                            last_attempt_ms, avg_confidence_score, total_tokens_used \
                     FROM agent_capability_scores WHERE capability_tag = ?1",
                )?;
                for capability_tag in capability_tags {
                    let mapped = stmt.query_map(params![capability_tag], |row| {
                        Ok(CapabilityScoreRow {
                            agent_id: row.get(0)?,
                            capability_tag: row.get(1)?,
                            attempts: row.get::<_, i64>(2)? as u64,
                            successes: row.get::<_, i64>(3)? as u64,
                            failures: row.get::<_, i64>(4)? as u64,
                            partials: row.get::<_, i64>(5)? as u64,
                            last_attempt_ms: row.get::<_, Option<i64>>(6)?.map(|v| v as u64),
                            avg_confidence_score: row.get(7)?,
                            total_tokens_used: row.get::<_, i64>(8)? as u64,
                        })
                    })?;
                    for row in mapped {
                        rows_out.push(row?);
                    }
                }
                Ok(rows_out)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn record_capability_outcome(
        &self,
        agent_id: &str,
        capability_tags: &[String],
        outcome: &str,
        confidence: f64,
        tokens_used: u64,
        confidence_ema_alpha: f64,
    ) -> Result<()> {
        let agent_id = agent_id.to_string();
        let capability_tags = capability_tags.to_vec();
        let outcome = outcome.to_string();
        let morphogenesis_agent_id = agent_id.clone();
        let morphogenesis_capability_tags = capability_tags.clone();
        let morphogenesis_outcome = match outcome.as_str() {
            "success" | "completed" | "accepted" => MorphogenesisOutcome::Success,
            "partial" | "rejected" => MorphogenesisOutcome::Partial,
            _ => MorphogenesisOutcome::Failure,
        };
        let confidence = clamp_unit_interval(confidence);
        let confidence_ema_alpha = if confidence_ema_alpha.is_finite() {
            confidence_ema_alpha.clamp(0.0, 1.0)
        } else {
            0.3
        };
        let now_ms = (now_ts() as i64) * 1000;

        self.history
            .conn
            .call(move |conn| {
                for capability_tag in capability_tags {
                    conn.execute(
                        "INSERT INTO agent_capability_scores (
                            agent_id, capability_tag, attempts, successes, failures, partials,
                            last_attempt_ms, avg_confidence_score, total_tokens_used
                        ) VALUES (?1, ?2, 0, 0, 0, 0, NULL, 0.5, 0)
                        ON CONFLICT(agent_id, capability_tag) DO NOTHING",
                        params![&agent_id, &capability_tag],
                    )?;

                    let (success_delta, failure_delta, partial_delta) = match outcome.as_str() {
                        "success" | "completed" | "accepted" => (1_i64, 0_i64, 0_i64),
                        "partial" | "rejected" => (0_i64, 0_i64, 1_i64),
                        _ => (0_i64, 1_i64, 0_i64),
                    };

                    conn.execute(
                        "UPDATE agent_capability_scores
                         SET attempts = attempts + 1,
                             successes = successes + ?3,
                             failures = failures + ?4,
                             partials = partials + ?5,
                             last_attempt_ms = ?6,
                             avg_confidence_score = ((1.0 - ?7) * avg_confidence_score) + (?7 * ?8),
                             total_tokens_used = total_tokens_used + ?9
                         WHERE agent_id = ?1 AND capability_tag = ?2",
                        params![
                            &agent_id,
                            &capability_tag,
                            success_delta,
                            failure_delta,
                            partial_delta,
                            now_ms,
                            confidence_ema_alpha,
                            confidence,
                            tokens_used as i64,
                        ],
                    )?;
                }
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        self.record_morphogenesis_outcome(
            &morphogenesis_agent_id,
            &morphogenesis_capability_tags,
            morphogenesis_outcome,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_audit_payload_structure() {
        let payload = format_handoff_audit_payload(
            "task-001",
            "backend-developer",
            "task-002",
            "Implement auth endpoint",
            "dispatched",
            None,
            Some("confident"),
            "hlog-001",
            "probabilistic",
            r#"["rust","backend"]"#,
            0.92,
            false,
        );

        assert_eq!(payload["from_task_id"], "task-001");
        assert_eq!(payload["to_specialist"], "backend-developer");
        assert_eq!(payload["to_task_id"], "task-002");
        assert_eq!(payload["task_description"], "Implement auth endpoint");
        assert_eq!(payload["outcome"], "dispatched");
        assert_eq!(payload["confidence_band"], "confident");
        assert_eq!(payload["handoff_log_id"], "hlog-001");
        assert_eq!(payload["routing_method"], "probabilistic");
        assert_eq!(payload["capability_tags_json"], r#"["rust","backend"]"#);
        assert_eq!(payload["routing_score"], 0.92);
        assert_eq!(payload["fallback_used"], false);
        assert!(payload["timestamp"].is_number());
        assert_eq!(payload["kind"], "handoff");
    }

    #[test]
    fn test_format_audit_payload_includes_duration() {
        let payload = format_handoff_audit_payload(
            "task-001",
            "researcher",
            "task-003",
            "Research API options",
            "completed",
            Some(5000),
            Some("likely"),
            "hlog-002",
            "deterministic",
            r#"["research"]"#,
            1.0,
            false,
        );

        assert_eq!(payload["duration_ms"], 5000);
        assert_eq!(payload["outcome"], "completed");
    }

    #[test]
    fn test_format_audit_payload_null_optionals() {
        let payload = format_handoff_audit_payload(
            "task-001",
            "generalist",
            "task-004",
            "Generic task",
            "dispatched",
            None,
            None,
            "hlog-003",
            "deterministic",
            "[]",
            0.0,
            true,
        );

        assert!(payload["duration_ms"].is_null());
        assert!(payload["confidence_band"].is_null());
        assert_eq!(payload["routing_method"], "deterministic");
        assert_eq!(payload["capability_tags_json"], "[]");
        assert_eq!(payload["routing_score"], 0.0);
        assert_eq!(payload["fallback_used"], true);
    }

    #[test]
    fn test_dispatched_is_not_terminal_handoff_outcome() {
        assert!(!is_terminal_handoff_outcome("dispatched"));
        assert!(!is_terminal_handoff_outcome("pending"));
    }

    #[test]
    fn test_terminal_handoff_outcomes_are_finalizing() {
        assert!(is_terminal_handoff_outcome("accepted"));
        assert!(is_terminal_handoff_outcome("rejected"));
        assert!(is_terminal_handoff_outcome("completed"));
        assert!(is_terminal_handoff_outcome("failed"));
    }
}
