use super::*;


fn metrics(
    elapsed_secs: u64,
    total: u32,
    succeeded: u32,
    consecutive_err: u32,
    total_err: u32,
) -> TerminationMetrics {
    TerminationMetrics {
        elapsed_secs,
        tool_calls_total: total,
        tool_calls_succeeded: succeeded,
        consecutive_errors: consecutive_err,
        total_errors: total_err,
    }
}


#[test]
fn parse_timeout() {
    let ev = TerminationEvaluator::parse("timeout(300)").unwrap();
    assert_eq!(ev.condition(), &TerminationCondition::Timeout(300));
}

#[test]
fn parse_tool_success_count() {
    let ev = TerminationEvaluator::parse("tool_success_count(5)").unwrap();
    assert_eq!(ev.condition(), &TerminationCondition::ToolSuccessCount(5));
}

#[test]
fn parse_error_count() {
    let ev = TerminationEvaluator::parse("error_count(3)").unwrap();
    assert_eq!(ev.condition(), &TerminationCondition::ErrorCount(3));
}

#[test]
fn parse_tool_call_count() {
    let ev = TerminationEvaluator::parse("tool_call_count(50)").unwrap();
    assert_eq!(ev.condition(), &TerminationCondition::ToolCallCount(50));
}


#[test]
fn timeout_fires_when_elapsed() {
    let ev = TerminationEvaluator::default_timeout(60);
    let (stop, reason) = ev.should_terminate(&metrics(60, 0, 0, 0, 0));
    assert!(stop);
    assert!(reason.unwrap().contains("timeout"));
}

#[test]
fn timeout_does_not_fire_before_elapsed() {
    let ev = TerminationEvaluator::default_timeout(60);
    let (stop, reason) = ev.should_terminate(&metrics(59, 0, 0, 0, 0));
    assert!(!stop);
    assert!(reason.is_none());
}

#[test]
fn tool_success_count_fires() {
    let ev = TerminationEvaluator::parse("tool_success_count(3)").unwrap();
    let (stop, _) = ev.should_terminate(&metrics(0, 5, 3, 0, 0));
    assert!(stop);
}

#[test]
fn tool_success_count_does_not_fire() {
    let ev = TerminationEvaluator::parse("tool_success_count(3)").unwrap();
    let (stop, _) = ev.should_terminate(&metrics(0, 5, 2, 0, 0));
    assert!(!stop);
}

#[test]
fn error_count_fires() {
    let ev = TerminationEvaluator::parse("error_count(3)").unwrap();
    let (stop, reason) = ev.should_terminate(&metrics(0, 10, 7, 3, 5));
    assert!(stop);
    assert!(reason.unwrap().contains("consecutive errors"));
}

#[test]
fn tool_call_count_fires() {
    let ev = TerminationEvaluator::parse("tool_call_count(10)").unwrap();
    let (stop, _) = ev.should_terminate(&metrics(0, 10, 8, 0, 2));
    assert!(stop);
}


#[test]
fn or_fires_on_left() {
    let ev = TerminationEvaluator::parse("timeout(300) OR error_count(3)").unwrap();
    let (stop, reason) = ev.should_terminate(&metrics(300, 0, 0, 0, 0));
    assert!(stop);
    assert!(reason.unwrap().contains("timeout"));
}

#[test]
fn or_fires_on_right() {
    let ev = TerminationEvaluator::parse("timeout(300) OR error_count(3)").unwrap();
    let (stop, reason) = ev.should_terminate(&metrics(10, 0, 0, 3, 3));
    assert!(stop);
    assert!(reason.unwrap().contains("error_count"));
}

#[test]
fn or_does_not_fire_when_neither() {
    let ev = TerminationEvaluator::parse("timeout(300) OR error_count(3)").unwrap();
    let (stop, _) = ev.should_terminate(&metrics(100, 0, 0, 2, 2));
    assert!(!stop);
}


#[test]
fn and_fires_when_both() {
    let ev = TerminationEvaluator::parse("timeout(60) AND tool_call_count(5)").unwrap();
    let (stop, _) = ev.should_terminate(&metrics(60, 5, 3, 0, 0));
    assert!(stop);
}

#[test]
fn and_does_not_fire_when_only_one() {
    let ev = TerminationEvaluator::parse("timeout(60) AND tool_call_count(5)").unwrap();
    let (stop, _) = ev.should_terminate(&metrics(60, 4, 3, 0, 0));
    assert!(!stop);
}


#[test]
fn not_inverts_false_to_true() {
    let ev = TerminationEvaluator::parse("NOT timeout(300)").unwrap();
    let (stop, reason) = ev.should_terminate(&metrics(100, 0, 0, 0, 0));
    assert!(stop);
    assert!(reason.unwrap().contains("NOT"));
}

#[test]
fn not_inverts_true_to_false() {
    let ev = TerminationEvaluator::parse("NOT timeout(300)").unwrap();
    let (stop, _) = ev.should_terminate(&metrics(300, 0, 0, 0, 0));
    assert!(!stop);
}


#[test]
fn nested_and_or() {
    let ev =
        TerminationEvaluator::parse("(timeout(300) AND tool_call_count(50)) OR error_count(3)")
            .unwrap();

    let (stop, _) = ev.should_terminate(&metrics(10, 2, 2, 3, 3));
    assert!(stop);

    let (stop, _) = ev.should_terminate(&metrics(300, 50, 40, 0, 0));
    assert!(stop);

    let (stop, _) = ev.should_terminate(&metrics(300, 49, 40, 2, 2));
    assert!(!stop);
}

#[test]
fn double_not() {
    let ev = TerminationEvaluator::parse("NOT NOT timeout(60)").unwrap();
    let (stop, _) = ev.should_terminate(&metrics(60, 0, 0, 0, 0));
    assert!(stop);
    let (stop, _) = ev.should_terminate(&metrics(59, 0, 0, 0, 0));
    assert!(!stop);
}


#[test]
fn error_empty_string() {
    let result = TerminationEvaluator::parse("");
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("empty"));
}

#[test]
fn error_whitespace_only() {
    let result = TerminationEvaluator::parse("   ");
    assert!(result.is_err());
}

#[test]
fn error_unknown_condition() {
    let result = TerminationEvaluator::parse("foobar(42)");
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("unknown condition"));
}

#[test]
fn error_missing_paren() {
    let result = TerminationEvaluator::parse("timeout 300");
    assert!(result.is_err());
}

#[test]
fn error_unclosed_paren() {
    let result = TerminationEvaluator::parse("(timeout(300)");
    assert!(result.is_err());
}

#[test]
fn error_invalid_number() {
    let result = TerminationEvaluator::parse("timeout(abc)");
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("invalid number"));
}

#[test]
fn error_trailing_tokens() {
    let result = TerminationEvaluator::parse("timeout(60) timeout(120)");
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("trailing"));
}


#[test]
fn zero_timeout_fires_immediately() {
    let ev = TerminationEvaluator::parse("timeout(0)").unwrap();
    let (stop, _) = ev.should_terminate(&metrics(0, 0, 0, 0, 0));
    assert!(stop);
}

#[test]
fn zero_error_count_fires_immediately() {
    let ev = TerminationEvaluator::parse("error_count(0)").unwrap();
    let (stop, _) = ev.should_terminate(&metrics(0, 0, 0, 0, 0));
    assert!(stop);
}

#[test]
fn default_timeout_constructor() {
    let ev = TerminationEvaluator::default_timeout(120);
    assert_eq!(ev.condition(), &TerminationCondition::Timeout(120));
    let (stop, _) = ev.should_terminate(&metrics(119, 0, 0, 0, 0));
    assert!(!stop);
    let (stop, _) = ev.should_terminate(&metrics(120, 0, 0, 0, 0));
    assert!(stop);
}

#[test]
fn or_both_sides_fire_reports_both() {
    let ev = TerminationEvaluator::parse("timeout(60) OR error_count(3)").unwrap();
    let (stop, reason) = ev.should_terminate(&metrics(60, 0, 0, 3, 3));
    assert!(stop);
    let reason = reason.unwrap();
    assert!(reason.contains("timeout"));
    assert!(reason.contains("error_count"));
}

#[test]
fn deeply_nested_parens() {
    let ev = TerminationEvaluator::parse(
        "((timeout(300) AND tool_call_count(50)) OR (error_count(3) AND tool_success_count(1)))",
    )
    .unwrap();
    let (stop, _) = ev.should_terminate(&metrics(10, 5, 1, 3, 3));
    assert!(stop);
    let (stop, _) = ev.should_terminate(&metrics(10, 5, 0, 3, 3));
    assert!(!stop);
}

#[test]
fn large_timeout_value() {
    let ev = TerminationEvaluator::parse("timeout(999999999)").unwrap();
    assert_eq!(ev.condition(), &TerminationCondition::Timeout(999_999_999));
}

#[test]
fn chained_or_three_conditions() {
    let ev = TerminationEvaluator::parse("timeout(300) OR error_count(5) OR tool_call_count(100)")
        .unwrap();
    let (stop, _) = ev.should_terminate(&metrics(10, 100, 80, 2, 4));
    assert!(stop);
}

#[test]
fn chained_and_three_conditions() {
    let ev = TerminationEvaluator::parse("timeout(60) AND error_count(1) AND tool_call_count(10)")
        .unwrap();
    let (stop, _) = ev.should_terminate(&metrics(60, 10, 5, 1, 1));
    assert!(stop);
    let (stop, _) = ev.should_terminate(&metrics(60, 9, 5, 1, 1));
    assert!(!stop);
}

#[test]
fn not_with_and() {
    let ev = TerminationEvaluator::parse("NOT (timeout(300) AND error_count(3))").unwrap();
    let (stop, _) = ev.should_terminate(&metrics(300, 0, 0, 3, 3));
    assert!(!stop);
    let (stop, _) = ev.should_terminate(&metrics(100, 0, 0, 3, 3));
    assert!(stop);
}
