use crate::session_manager::SessionManager;

#[tokio::test]
async fn goal_projection_writes_files_on_create_and_refresh() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let goal_run = engine
        .start_goal_run(
            "Ship goal projections".to_string(),
            Some("Goal projections".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await;

    let projection_dir = root.path().join(".tamux/goals").join(&goal_run.id);
    let dossier_path = projection_dir.join("dossier.json");
    let proof_ledger_path = projection_dir.join("proof-ledger.json");
    let goal_md_path = projection_dir.join("goal.md");
    let inventory_dir = projection_dir.join("inventory");
    let specs_dir = inventory_dir.join("specs");
    let plans_dir = inventory_dir.join("plans");
    let execution_dir = inventory_dir.join("execution");

    assert!(projection_dir.exists(), "projection directory should exist");
    assert!(dossier_path.exists(), "dossier projection should exist");
    assert!(proof_ledger_path.exists(), "proof ledger projection should exist");
    assert!(goal_md_path.exists(), "goal markdown projection should exist");
    assert!(inventory_dir.exists(), "inventory directory should exist");
    assert!(specs_dir.exists(), "specs directory should exist");
    assert!(plans_dir.exists(), "plans directory should exist");
    assert!(execution_dir.exists(), "execution directory should exist");

    let initial_markdown = tokio::fs::read_to_string(&goal_md_path)
        .await
        .expect("read goal markdown");
    assert!(
        initial_markdown.contains("Ship goal projections"),
        "goal markdown should include the live goal text"
    );

    assert!(
        engine.control_goal_run(&goal_run.id, "pause", None).await,
        "pausing the goal should succeed"
    );

    let refreshed_markdown = tokio::fs::read_to_string(&goal_md_path)
        .await
        .expect("read refreshed goal markdown");
    assert!(
        refreshed_markdown.contains("Goal paused"),
        "goal markdown should refresh after a state transition"
    );
}
