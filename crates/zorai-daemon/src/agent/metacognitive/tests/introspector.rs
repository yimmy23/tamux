use crate::agent::metacognitive::introspector::{
    introspect, InterventionStrength, IntrospectionInput, RecentToolOutcome,
};
use crate::agent::metacognitive::types::SelfModel;

fn input_with_recent_failures(tool: &str, failures: usize) -> IntrospectionInput {
    IntrospectionInput {
        proposed_tool_name: tool.to_string(),
        proposed_tool_arguments: "{}".to_string(),
        normalized_tool_signature: format!("{tool}:{{}}"),
        predicted_repeat_count: 1,
        recent_tool_outcomes: (0..failures)
            .map(|index| RecentToolOutcome {
                tool_name: tool.to_string(),
                outcome: "failure".to_string(),
                summary: format!("Error: unknown operation id attempt {index}"),
            })
            .collect(),
        task_retry_count: 0,
        decision_reasoning: None,
    }
}

#[test]
fn sunk_cost_failure_loop_ignores_tools_outside_bias_scope() {
    let model = SelfModel::default();
    let outcome = introspect(
        &model,
        &input_with_recent_failures("get_operation_status", 4),
    );

    assert!(
        outcome
            .signals
            .iter()
            .all(|signal| signal.bias_name != "sunk_cost"),
        "sunk_cost is scoped to bash_command; repeated status-poll failures (e.g. mistyped operation ids) must not lock out unrelated tools: {:?}",
        outcome.signals
    );
    assert_eq!(outcome.strength, InterventionStrength::None);
}

#[test]
fn sunk_cost_failure_loop_blocks_tool_within_bias_scope() {
    let model = SelfModel::default();
    let outcome = introspect(&model, &input_with_recent_failures("bash_command", 4));

    assert!(
        outcome
            .signals
            .iter()
            .any(|signal| signal.bias_name == "sunk_cost"),
        "repeated failures of a tool in the bias tool_sequence should still trip the sunk-cost loop"
    );
    assert_eq!(outcome.strength, InterventionStrength::Block);
}
