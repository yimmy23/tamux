use super::*;

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct HermesTerminalConfig {
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct HermesProviderModelConfig {
    #[serde(default)]
    provider: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct HermesProviderConfig {
    #[serde(default)]
    models: std::collections::HashMap<String, HermesProviderModelConfig>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct HermesConfigDoc {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    terminal: HermesTerminalConfig,
    #[serde(default)]
    providers: std::collections::HashMap<String, HermesProviderConfig>,
    #[serde(default)]
    mcp_servers: std::collections::HashMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct OpenClawModelConfig {
    #[serde(default)]
    primary: Option<String>,
    #[serde(default)]
    fallbacks: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct OpenClawAgentDefaultsConfig {
    #[serde(default)]
    workspace: Option<String>,
    #[serde(default)]
    model: OpenClawModelConfig,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct OpenClawAgentsConfig {
    #[serde(default)]
    defaults: OpenClawAgentDefaultsConfig,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct OpenClawConfigDoc {
    #[serde(default)]
    agents: OpenClawAgentsConfig,
    #[serde(default)]
    mcp_servers: std::collections::HashMap<String, serde_json::Value>,
}

pub(crate) fn parse_hermes_config_profile(
    raw: &str,
    source_config_path: &str,
    imported_at_ms: u64,
) -> anyhow::Result<ExternalRuntimeProfile> {
    let parsed: HermesConfigDoc = serde_yaml::from_str(raw).context("parse Hermes config.yaml")?;

    let model = parsed
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let provider = parsed
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            model.as_deref().and_then(|model_id| {
                parsed
                    .providers
                    .iter()
                    .find_map(|(provider_id, provider_cfg)| {
                        provider_cfg
                            .models
                            .contains_key(model_id)
                            .then(|| provider_id.clone())
                    })
            })
        });

    let cwd = parsed
        .terminal
        .cwd
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let has_zorai_mcp = parsed.mcp_servers.contains_key("zorai");

    Ok(ExternalRuntimeProfile {
        runtime: "hermes".to_string(),
        source_config_path: source_config_path.to_string(),
        provider,
        model,
        cwd,
        has_zorai_mcp,
        imported_at_ms,
    })
}

pub(crate) fn parse_openclaw_config_profile(
    raw: &str,
    source_config_path: &str,
    imported_at_ms: u64,
) -> anyhow::Result<ExternalRuntimeProfile> {
    let parsed: OpenClawConfigDoc =
        serde_json::from_str(raw).context("parse OpenClaw openclaw.json")?;

    let model = parsed
        .agents
        .defaults
        .model
        .primary
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let provider = model.as_deref().and_then(|model_id| {
        model_id
            .split_once('/')
            .map(|(provider, _)| provider.to_string())
    });

    let cwd = parsed
        .agents
        .defaults
        .workspace
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let has_zorai_mcp = parsed.mcp_servers.contains_key("zorai");

    Ok(ExternalRuntimeProfile {
        runtime: "openclaw".to_string(),
        source_config_path: source_config_path.to_string(),
        provider,
        model,
        cwd,
        has_zorai_mcp,
        imported_at_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HERMES_CONFIG_FIXTURE: &str = r#"
provider: openrouter
model: nousresearch/hermes-4-70b
terminal:
  backend: local
  cwd: /workspace/repo
providers:
  openrouter:
    models:
      nousresearch/hermes-4-70b:
        provider: openrouter
mcp_servers:
  zorai:
    command: "/usr/local/bin/zorai-mcp"
    args: []
  github:
    command: "npx"
    args: ["-y", "@modelcontextprotocol/server-github"]
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
    fn hermes_config_parser_extracts_runtime_profile_fields() {
        let profile = parse_hermes_config_profile(
            HERMES_CONFIG_FIXTURE,
            "~/.hermes/config.yaml",
            1_777_200_000_000,
        )
        .expect("Hermes config fixture should parse");

        assert_eq!(profile.runtime, "hermes");
        assert_eq!(profile.source_config_path, "~/.hermes/config.yaml");
        assert_eq!(profile.provider.as_deref(), Some("openrouter"));
        assert_eq!(profile.model.as_deref(), Some("nousresearch/hermes-4-70b"));
        assert_eq!(profile.cwd.as_deref(), Some("/workspace/repo"));
        assert!(profile.has_zorai_mcp);
        assert_eq!(profile.imported_at_ms, 1_777_200_000_000);
    }

    #[test]
    fn openclaw_config_parser_extracts_runtime_profile_fields() {
        let profile = parse_openclaw_config_profile(
            OPENCLAW_CONFIG_FIXTURE,
            "~/.openclaw/openclaw.json",
            1_777_200_000_001,
        )
        .expect("OpenClaw config fixture should parse");

        assert_eq!(profile.runtime, "openclaw");
        assert_eq!(profile.source_config_path, "~/.openclaw/openclaw.json");
        assert_eq!(profile.provider.as_deref(), Some("anthropic"));
        assert_eq!(
            profile.model.as_deref(),
            Some("anthropic/claude-sonnet-4-6")
        );
        assert_eq!(profile.cwd.as_deref(), Some("~/.openclaw/workspace"));
        assert!(profile.has_zorai_mcp);
        assert_eq!(profile.imported_at_ms, 1_777_200_000_001);
    }
}
