//! Helper for emitting `governance_evaluations` audit rows at transition sites
//! that don't run through the full `evaluate_governance` engine (i.e. anything
//! other than managed-command dispatch and approval reuse).
//!
//! The goal is to make every `TransitionKind` variant a real, queryable event
//! in the audit log rather than a dead enum variant.

use anyhow::Result;
use serde_json::json;

use crate::history::{GovernanceEvaluationRow, HistoryStore};

use super::TransitionKind;

/// Identifiers attached to a transition audit row. All fields are optional so
/// the helper can be called from sites that only know a subset (e.g. a session
/// spawn has no goal_run_id; a goal-run terminal disposition has no thread_id
/// at the call boundary).
#[derive(Debug, Clone, Default)]
pub(crate) struct TransitionAuditIds {
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub thread_id: Option<String>,
}

/// Insert a single `governance_evaluations` row recording that `transition_kind`
/// happened. `detail` is a free-form JSON object captured into the row's
/// `input_json`; `outcome` is captured into `verdict_json`. Failures are logged
/// but never propagated — audit emission is best-effort and must not break the
/// production code path that called it.
pub(crate) async fn record_transition_audit(
    history: &HistoryStore,
    transition_kind: TransitionKind,
    ids: TransitionAuditIds,
    detail: serde_json::Value,
    outcome: &str,
) {
    let row = GovernanceEvaluationRow {
        id: format!("gov_{}", uuid::Uuid::new_v4()),
        run_id: ids.run_id,
        task_id: ids.task_id,
        goal_run_id: ids.goal_run_id,
        thread_id: ids.thread_id,
        transition_kind: transition_kind.as_str().to_string(),
        input_json: detail.to_string(),
        verdict_json: json!({
            "outcome": outcome,
            "recorded_at": crate::history::now_ts(),
        })
        .to_string(),
        policy_fingerprint: String::new(),
        created_at: crate::history::now_ts(),
    };
    if let Err(error) = history.insert_governance_evaluation(&row).await {
        tracing::warn!(
            transition_kind = transition_kind.as_str(),
            error = %error,
            "failed to record governance transition audit row",
        );
    }
}

/// Helper: pull a `Result<(bool, String)>` snapshot-restore outcome into a
/// compact outcome string for `CompensationEntry` rows.
pub(crate) fn snapshot_restore_outcome(result: &Result<(bool, String)>) -> String {
    match result {
        Ok((true, message)) => format!("restored: {message}"),
        Ok((false, message)) => format!("restore declined: {message}"),
        Err(error) => format!("restore failed: {error}"),
    }
}
