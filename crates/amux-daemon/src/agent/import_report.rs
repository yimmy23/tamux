use super::*;

fn parsed_import_profile(
    row: &crate::history::ExternalRuntimeProfileRow,
) -> Option<crate::agent::types::ExternalRuntimeProfile> {
    serde_json::from_str::<crate::agent::types::ExternalRuntimeProfile>(&row.profile_json).ok()
}

fn import_profile_row_json(row: &crate::history::ExternalRuntimeProfileRow) -> serde_json::Value {
    let parsed = parsed_import_profile(row);

    serde_json::json!({
        "runtime": row.runtime,
        "source_config_path": parsed.as_ref().map(|profile| profile.source_config_path.clone()),
        "provider": parsed.as_ref().and_then(|profile| profile.provider.clone()),
        "model": parsed.as_ref().and_then(|profile| profile.model.clone()),
        "cwd": parsed.as_ref().and_then(|profile| profile.cwd.clone()),
        "has_tamux_mcp": parsed.as_ref().map(|profile| profile.has_tamux_mcp).unwrap_or(false),
        "imported_at_ms": parsed.as_ref().map(|profile| profile.imported_at_ms),
        "updated_at": row.updated_at,
    })
}

fn compare_optional_str(imported: Option<&str>, current: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "imported": imported,
        "current": current,
        "matches": imported == current,
    })
}

fn compare_bool(imported: bool, current: bool) -> serde_json::Value {
    serde_json::json!({
        "imported": imported,
        "current": current,
        "matches": imported == current,
    })
}

impl AgentEngine {
    pub(crate) async fn show_import_report_json(
        &self,
        runtime_filter: Option<&str>,
        limit: usize,
    ) -> Result<serde_json::Value> {
        let normalized_filter = runtime_filter
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());
        let effective_limit = limit.max(1);

        let rows = self.history.list_external_runtime_profiles().await?;
        let filtered = rows
            .into_iter()
            .filter(|row| {
                normalized_filter
                    .as_deref()
                    .is_none_or(|runtime| row.runtime.eq_ignore_ascii_case(runtime))
            })
            .take(effective_limit)
            .collect::<Vec<_>>();

        let profiles = filtered
            .iter()
            .map(import_profile_row_json)
            .collect::<Vec<_>>();
        let runtimes = filtered
            .iter()
            .map(|row| row.runtime.clone())
            .collect::<Vec<_>>();
        let with_tamux_mcp = profiles
            .iter()
            .filter(|row| row.get("has_tamux_mcp").and_then(|value| value.as_bool()) == Some(true))
            .count();

        Ok(serde_json::json!({
            "summary": {
                "count": profiles.len(),
                "runtimes": runtimes,
                "with_tamux_mcp": with_tamux_mcp,
                "runtime_filter": normalized_filter,
                "limit": effective_limit,
            },
            "profiles": profiles,
        }))
    }

    pub(crate) async fn preview_shadow_run_json(&self, runtime: &str) -> Result<serde_json::Value> {
        let runtime = runtime.trim().to_ascii_lowercase();
        if !matches!(runtime.as_str(), "hermes" | "openclaw") {
            anyhow::bail!("unsupported runtime '{runtime}' for preview_shadow_run");
        }

        let row = self
            .history
            .get_external_runtime_profile(&runtime)
            .await?
            .ok_or_else(|| anyhow::anyhow!("imported runtime profile for {runtime} not found"))?;
        let imported = parsed_import_profile(&row)
            .ok_or_else(|| anyhow::anyhow!("stored runtime profile for {runtime} is invalid"))?;

        let config = self.config.read().await;
        let current_runtime = config.agent_backend.as_str().to_string();
        let current_provider = config.provider.clone();
        let current_model = config.model.clone();
        let current_cwd = self
            .workspace_root
            .as_ref()
            .map(|path| path.display().to_string());
        drop(config);

        let current_has_tamux_mcp = true;

        Ok(serde_json::json!({
            "runtime": runtime,
            "isolated": true,
            "guardrails": {
                "isolated": true,
                "will_enqueue_tasks": false,
                "will_launch_runner": false,
                "will_spawn_session": false,
            },
            "imported": {
                "runtime": imported.runtime,
                "source_config_path": imported.source_config_path,
                "provider": imported.provider,
                "model": imported.model,
                "cwd": imported.cwd,
                "has_tamux_mcp": imported.has_tamux_mcp,
                "imported_at_ms": imported.imported_at_ms,
                "updated_at": row.updated_at,
            },
            "current": {
                "runtime": current_runtime,
                "provider": current_provider,
                "model": current_model,
                "cwd": current_cwd,
                "has_tamux_mcp": current_has_tamux_mcp,
            },
            "comparison": {
                "runtime": compare_optional_str(Some(imported.runtime.as_str()), Some(current_runtime.as_str())),
                "provider": compare_optional_str(imported.provider.as_deref(), Some(current_provider.as_str())),
                "model": compare_optional_str(imported.model.as_deref(), Some(current_model.as_str())),
                "cwd": compare_optional_str(imported.cwd.as_deref(), current_cwd.as_deref()),
                "has_tamux_mcp": compare_bool(imported.has_tamux_mcp, current_has_tamux_mcp),
            }
        }))
    }
}
