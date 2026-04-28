use super::*;

fn default_criteria() -> EscalationCriteria {
    EscalationCriteria::default()
}

// 1. New state starts at SelfCorrection
#[test]
fn new_state_starts_at_self_correction() {
    let state = EscalationState::new(1000);
    assert_eq!(state.current_level(), EscalationLevel::SelfCorrection);
    assert_eq!(state.attempts_at_level, 0);
    assert_eq!(state.total_escalations, 0);
    assert!(state.escalation_history.is_empty());
}

// 2. Success at current level -> no escalation
#[test]
fn success_at_current_level_no_escalation() {
    let state = EscalationState::new(1000);
    let decision = state.evaluate(&default_criteria(), true);
    assert!(!decision.should_escalate);
    assert_eq!(decision.target_level, EscalationLevel::SelfCorrection);
}

// 3. L0 fails twice -> escalate to L1
#[test]
fn l0_fails_twice_escalates_to_l1() {
    let mut state = EscalationState::new(1000);
    let criteria = default_criteria();

    // First failure — still under threshold.
    let d1 = state.evaluate(&criteria, false);
    assert!(!d1.should_escalate);
    state.apply(&d1, 1001);
    assert_eq!(state.attempts_at_level, 1);

    // Second failure — still under threshold (need >= 2 attempts recorded).
    let d2 = state.evaluate(&criteria, false);
    assert!(!d2.should_escalate);
    state.apply(&d2, 1002);
    assert_eq!(state.attempts_at_level, 2);

    // Third evaluation — now at threshold, should escalate.
    let d3 = state.evaluate(&criteria, false);
    assert!(d3.should_escalate);
    assert_eq!(d3.target_level, EscalationLevel::SubAgent);
}

// 4. L1 fails once -> escalate to L2
#[test]
fn l1_fails_once_escalates_to_l2() {
    let mut state = EscalationState::new(1000);
    state.current_level = EscalationLevel::SubAgent;

    let criteria = default_criteria(); // max_subagent_attempts = 1

    // First failure — under threshold.
    let d1 = state.evaluate(&criteria, false);
    assert!(!d1.should_escalate);
    state.apply(&d1, 1001);

    // Second evaluation — at threshold.
    let d2 = state.evaluate(&criteria, false);
    assert!(d2.should_escalate);
    assert_eq!(d2.target_level, EscalationLevel::User);
}

// 5. L2 -> escalate to L3
#[test]
fn l2_escalates_to_l3() {
    let mut state = EscalationState::new(1000);
    state.current_level = EscalationLevel::User;

    let decision = state.evaluate(&default_criteria(), false);
    assert!(decision.should_escalate);
    assert_eq!(decision.target_level, EscalationLevel::External);
}

// 6. L3 stays at L3 (no further escalation)
#[test]
fn l3_stays_at_l3() {
    let mut state = EscalationState::new(1000);
    state.current_level = EscalationLevel::External;

    let decision = state.evaluate(&default_criteria(), false);
    assert!(!decision.should_escalate);
    assert_eq!(decision.target_level, EscalationLevel::External);
}

// 7. Apply updates state correctly
#[test]
fn apply_updates_state_correctly() {
    let mut state = EscalationState::new(1000);

    // Escalation decision.
    let decision = EscalationDecision {
        should_escalate: true,
        target_level: EscalationLevel::SubAgent,
        reason: "test escalation".into(),
        message: None,
    };
    state.apply(&decision, 2000);

    assert_eq!(state.current_level(), EscalationLevel::SubAgent);
    assert_eq!(state.attempts_at_level, 0);
    assert_eq!(state.total_escalations, 1);
    assert_eq!(state.escalation_history.len(), 1);
}

// 8. Reset returns to L0
#[test]
fn reset_returns_to_l0() {
    let mut state = EscalationState::new(1000);
    state.current_level = EscalationLevel::External;
    state.attempts_at_level = 5;

    state.reset(3000);

    assert_eq!(state.current_level(), EscalationLevel::SelfCorrection);
    assert_eq!(state.attempts_at_level, 0);
    assert_eq!(state.started_at, 3000);
}

// 9. Escalation history tracks events
#[test]
fn escalation_history_tracks_events() {
    let mut state = EscalationState::new(1000);

    let d1 = EscalationDecision {
        should_escalate: true,
        target_level: EscalationLevel::SubAgent,
        reason: "first".into(),
        message: None,
    };
    state.apply(&d1, 2000);

    let d2 = EscalationDecision {
        should_escalate: true,
        target_level: EscalationLevel::User,
        reason: "second".into(),
        message: Some("help".into()),
    };
    state.apply(&d2, 3000);

    assert_eq!(state.escalation_history.len(), 2);
    assert_eq!(state.escalation_history[0].reason, "first");
    assert_eq!(state.escalation_history[0].timestamp, 2000);
    assert_eq!(state.escalation_history[1].reason, "second");
    assert_eq!(state.escalation_history[1].level, EscalationLevel::User);
}

// 10. Message for User level includes goal title
#[test]
fn message_for_user_level_includes_goal_title() {
    let msg = build_escalation_message(
        EscalationLevel::User,
        "Deploy Service",
        "Run migrations",
        "migration failed",
    );
    assert!(msg.contains("Deploy Service"));
    assert!(msg.contains("Run migrations"));
    assert!(msg.contains("migration failed"));
}

// 11. Message for External level includes details
#[test]
fn message_for_external_level_includes_details() {
    let msg = build_escalation_message(
        EscalationLevel::External,
        "Critical Pipeline",
        "Health check",
        "service unreachable",
    );
    assert!(msg.contains("Critical Pipeline"));
    assert!(msg.contains("Health check"));
    assert!(msg.contains("service unreachable"));
    assert!(msg.contains("External Notification"));
}

// 12. Total escalations counter increments
#[test]
fn total_escalations_counter_increments() {
    let mut state = EscalationState::new(1000);

    let escalate = |target: EscalationLevel, reason: &str| EscalationDecision {
        should_escalate: true,
        target_level: target,
        reason: reason.into(),
        message: None,
    };

    state.apply(&escalate(EscalationLevel::SubAgent, "a"), 2000);
    assert_eq!(state.total_escalations, 1);

    state.apply(&escalate(EscalationLevel::User, "b"), 3000);
    assert_eq!(state.total_escalations, 2);

    state.apply(&escalate(EscalationLevel::External, "c"), 4000);
    assert_eq!(state.total_escalations, 3);
}

// 13. Non-escalation apply increments attempts_at_level
#[test]
fn non_escalation_increments_attempts() {
    let mut state = EscalationState::new(1000);

    let no_escalate = EscalationDecision {
        should_escalate: false,
        target_level: EscalationLevel::SelfCorrection,
        reason: "retry".into(),
        message: None,
    };

    state.apply(&no_escalate, 2000);
    assert_eq!(state.attempts_at_level, 1);
    assert_eq!(state.total_escalations, 0);
    assert!(state.escalation_history.is_empty());

    state.apply(&no_escalate, 3000);
    assert_eq!(state.attempts_at_level, 2);
}

// 14. Default criteria has expected values
#[test]
fn default_criteria_values() {
    let c = EscalationCriteria::default();
    assert_eq!(c.max_self_correction_attempts, 2);
    assert_eq!(c.max_subagent_attempts, 1);
    assert_eq!(c.user_response_timeout_secs, 300);
}

// 15. EscalationLevel ordering
#[test]
fn escalation_level_ordering() {
    assert!(EscalationLevel::SelfCorrection < EscalationLevel::SubAgent);
    assert!(EscalationLevel::SubAgent < EscalationLevel::User);
    assert!(EscalationLevel::User < EscalationLevel::External);
}

// 16. Full escalation walkthrough L0 -> L1 -> L2 -> L3
#[test]
fn full_escalation_walkthrough() {
    let mut state = EscalationState::new(0);
    let criteria = default_criteria();

    // L0: fail twice, then escalate.
    for t in 1..=2 {
        let d = state.evaluate(&criteria, false);
        state.apply(&d, t);
    }
    let d = state.evaluate(&criteria, false);
    assert!(d.should_escalate);
    state.apply(&d, 3);
    assert_eq!(state.current_level(), EscalationLevel::SubAgent);

    // L1: fail once, then escalate.
    let d = state.evaluate(&criteria, false);
    state.apply(&d, 4);
    let d = state.evaluate(&criteria, false);
    assert!(d.should_escalate);
    state.apply(&d, 5);
    assert_eq!(state.current_level(), EscalationLevel::User);

    // L2: immediate escalation on failure.
    let d = state.evaluate(&criteria, false);
    assert!(d.should_escalate);
    state.apply(&d, 6);
    assert_eq!(state.current_level(), EscalationLevel::External);

    // L3: stays.
    let d = state.evaluate(&criteria, false);
    assert!(!d.should_escalate);
    assert_eq!(state.current_level(), EscalationLevel::External);

    assert_eq!(state.total_escalations, 3);
    assert_eq!(state.escalation_history.len(), 3);
}

// 17. EscalationLevel::as_label returns correct labels
#[test]
fn escalation_level_labels() {
    assert_eq!(EscalationLevel::SelfCorrection.as_label(), "L0");
    assert_eq!(EscalationLevel::SubAgent.as_label(), "L1");
    assert_eq!(EscalationLevel::User.as_label(), "L2");
    assert_eq!(EscalationLevel::External.as_label(), "L3");
}

// 18. escalation_audit_data produces correct simple summary
#[test]
fn escalation_audit_data_simple() {
    let data = escalation_audit_data(
        &EscalationLevel::SelfCorrection,
        &EscalationLevel::SubAgent,
        "timeout after 2 retries",
        2,
        Some("thread-1"),
        &[serde_json::json!("factor1")],
        5000,
    );
    assert!(data.audit_id.starts_with("audit-esc-"));
    assert_eq!(data.timestamp, 5000);
    assert_eq!(data.from_label, "L0");
    assert_eq!(data.to_label, "L1");
    assert!(data.summary.contains("L0"));
    assert!(data.summary.contains("L1"));
    assert!(data.summary.contains("timeout"));
    assert_eq!(data.attempts, 2);
}

// 19. escalation_audit_data with many causal factors includes count
#[test]
fn escalation_audit_data_complex() {
    let data = escalation_audit_data(
        &EscalationLevel::SubAgent,
        &EscalationLevel::User,
        "multiple failures",
        1,
        None,
        &[
            serde_json::json!("f1"),
            serde_json::json!("f2"),
            serde_json::json!("f3"),
        ],
        6000,
    );
    assert!(data.summary.contains("3 causal factors"));
    assert!(data.summary.contains("L1"));
    assert!(data.summary.contains("L2"));
}

// 20. escalation_audit_data raw_data_json is valid JSON
#[test]
fn escalation_audit_data_raw_json_valid() {
    let data = escalation_audit_data(
        &EscalationLevel::User,
        &EscalationLevel::External,
        "user timeout",
        0,
        Some("t-42"),
        &[],
        7000,
    );
    let parsed: serde_json::Value = serde_json::from_str(&data.raw_data_json).expect("valid JSON");
    assert_eq!(parsed["from_level"], "L2");
    assert_eq!(parsed["to_level"], "L3");
    assert_eq!(parsed["reason"], "user timeout");
    assert_eq!(parsed["thread_id"], "t-42");
}

// 21. cancel_escalation at L0 with no history fails
#[test]
fn cancel_escalation_no_active_fails() {
    let mut state = EscalationState::new(1000);
    let result = state.cancel_escalation(2000);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("No active escalation"));
}

// 22. cancel_escalation at active level resets to L0
#[test]
fn cancel_escalation_resets_to_l0() {
    let mut state = EscalationState::new(1000);
    // Escalate to L1 first.
    let decision = EscalationDecision {
        should_escalate: true,
        target_level: EscalationLevel::SubAgent,
        reason: "test".into(),
        message: None,
    };
    state.apply(&decision, 2000);
    assert_eq!(state.current_level(), EscalationLevel::SubAgent);

    let result = state.cancel_escalation(3000);
    assert!(result.is_ok());
    let msg = result.unwrap();
    assert!(msg.contains("L1"));
    assert!(msg.contains("cancelled"));
    assert_eq!(state.current_level(), EscalationLevel::SelfCorrection);
    assert_eq!(state.attempts_at_level, 0);
    // History should include the cancel event.
    let last = state.escalation_history.last().unwrap();
    assert_eq!(last.outcome.as_deref(), Some("cancelled_by_user"));
}

// 23. cancel_escalation race condition: already resolved back to L0
#[test]
fn cancel_escalation_already_resolved() {
    let mut state = EscalationState::new(1000);
    // Escalate then reset (simulating resolution).
    let decision = EscalationDecision {
        should_escalate: true,
        target_level: EscalationLevel::SubAgent,
        reason: "test".into(),
        message: None,
    };
    state.apply(&decision, 2000);
    state.current_level = EscalationLevel::SelfCorrection;
    state.attempts_at_level = 0;

    let result = state.cancel_escalation(3000);
    assert!(result.is_ok());
    assert!(result.unwrap().contains("already resolved"));
}

// 24. cancel_escalation at L2 includes correct label
#[test]
fn cancel_escalation_at_l2() {
    let mut state = EscalationState::new(1000);
    state.current_level = EscalationLevel::User;
    state.total_escalations = 2;

    let result = state.cancel_escalation(2000);
    assert!(result.is_ok());
    let msg = result.unwrap();
    assert!(msg.contains("L2"));
    assert_eq!(state.current_level(), EscalationLevel::SelfCorrection);
}
