use tempfile::tempdir;

use crate::agent::engine::AgentEngine;
use crate::agent::types::AgentConfig;
use crate::session_manager::SessionManager;

#[tokio::test]
async fn record_morphogenesis_outcome_persists_affinity_update_log_and_soul_adaptation() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    for _ in 0..11 {
        engine
            .record_morphogenesis_outcome(
                "researcher",
                &["research".to_string()],
                crate::agent::morphogenesis::types::MorphogenesisOutcome::Success,
            )
            .await
            .expect("record morphogenesis outcome");
    }

    let updates = engine
        .load_morphogenesis_affinity_updates("researcher", Some("research"), 32)
        .await
        .expect("load affinity updates");
    assert!(
        !updates.is_empty(),
        "affinity update log should not be empty"
    );
    assert!(updates
        .iter()
        .any(|update| update.new_affinity > update.old_affinity));

    let adaptations = engine
        .load_soul_adaptations("researcher", 16)
        .await
        .expect("load soul adaptations");
    assert!(adaptations.iter().any(|adaptation| {
        adaptation.domain == "research"
            && matches!(
                adaptation.adaptation_type,
                crate::agent::morphogenesis::types::AdaptationType::Added
                    | crate::agent::morphogenesis::types::AdaptationType::Updated
            )
            && adaptation.soul_snippet.contains("Current Specialization")
    }));
}

#[test]
fn classify_domains_infers_domains_from_prompt_and_capability_tags() {
    let domains = crate::agent::morphogenesis::task_router::classify_domains(
        "Investigate the Cargo build failure in src/lib.rs and explain the fix",
        &["research".to_string()],
    );

    assert!(domains.iter().any(|domain| domain == "rust"));
    assert!(domains.iter().any(|domain| domain == "research"));
}

#[test]
fn classify_domains_falls_back_to_general_when_no_signal_exists() {
    let domains = crate::agent::morphogenesis::task_router::classify_domains("Handle this", &[]);

    assert_eq!(domains, vec!["general".to_string()]);
}
