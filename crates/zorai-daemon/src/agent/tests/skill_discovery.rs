#[cfg(test)]
use super::*;
use crate::agent::engine::AgentEngine;
use crate::agent::learning::patterns::{PatternType, ToolPattern};
use crate::agent::types::AgentConfig;
use crate::history::SkillVariantConsultationRecord;
use crate::session_manager::SessionManager;
use tempfile::tempdir;

fn default_config() -> SkillDiscoveryConfig {
    SkillDiscoveryConfig::default()
}

fn make_pattern(tools: &[&str]) -> ToolPattern {
    ToolPattern {
        id: "test-pattern".to_string(),
        pattern_type: PatternType::SuccessSequence,
        tool_sequence: tools.iter().map(|s| s.to_string()).collect(),
        task_type: "coding".to_string(),
        occurrences: 5,
        success_rate: 0.9,
        confidence: 0.8,
        last_seen_at: 1000,
        created_at: 500,
    }
}

fn seq(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

// -----------------------------------------------------------------------
// meets_complexity_threshold
// -----------------------------------------------------------------------

#[test]
fn skill_drafting_prompt_uses_agentskills_schema_and_zorai_context_tags() {
    let prompt = build_skill_drafting_prompt(
        &seq(&["read_file", "cargo_test", "apply_patch"]),
        "coding",
        &["rust".to_string(), "async".to_string()],
    );

    assert!(prompt.contains("Use agentskills.io-compatible YAML frontmatter."));
    assert!(prompt.contains("name: coding-read-file-cargo-test-apply-patch"));
    assert!(prompt.contains("zorai:"));
    assert!(prompt.contains("context_tags:"));
    assert!(prompt.contains("- rust"));
    assert!(prompt.contains("- async"));
    assert!(!prompt.contains("name: <concise snake_case skill name>"));
    assert!(!prompt.contains("context_tags: [<relevant tags>]"));
}

#[test]
fn skill_discovery_complexity_returns_false_when_outcome_not_success() {
    let cfg = default_config();
    assert!(!meets_complexity_threshold(
        20,
        2,
        Some(0.95),
        "failure",
        &cfg
    ));
}

#[test]
fn skill_discovery_complexity_returns_false_when_tool_count_at_threshold() {
    let cfg = default_config();
    // tool_count == min_tool_count (8), not >, so false
    assert!(!meets_complexity_threshold(
        8,
        2,
        Some(0.95),
        "success",
        &cfg
    ));
}

#[test]
fn skill_discovery_complexity_returns_true_with_replan() {
    let cfg = default_config();
    // tool_count > 8, replan_count >= 1, outcome success
    assert!(meets_complexity_threshold(10, 1, None, "success", &cfg));
}

#[test]
fn skill_discovery_complexity_returns_true_with_quality() {
    let cfg = default_config();
    // tool_count > 8, replan_count=0, quality > 0.8, outcome success
    assert!(meets_complexity_threshold(
        10,
        0,
        Some(0.85),
        "success",
        &cfg
    ));
}

#[test]
fn skill_discovery_complexity_returns_false_no_replan_no_quality() {
    let cfg = default_config();
    // tool_count > 8, replan_count=0, quality <= 0.8
    assert!(!meets_complexity_threshold(
        10,
        0,
        Some(0.8),
        "success",
        &cfg
    ));
    assert!(!meets_complexity_threshold(10, 0, None, "success", &cfg));
}

// -----------------------------------------------------------------------
// jaccard_similarity
// -----------------------------------------------------------------------

#[test]
fn skill_discovery_jaccard_identical_sets() {
    let a = seq(&["A", "B", "C"]);
    let b = seq(&["A", "B", "C"]);
    assert!((jaccard_similarity(&a, &b) - 1.0).abs() < f64::EPSILON);
}

#[test]
fn skill_discovery_jaccard_disjoint_sets() {
    let a = seq(&["A", "B"]);
    let b = seq(&["C", "D"]);
    assert!((jaccard_similarity(&a, &b) - 0.0).abs() < f64::EPSILON);
}

#[test]
fn skill_discovery_jaccard_partial_overlap() {
    let a = seq(&["A", "B", "C"]);
    let b = seq(&["B", "C", "D"]);
    // intersection={B,C}=2, union={A,B,C,D}=4 => 0.5
    assert!((jaccard_similarity(&a, &b) - 0.5).abs() < f64::EPSILON);
}

#[test]
fn skill_discovery_jaccard_empty_sets() {
    let a: Vec<String> = vec![];
    let b: Vec<String> = vec![];
    assert!((jaccard_similarity(&a, &b) - 1.0).abs() < f64::EPSILON);
}

// -----------------------------------------------------------------------
// is_novel_sequence
// -----------------------------------------------------------------------

#[test]
fn skill_discovery_novel_when_no_patterns_match() {
    let candidate = seq(&["X", "Y", "Z"]);
    let pattern = make_pattern(&["A", "B", "C"]);
    let patterns = vec![&pattern];
    assert!(is_novel_sequence(&candidate, "coding", &patterns, 0.7));
}

#[test]
fn skill_discovery_not_novel_when_pattern_similar() {
    let candidate = seq(&["A", "B", "C"]);
    let pattern = make_pattern(&["A", "B", "C"]);
    let patterns = vec![&pattern];
    // similarity=1.0 >= 0.7 threshold
    assert!(!is_novel_sequence(&candidate, "coding", &patterns, 0.7));
}

// -----------------------------------------------------------------------
// extract_tool_sequence_from_json
// -----------------------------------------------------------------------

#[test]
fn skill_discovery_extract_tool_sequence_valid_json() {
    let json = r#"["file_read", "terminal_exec", "file_write"]"#;
    let result = extract_tool_sequence_from_json(Some(json));
    assert_eq!(result, vec!["file_read", "terminal_exec", "file_write"]);
}

#[test]
fn skill_discovery_extract_tool_sequence_none() {
    let result = extract_tool_sequence_from_json(None);
    assert!(result.is_empty());
}

#[test]
fn skill_discovery_extract_tool_sequence_invalid_json() {
    let result = extract_tool_sequence_from_json(Some("not json"));
    assert!(result.is_empty());
}

// -----------------------------------------------------------------------
// parse_mental_test_results
// -----------------------------------------------------------------------

#[test]
fn skill_discovery_mental_test_parses_valid_json() {
    let response = r#"[
            {"scenario": "Debugging a CI failure", "would_help": true},
            {"scenario": "Writing a README", "would_help": false},
            {"scenario": "Refactoring a module", "would_help": true}
        ]"#;
    assert_eq!(parse_mental_test_results(response), 2);
}

#[test]
fn skill_discovery_mental_test_parses_json_in_code_block() {
    let response = "```json\n[\n  {\"scenario\": \"A\", \"would_help\": true},\n  {\"scenario\": \"B\", \"would_help\": true},\n  {\"scenario\": \"C\", \"would_help\": true}\n]\n```";
    assert_eq!(parse_mental_test_results(response), 3);
}

#[test]
fn skill_discovery_mental_test_returns_zero_for_all_false() {
    let response = r#"[
            {"scenario": "A", "would_help": false},
            {"scenario": "B", "would_help": false},
            {"scenario": "C", "would_help": false}
        ]"#;
    assert_eq!(parse_mental_test_results(response), 0);
}

#[test]
fn skill_discovery_mental_test_returns_zero_for_invalid_response() {
    assert_eq!(
        parse_mental_test_results("I cannot evaluate this skill."),
        0
    );
}

#[test]
fn skill_discovery_mental_test_fallback_counts_would_help() {
    let response = r#"Some text "would_help": true and "would_help":true but "would_help": false"#;
    assert_eq!(parse_mental_test_results(response), 2);
}

#[tokio::test]
async fn flag_skill_draft_candidates_skips_successful_traces_when_a_skill_was_consulted() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.skill_discovery.min_tool_count = 3;
    config.skill_discovery.novelty_similarity_threshold = 0.95;
    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;

    let skill_path = root.path().join("skills/generated/consulted-build.md");
    std::fs::create_dir_all(skill_path.parent().expect("skill parent")).expect("create skill dir");
    std::fs::write(
        &skill_path,
        "# Consulted Build\nUse cargo build and cargo test.\n",
    )
    .expect("write skill");
    let variant = engine
        .history
        .register_skill_document(&skill_path)
        .await
        .expect("register skill");

    engine
        .history
        .insert_execution_trace(
            "trace-skill-consulted",
            None,
            None,
            Some("task-skill-consulted"),
            "coding",
            "success",
            Some(0.95),
            r#"["read_file","search_files","replace_in_file","cargo_test"]"#,
            "{}",
            250,
            120,
            "svarog",
            1_000,
            1_000,
            1_000,
        )
        .await
        .expect("insert execution trace");

    engine
        .history
        .record_skill_variant_consultation(&SkillVariantConsultationRecord {
            usage_id: "usage-skill-consulted",
            variant_id: &variant.variant_id,
            thread_id: Some("thread-skill-consulted"),
            task_id: Some("task-skill-consulted"),
            goal_run_id: None,
            context_tags: &["coding".to_string(), "rust".to_string()],
            consulted_at: 1_000,
        })
        .await
        .expect("record skill consultation");
    engine
        .history
        .settle_skill_variant_usage(
            Some("thread-skill-consulted"),
            Some("task-skill-consulted"),
            None,
            "success",
        )
        .await
        .expect("settle skill usage");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let flagged = engine.flag_skill_draft_candidates(&config, &deadline).await;

    assert_eq!(
        flagged, 0,
        "successful traces that already used a matched skill should not become new draft candidates"
    );
    let states = engine
        .history
        .list_consolidation_state_by_prefix("skill_draft_candidate:")
        .await
        .expect("list draft candidate states");
    assert!(
        states.is_empty(),
        "no draft candidate state should be recorded for assisted traces"
    );
}
