#![allow(dead_code)]

//! Trusted execution provenance helpers built on the local WORM telemetry ledger.

use super::*;

pub(crate) const PROVENANCE_EVENT_PROACTIVE_CACHE_PREPARED: &str = "proactive_cache_prepared";
pub(crate) const PROVENANCE_EVENT_PROACTIVE_CACHE_USED: &str = "proactive_cache_used";
pub(crate) const PROVENANCE_EVENT_SPECULATIVE_RESULT_PREPARED: &str = "speculative_result_prepared";
pub(crate) const PROVENANCE_EVENT_SPECULATIVE_RESULT_USED: &str = "speculative_result_used";
pub(crate) const PROVENANCE_EVENT_DREAM_HINTS_PERSISTED: &str = "dream_hints_persisted";
pub(crate) const PROVENANCE_EVENT_FORGE_DREAM_HINTS_PERSISTED: &str = "forge_dream_hints_persisted";

pub(crate) fn is_proactive_provenance_event(event_type: &str) -> bool {
    matches!(
        event_type,
        PROVENANCE_EVENT_PROACTIVE_CACHE_PREPARED
            | PROVENANCE_EVENT_PROACTIVE_CACHE_USED
            | PROVENANCE_EVENT_SPECULATIVE_RESULT_PREPARED
            | PROVENANCE_EVENT_SPECULATIVE_RESULT_USED
    )
}

pub(crate) fn proactive_provenance_summary(
    report: &crate::history::ProvenanceReport,
    recent_limit: usize,
) -> serde_json::Value {
    let prepared_cache_count = report
        .summary_by_event
        .get(PROVENANCE_EVENT_PROACTIVE_CACHE_PREPARED)
        .copied()
        .unwrap_or(0);
    let used_cache_count = report
        .summary_by_event
        .get(PROVENANCE_EVENT_PROACTIVE_CACHE_USED)
        .copied()
        .unwrap_or(0);
    let prepared_speculative_count = report
        .summary_by_event
        .get(PROVENANCE_EVENT_SPECULATIVE_RESULT_PREPARED)
        .copied()
        .unwrap_or(0);
    let used_speculative_count = report
        .summary_by_event
        .get(PROVENANCE_EVENT_SPECULATIVE_RESULT_USED)
        .copied()
        .unwrap_or(0);
    let recent_events = report
        .entries
        .iter()
        .filter(|entry| is_proactive_provenance_event(&entry.event_type))
        .take(recent_limit.max(1))
        .map(|entry| {
            serde_json::json!({
                "event_type": entry.event_type,
                "summary": entry.summary,
                "timestamp_ms": entry.timestamp,
                "thread_id": entry.thread_id,
                "task_id": entry.task_id,
                "goal_run_id": entry.goal_run_id,
                "causal_trace_id": entry.causal_trace_id,
                "hash_valid": entry.hash_valid,
                "chain_valid": entry.chain_valid,
                "signature_valid": entry.signature_valid,
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "prepared_count": prepared_cache_count + prepared_speculative_count,
        "used_count": used_cache_count + used_speculative_count,
        "prepared_cache_count": prepared_cache_count,
        "used_cache_count": used_cache_count,
        "prepared_speculative_count": prepared_speculative_count,
        "used_speculative_count": used_speculative_count,
        "recent_event_count": recent_events.len(),
        "recent_events": recent_events,
    })
}

pub(crate) fn is_adaptive_carryover_provenance_event(event_type: &str) -> bool {
    matches!(
        event_type,
        PROVENANCE_EVENT_DREAM_HINTS_PERSISTED | PROVENANCE_EVENT_FORGE_DREAM_HINTS_PERSISTED
    )
}

pub(crate) fn adaptive_carryover_source_kind(event_type: &str) -> &'static str {
    match event_type {
        PROVENANCE_EVENT_DREAM_HINTS_PERSISTED => "dream_state",
        PROVENANCE_EVENT_FORGE_DREAM_HINTS_PERSISTED => "forge",
        _ => "unknown",
    }
}

pub(crate) fn adaptive_carryover_provenance_summary(
    report: &crate::history::ProvenanceReport,
    recent_limit: usize,
) -> serde_json::Value {
    let dream_hint_persist_count = report
        .summary_by_event
        .get(PROVENANCE_EVENT_DREAM_HINTS_PERSISTED)
        .copied()
        .unwrap_or(0);
    let forge_hint_persist_count = report
        .summary_by_event
        .get(PROVENANCE_EVENT_FORGE_DREAM_HINTS_PERSISTED)
        .copied()
        .unwrap_or(0);
    let recent_events = report
        .entries
        .iter()
        .filter(|entry| is_adaptive_carryover_provenance_event(&entry.event_type))
        .take(recent_limit.max(1))
        .map(|entry| {
            serde_json::json!({
                "sequence": entry.sequence,
                "provenance_ref": format!("provenance:{}", entry.sequence),
                "source_kind": adaptive_carryover_source_kind(&entry.event_type),
                "event_type": entry.event_type,
                "summary": entry.summary,
                "timestamp_ms": entry.timestamp,
                "thread_id": entry.thread_id,
                "task_id": entry.task_id,
                "goal_run_id": entry.goal_run_id,
                "causal_trace_id": entry.causal_trace_id,
                "hash_valid": entry.hash_valid,
                "chain_valid": entry.chain_valid,
                "signature_present": entry.signature_present,
                "signature_valid": entry.signature_valid,
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "inspection_tool": "show_dreams",
        "persisted_event_count": dream_hint_persist_count + forge_hint_persist_count,
        "dream_hint_event_count": dream_hint_persist_count,
        "forge_hint_event_count": forge_hint_persist_count,
        "recent_event_count": recent_events.len(),
        "recent_events": recent_events,
    })
}

pub(crate) fn adaptive_carryover_is_effectively_empty(summary: &serde_json::Value) -> bool {
    summary
        .get("persisted_event_count")
        .and_then(|value| value.as_u64())
        .unwrap_or(0)
        == 0
        && summary
            .get("recent_event_count")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            == 0
}

impl AgentEngine {
    pub(super) fn compliance_mode_label(&self) -> String {
        let config = self.config.blocking_read();
        match config.compliance.mode {
            ComplianceMode::Standard => "standard",
            ComplianceMode::Soc2 => "soc2",
            ComplianceMode::Hipaa => "hipaa",
            ComplianceMode::Fedramp => "fedramp",
        }
        .to_string()
    }

    pub(crate) async fn record_provenance_event(
        &self,
        event_type: &str,
        summary: &str,
        details: serde_json::Value,
        goal_run_id: Option<&str>,
        task_id: Option<&str>,
        thread_id: Option<&str>,
        approval_id: Option<&str>,
        causal_trace_id: Option<&str>,
    ) {
        let config = self.config.read().await.clone();
        if !config.enabled {
            return;
        }
        if let Err(error) = self
            .history
            .record_provenance_event(&crate::history::ProvenanceEventRecord {
                event_type,
                summary,
                details: &details,
                agent_id: "zorai-daemon",
                goal_run_id,
                task_id,
                thread_id,
                approval_id,
                causal_trace_id,
                compliance_mode: match config.compliance.mode {
                    ComplianceMode::Standard => "standard",
                    ComplianceMode::Soc2 => "soc2",
                    ComplianceMode::Hipaa => "hipaa",
                    ComplianceMode::Fedramp => "fedramp",
                },
                sign: config.compliance.sign_all_events,
                created_at: now_millis(),
            })
            .await
        {
            tracing::warn!(event_type, error = %error, "failed to record provenance event");
        }
    }

    pub(super) async fn record_proactive_cache_prepared(
        &self,
        thread_id: &str,
        summary: &str,
        precomputation_id: Option<i64>,
    ) {
        let summary_text = format!("Prepared proactive cache for thread {thread_id}");
        self.record_provenance_event(
            PROVENANCE_EVENT_PROACTIVE_CACHE_PREPARED,
            &summary_text,
            serde_json::json!({
                "thread_id": thread_id,
                "cache_kind": super::anticipatory::SPECULATIVE_ACTION_REPO_CONTEXT_REFRESH,
                "summary": summary,
                "precomputation_id": precomputation_id,
            }),
            None,
            None,
            Some(thread_id),
            None,
            None,
        )
        .await;
    }

    pub(super) async fn record_proactive_cache_used(
        &self,
        thread_id: &str,
        summary: &str,
        precomputation_id: Option<i64>,
        source: &str,
    ) {
        let summary_text = format!("Used proactive cache for thread {thread_id}");
        self.record_provenance_event(
            PROVENANCE_EVENT_PROACTIVE_CACHE_USED,
            &summary_text,
            serde_json::json!({
                "thread_id": thread_id,
                "cache_kind": super::anticipatory::SPECULATIVE_ACTION_REPO_CONTEXT_REFRESH,
                "summary": summary,
                "precomputation_id": precomputation_id,
                "source": source,
            }),
            None,
            None,
            Some(thread_id),
            None,
            None,
        )
        .await;
    }

    pub(super) async fn record_speculative_result_prepared(
        &self,
        opportunity: &SpeculativeOpportunity,
        result: &SpeculativeResult,
    ) {
        let Some(thread_id) = result.thread_id.as_deref() else {
            return;
        };
        let summary_text = format!(
            "Prepared speculative {} for thread {thread_id}",
            result.action_kind
        );
        self.record_provenance_event(
            PROVENANCE_EVENT_SPECULATIVE_RESULT_PREPARED,
            &summary_text,
            serde_json::json!({
                "thread_id": thread_id,
                "opportunity_id": result.opportunity_id,
                "action_kind": result.action_kind,
                "source_kind": opportunity.source_kind,
                "opportunity_confidence": opportunity.confidence,
                "opportunity_summary": opportunity.summary,
                "result_summary": result.summary,
                "precomputation_id": result.precomputation_id,
                "expires_at_ms": result.expires_at_ms,
            }),
            None,
            None,
            Some(thread_id),
            None,
            None,
        )
        .await;
    }

    pub(super) async fn record_speculative_result_used(
        &self,
        result: &SpeculativeResult,
        source: &str,
    ) {
        let Some(thread_id) = result.thread_id.as_deref() else {
            return;
        };
        let summary_text = format!(
            "Used speculative {} for thread {thread_id}",
            result.action_kind
        );
        self.record_provenance_event(
            PROVENANCE_EVENT_SPECULATIVE_RESULT_USED,
            &summary_text,
            serde_json::json!({
                "thread_id": thread_id,
                "opportunity_id": result.opportunity_id,
                "action_kind": result.action_kind,
                "result_summary": result.summary,
                "precomputation_id": result.precomputation_id,
                "used_at_ms": result.used_at_ms,
                "expires_at_ms": result.expires_at_ms,
                "source": source,
            }),
            None,
            None,
            Some(thread_id),
            None,
            None,
        )
        .await;
    }

    pub(super) async fn record_dream_hints_persisted(
        &self,
        scope_id: &str,
        persisted_count: usize,
        hints: &[String],
        dream_cycle_id: Option<i64>,
    ) {
        let summary_text =
            format!("Persisted {persisted_count} dream carryover hint(s) in scope {scope_id}");
        self.record_provenance_event(
            PROVENANCE_EVENT_DREAM_HINTS_PERSISTED,
            &summary_text,
            serde_json::json!({
                "scope_id": scope_id,
                "persisted_count": persisted_count,
                "persisted_hints": hints,
                "hint_preview": hints.iter().take(3).cloned().collect::<Vec<_>>(),
                "dream_cycle_id": dream_cycle_id,
                "source": "dream_state",
            }),
            None,
            None,
            None,
            None,
            None,
        )
        .await;
    }

    pub(super) async fn record_forge_dream_hints_persisted(
        &self,
        scope_id: &str,
        persisted_count: usize,
        hints: &[String],
    ) {
        let summary_text = format!(
            "Persisted {persisted_count} forge-derived dream carryover hint(s) in scope {scope_id}"
        );
        self.record_provenance_event(
            PROVENANCE_EVENT_FORGE_DREAM_HINTS_PERSISTED,
            &summary_text,
            serde_json::json!({
                "scope_id": scope_id,
                "persisted_count": persisted_count,
                "persisted_hints": hints,
                "hint_preview": hints.iter().take(3).cloned().collect::<Vec<_>>(),
                "source": "forge",
            }),
            None,
            None,
            None,
            None,
            None,
        )
        .await;
    }

    pub async fn provenance_report_json(&self, limit: usize) -> Result<String> {
        Ok(serde_json::to_string_pretty(
            &self.history.provenance_report(limit)?,
        )?)
    }

    pub async fn generate_soc2_artifact(&self, period_days: u32) -> Result<String> {
        Ok(self
            .history
            .generate_soc2_artifact(period_days)?
            .display()
            .to_string())
    }
}
