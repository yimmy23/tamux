use super::super::*;
use super::common::*;

#[test]
fn format_negative_constraints_empty_returns_empty() {
    assert!(format_negative_constraints(&[], 1_000_000_000).is_empty());
}

#[test]
fn format_negative_constraints_groups_and_sorts_by_state_then_created_at() {
    let constraints = vec![
        make_constraint_with_details(
            "suspicious old",
            ConstraintState::Suspicious,
            100,
            true,
            &[],
        ),
        make_constraint_with_details("dead newer", ConstraintState::Dead, 300, true, &["nc-1"]),
        make_constraint_with_details(
            "dying newest",
            ConstraintState::Dying,
            400,
            false,
            &["nc-2"],
        ),
        make_constraint_with_details("dead oldest", ConstraintState::Dead, 200, true, &[]),
        make_constraint_with_details(
            "suspicious newer",
            ConstraintState::Suspicious,
            500,
            false,
            &[],
        ),
    ];

    let result = format_negative_constraints(&constraints, 1_000_000_000);

    let dead_newer = result.find("DO NOT attempt: dead newer").unwrap();
    let dead_oldest = result.find("DO NOT attempt: dead oldest").unwrap();
    let dying_newest = result
        .find("Avoid unless you have new evidence: dying newest")
        .unwrap();
    let suspicious_newer = result.find("Use caution: suspicious newer").unwrap();
    let suspicious_old = result.find("Use caution: suspicious old").unwrap();

    assert!(dead_newer < dead_oldest);
    assert!(dead_oldest < dying_newest);
    assert!(dying_newest < suspicious_newer);
    assert!(suspicious_newer < suspicious_old);
}

#[test]
fn format_negative_constraints_renders_state_metadata_and_conditional_provenance() {
    let constraints = vec![
        make_constraint_with_details("dead path", ConstraintState::Dead, 300, true, &["nc-1"]),
        make_constraint_with_details("dying path", ConstraintState::Dying, 200, false, &["nc-2"]),
        make_constraint_with_details(
            "suspicious path",
            ConstraintState::Suspicious,
            100,
            true,
            &[],
        ),
    ];

    let result = format_negative_constraints(&constraints, 1_000_000_000);

    assert!(result.starts_with("## Ruled-Out Approaches (Negative Knowledge)\n"));
    assert!(result.contains("DO NOT attempt: dead path"));
    assert!(result.contains("Avoid unless you have new evidence: dying path"));
    assert!(result.contains("Use caution: suspicious path"));
    assert!(result.contains("State: dead"));
    assert!(result.contains("State: dying"));
    assert!(result.contains("State: suspicious"));
    assert!(result.contains("Reason: Reason for dead path"));
    assert!(result.contains("Type: ruled_out"));
    assert!(result.contains("confidence: 85%"));
    assert!(result.contains("Source: direct"));
    assert!(result.contains("Source: inferred"));
    assert!(result.contains("Source: direct from 1 related dead constraint"));
    assert!(result.contains("Source: inferred from 1 related dead constraint"));
    assert!(result.contains("\n  Source: direct\n  Solution class: test-class\n"));
    assert!(!result.contains("Source: direct from 0 related dead constraints"));
}

#[test]
fn format_negative_constraints_excludes_expired_constraints() {
    let constraints = vec![
        make_constraint_with_details("active path", ConstraintState::Dead, 300, true, &[]),
        make_constraint("expired path", Some(999_999_999)),
    ];

    let result = format_negative_constraints(&constraints, 1_000_000_000);

    assert!(result.contains("DO NOT attempt: active path"));
    assert!(!result.contains("expired path"));
}

#[test]
fn format_negative_constraints_caps_display_at_ten_and_shows_overflow_count() {
    let constraints: Vec<NegativeConstraint> = (0..11)
        .map(|idx| {
            make_constraint_with_details(
                &format!("constraint {idx}"),
                ConstraintState::Dead,
                1_000 + idx,
                true,
                &[],
            )
        })
        .collect();

    let result = format_negative_constraints(&constraints, 1_000_000_000);

    assert!(result.contains("DO NOT attempt: constraint 10"));
    assert!(result.contains("DO NOT attempt: constraint 1"));
    assert!(!result.contains("DO NOT attempt: constraint 0"));
    assert!(result.contains("(1 more constraints not shown)"));
}

#[test]
fn format_negative_constraints_renders_exact_inferred_source_without_provenance() {
    let constraints = vec![make_constraint_with_details(
        "inferred path",
        ConstraintState::Suspicious,
        100,
        false,
        &[],
    )];

    let result = format_negative_constraints(&constraints, 1_000_000_000);

    assert!(result.contains("\n  Source: inferred\n  Solution class: test-class\n"));
    assert!(!result.contains("Source: inferred from"));
}

#[test]
fn format_negative_constraints_with_two_constraints() {
    let constraints = vec![
        make_constraint("npm install approach", Some(2_000_000_000)),
        make_constraint("yarn install approach", None),
    ];
    let result = format_negative_constraints(&constraints, 1_000_000_000);
    assert!(result.contains("Avoid unless you have new evidence: npm install approach"));
    assert!(result.contains("Avoid unless you have new evidence: yarn install approach"));
    assert!(result.contains("Ruled-Out Approaches"));
    assert!(result.contains("State: dying"));
    assert!(result.contains("Reason: Reason for npm install approach"));
    assert!(result.contains("Type: ruled_out"));
    assert!(result.contains("confidence: 85%"));
    assert!(result.contains("Source: direct"));
}

#[test]
fn format_negative_constraints_includes_solution_class() {
    let constraints = vec![make_constraint("bad approach", Some(2_000_000_000))];
    let result = format_negative_constraints(&constraints, 1_000_000_000);
    assert!(result.contains("Solution class: test-class"));
}

#[test]
fn format_negative_constraints_includes_expiry() {
    let now_ms = 1_000_000_000u64;
    let in_10_days = now_ms + 10 * 86400 * 1000;
    let constraints = vec![make_constraint("approach", Some(in_10_days))];
    let result = format_negative_constraints(&constraints, now_ms);
    assert!(result.contains("Expires: in 10 days"));
}

#[test]
fn is_constraint_active_variants() {
    assert!(is_constraint_active(
        &make_constraint("test", None),
        9_999_999_999
    ));
    assert!(is_constraint_active(
        &make_constraint("test", Some(2_000_000_000)),
        1_000_000_000
    ));
    assert!(!is_constraint_active(
        &make_constraint("test", Some(500_000_000)),
        1_000_000_000
    ));
}
