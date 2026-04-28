use super::*;

#[tokio::test]
async fn legacy_weles_like_persisted_collisions_are_excluded_and_audited() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut config = engine.get_config().await;
    config.sub_agents = vec![
        test_user_sub_agent("weles_builtin", "Legacy WELES"),
        test_user_sub_agent("legacy-shadow", "WELES"),
        test_user_sub_agent("reviewer", "Reviewer"),
    ];
    config.provider = "anthropic".to_string();
    config.model = "claude-sonnet".to_string();
    config.system_prompt = "Main prompt".to_string();
    engine.set_config(config).await;

    let sub_agents = engine.list_sub_agents().await;
    assert_eq!(
        sub_agents
            .iter()
            .filter(|entry| entry.id == "weles_builtin")
            .count(),
        1,
        "effective registry should expose exactly one daemon-owned WELES entry"
    );
    assert!(sub_agents.iter().any(|entry| entry.id == "reviewer"));
    assert!(!sub_agents.iter().any(|entry| {
        entry.id == "legacy-shadow" || (entry.name == "Legacy WELES" && !entry.builtin)
    }));

    let stored = engine.get_config().await;
    assert_eq!(stored.sub_agents.len(), 1);
    assert_eq!(stored.sub_agents[0].id, "reviewer");

    let audit_entries = engine
        .history
        .list_action_audit(None, None, 20)
        .await
        .expect("audit query should succeed");
    assert!(audit_entries.iter().any(|entry| {
        entry.action_type == "subagent"
            && entry.summary.contains("WELES")
            && entry.summary.contains("collision")
    }));
}

#[tokio::test]
async fn repeated_list_sub_agents_does_not_duplicate_weles_collision_audits() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut config = engine.get_config().await;
    config.sub_agents = vec![
        test_user_sub_agent("weles_builtin", "Legacy WELES"),
        test_user_sub_agent("legacy-shadow", "WELES"),
    ];
    engine.set_config(config).await;

    let _ = engine.list_sub_agents().await;
    let first_audits = engine
        .history
        .list_action_audit(None, None, 50)
        .await
        .expect("audit query should succeed");
    let first_collision_count = first_audits
        .iter()
        .filter(|entry| entry.action_type == "subagent" && entry.summary.contains("collision"))
        .count();

    let _ = engine.list_sub_agents().await;
    let second_audits = engine
        .history
        .list_action_audit(None, None, 50)
        .await
        .expect("audit query should succeed");
    let second_collision_count = second_audits
        .iter()
        .filter(|entry| entry.action_type == "subagent" && entry.summary.contains("collision"))
        .count();

    assert_eq!(first_collision_count, 2);
    assert_eq!(second_collision_count, 2);
}

#[tokio::test]
async fn set_config_durably_cleans_weles_collisions_from_raw_config() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut config = engine.get_config().await;
    config.sub_agents = vec![
        test_user_sub_agent("weles_builtin", "Legacy WELES"),
        test_user_sub_agent("legacy-shadow", "WELES"),
        test_user_sub_agent("reviewer", "Reviewer"),
    ];
    engine.set_config(config).await;

    let stored = engine.get_config().await;
    assert_eq!(stored.sub_agents.len(), 1);
    assert_eq!(stored.sub_agents[0].id, "reviewer");
    assert_eq!(stored.sub_agents[0].name, "Reviewer");
}

#[tokio::test]
async fn cleaned_weles_collisions_do_not_reaudit_on_later_set_config_writes() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut config = engine.get_config().await;
    config.sub_agents = vec![
        test_user_sub_agent("weles_builtin", "Legacy WELES"),
        test_user_sub_agent("legacy-shadow", "WELES"),
    ];
    engine.set_config(config).await;

    let first_audits = engine
        .history
        .list_action_audit(None, None, 50)
        .await
        .expect("audit query should succeed");
    let first_collision_count = first_audits
        .iter()
        .filter(|entry| entry.action_type == "subagent" && entry.summary.contains("collision"))
        .count();

    let mut updated = engine.get_config().await;
    updated.system_prompt = "Updated main prompt".to_string();
    engine.set_config(updated).await;

    let second_audits = engine
        .history
        .list_action_audit(None, None, 50)
        .await
        .expect("audit query should succeed");
    let second_collision_count = second_audits
        .iter()
        .filter(|entry| entry.action_type == "subagent" && entry.summary.contains("collision"))
        .count();

    assert_eq!(first_collision_count, 2);
    assert_eq!(second_collision_count, 2);
}

#[tokio::test]
async fn hydrate_durably_cleans_weles_collisions_from_raw_config() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    replace_raw_config_items(&engine, &stale_weles_collision_config()).await;
    assert_eq!(
        persisted_sub_agent_ids(&engine).await,
        vec![
            "legacy-shadow".to_string(),
            "reviewer".to_string(),
            "weles_builtin".to_string(),
        ]
    );

    engine.hydrate().await.expect("hydrate should succeed");

    let stored = engine.get_config().await;
    assert_eq!(stored.sub_agents.len(), 1);
    assert_eq!(stored.sub_agents[0].id, "reviewer");
    assert_eq!(
        persisted_sub_agent_ids(&engine).await,
        vec!["reviewer".to_string()]
    );
    assert_eq!(weles_collision_audit_count(&engine).await, 2);
}

#[tokio::test]
async fn persist_config_durably_cleans_stale_weles_collisions() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    *engine.config.write().await = stale_weles_collision_config();

    engine.persist_config().await;

    let stored = engine.get_config().await;
    assert_eq!(stored.sub_agents.len(), 1);
    assert_eq!(stored.sub_agents[0].id, "reviewer");
    assert_eq!(
        persisted_sub_agent_ids(&engine).await,
        vec!["reviewer".to_string()]
    );
    assert_eq!(weles_collision_audit_count(&engine).await, 2);

    engine.persist_config().await;
    assert_eq!(weles_collision_audit_count(&engine).await, 2);
}

#[tokio::test]
async fn set_config_item_json_durably_cleans_stale_weles_collisions() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    *engine.config.write().await = stale_weles_collision_config();

    let updated = engine
        .set_config_item_json("/system_prompt", "\"Updated main prompt\"")
        .await
        .expect("config item update should succeed");

    assert_eq!(updated.sub_agents.len(), 1);
    assert_eq!(updated.sub_agents[0].id, "reviewer");
    assert_eq!(
        persisted_sub_agent_ids(&engine).await,
        vec!["reviewer".to_string()]
    );
    assert_eq!(weles_collision_audit_count(&engine).await, 2);

    engine
        .set_config_item_json("/model", "\"gpt-5.4\"")
        .await
        .expect("second config item update should succeed");
    assert_eq!(weles_collision_audit_count(&engine).await, 2);
}
