//! WORM audit trail and SQLite logging for handoff events.
//!
//! Records every handoff to the WORM telemetry ledger (immutable) and
//! to the handoff_log SQLite table (queryable detail).

use anyhow::Result;
use rusqlite::params;
use serde_json::{json, Value};

use crate::agent::engine::AgentEngine;

/// Current Unix timestamp in seconds.
fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
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
        handoff_depth: u8,
        outcome: &str,
        confidence_band: Option<&str>,
    ) -> Result<()> {
        let handoff_log_id = handoff_log_id.to_string();
        let from_task_id = from_task_id.to_string();
        let to_specialist_id = to_specialist_id.to_string();
        let to_task_id = to_task_id.map(str::to_string);
        let task_description = task_description.to_string();
        let acceptance_criteria_json = acceptance_criteria_json.to_string();
        let context_bundle_json = context_bundle_json.to_string();
        let outcome = outcome.to_string();
        let confidence_band = confidence_band.map(str::to_string);
        let now = now_ts() as i64;

        self.history
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO handoff_log (
                        id, from_task_id, to_specialist_id, to_task_id,
                        task_description, acceptance_criteria_json, context_bundle_json,
                        handoff_depth, outcome, confidence_band, created_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        handoff_log_id,
                        from_task_id,
                        to_specialist_id,
                        to_task_id,
                        task_description,
                        acceptance_criteria_json,
                        context_bundle_json,
                        handoff_depth as i64,
                        outcome,
                        confidence_band,
                        now,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Update the outcome of an existing handoff log entry.
    pub(super) async fn update_handoff_outcome(
        &self,
        handoff_log_id: &str,
        outcome: &str,
        duration_ms: Option<u64>,
        error_message: Option<&str>,
    ) -> Result<()> {
        let handoff_log_id = handoff_log_id.to_string();
        let outcome = outcome.to_string();
        let error_message = error_message.map(str::to_string);
        let now = now_ts() as i64;

        self.history
            .conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE handoff_log SET outcome = ?1, duration_ms = ?2, \
                     completed_at = ?3, error_message = ?4 WHERE id = ?5",
                    params![
                        outcome,
                        duration_ms.map(|d| d as i64),
                        now,
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
        );

        assert_eq!(payload["from_task_id"], "task-001");
        assert_eq!(payload["to_specialist"], "backend-developer");
        assert_eq!(payload["to_task_id"], "task-002");
        assert_eq!(payload["task_description"], "Implement auth endpoint");
        assert_eq!(payload["outcome"], "dispatched");
        assert_eq!(payload["confidence_band"], "confident");
        assert_eq!(payload["handoff_log_id"], "hlog-001");
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
        );

        assert!(payload["duration_ms"].is_null());
        assert!(payload["confidence_band"].is_null());
    }
}
