use super::super::*;
use super::common::*;

#[test]
fn policy_eval_prompt_builder_includes_recent_context_sections() {
    let prompt = build_policy_eval_prompt(&policy_eval_context());

    assert!(prompt.contains("Recent tool outcomes"));
    assert!(prompt.contains("read_file => success: Read the config but found no obvious mismatch."));
    assert!(
        prompt.contains("bash => failure: Retrying the same test command still exits with code 1.")
    );
    assert!(prompt.contains("Awareness summary"));
    assert!(prompt.contains("Continuity summary"));
    assert!(prompt.contains("Counter-who context"));
    assert!(prompt.contains("Ruled-out approaches"));
    assert!(prompt.contains("Self-assessment summary"));
    assert!(prompt.contains("Thread context"));
    assert!(prompt.contains("Recent policy decision summary"));
    assert!(prompt.contains("thread-9"));
    assert!(prompt.contains("goal-9"));
    assert!(!prompt.contains("\"retry_guard\""));
    assert!(prompt.contains("Do not return `retry_guard`"));
}

#[test]
fn policy_eval_prompt_caps_rendered_tool_outcomes() {
    let mut context = policy_eval_context();
    context.recent_tool_outcomes = (0..8)
        .map(|index| PolicyToolOutcomeSummary {
            tool_name: format!("tool-{index}"),
            outcome: "failure".to_string(),
            summary: format!("summary-{index}"),
        })
        .collect();

    let prompt = build_policy_eval_prompt(&context);

    assert!(prompt.contains("tool-0 => failure: summary-0"));
    assert!(prompt.contains("tool-1 => failure: summary-1"));
    assert!(prompt.contains("tool-2 => failure: summary-2"));
    assert!(prompt.contains("tool-3 => failure: summary-3"));
    assert!(!prompt.contains("tool-4 => failure: summary-4"));
    assert!(prompt.contains("- ... 4 additional tool outcomes omitted"));
}

#[test]
fn policy_eval_prompt_normalizes_and_truncates_free_form_fields() {
    let mut context = policy_eval_context();
    context.recent_tool_outcomes = vec![PolicyToolOutcomeSummary {
        tool_name: "bash\nscript".to_string(),
        outcome: "failure\nretry".to_string(),
        summary: format!("{}\n{}", "very long summary ".repeat(20), "final line"),
    }];
    context.awareness_summary = Some(format!(
        "line one\nline two\n{}",
        "extra context ".repeat(30),
    ));
    context.continuity_summary = Some("  carry\n\nforward  ".to_string());
    context.counter_who_context = Some("  first line\n\nsecond line  ".to_string());
    context.negative_constraints_context = Some(" ruled\n\nout ".to_string());
    context.self_assessment_summary = Some("alpha\nbeta\ngamma".to_string());
    context.thread_context = Some(" operator request\nwith details ".to_string());
    context.recent_decision_summary = Some(format!("{}", "decision ".repeat(40)));

    let prompt = build_policy_eval_prompt(&context);

    assert!(!prompt.contains("bash\nscript"));
    assert!(prompt.contains("bash script => failure retry:"));
    assert!(!prompt.contains("line one\nline two"));
    assert!(prompt.contains("line one line two"));
    assert!(prompt.contains("carry forward"));
    assert!(prompt.contains("first line second line"));
    assert!(prompt.contains("ruled out"));
    assert!(prompt.contains("alpha beta gamma"));
    assert!(prompt.contains("operator request with details"));
    assert!(prompt.contains("..."));
}

#[test]
fn policy_eval_prompt_keeps_required_sections_after_normalization() {
    let mut context = policy_eval_context();
    context.recent_tool_outcomes.clear();
    context.awareness_summary = Some("\n\n".to_string());
    context.continuity_summary = Some("continuity\nsummary".to_string());
    context.counter_who_context = Some("counter\nwho".to_string());
    context.negative_constraints_context = Some("negative\nconstraints".to_string());
    context.self_assessment_summary = Some("self\nassessment".to_string());
    context.thread_context = Some("thread\ncontext".to_string());
    context.recent_decision_summary = Some("recent\ndecision".to_string());

    let prompt = build_policy_eval_prompt(&context);

    assert!(prompt.contains("## Trigger context"));
    assert!(prompt.contains("## Recent tool outcomes\n- none"));
    assert!(prompt.contains("## Awareness summary\nnone"));
    assert!(prompt.contains("## Continuity summary\ncontinuity summary"));
    assert!(prompt.contains("## Counter-who context\ncounter who"));
    assert!(prompt.contains("## Ruled-out approaches\nnegative constraints"));
    assert!(prompt.contains("## Self-assessment summary\nself assessment"));
    assert!(prompt.contains("## Thread context\nthread context"));
    assert!(prompt.contains("## Recent policy decision summary\nrecent decision"));
}

#[test]
fn policy_eval_invalid_structured_output_falls_back_safely() {
    let invalid = PolicyDecision {
        action: PolicyAction::HaltRetries,
        reason: "Stop repeating the same failing path.".to_string(),
        strategy_hint: Some("Try a different recovery path.".to_string()),
        retry_guard: None,
    };

    assert_eq!(
        normalize_policy_eval_decision(Some(invalid)),
        PolicyDecision {
            action: PolicyAction::Continue,
            reason: "Policy evaluation returned an invalid decision; continuing current execution."
                .to_string(),
            strategy_hint: None,
            retry_guard: None,
        },
    );
}

#[test]
fn policy_eval_missing_result_degrades_to_continue() {
    assert_eq!(
        normalize_policy_eval_decision(None),
        PolicyDecision {
            action: PolicyAction::Continue,
            reason: "Policy evaluation unavailable; continuing current execution.".to_string(),
            strategy_hint: None,
            retry_guard: None,
        },
    );
}

#[test]
fn policy_eval_runtime_owns_halt_retry_guard_and_ignores_hallucinated_value() {
    let evaluated = runtime_owns_policy_retry_guard(
        PolicyDecision {
            action: PolicyAction::HaltRetries,
            reason: "Stop retrying the same failing path.".to_string(),
            strategy_hint: None,
            retry_guard: Some("hallucinated-guard".to_string()),
        },
        Some("approach-hash-1"),
    );

    assert_eq!(evaluated.action, PolicyAction::HaltRetries);
    assert_eq!(evaluated.retry_guard.as_deref(), Some("approach-hash-1"));
}

#[test]
fn policy_eval_runtime_drops_retry_guard_for_non_guarded_decisions() {
    let evaluated = runtime_owns_policy_retry_guard(
        PolicyDecision {
            action: PolicyAction::Pivot,
            reason: "Try a different bounded strategy.".to_string(),
            strategy_hint: Some("Inspect state before retrying.".to_string()),
            retry_guard: Some("hallucinated-guard".to_string()),
        },
        Some("approach-hash-1"),
    );

    assert_eq!(evaluated.action, PolicyAction::Pivot);
    assert_eq!(evaluated.retry_guard, None);
    assert_eq!(
        evaluated.strategy_hint.as_deref(),
        Some("Inspect state before retrying.")
    );
}

#[test]
fn policy_eval_halt_retries_without_live_runtime_guard_degrades_to_continue() {
    let evaluated = runtime_owns_policy_retry_guard(
        PolicyDecision {
            action: PolicyAction::HaltRetries,
            reason: "Stop retrying the same failing path.".to_string(),
            strategy_hint: None,
            retry_guard: Some("hallucinated-guard".to_string()),
        },
        None,
    );

    assert_eq!(evaluated.action, PolicyAction::Continue);
    assert_eq!(evaluated.retry_guard, None);
    assert!(evaluated.reason.contains("without a live retry guard"));
}
