use super::*;
use crate::agent::metacognitive::escalation::{EscalationAuditData, EscalationLevel};
use zorai_protocol::InboxNotification;

impl AgentEngine {
    pub(super) async fn upsert_inbox_notification(
        &self,
        notification: InboxNotification,
    ) -> Result<()> {
        self.history.upsert_notification(&notification).await?;
        let notification = self
            .history
            .get_notification_by_id(&notification.id)
            .await?
            .unwrap_or(notification);
        let _ = self
            .event_tx
            .send(AgentEvent::NotificationInboxUpsert { notification });
        Ok(())
    }

    /// Emit a critical inbox notification when an escalation reaches the
    /// external (L3) tier. Before this dispatcher was wired the L3 stage was
    /// dead surface — the escalation state machine could reach `External` but
    /// nothing operator-visible fired. This bridges the dead branch onto the
    /// same inbox-notification path used by other operator-facing escalations
    /// (e.g. quiet-goal recovery's pause notification).
    pub(super) async fn dispatch_external_escalation_notification(
        &self,
        thread_id: &str,
        audit: &EscalationAuditData,
    ) -> Result<()> {
        let notification = build_external_escalation_notification(thread_id, audit);
        self.upsert_inbox_notification(notification).await
    }

    /// Heartbeat-level sweep: find awaiting-approval tasks whose deadline has
    /// passed and emit external (L3) escalation notifications for each. This
    /// is the production producer that finally makes the L3 stage reachable
    /// from real work (previously only tests constructed L3 audits).
    ///
    /// Idempotency: each task gets a deterministic `audit_id` of
    /// `escalation-timeout-{task_id}`, which threads through into the
    /// notification ID. Re-running the sweep on subsequent heartbeat ticks
    /// upserts the same notification row instead of stacking up duplicates;
    /// the operator sees one critical entry per stuck approval, with a
    /// refreshed `updated_at`.
    ///
    /// Returns the number of tasks for which an L3 notification was emitted
    /// (zero is the normal-path return).
    pub(super) async fn dispatch_stale_approval_escalations_to_external(&self) -> Result<usize> {
        let now = now_millis();
        let stale = self.history.list_tasks_past_approval_deadline(now).await?;
        if stale.is_empty() {
            return Ok(0);
        }
        let mut dispatched = 0;
        for (task_id, thread_id, approval_id, expires_at) in stale {
            let Some(thread_id) = thread_id else {
                // No thread → no place to surface the inbox-action link;
                // skip rather than create an orphan notification.
                continue;
            };
            let overdue_ms = now.saturating_sub(expires_at);
            let audit = stale_approval_external_audit(
                &task_id,
                &thread_id,
                &approval_id,
                expires_at,
                overdue_ms,
                now,
            );
            if let Err(error) = self
                .dispatch_external_escalation_notification(&thread_id, &audit)
                .await
            {
                tracing::warn!(
                    %task_id,
                    %error,
                    "failed to dispatch stale-approval external escalation",
                );
                continue;
            }
            dispatched += 1;
        }
        Ok(dispatched)
    }
}

/// Build an `EscalationAuditData` for a stale-approval L2→L3 transition.
/// Factored out so a heartbeat tick can be unit-tested without bringing up an
/// `AgentEngine` or hitting SQLite — the test can feed the function fixed
/// input and assert the audit payload it produces.
pub(super) fn stale_approval_external_audit(
    task_id: &str,
    thread_id: &str,
    approval_id: &str,
    expires_at: u64,
    overdue_ms: u64,
    now_ms: u64,
) -> EscalationAuditData {
    let overdue_secs = overdue_ms / 1000;
    let raw_data = serde_json::json!({
        "trigger": "operator_response_timeout",
        "task_id": task_id,
        "thread_id": thread_id,
        "approval_id": approval_id,
        "approval_expires_at_ms": expires_at,
        "overdue_ms": overdue_ms,
        "checked_at_ms": now_ms,
    });
    EscalationAuditData {
        // Deterministic so the inbox notification ID stays stable across
        // heartbeat ticks → upserts not duplicates.
        audit_id: format!("escalation-timeout-{task_id}"),
        timestamp: now_ms,
        summary: format!(
            "Approval for task {task_id} on thread {thread_id} has been awaiting operator response for {overdue_secs}s past its deadline"
        ),
        from_label: EscalationLevel::User.as_label().to_string(),
        to_label: EscalationLevel::External.as_label().to_string(),
        reason: format!(
            "operator did not respond to approval {approval_id} within the response window ({overdue_secs}s overdue)"
        ),
        attempts: 1,
        raw_data_json: raw_data.to_string(),
    }
}

/// Construct an `InboxNotification` for an L3 (External) escalation audit.
/// Factored out so it can be unit-tested without bringing up an `AgentEngine`.
pub(super) fn build_external_escalation_notification(
    thread_id: &str,
    audit: &EscalationAuditData,
) -> InboxNotification {
    let timestamp = audit.timestamp as i64;
    InboxNotification {
        id: format!("escalation-external:{}", audit.audit_id),
        source: "metacognitive_escalation".to_string(),
        kind: "external_escalation".to_string(),
        title: "External escalation requested".to_string(),
        body: format!(
            "Goal escalation has reached the external ({}) tier after {} attempts at {}. \
             Reason: {}\n\nThis goal cannot proceed without intervention beyond what the agent or operator approval flow can provide.",
            EscalationLevel::External.as_label(),
            audit.attempts,
            audit.from_label,
            audit.reason,
        ),
        subtitle: Some(audit.summary.clone()),
        severity: "critical".to_string(),
        created_at: timestamp,
        updated_at: timestamp,
        read_at: None,
        archived_at: None,
        deleted_at: None,
        actions: vec![crate::notifications::open_thread_action(thread_id)],
        metadata_json: Some(audit.raw_data_json.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_external_audit() -> EscalationAuditData {
        EscalationAuditData {
            audit_id: "audit-test-1".to_string(),
            timestamp: 1_700_000_000_000,
            summary: "Escalating from L2 to L3 after operator timeout".to_string(),
            from_label: "L2".to_string(),
            to_label: EscalationLevel::External.as_label().to_string(),
            reason: "operator did not respond within 300s".to_string(),
            attempts: 3,
            raw_data_json: r#"{"trigger":"timeout"}"#.to_string(),
        }
    }

    #[test]
    fn build_external_notification_uses_critical_severity_and_canonical_label() {
        let audit = sample_external_audit();
        let notification = build_external_escalation_notification("thread-7", &audit);
        assert_eq!(notification.severity, "critical");
        assert_eq!(notification.kind, "external_escalation");
        assert_eq!(notification.source, "metacognitive_escalation");
        assert!(notification.id.contains(&audit.audit_id));
        // Body should mention the canonical level label (L3), not just prose.
        assert!(notification.body.contains("L3"));
        // Subtitle echoes the audit summary so the operator can see the chain.
        assert_eq!(
            notification.subtitle.as_deref(),
            Some(audit.summary.as_str())
        );
    }

    #[test]
    fn build_external_notification_attaches_open_thread_action() {
        let audit = sample_external_audit();
        let notification = build_external_escalation_notification("thread-42", &audit);
        // Operator should be able to navigate to the originating thread from
        // the inbox entry; the action surface is what makes the notification
        // operator-actionable rather than a static alert.
        assert!(
            !notification.actions.is_empty(),
            "external escalation notification should expose at least one action"
        );
    }

    #[test]
    fn build_external_notification_preserves_raw_audit_metadata() {
        let audit = sample_external_audit();
        let notification = build_external_escalation_notification("thread-1", &audit);
        // The audit's raw_data_json flows through into metadata so an audit
        // trail consumer (or a re-display) can recover the original escalation
        // payload without re-querying the audit table.
        assert_eq!(
            notification.metadata_json.as_deref(),
            Some(audit.raw_data_json.as_str())
        );
    }

    #[test]
    fn stale_approval_audit_uses_deterministic_id_for_upsert_idempotency() {
        // Two successive heartbeat sweeps over the same stuck task must
        // produce the *same* audit_id so that downstream notification
        // upserts overwrite the existing row instead of stacking duplicates.
        let first = stale_approval_external_audit(
            "task-7",
            "thread-7",
            "approval-7",
            1_700_000_000_000,
            5_000,
            1_700_000_005_000,
        );
        let second = stale_approval_external_audit(
            "task-7",
            "thread-7",
            "approval-7",
            1_700_000_000_000,
            10_000,
            1_700_000_010_000,
        );
        assert_eq!(first.audit_id, second.audit_id);
        assert!(first.audit_id.contains("task-7"));
        // The notification ID is derived from audit_id, so deterministic
        // audit_id implies deterministic notification_id.
        let first_notification = build_external_escalation_notification("thread-7", &first);
        let second_notification = build_external_escalation_notification("thread-7", &second);
        assert_eq!(first_notification.id, second_notification.id);
    }

    #[test]
    fn stale_approval_audit_records_l2_to_l3_transition_and_overdue_seconds() {
        let audit = stale_approval_external_audit(
            "task-x",
            "thread-x",
            "approval-x",
            1_700_000_000_000,
            7_500,
            1_700_000_007_500,
        );
        assert_eq!(audit.from_label, "L2");
        assert_eq!(audit.to_label, "L3");
        // Summary surfaces overdue in seconds (7500ms → 7s) so an operator
        // skimming the notification immediately sees how long it's been
        // overdue, not a millisecond count.
        assert!(
            audit.summary.contains("7s past"),
            "summary should report overdue seconds, got: {}",
            audit.summary
        );
        assert!(audit.reason.contains("approval-x"));
    }

    #[test]
    fn stale_approval_audit_embeds_full_context_in_raw_data() {
        let audit = stale_approval_external_audit(
            "task-y",
            "thread-y",
            "approval-y",
            1_700_000_000_000,
            12_000,
            1_700_000_012_000,
        );
        // The raw_data_json should carry the full set of fields a future
        // audit consumer might want (trigger, task/thread/approval ids,
        // deadlines, timestamps) so the operator-facing summary stays
        // concise while the persistent audit retains the full payload.
        let parsed: serde_json::Value =
            serde_json::from_str(&audit.raw_data_json).expect("raw_data_json should be valid JSON");
        assert_eq!(parsed["trigger"], "operator_response_timeout");
        assert_eq!(parsed["task_id"], "task-y");
        assert_eq!(parsed["thread_id"], "thread-y");
        assert_eq!(parsed["approval_id"], "approval-y");
        assert_eq!(parsed["overdue_ms"], 12_000);
    }
}
