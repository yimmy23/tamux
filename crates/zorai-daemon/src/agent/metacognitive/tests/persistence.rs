use crate::agent::{metacognitive::types::SelfModel, types::AgentConfig, AgentEngine};
use crate::session_manager::SessionManager;
use tempfile::tempdir;

#[tokio::test]
async fn meta_cognitive_self_model_persists_and_loads() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut model = SelfModel::default();
    model.agent_id = "svarog".to_string();
    model.calibration_offset = -0.18;
    model.last_updated_ms = 1_717_200_100;
    model.biases[0].occurrence_count = 3;
    model.workflow_profiles[0].avg_success_rate = 0.66;

    engine
        .persist_meta_cognitive_self_model(&model)
        .await
        .expect("self model should persist");

    let loaded = engine
        .load_meta_cognitive_self_model()
        .await
        .expect("self model should load");

    assert_eq!(loaded.agent_id, "svarog");
    assert!((loaded.calibration_offset + 0.18).abs() < f64::EPSILON);
    assert!(loaded
        .biases
        .iter()
        .any(|bias| bias.name == "sunk_cost" && bias.occurrence_count == 3));
    assert!(loaded
        .workflow_profiles
        .iter()
        .any(|profile| profile.name == "debug_loop"
            && (profile.avg_success_rate - 0.66).abs() < f64::EPSILON));
}

#[tokio::test]
async fn hydrate_restores_runtime_meta_cognitive_self_model() {
    let root = tempdir().expect("tempdir");

    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut model = SelfModel::default();
    model.agent_id = "svarog".to_string();
    model.calibration_offset = -0.31;
    model.last_updated_ms = 1_717_200_777;
    model.biases[0].occurrence_count = 9;

    {
        let mut runtime_model = engine.meta_cognitive_self_model.write().await;
        *runtime_model = model.clone();
    }
    engine.persist_learning_stores().await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    rehydrated.hydrate().await.expect("hydrate should succeed");

    let loaded = rehydrated.meta_cognitive_self_model.read().await.clone();
    assert_eq!(loaded.agent_id, "svarog");
    assert!((loaded.calibration_offset + 0.31).abs() < f64::EPSILON);
    assert!(loaded
        .biases
        .iter()
        .any(|bias| bias.name == "sunk_cost" && bias.occurrence_count == 9));
}
