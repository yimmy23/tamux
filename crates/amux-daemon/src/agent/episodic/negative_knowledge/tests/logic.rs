use super::super::*;
use super::common::*;

#[test]
fn normalize_subject_tokens_sorts_dedupes_and_filters() {
    assert_eq!(
        normalize_subject_tokens("Fix deploy-config in prod!"),
        vec!["config", "deploy", "fix", "prod"]
    );
}

#[test]
fn normalize_subject_key_and_merge_matching_rules_are_stable() {
    assert_eq!(
        normalized_subject_key("Fix deploy-config in prod!"),
        "config deploy fix prod"
    );
    assert_eq!(
        normalized_subject_key("prod deploy fix config fix"),
        "config deploy fix prod"
    );

    let a = make_constraint_with_class("Fix deploy-config in prod!", Some("deploy-fix"));
    let b = make_constraint_with_class("prod deploy fix config", Some("deploy-fix"));
    assert!(constraints_match_for_merge(&a, &b));
}

#[test]
fn constraints_match_for_merge_rejects_mismatched_cases() {
    let a = make_constraint_with_class("Fix deploy-config in prod!", Some("deploy-fix"));
    assert!(!constraints_match_for_merge(
        &a,
        &make_constraint_with_class("prod deploy fix config", Some("ops-fix"))
    ));
    assert!(!constraints_match_for_merge(
        &a,
        &make_constraint_with_class("prod deploy fix config", None)
    ));
    assert!(constraints_match_for_merge(
        &make_constraint_with_class("Fix deploy-config in prod!", None),
        &make_constraint_with_class("prod deploy fix config", None)
    ));
    assert!(!constraints_match_for_merge(
        &make_constraint_with_class("CI CD", Some("deploy-fix")),
        &make_constraint_with_class("QA DB", Some("deploy-fix"))
    ));
    assert!(!constraints_match_for_merge(
        &make_constraint_with_class("deploy config rollback", Some("deploy-fix")),
        &make_constraint_with_class("cache rebuild timeout", Some("deploy-fix"))
    ));
}

#[test]
fn related_for_propagation_applies_class_and_token_rules() {
    assert!(related_for_propagation(
        &make_constraint_with_class("fix deploy config prod", Some("deploy-fix")),
        &make_constraint_with_class("deploy config rollback", Some("deploy-fix"))
    ));
    assert!(!related_for_propagation(
        &make_constraint_with_class("fix deploy config prod", Some("deploy-fix")),
        &make_constraint_with_class("deploy cache rebuild", Some("deploy-fix"))
    ));
    assert!(related_for_propagation(
        &make_constraint_with_class("deploy config prod fix", None),
        &make_constraint_with_class("prod deploy config rollback", None)
    ));
    assert!(!related_for_propagation(
        &make_constraint_with_class("deploy config prod fix", Some("deploy-fix")),
        &make_constraint_with_class("prod deploy config rollback", None)
    ));
}

#[test]
fn next_constraint_state_transitions_match_thresholds() {
    assert_eq!(
        next_constraint_state(ConstraintState::Dead, 1, false, 0.2),
        ConstraintState::Dead
    );
    assert_eq!(
        next_constraint_state(ConstraintState::Suspicious, 1, true, 0.85),
        ConstraintState::Dead
    );
    assert_eq!(
        next_constraint_state(ConstraintState::Dying, 3, false, 0.4),
        ConstraintState::Dead
    );
    assert_eq!(
        next_constraint_state(ConstraintState::Suspicious, 2, false, 0.4),
        ConstraintState::Dying
    );
    assert_eq!(
        next_constraint_state(ConstraintState::Suspicious, 1, true, 0.84),
        ConstraintState::Suspicious
    );
    assert_eq!(
        next_constraint_state(ConstraintState::Dying, 1, false, 0.4),
        ConstraintState::Dying
    );
}

#[test]
fn direct_episode_constraints_capture_confidence_and_tokens() {
    let episode = make_failure_episode("deploy config rollback failed", Some(0.7));
    let constraint =
        build_direct_constraint_from_episode(&episode, 10_000, 20_000, "nc-new".to_string());
    assert_eq!(constraint.state, ConstraintState::Dying);
    assert_eq!(constraint.evidence_count, 1);
    assert!(constraint.direct_observation);

    let high_conf = make_failure_episode("deploy config rollback failed", Some(0.93));
    let dead =
        build_direct_constraint_from_episode(&high_conf, 10_000, 20_000, "nc-dead".to_string());
    assert_eq!(dead.state, ConstraintState::Dead);
}

#[test]
fn merge_constraint_evidence_escalates_states() {
    let merged = merge_constraint_evidence(
        &NegativeConstraint {
            state: ConstraintState::Suspicious,
            evidence_count: 1,
            confidence: 0.4,
            direct_observation: false,
            ..make_constraint_with_class("deploy config rollback", Some("deploy-fix"))
        },
        &NegativeConstraint {
            confidence: 0.7,
            ..make_constraint_with_class("rollback deploy config", Some("deploy-fix"))
        },
    );
    assert_eq!(merged.state, ConstraintState::Dying);
    assert_eq!(merged.evidence_count, 2);

    let merged = merge_constraint_evidence(
        &NegativeConstraint {
            state: ConstraintState::Dying,
            evidence_count: 2,
            confidence: 0.7,
            direct_observation: true,
            ..make_constraint_with_class("deploy config rollback", Some("deploy-fix"))
        },
        &NegativeConstraint {
            confidence: 0.72,
            ..make_constraint_with_class("rollback deploy config", Some("deploy-fix"))
        },
    );
    assert_eq!(merged.state, ConstraintState::Dead);
    assert_eq!(merged.evidence_count, 3);
}

#[test]
fn propagation_updates_related_targets_without_recursive_spread() {
    let source = NegativeConstraint {
        id: "nc-source".to_string(),
        state: ConstraintState::Dead,
        ..make_constraint_with_class("deploy config prod fix", Some("deploy-fix"))
    };
    let target = NegativeConstraint {
        id: "nc-target".to_string(),
        state: ConstraintState::Suspicious,
        direct_observation: false,
        derived_from_constraint_ids: vec!["nc-other".to_string()],
        ..make_constraint_with_class("deploy config rollback", Some("deploy-fix"))
    };

    let propagated = propagate_dead_constraint(&source, &[source.clone(), target]);
    assert_eq!(propagated.len(), 1);
    assert_eq!(propagated[0].id, "nc-target");
    assert_eq!(propagated[0].state, ConstraintState::Dying);
    assert_eq!(
        propagated[0].derived_from_constraint_ids,
        vec!["nc-other".to_string(), "nc-source".to_string()]
    );
    assert!(!propagated[0].direct_observation);
}

#[test]
fn propagation_preserves_direct_targets_and_caps_fanout() {
    let source = NegativeConstraint {
        id: "nc-source".to_string(),
        state: ConstraintState::Dead,
        ..make_constraint_with_class("deploy config prod fix", Some("deploy-fix"))
    };
    let inferred_target = NegativeConstraint {
        id: "nc-inferred".to_string(),
        state: ConstraintState::Suspicious,
        direct_observation: false,
        created_at: 200,
        ..make_constraint_with_class("deploy config rollback", Some("deploy-fix"))
    };
    let direct_target = NegativeConstraint {
        id: "nc-direct".to_string(),
        state: ConstraintState::Suspicious,
        direct_observation: true,
        created_at: 100,
        ..make_constraint_with_class("deploy config fallback", Some("deploy-fix"))
    };
    let propagated =
        propagate_dead_constraint(&source, &[source.clone(), inferred_target, direct_target]);
    assert_eq!(propagated.len(), 2);
    assert!(
        !propagated
            .iter()
            .find(|c| c.id == "nc-inferred")
            .unwrap()
            .direct_observation
    );
    assert!(
        propagated
            .iter()
            .find(|c| c.id == "nc-direct")
            .unwrap()
            .direct_observation
    );

    let mut constraints = vec![NegativeConstraint {
        id: "nc-source".to_string(),
        state: ConstraintState::Dead,
        ..make_constraint_with_class("alpha beta gamma root", Some("deploy-fix"))
    }];
    for idx in 0..11 {
        constraints.push(NegativeConstraint {
            id: format!("nc-related-{idx}"),
            state: ConstraintState::Suspicious,
            direct_observation: false,
            created_at: 100 + idx,
            ..make_constraint_with_class(&format!("alpha beta related {idx}"), Some("deploy-fix"))
        });
    }
    constraints.push(NegativeConstraint {
        id: "nc-second-hop".to_string(),
        state: ConstraintState::Suspicious,
        direct_observation: false,
        created_at: 10_000,
        subject: "alpha related branch leaf".to_string(),
        ..make_constraint_with_class("alpha related branch leaf", Some("deploy-fix"))
    });

    let propagated = propagate_dead_constraint(&constraints[0], &constraints);
    let propagated_ids: Vec<&str> = propagated
        .iter()
        .map(|constraint| constraint.id.as_str())
        .collect();
    assert_eq!(propagated.len(), 10);
    assert!(propagated
        .iter()
        .all(|constraint| constraint.state == ConstraintState::Dying));
    assert!(!propagated_ids.contains(&"nc-related-0"));
    assert!(propagated_ids.contains(&"nc-related-10"));
    assert!(!propagated_ids.contains(&"nc-second-hop"));
}
