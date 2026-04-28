use super::*;

#[test]
fn compute_approach_hash_consistent_for_same_input() {
    let h1 = compute_approach_hash("read_file", "/src/main.rs");
    let h2 = compute_approach_hash("read_file", "/src/main.rs");
    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 16);
}

#[test]
fn compute_approach_hash_different_for_different_input() {
    let h1 = compute_approach_hash("read_file", "/src/main.rs");
    let h2 = compute_approach_hash("write_file", "/src/main.rs");
    let h3 = compute_approach_hash("read_file", "/src/lib.rs");
    assert_ne!(h1, h2);
    assert_ne!(h1, h3);
}

#[test]
fn detect_repeated_approaches_none_below_threshold() {
    let tried = vec![
        TriedApproach {
            approach_hash: "aaa".to_string(),
            description: "tool_a(args)".to_string(),
            outcome: EpisodeOutcome::Failure,
            timestamp: 1000,
        },
        TriedApproach {
            approach_hash: "aaa".to_string(),
            description: "tool_a(args)".to_string(),
            outcome: EpisodeOutcome::Failure,
            timestamp: 2000,
        },
    ];
    assert!(detect_repeated_approaches(&tried, 3).is_none());
}

#[test]
fn detect_repeated_approaches_some_at_threshold() {
    let tried = vec![
        TriedApproach {
            approach_hash: "aaa".to_string(),
            description: "tool_a(args)".to_string(),
            outcome: EpisodeOutcome::Failure,
            timestamp: 1000,
        },
        TriedApproach {
            approach_hash: "aaa".to_string(),
            description: "tool_a(args)".to_string(),
            outcome: EpisodeOutcome::Failure,
            timestamp: 2000,
        },
        TriedApproach {
            approach_hash: "aaa".to_string(),
            description: "tool_a(args)".to_string(),
            outcome: EpisodeOutcome::Failure,
            timestamp: 3000,
        },
    ];
    let result = detect_repeated_approaches(&tried, 3);
    assert!(result.is_some());
    let msg = result.unwrap();
    assert!(msg.contains("tool_a(args)"));
    assert!(msg.contains("3 times"));
    assert!(msg.contains("Consider a different approach"));
}

#[test]
fn record_correction_creates_new_entry() {
    let mut state = CounterWhoState::default();
    record_correction(&mut state, "wrong file path", 1000);
    assert_eq!(state.correction_patterns.len(), 1);
    assert_eq!(state.correction_patterns[0].correction_count, 1);
    assert_eq!(state.correction_patterns[0].pattern, "wrong file path");
}

#[test]
fn record_correction_increments_existing() {
    let mut state = CounterWhoState::default();
    record_correction(&mut state, "wrong file path", 1000);
    record_correction(&mut state, "wrong file path", 2000);
    assert_eq!(state.correction_patterns.len(), 1);
    assert_eq!(state.correction_patterns[0].correction_count, 2);
    assert_eq!(state.correction_patterns[0].last_correction_at, 2000);
}

#[test]
fn format_counter_who_context_empty_state_returns_empty() {
    let state = CounterWhoState::default();
    assert!(format_counter_who_context(&state).is_empty());
}

#[test]
fn format_counter_who_context_with_data_produces_formatted_text() {
    let mut state = CounterWhoState::default();
    state.current_focus = Some("Tool: read_file".to_string());
    state.tried_approaches.push(TriedApproach {
        approach_hash: "abc".to_string(),
        description: "read_file(/src/main.rs)".to_string(),
        outcome: EpisodeOutcome::Success,
        timestamp: 1000,
    });
    state.correction_patterns.push(CorrectionPattern {
        pattern: "wrong config path".to_string(),
        correction_count: 2,
        last_correction_at: 3000,
    });

    let result = format_counter_who_context(&state);
    assert!(result.contains("Self-Awareness (Counter-Who)"));
    assert!(result.contains("Tool: read_file"));
    assert!(result.contains("read_file(/src/main.rs)"));
    assert!(result.contains("success"));
    assert!(result.contains("wrong config path"));
    assert!(result.contains("corrected 2 times"));
}

#[test]
fn prune_old_approaches_removes_old_entries() {
    let now_ms = 1_000_000_000u64;
    let eight_days_ms = 8 * 86400 * 1000;
    let mut state = CounterWhoState::default();
    state.tried_approaches.push(TriedApproach {
        approach_hash: "old".to_string(),
        description: "old_tool".to_string(),
        outcome: EpisodeOutcome::Failure,
        timestamp: now_ms - eight_days_ms,
    });
    state.tried_approaches.push(TriedApproach {
        approach_hash: "new".to_string(),
        description: "new_tool".to_string(),
        outcome: EpisodeOutcome::Success,
        timestamp: now_ms - 1000,
    });

    prune_old_approaches(&mut state, now_ms, 7, 20);
    assert_eq!(state.tried_approaches.len(), 1);
    assert_eq!(state.tried_approaches[0].approach_hash, "new");
}

#[test]
fn prune_old_approaches_caps_at_max_entries() {
    let now_ms = 1_000_000_000u64;
    let mut state = CounterWhoState::default();
    for i in 0..25 {
        state.tried_approaches.push(TriedApproach {
            approach_hash: format!("h{i}"),
            description: format!("tool_{i}"),
            outcome: EpisodeOutcome::Success,
            timestamp: now_ms - (25 - i) * 1000,
        });
    }

    prune_old_approaches(&mut state, now_ms, 7, 20);
    assert_eq!(state.tried_approaches.len(), 20);
    assert_eq!(state.tried_approaches[0].approach_hash, "h24");
}
