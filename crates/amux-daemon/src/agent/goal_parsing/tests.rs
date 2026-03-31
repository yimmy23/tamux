use super::*;

#[test]
fn parse_json_block_preserves_optional_llm_confidence_fields() {
    let json = r#"
    {
      "title": "Test Plan",
      "summary": "Testing confidence field parsing",
      "steps": [
        {
          "title": "Step 1",
          "instructions": "Do thing",
          "kind": "command",
          "success_criteria": "Thing done",
          "session_id": null,
          "llm_confidence": "0.82",
          "llm_confidence_rationale": "high confidence due to deterministic flow"
        }
      ]
    }
    "#;

    let parsed: GoalPlanResponse = parse_json_block(json).expect("json should parse");
    assert_eq!(parsed.steps.len(), 1);

    let step = &parsed.steps[0];
    assert_eq!(step.llm_confidence.as_deref(), Some("0.82"));
    assert_eq!(
        step.llm_confidence_rationale.as_deref(),
        Some("high confidence due to deterministic flow")
    );
}

#[test]
fn goal_plan_response_without_rejected_alternatives_defaults_empty() {
    let json = r#"
    {
      "title": "Plan",
      "summary": "No rejected alternatives field",
      "steps": [
        {
          "title": "Step 1",
          "instructions": "Do thing",
          "kind": "command",
          "success_criteria": "Thing done",
          "session_id": null
        }
      ]
    }
    "#;

    let parsed: GoalPlanResponse = parse_json_block(json).expect("json should parse");
    assert!(parsed.rejected_alternatives.is_empty());
}

#[test]
fn goal_plan_response_with_rejected_alternatives_round_trips() {
    let response = GoalPlanResponse {
        title: Some("Roundtrip Plan".to_string()),
        summary: "Verify rejected alternatives serde".to_string(),
        steps: vec![GoalPlanStepResponse {
            title: "Step 1".to_string(),
            instructions: "Do work".to_string(),
            kind: GoalRunStepKind::Command,
            success_criteria: "Done".to_string(),
            session_id: None,
            llm_confidence: Some("0.5".to_string()),
            llm_confidence_rationale: Some("moderate confidence".to_string()),
        }],
        rejected_alternatives: vec![
            "Alternative A: too risky".to_string(),
            "Alternative B: too slow".to_string(),
        ],
    };

    let encoded = serde_json::to_string(&response).expect("serialize");
    let decoded: GoalPlanResponse = serde_json::from_str(&encoded).expect("deserialize");
    assert_eq!(decoded.rejected_alternatives.len(), 2);
    assert_eq!(decoded.rejected_alternatives[0], "Alternative A: too risky");
    assert_eq!(decoded.rejected_alternatives[1], "Alternative B: too slow");
}
