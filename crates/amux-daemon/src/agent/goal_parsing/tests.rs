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

#[test]
fn apply_plan_defaults_truncates_and_normalizes_plan_fields() {
    let mut plan = GoalPlanResponse {
        title: Some("  Compact Plan  ".to_string()),
        summary: "   ".to_string(),
        steps: (0..8)
            .map(|index| GoalPlanStepResponse {
                title: if index == 0 {
                    "   ".to_string()
                } else {
                    format!(" Step {} ", index + 1)
                },
                instructions: if index == 0 {
                    "   ".to_string()
                } else {
                    format!(" Do {} ", index + 1)
                },
                kind: if index == 0 {
                    GoalRunStepKind::Unknown
                } else {
                    GoalRunStepKind::Command
                },
                success_criteria: if index == 0 {
                    "   ".to_string()
                } else {
                    format!(" Done {} ", index + 1)
                },
                session_id: Some("  session-1  ".to_string()),
                llm_confidence: Some("  LIKELY  ".to_string()),
                llm_confidence_rationale: Some("  deterministic fix path  ".to_string()),
            })
            .collect(),
        rejected_alternatives: vec![
            "A".to_string(),
            "B".to_string(),
            "C".to_string(),
            "D".to_string(),
        ],
    };

    apply_plan_defaults(&mut plan);

    assert_eq!(plan.summary, "Compact Plan");
    assert_eq!(
        plan.steps.len(),
        SatisfactionAdaptationMode::Normal.max_goal_plan_steps()
    );
    assert_eq!(
        plan.rejected_alternatives.len(),
        SatisfactionAdaptationMode::Normal.max_rejected_alternatives()
    );
    assert_eq!(plan.steps[0].title, "Step 1");
    assert_eq!(plan.steps[0].instructions, "Step 1");
    assert_eq!(
        plan.steps[0].success_criteria,
        "Step completed successfully"
    );
    assert_eq!(plan.steps[0].kind, GoalRunStepKind::Command);
    assert_eq!(plan.steps[0].session_id.as_deref(), Some("session-1"));
    assert_eq!(plan.steps[0].llm_confidence.as_deref(), Some("likely"));
    assert_eq!(
        plan.steps[0].llm_confidence_rationale.as_deref(),
        Some("deterministic fix path")
    );
}

#[test]
fn goal_plan_json_accepts_debate_step_kind() {
    let json = r#"
    {
      "title": "Debate Plan",
      "summary": "Use debate for a contentious decision",
      "steps": [
        {
          "title": "Debate rollout tradeoffs",
          "instructions": "Debate the rollout strategy for the migration",
          "kind": "debate",
          "success_criteria": "Debate session started",
          "session_id": null
        }
      ]
    }
    "#;

    let parsed: GoalPlanResponse = parse_json_block(json).expect("json should parse");
    assert_eq!(parsed.steps.len(), 1);
    assert_eq!(parsed.steps[0].kind, GoalRunStepKind::Debate);
}
