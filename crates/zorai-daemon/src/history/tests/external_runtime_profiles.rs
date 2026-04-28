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

    fs::remove_dir_all(root)?;
    Ok(())
}
