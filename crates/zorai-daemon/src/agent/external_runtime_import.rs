use super::*;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct HermesTerminalConfig {
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct HermesProviderModelConfig {}

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
    #[serde(default)]
    persona: Option<String>,
    #[serde(default)]
    system_prompt: Option<String>,
    #[serde(default)]
    memory: Option<serde_yaml::Value>,
    #[serde(default)]
    routines: Vec<serde_yaml::Value>,
    #[serde(default)]
    jobs: Vec<serde_yaml::Value>,
    #[serde(default)]
    skills: Vec<serde_yaml::Value>,
    #[serde(default)]
    templates: Vec<serde_yaml::Value>,
    #[serde(default)]
    connectors: std::collections::HashMap<String, serde_yaml::Value>,
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
    #[serde(default)]
    persona: Option<String>,
    #[serde(default)]
    system_prompt: Option<String>,
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
    #[serde(default)]
    memory: Option<serde_json::Value>,
    #[serde(default)]
    routines: Vec<serde_json::Value>,
    #[serde(default)]
    jobs: Vec<serde_json::Value>,
    #[serde(default)]
    skills: Vec<serde_json::Value>,
    #[serde(default)]
    templates: Vec<serde_json::Value>,
    #[serde(default)]
    connectors: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
struct ArchiveSeed {
    thread_id: String,
    query_hint: String,
    summary: String,
    content: String,
    metadata: serde_json::Value,
}

#[derive(Debug, Clone)]
struct ParsedImportBundle {
    profile: ExternalRuntimeProfile,
    assets: Vec<ImportedRuntimeAsset>,
    archive: ArchiveSeed,
    notes: Vec<String>,
}

fn runtime_default_config_path(runtime: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    match runtime {
        "hermes" => Some(home.join(".hermes/config.yaml")),
        "openclaw" => Some(home.join(".openclaw/openclaw.json")),
        _ => None,
    }
}

fn expand_home_path(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}

fn now_ms() -> u64 {
    crate::history::now_ts()
}

fn source_fingerprint(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn yaml_to_json(value: &serde_yaml::Value) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or_else(|_| serde_json::json!(null))
}

fn trimmed_option(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn unique_sorted_strings(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut values = values
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn recommend_for_asset(kind: &str, bucket: ExternalRuntimeAssetBucket) -> Option<String> {
    match (kind, bucket) {
        ("connectors", ExternalRuntimeAssetBucket::ManualActionRequired) => {
            Some("re-auth connector".to_string())
        }
        ("mcp", ExternalRuntimeAssetBucket::ManualActionRequired) => {
            Some("enable matching plugin".to_string())
        }
        ("memory", ExternalRuntimeAssetBucket::Imported) => {
            Some("review staged memory".to_string())
        }
        (_, ExternalRuntimeAssetBucket::Unsupported) => {
            Some("unsupported in current build".to_string())
        }
        (_, ExternalRuntimeAssetBucket::Missing) => Some("manual mapping needed".to_string()),
        _ => None,
    }
}

fn asset(
    session_id: &str,
    runtime: &str,
    asset_kind: &str,
    bucket: ExternalRuntimeAssetBucket,
    severity: ExternalRuntimeReportSeverity,
    reason: Option<String>,
    source_path: Option<String>,
    source_fingerprint: &str,
    conflict_policy: ExternalRuntimeConflictPolicy,
    payload: serde_json::Value,
    created_at_ms: u64,
) -> ImportedRuntimeAsset {
    ImportedRuntimeAsset {
        asset_id: format!("imported-asset-{}", Uuid::new_v4()),
        session_id: session_id.to_string(),
        runtime: runtime.to_string(),
        asset_kind: asset_kind.to_string(),
        bucket,
        severity,
        recommended_action: recommend_for_asset(asset_kind, bucket),
        reason,
        source_path,
        source_fingerprint: Some(source_fingerprint.to_string()),
        conflict_policy,
        archive_thread_id: None,
        archive_query_hint: None,
        payload,
        created_at_ms,
    }
}

fn archive_asset(
    session_id: &str,
    runtime: &str,
    source_path: &str,
    source_fingerprint: &str,
    conflict_policy: ExternalRuntimeConflictPolicy,
    created_at_ms: u64,
    archive: &ArchiveSeed,
) -> ImportedRuntimeAsset {
    ImportedRuntimeAsset {
        asset_id: format!("imported-asset-{}", Uuid::new_v4()),
        session_id: session_id.to_string(),
        runtime: runtime.to_string(),
        asset_kind: "archive".to_string(),
        bucket: ExternalRuntimeAssetBucket::Imported,
        severity: ExternalRuntimeReportSeverity::Informational,
        recommended_action: None,
        reason: Some("searchable migration archive snapshot created".to_string()),
        source_path: Some(source_path.to_string()),
        source_fingerprint: Some(source_fingerprint.to_string()),
        conflict_policy,
        archive_thread_id: Some(archive.thread_id.clone()),
        archive_query_hint: Some(archive.query_hint.clone()),
        payload: serde_json::json!({
            "thread_id": archive.thread_id,
            "query_hint": archive.query_hint,
            "summary": archive.summary,
        }),
        created_at_ms,
    }
}

fn profile_asset_payload(profile: &ExternalRuntimeProfile) -> serde_json::Value {
    serde_json::json!({
        "runtime": profile.runtime,
        "source_config_path": profile.source_config_path,
        "provider": profile.provider,
        "model": profile.model,
        "cwd": profile.cwd,
        "has_zorai_mcp": profile.has_zorai_mcp,
        "imported_at_ms": profile.imported_at_ms,
    })
}

fn parse_memory_hint_hermes(doc: &HermesConfigDoc) -> Option<serde_json::Value> {
    if let Some(memory) = doc.memory.as_ref() {
        return Some(yaml_to_json(memory));
    }
    let hint = trimmed_option(doc.persona.as_deref())
        .or_else(|| trimmed_option(doc.system_prompt.as_deref()));
    hint.map(|text| serde_json::json!({ "hint": text }))
}

fn parse_memory_hint_openclaw(doc: &OpenClawConfigDoc) -> Option<serde_json::Value> {
    if let Some(memory) = doc.memory.clone() {
        return Some(memory);
    }
    let hint = trimmed_option(doc.agents.defaults.persona.as_deref())
        .or_else(|| trimmed_option(doc.agents.defaults.system_prompt.as_deref()));
    hint.map(|text| serde_json::json!({ "hint": text }))
}

fn build_archive_seed(
    runtime: &str,
    session_id: &str,
    source_path: &str,
    created_at_ms: u64,
    asset_kinds: &[String],
) -> ArchiveSeed {
    let thread_id = format!("imported-runtime:{runtime}:{session_id}");
    let query_hint = runtime.to_string();
    let summary = format!("Imported {runtime} migration snapshot from {source_path}");
    let content = serde_json::to_string_pretty(&serde_json::json!({
        "runtime": runtime,
        "session_id": session_id,
        "source_config_path": source_path,
        "created_at_ms": created_at_ms,
        "asset_kinds": asset_kinds,
    }))
    .unwrap_or_else(|_| summary.clone());
    let metadata = serde_json::json!({
        "runtime": runtime,
        "session_id": session_id,
        "source_config_path": source_path,
        "import_kind": "external_runtime_snapshot",
    });
    ArchiveSeed {
        thread_id,
        query_hint,
        summary,
        content,
        metadata,
    }
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

fn extract_hermes_bundle(
    raw: &str,
    source_config_path: &str,
    created_at_ms: u64,
    session_id: &str,
    conflict_policy: ExternalRuntimeConflictPolicy,
    source_fp: &str,
) -> Result<ParsedImportBundle> {
    let parsed: HermesConfigDoc = serde_yaml::from_str(raw).context("parse Hermes config.yaml")?;
    let profile = parse_hermes_config_profile(raw, source_config_path, created_at_ms)?;
    let mut assets = Vec::new();
    let mut notes = Vec::new();

    assets.push(asset(
        session_id,
        "hermes",
        "profile",
        ExternalRuntimeAssetBucket::Imported,
        ExternalRuntimeReportSeverity::Safe,
        Some("runtime profile parsed successfully".to_string()),
        Some(source_config_path.to_string()),
        source_fp,
        conflict_policy,
        profile_asset_payload(&profile),
        created_at_ms,
    ));

    assets.push(asset(
        session_id,
        "hermes",
        "settings",
        ExternalRuntimeAssetBucket::Imported,
        ExternalRuntimeReportSeverity::Safe,
        Some("provider/model/workspace settings extracted".to_string()),
        Some(source_config_path.to_string()),
        source_fp,
        conflict_policy,
        serde_json::json!({
            "provider": profile.provider,
            "model": profile.model,
            "cwd": profile.cwd,
            "providers": parsed.providers.keys().cloned().collect::<Vec<_>>(),
        }),
        created_at_ms,
    ));

    if let Some(memory) = parse_memory_hint_hermes(&parsed) {
        assets.push(asset(
            session_id,
            "hermes",
            "memory",
            ExternalRuntimeAssetBucket::Imported,
            ExternalRuntimeReportSeverity::Informational,
            Some("persona or memory hints staged for review".to_string()),
            Some(source_config_path.to_string()),
            source_fp,
            conflict_policy,
            memory,
            created_at_ms,
        ));
    } else {
        assets.push(asset(
            session_id,
            "hermes",
            "memory",
            ExternalRuntimeAssetBucket::Missing,
            ExternalRuntimeReportSeverity::Informational,
            Some("no memory or persona hints were exposed by the source config".to_string()),
            Some(source_config_path.to_string()),
            source_fp,
            conflict_policy,
            serde_json::json!({ "available": false }),
            created_at_ms,
        ));
    }

    let routines = parsed
        .routines
        .iter()
        .chain(parsed.jobs.iter())
        .map(yaml_to_json)
        .collect::<Vec<_>>();
    if routines.is_empty() {
        assets.push(asset(
            session_id,
            "hermes",
            "routines",
            ExternalRuntimeAssetBucket::Missing,
            ExternalRuntimeReportSeverity::Informational,
            Some("no reusable routines or jobs detected in Hermes config".to_string()),
            Some(source_config_path.to_string()),
            source_fp,
            conflict_policy,
            serde_json::json!({ "count": 0 }),
            created_at_ms,
        ));
    } else {
        assets.push(asset(
            session_id,
            "hermes",
            "routines",
            ExternalRuntimeAssetBucket::Imported,
            ExternalRuntimeReportSeverity::Informational,
            Some("routine and job definitions imported as staged metadata".to_string()),
            Some(source_config_path.to_string()),
            source_fp,
            conflict_policy,
            serde_json::json!({ "count": routines.len(), "items": routines }),
            created_at_ms,
        ));
    }

    let skills = parsed
        .skills
        .iter()
        .chain(parsed.templates.iter())
        .map(yaml_to_json)
        .collect::<Vec<_>>();
    if skills.is_empty() {
        assets.push(asset(
            session_id,
            "hermes",
            "skills",
            ExternalRuntimeAssetBucket::Missing,
            ExternalRuntimeReportSeverity::Informational,
            Some("no Hermes skill/template payloads were detected".to_string()),
            Some(source_config_path.to_string()),
            source_fp,
            conflict_policy,
            serde_json::json!({ "count": 0 }),
            created_at_ms,
        ));
    } else {
        assets.push(asset(
            session_id,
            "hermes",
            "skills",
            ExternalRuntimeAssetBucket::Imported,
            ExternalRuntimeReportSeverity::Informational,
            Some("skill/template metadata imported for review".to_string()),
            Some(source_config_path.to_string()),
            source_fp,
            conflict_policy,
            serde_json::json!({ "count": skills.len(), "items": skills }),
            created_at_ms,
        ));
    }

    let connector_names = unique_sorted_strings(
        parsed
            .mcp_servers
            .keys()
            .filter(|name| name.as_str() != "zorai")
            .cloned()
            .chain(parsed.connectors.keys().cloned()),
    );
    if connector_names.is_empty() {
        assets.push(asset(
            session_id,
            "hermes",
            "connectors",
            ExternalRuntimeAssetBucket::Missing,
            ExternalRuntimeReportSeverity::Informational,
            Some("no external connector hints detected".to_string()),
            Some(source_config_path.to_string()),
            source_fp,
            conflict_policy,
            serde_json::json!({ "count": 0, "names": [] }),
            created_at_ms,
        ));
    } else {
        notes.push(format!(
            "{} connector hint(s) require follow-up authentication",
            connector_names.len()
        ));
        assets.push(asset(
            session_id,
            "hermes",
            "connectors",
            ExternalRuntimeAssetBucket::ManualActionRequired,
            ExternalRuntimeReportSeverity::Warning,
            Some("connector hints imported but credentials are not migrated".to_string()),
            Some(source_config_path.to_string()),
            source_fp,
            conflict_policy,
            serde_json::json!({ "count": connector_names.len(), "names": connector_names }),
            created_at_ms,
        ));
    }

    let mcp_names = unique_sorted_strings(parsed.mcp_servers.keys().cloned());
    let mcp_bucket = if profile.has_zorai_mcp {
        ExternalRuntimeAssetBucket::Mapped
    } else {
        ExternalRuntimeAssetBucket::ManualActionRequired
    };
    let mcp_severity = if profile.has_zorai_mcp {
        ExternalRuntimeReportSeverity::Safe
    } else {
        ExternalRuntimeReportSeverity::Warning
    };
    let mcp_reason = if profile.has_zorai_mcp {
        Some("zorai MCP wiring detected in source runtime".to_string())
    } else {
        Some("source runtime does not expose zorai MCP configuration".to_string())
    };
    assets.push(asset(
        session_id,
        "hermes",
        "mcp",
        mcp_bucket,
        mcp_severity,
        mcp_reason,
        Some(source_config_path.to_string()),
        source_fp,
        conflict_policy,
        serde_json::json!({ "count": mcp_names.len(), "names": mcp_names, "has_zorai_mcp": profile.has_zorai_mcp }),
        created_at_ms,
    ));

    let archive = build_archive_seed(
        "hermes",
        session_id,
        source_config_path,
        created_at_ms,
        &assets
            .iter()
            .map(|item| item.asset_kind.clone())
            .collect::<Vec<_>>(),
    );
    assets.push(archive_asset(
        session_id,
        "hermes",
        source_config_path,
        source_fp,
        conflict_policy,
        created_at_ms,
        &archive,
    ));

    Ok(ParsedImportBundle {
        profile,
        assets,
        archive,
        notes,
    })
}

fn extract_openclaw_bundle(
    raw: &str,
    source_config_path: &str,
    created_at_ms: u64,
    session_id: &str,
    conflict_policy: ExternalRuntimeConflictPolicy,
    source_fp: &str,
) -> Result<ParsedImportBundle> {
    let parsed: OpenClawConfigDoc =
        serde_json::from_str(raw).context("parse OpenClaw openclaw.json")?;
    let profile = parse_openclaw_config_profile(raw, source_config_path, created_at_ms)?;
    let mut assets = Vec::new();
    let mut notes = Vec::new();

    assets.push(asset(
        session_id,
        "openclaw",
        "profile",
        ExternalRuntimeAssetBucket::Imported,
        ExternalRuntimeReportSeverity::Safe,
        Some("runtime profile parsed successfully".to_string()),
        Some(source_config_path.to_string()),
        source_fp,
        conflict_policy,
        profile_asset_payload(&profile),
        created_at_ms,
    ));

    assets.push(asset(
        session_id,
        "openclaw",
        "settings",
        ExternalRuntimeAssetBucket::Imported,
        ExternalRuntimeReportSeverity::Safe,
        Some("agent defaults and model settings extracted".to_string()),
        Some(source_config_path.to_string()),
        source_fp,
        conflict_policy,
        serde_json::json!({
            "provider": profile.provider,
            "model": profile.model,
            "cwd": profile.cwd,
            "fallbacks": parsed.agents.defaults.model.fallbacks,
        }),
        created_at_ms,
    ));

    if let Some(memory) = parse_memory_hint_openclaw(&parsed) {
        assets.push(asset(
            session_id,
            "openclaw",
            "memory",
            ExternalRuntimeAssetBucket::Imported,
            ExternalRuntimeReportSeverity::Informational,
            Some("memory/persona hints staged for review".to_string()),
            Some(source_config_path.to_string()),
            source_fp,
            conflict_policy,
            memory,
            created_at_ms,
        ));
    } else {
        assets.push(asset(
            session_id,
            "openclaw",
            "memory",
            ExternalRuntimeAssetBucket::Missing,
            ExternalRuntimeReportSeverity::Informational,
            Some("no memory/persona hints detected in OpenClaw config".to_string()),
            Some(source_config_path.to_string()),
            source_fp,
            conflict_policy,
            serde_json::json!({ "available": false }),
            created_at_ms,
        ));
    }

    let routines = parsed
        .routines
        .iter()
        .chain(parsed.jobs.iter())
        .cloned()
        .collect::<Vec<_>>();
    if routines.is_empty() {
        assets.push(asset(
            session_id,
            "openclaw",
            "routines",
            ExternalRuntimeAssetBucket::Missing,
            ExternalRuntimeReportSeverity::Informational,
            Some("no persisted OpenClaw routines/jobs detected".to_string()),
            Some(source_config_path.to_string()),
            source_fp,
            conflict_policy,
            serde_json::json!({ "count": 0 }),
            created_at_ms,
        ));
    } else {
        assets.push(asset(
            session_id,
            "openclaw",
            "routines",
            ExternalRuntimeAssetBucket::Imported,
            ExternalRuntimeReportSeverity::Informational,
            Some("routine/job metadata imported for review".to_string()),
            Some(source_config_path.to_string()),
            source_fp,
            conflict_policy,
            serde_json::json!({ "count": routines.len(), "items": routines }),
            created_at_ms,
        ));
    }

    let skills = parsed
        .skills
        .iter()
        .chain(parsed.templates.iter())
        .cloned()
        .collect::<Vec<_>>();
    if skills.is_empty() {
        assets.push(asset(
            session_id,
            "openclaw",
            "skills",
            ExternalRuntimeAssetBucket::Missing,
            ExternalRuntimeReportSeverity::Informational,
            Some("no OpenClaw skill/template payloads were detected".to_string()),
            Some(source_config_path.to_string()),
            source_fp,
            conflict_policy,
            serde_json::json!({ "count": 0 }),
            created_at_ms,
        ));
    } else {
        assets.push(asset(
            session_id,
            "openclaw",
            "skills",
            ExternalRuntimeAssetBucket::Imported,
            ExternalRuntimeReportSeverity::Informational,
            Some("skill/template metadata imported for review".to_string()),
            Some(source_config_path.to_string()),
            source_fp,
            conflict_policy,
            serde_json::json!({ "count": skills.len(), "items": skills }),
            created_at_ms,
        ));
    }

    let connector_names = unique_sorted_strings(
        parsed
            .mcp_servers
            .keys()
            .filter(|name| name.as_str() != "zorai")
            .cloned()
            .chain(parsed.connectors.keys().cloned()),
    );
    if connector_names.is_empty() {
        assets.push(asset(
            session_id,
            "openclaw",
            "connectors",
            ExternalRuntimeAssetBucket::Missing,
            ExternalRuntimeReportSeverity::Informational,
            Some("no external connector hints detected".to_string()),
            Some(source_config_path.to_string()),
            source_fp,
            conflict_policy,
            serde_json::json!({ "count": 0, "names": [] }),
            created_at_ms,
        ));
    } else {
        notes.push(format!(
            "{} connector hint(s) require re-authentication",
            connector_names.len()
        ));
        assets.push(asset(
            session_id,
            "openclaw",
            "connectors",
            ExternalRuntimeAssetBucket::ManualActionRequired,
            ExternalRuntimeReportSeverity::Warning,
            Some("connector hints imported but credentials remain manual".to_string()),
            Some(source_config_path.to_string()),
            source_fp,
            conflict_policy,
            serde_json::json!({ "count": connector_names.len(), "names": connector_names }),
            created_at_ms,
        ));
    }

    let mcp_names = unique_sorted_strings(parsed.mcp_servers.keys().cloned());
    let mcp_bucket = if profile.has_zorai_mcp {
        ExternalRuntimeAssetBucket::Mapped
    } else {
        ExternalRuntimeAssetBucket::ManualActionRequired
    };
    let mcp_severity = if profile.has_zorai_mcp {
        ExternalRuntimeReportSeverity::Safe
    } else {
        ExternalRuntimeReportSeverity::Warning
    };
    assets.push(asset(
        session_id,
        "openclaw",
        "mcp",
        mcp_bucket,
        mcp_severity,
        Some(if profile.has_zorai_mcp {
            "zorai MCP wiring detected in source runtime".to_string()
        } else {
            "source runtime does not expose zorai MCP wiring".to_string()
        }),
        Some(source_config_path.to_string()),
        source_fp,
        conflict_policy,
        serde_json::json!({ "count": mcp_names.len(), "names": mcp_names, "has_zorai_mcp": profile.has_zorai_mcp }),
        created_at_ms,
    ));

    let archive = build_archive_seed(
        "openclaw",
        session_id,
        source_config_path,
        created_at_ms,
        &assets
            .iter()
            .map(|item| item.asset_kind.clone())
            .collect::<Vec<_>>(),
    );
    assets.push(archive_asset(
        session_id,
        "openclaw",
        source_config_path,
        source_fp,
        conflict_policy,
        created_at_ms,
        &archive,
    ));

    Ok(ParsedImportBundle {
        profile,
        assets,
        archive,
        notes,
    })
}

fn summarize_assets(assets: &[ImportedRuntimeAsset]) -> serde_json::Value {
    let mut by_kind = serde_json::Map::new();
    let mut buckets = std::collections::BTreeMap::<String, usize>::new();
    let mut severities = std::collections::BTreeMap::<String, usize>::new();
    for asset in assets {
        *buckets
            .entry(asset.bucket.as_str().to_string())
            .or_default() += 1;
        *severities
            .entry(asset.severity.as_str().to_string())
            .or_default() += 1;
        by_kind.insert(
            asset.asset_kind.clone(),
            serde_json::json!({
                "bucket": asset.bucket.as_str(),
                "severity": asset.severity.as_str(),
                "recommended_action": asset.recommended_action,
                "reason": asset.reason,
            }),
        );
    }
    serde_json::json!({
        "count": assets.len(),
        "by_kind": by_kind,
        "buckets": buckets,
        "severities": severities,
    })
}

impl AgentEngine {
    pub(crate) async fn external_runtime_migration_status_json(&self) -> serde_json::Value {
        let mut runtimes = Vec::new();
        for runtime in ["hermes", "openclaw"] {
            let default_config_path = runtime_default_config_path(runtime);
            let default_config_path_text = default_config_path
                .as_ref()
                .map(|path| path.display().to_string());
            let config_exists = default_config_path
                .as_ref()
                .is_some_and(|path| path.exists());
            let status = self.external_agent_status(runtime).await;
            runtimes.push(serde_json::json!({
                "runtime": runtime,
                "installed": status.as_ref().is_some_and(|status| status.available),
                "executable": status.as_ref().and_then(|status| status.executable.clone()),
                "default_config_path": default_config_path_text,
                "config_exists": config_exists,
                "has_zorai_mcp": status.as_ref().is_some_and(|status| status.has_zorai_mcp),
                "migration_source": true,
                "can_preview": config_exists,
                "can_apply": config_exists,
            }));
        }

        serde_json::json!({
            "runtime": "daemon",
            "daemon_only": true,
            "sources": runtimes,
        })
    }

    pub(crate) async fn import_external_runtime_json(
        &self,
        runtime: &str,
        config_path: Option<&str>,
        dry_run: bool,
        conflict_policy: ExternalRuntimeConflictPolicy,
    ) -> Result<serde_json::Value> {
        let runtime = runtime.trim().to_ascii_lowercase();
        if !matches!(runtime.as_str(), "hermes" | "openclaw") {
            anyhow::bail!("unsupported runtime '{runtime}' for import_external_runtime");
        }

        let resolved_path = config_path
            .map(expand_home_path)
            .or_else(|| runtime_default_config_path(&runtime))
            .ok_or_else(|| {
                anyhow::anyhow!("unable to resolve config path for runtime '{runtime}'")
            })?;
        let source_config_path = resolved_path.display().to_string();

        let raw = tokio::fs::read_to_string(&resolved_path)
            .await
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::NotFound => {
                    anyhow::anyhow!("config file not found: {source_config_path}")
                }
                std::io::ErrorKind::PermissionDenied => {
                    anyhow::anyhow!("permission denied reading config file: {source_config_path}")
                }
                _ => anyhow::anyhow!("failed to read config file {source_config_path}: {error}"),
            })?;
        let created_at_ms = now_ms();
        let source_fp = source_fingerprint(&raw);

        if let Some(existing) = self
            .history
            .find_external_runtime_import_session_by_fingerprint(
                &runtime,
                &source_config_path,
                &source_fp,
                dry_run,
            )
            .await?
        {
            let session =
                serde_json::from_str::<ExternalRuntimeImportSession>(&existing.session_json)
                    .unwrap_or(ExternalRuntimeImportSession {
                        session_id: existing.session_id.clone(),
                        runtime: existing.runtime.clone(),
                        source_config_path: existing.source_config_path.clone(),
                        source_fingerprint: existing.source_fingerprint.clone(),
                        dry_run: existing.dry_run,
                        conflict_policy: existing.conflict_policy.parse().unwrap_or_default(),
                        source_surface: existing.source_surface.clone(),
                        imported_at_ms: existing.imported_at_ms,
                        schema_version: 1,
                        asset_count: 0,
                        notes: vec![],
                    });
            let assets = self
                .history
                .list_imported_runtime_assets(Some(&runtime), Some(&existing.session_id))
                .await?
                .into_iter()
                .filter_map(|row| {
                    serde_json::from_str::<ImportedRuntimeAsset>(&row.asset_json).ok()
                })
                .collect::<Vec<_>>();
            let profile = self
                .history
                .get_external_runtime_profile(&runtime)
                .await?
                .and_then(|row| {
                    serde_json::from_str::<ExternalRuntimeProfile>(&row.profile_json).ok()
                });
            return Ok(serde_json::json!({
                "runtime": runtime,
                "dry_run": dry_run,
                "persisted": !dry_run,
                "idempotent": true,
                "session": session,
                "profile": profile,
                "asset_summary": summarize_assets(&assets),
                "assets": assets,
                "notes": ["reused existing import session by source fingerprint"],
            }));
        }

        let session_id = format!("import-session-{}", Uuid::new_v4());
        let bundle = match runtime.as_str() {
            "hermes" => extract_hermes_bundle(
                &raw,
                &source_config_path,
                created_at_ms,
                &session_id,
                conflict_policy,
                &source_fp,
            )?,
            "openclaw" => extract_openclaw_bundle(
                &raw,
                &source_config_path,
                created_at_ms,
                &session_id,
                conflict_policy,
                &source_fp,
            )?,
            _ => unreachable!(),
        };

        let session = ExternalRuntimeImportSession {
            session_id: session_id.clone(),
            runtime: runtime.clone(),
            source_config_path: source_config_path.clone(),
            source_fingerprint: source_fp.clone(),
            dry_run,
            conflict_policy,
            source_surface: "tool_import_external_runtime".to_string(),
            imported_at_ms: created_at_ms,
            schema_version: 1,
            asset_count: bundle.assets.len() as u32,
            notes: bundle.notes.clone(),
        };

        if !dry_run {
            self.history
                .upsert_external_runtime_import_session(&session)
                .await?;
            self.history
                .upsert_external_runtime_profile_with_provenance(
                    &runtime,
                    &bundle.profile,
                    Some(&session_id),
                    Some(&source_fp),
                )
                .await?;
            self.history
                .replace_imported_runtime_assets(&session_id, &bundle.assets)
                .await?;
            self.history
                .insert_context_archive(
                    &format!("context-archive-{}", Uuid::new_v4()),
                    &bundle.archive.thread_id,
                    Some("tool"),
                    &bundle.archive.content,
                    Some(&bundle.archive.summary),
                    1.0,
                    bundle.archive.content.len() as u32,
                    bundle.archive.content.len() as u32,
                    Some(&bundle.archive.metadata.to_string()),
                    created_at_ms,
                )
                .await?;
        }

        Ok(serde_json::json!({
            "runtime": runtime,
            "dry_run": dry_run,
            "persisted": !dry_run,
            "idempotent": false,
            "session": session,
            "profile": bundle.profile,
            "asset_summary": summarize_assets(&bundle.assets),
            "assets": bundle.assets,
            "archive_search": {
                "thread_id": bundle.archive.thread_id,
                "query_hint": bundle.archive.query_hint,
            },
            "notes": bundle.notes,
        }))
    }
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
persona: helpful migration assistant
"#;

    const OPENCLAW_CONFIG_FIXTURE: &str = r#"{
  "agents": {
    "defaults": {
      "workspace": "~/.openclaw/workspace",
      "persona": "task finisher",
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

    #[test]
    fn extracted_hermes_bundle_covers_expected_asset_classes() {
        let session_id = "import-session-test";
        let bundle = extract_hermes_bundle(
            HERMES_CONFIG_FIXTURE,
            "~/.hermes/config.yaml",
            1_777_200_000_000,
            session_id,
            ExternalRuntimeConflictPolicy::StageForReview,
            "fingerprint",
        )
        .expect("bundle should parse");

        let kinds = bundle
            .assets
            .iter()
            .map(|asset| asset.asset_kind.as_str())
            .collect::<Vec<_>>();
        for expected in [
            "profile",
            "settings",
            "memory",
            "routines",
            "skills",
            "connectors",
            "mcp",
            "archive",
        ] {
            assert!(kinds.contains(&expected), "missing asset kind {expected}");
        }
    }
}
