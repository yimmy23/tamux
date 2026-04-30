use super::*;

const HERMES_CONFIG_FIXTURE: &str = r#"
provider: openrouter
model: nousresearch/hermes-4-70b
terminal:
  backend: local
  cwd: /workspace/repo
mcp_servers:
  zorai:
    command: "/usr/local/bin/zorai-mcp"
    args: []
"#;

const OPENCLAW_CONFIG_FIXTURE: &str = r#"{
  "agents": {
    "defaults": {
      "workspace": "~/.openclaw/workspace",
      "model": {
        "primary": "anthropic/claude-sonnet-4-6",
        "fallbacks": ["openai/gpt-5.4"]
      }
    }
  },
  "mcp_servers": {
    "zorai": {
      "command": "/usr/local/bin/zorai-mcp",
      "args": []
    }
  }
}"#;

#[test]
fn schema_init_migrates_legacy_external_runtime_profiles_before_session_index() -> Result<()> {
    let root = std::env::temp_dir().join(format!("zorai-history-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&root)?;
    let db_path = root.join("history.sqlite");
    let conn = rusqlite::Connection::open(&db_path)?;
    conn.execute_batch(
        "CREATE TABLE external_runtime_profiles (
            runtime TEXT PRIMARY KEY,
            profile_json TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );
        INSERT INTO external_runtime_profiles (runtime, profile_json, updated_at)
        VALUES ('hermes', '{}', 1);",
    )?;

    let init_result = crate::history::schema::init_schema_on_connection(&conn, &root);

    init_result?;
    let mut column_stmt = conn.prepare("PRAGMA table_info(external_runtime_profiles)")?;
    let columns = column_stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    assert!(columns.iter().any(|column| column == "session_id"));
    assert!(columns.iter().any(|column| column == "source_config_path"));
    assert!(columns.iter().any(|column| column == "source_fingerprint"));

    let session_index_exists: bool = conn.query_row(
        "SELECT EXISTS (
            SELECT 1 FROM sqlite_master
            WHERE type = 'index'
              AND name = 'idx_external_runtime_profiles_session'
        )",
        [],
        |row| row.get(0),
    )?;
    assert!(session_index_exists);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn hermes_config_parser_round_trips_through_runtime_profile_persistence() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let profile = crate::agent::external_runtime_import::parse_hermes_config_profile(
        HERMES_CONFIG_FIXTURE,
        "~/.hermes/config.yaml",
        1_777_200_000_000,
    )?;

    store
        .upsert_external_runtime_profile("hermes", &profile)
        .await?;

    let row = store
        .get_external_runtime_profile("hermes")
        .await?
        .expect("runtime profile should exist");
    assert_eq!(row.runtime, "hermes");

    let loaded: crate::agent::types::ExternalRuntimeProfile =
        serde_json::from_str(&row.profile_json)?;
    assert_eq!(loaded, profile);
    assert_eq!(loaded.provider.as_deref(), Some("openrouter"));
    assert_eq!(loaded.model.as_deref(), Some("nousresearch/hermes-4-70b"));
    assert_eq!(loaded.cwd.as_deref(), Some("/workspace/repo"));
    assert!(loaded.has_zorai_mcp);
    assert_eq!(row.session_id, None);
    assert_eq!(
        row.source_config_path.as_deref(),
        Some("~/.hermes/config.yaml")
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn openclaw_config_parser_round_trips_through_runtime_profile_persistence() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let profile = crate::agent::external_runtime_import::parse_openclaw_config_profile(
        OPENCLAW_CONFIG_FIXTURE,
        "~/.openclaw/openclaw.json",
        1_777_200_000_001,
    )?;

    store
        .upsert_external_runtime_profile("openclaw", &profile)
        .await?;

    let row = store
        .get_external_runtime_profile("openclaw")
        .await?
        .expect("runtime profile should exist");
    assert_eq!(row.runtime, "openclaw");

    let loaded: crate::agent::types::ExternalRuntimeProfile =
        serde_json::from_str(&row.profile_json)?;
    assert_eq!(loaded, profile);
    assert_eq!(loaded.provider.as_deref(), Some("anthropic"));
    assert_eq!(loaded.model.as_deref(), Some("anthropic/claude-sonnet-4-6"));
    assert_eq!(loaded.cwd.as_deref(), Some("~/.openclaw/workspace"));
    assert!(loaded.has_zorai_mcp);
    assert_eq!(row.session_id, None);
    assert_eq!(
        row.source_config_path.as_deref(),
        Some("~/.openclaw/openclaw.json")
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn import_session_assets_and_shadow_runs_round_trip() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let session = crate::agent::types::ExternalRuntimeImportSession {
        session_id: "import-session-test".to_string(),
        runtime: "hermes".to_string(),
        source_config_path: "~/.hermes/config.yaml".to_string(),
        source_fingerprint: "abc123".to_string(),
        dry_run: false,
        conflict_policy: crate::agent::types::ExternalRuntimeConflictPolicy::StageForReview,
        source_surface: "tool_import_external_runtime".to_string(),
        imported_at_ms: 1_777_200_000_002,
        schema_version: 1,
        asset_count: 2,
        notes: vec!["note".to_string()],
    };
    store
        .upsert_external_runtime_import_session(&session)
        .await?;

    let loaded_session = store
        .get_external_runtime_import_session("import-session-test")
        .await?
        .expect("session should exist");
    assert_eq!(loaded_session.runtime, "hermes");

    let assets = vec![crate::agent::types::ImportedRuntimeAsset {
        asset_id: "asset-1".to_string(),
        session_id: session.session_id.clone(),
        runtime: "hermes".to_string(),
        asset_kind: "profile".to_string(),
        bucket: crate::agent::types::ExternalRuntimeAssetBucket::Imported,
        severity: crate::agent::types::ExternalRuntimeReportSeverity::Safe,
        recommended_action: None,
        reason: Some("ok".to_string()),
        source_path: Some("~/.hermes/config.yaml".to_string()),
        source_fingerprint: Some("abc123".to_string()),
        conflict_policy: crate::agent::types::ExternalRuntimeConflictPolicy::StageForReview,
        archive_thread_id: None,
        archive_query_hint: None,
        payload: serde_json::json!({"runtime":"hermes"}),
        created_at_ms: 1_777_200_000_003,
    }];
    store
        .replace_imported_runtime_assets(&session.session_id, &assets)
        .await?;
    let loaded_assets = store
        .list_imported_runtime_assets(Some("hermes"), Some(&session.session_id))
        .await?;
    assert_eq!(loaded_assets.len(), 1);
    assert_eq!(loaded_assets[0].asset_kind, "profile");

    let outcome = crate::agent::types::ExternalRuntimeShadowRunOutcome {
        run_id: "shadow-1".to_string(),
        runtime: "hermes".to_string(),
        session_id: session.session_id.clone(),
        workflow: "migration_readiness".to_string(),
        readiness_score: 88,
        blocker_count: 1,
        summary: "summary".to_string(),
        payload: serde_json::json!({"isolated":true}),
        created_at_ms: 1_777_200_000_004,
    };
    store.upsert_external_runtime_shadow_run(&outcome).await?;
    let loaded_runs = store
        .list_external_runtime_shadow_runs(Some("hermes"), Some(&session.session_id))
        .await?;
    assert_eq!(loaded_runs.len(), 1);
    assert_eq!(loaded_runs[0].workflow, "migration_readiness");

    fs::remove_dir_all(root)?;
    Ok(())
}
