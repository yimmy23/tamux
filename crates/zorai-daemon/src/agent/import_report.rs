use super::*;

fn parsed_import_profile(
    row: &crate::history::ExternalRuntimeProfileRow,
) -> Option<crate::agent::types::ExternalRuntimeProfile> {
    serde_json::from_str::<crate::agent::types::ExternalRuntimeProfile>(&row.profile_json).ok()
}

fn parsed_import_session(
    row: &crate::history::ExternalRuntimeImportSessionRow,
) -> Option<crate::agent::types::ExternalRuntimeImportSession> {
    serde_json::from_str::<crate::agent::types::ExternalRuntimeImportSession>(&row.session_json)
        .ok()
}

fn parsed_import_asset(
    row: &crate::history::ImportedRuntimeAssetRow,
) -> Option<crate::agent::types::ImportedRuntimeAsset> {
    serde_json::from_str::<crate::agent::types::ImportedRuntimeAsset>(&row.asset_json).ok()
}

fn parsed_shadow_run(
    row: &crate::history::ExternalRuntimeShadowRunRow,
) -> Option<crate::agent::types::ExternalRuntimeShadowRunOutcome> {
    serde_json::from_str::<crate::agent::types::ExternalRuntimeShadowRunOutcome>(&row.payload_json)
        .ok()
}

fn import_profile_row_json(row: &crate::history::ExternalRuntimeProfileRow) -> serde_json::Value {
    let parsed = parsed_import_profile(row);

    serde_json::json!({
        "runtime": row.runtime,
        "session_id": row.session_id,
        "source_config_path": parsed
            .as_ref()
            .map(|profile| profile.source_config_path.clone())
            .or_else(|| row.source_config_path.clone()),
        "source_fingerprint": row.source_fingerprint,
        "provider": parsed.as_ref().and_then(|profile| profile.provider.clone()),
        "model": parsed.as_ref().and_then(|profile| profile.model.clone()),
        "cwd": parsed.as_ref().and_then(|profile| profile.cwd.clone()),
        "has_zorai_mcp": parsed.as_ref().map(|profile| profile.has_zorai_mcp).unwrap_or(false),
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

fn compare_count(imported: usize, current: usize) -> serde_json::Value {
    serde_json::json!({
        "imported": imported,
        "current": current,
        "matches": imported == current,
    })
}

fn severity_rank(severity: &str) -> u8 {
    match severity {
        "blocking" => 4,
        "warning" => 3,
        "informational" => 2,
        "safe" => 1,
        _ => 0,
    }
}

fn readiness_score(assets: &[crate::agent::types::ImportedRuntimeAsset]) -> u8 {
    if assets.is_empty() {
        return 0;
    }
    let blocking = assets
        .iter()
        .filter(|asset| asset.severity == ExternalRuntimeReportSeverity::Blocking)
        .count();
    let warning = assets
        .iter()
        .filter(|asset| asset.severity == ExternalRuntimeReportSeverity::Warning)
        .count();
    let missing = assets
        .iter()
        .filter(|asset| asset.bucket == ExternalRuntimeAssetBucket::Missing)
        .count();
    let total = assets.len() as i32;
    let penalty = (blocking as i32 * 25) + (warning as i32 * 12) + (missing as i32 * 6);
    (100 - penalty.clamp(0, 100).min(total * 20)).clamp(0, 100) as u8
}

fn summarize_bucket_counts(
    assets: &[crate::agent::types::ImportedRuntimeAsset],
) -> serde_json::Value {
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for asset in assets {
        *counts.entry(asset.bucket.as_str().to_string()).or_default() += 1;
    }
    serde_json::json!(counts)
}

fn summarize_severity_counts(
    assets: &[crate::agent::types::ImportedRuntimeAsset],
) -> serde_json::Value {
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for asset in assets {
        *counts
            .entry(asset.severity.as_str().to_string())
            .or_default() += 1;
    }
    serde_json::json!(counts)
}

fn asset_row_json(asset: &crate::agent::types::ImportedRuntimeAsset) -> serde_json::Value {
    serde_json::json!({
        "asset_id": asset.asset_id,
        "asset_kind": asset.asset_kind,
        "bucket": asset.bucket,
        "severity": asset.severity,
        "recommended_action": asset.recommended_action,
        "reason": asset.reason,
        "source_path": asset.source_path,
        "archive_thread_id": asset.archive_thread_id,
        "archive_query_hint": asset.archive_query_hint,
        "payload": asset.payload,
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
        let with_zorai_mcp = profiles
            .iter()
            .filter(|row| row.get("has_zorai_mcp").and_then(|value| value.as_bool()) == Some(true))
            .count();

        let mut profile_reports = Vec::new();
        let mut blockers = Vec::new();
        let mut recommended_actions = Vec::new();
        let mut latest_readiness = 0u8;

        for row in &filtered {
            let runtime = row.runtime.clone();
            let profile = parsed_import_profile(row);
            let session_row = if let Some(session_id) = row.session_id.as_deref() {
                self.history
                    .get_external_runtime_import_session(session_id)
                    .await?
            } else {
                None
            };
            let session = session_row.and_then(|session_row| parsed_import_session(&session_row));
            let assets = self
                .history
                .list_imported_runtime_assets(Some(&runtime), row.session_id.as_deref())
                .await?
                .into_iter()
                .filter_map(|asset_row| parsed_import_asset(&asset_row))
                .collect::<Vec<_>>();
            let asset_json = assets.iter().map(asset_row_json).collect::<Vec<_>>();
            let report_blockers = assets
                .iter()
                .filter(|asset| {
                    asset.severity == ExternalRuntimeReportSeverity::Blocking
                        || asset.bucket == ExternalRuntimeAssetBucket::ManualActionRequired
                })
                .map(|asset| {
                    serde_json::json!({
                        "runtime": runtime,
                        "asset_kind": asset.asset_kind,
                        "severity": asset.severity,
                        "reason": asset.reason,
                        "recommended_action": asset.recommended_action,
                    })
                })
                .collect::<Vec<_>>();
            let report_actions = assets
                .iter()
                .filter_map(|asset| asset.recommended_action.clone())
                .collect::<Vec<_>>();
            let readiness = readiness_score(&assets);
            latest_readiness = latest_readiness.max(readiness);
            blockers.extend(report_blockers);
            recommended_actions.extend(report_actions);

            profile_reports.push(serde_json::json!({
                "runtime": runtime,
                "profile": profile,
                "session": session,
                "assets": asset_json,
                "asset_buckets": summarize_bucket_counts(&assets),
                "severity_counts": summarize_severity_counts(&assets),
                "blockers": assets.iter().filter(|asset| asset.severity == ExternalRuntimeReportSeverity::Blocking).map(|asset| asset_row_json(asset)).collect::<Vec<_>>(),
                "readiness": {
                    "score": readiness,
                    "ready": readiness >= 70,
                },
            }));
        }

        blockers.sort_by(|left, right| {
            let left_rank = left
                .get("severity")
                .and_then(|value| value.as_str())
                .map(severity_rank)
                .unwrap_or(0);
            let right_rank = right
                .get("severity")
                .and_then(|value| value.as_str())
                .map(severity_rank)
                .unwrap_or(0);
            right_rank.cmp(&left_rank)
        });
        recommended_actions.sort();
        recommended_actions.dedup();

        Ok(serde_json::json!({
            "summary": {
                "count": profiles.len(),
                "runtimes": runtimes,
                "with_zorai_mcp": with_zorai_mcp,
                "runtime_filter": normalized_filter,
                "limit": effective_limit,
                "blocker_count": blockers.len(),
                "recommended_actions": recommended_actions,
                "readiness_score": latest_readiness,
                "migration_ready": latest_readiness >= 70 && blockers.is_empty(),
            },
            "profiles": profiles,
            "reports": profile_reports,
            "blockers": blockers,
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
        let session_id = row
            .session_id
            .clone()
            .ok_or_else(|| anyhow::anyhow!("import session provenance for {runtime} not found"))?;
        let assets = self
            .history
            .list_imported_runtime_assets(Some(&runtime), Some(&session_id))
            .await?
            .into_iter()
            .filter_map(|asset_row| parsed_import_asset(&asset_row))
            .collect::<Vec<_>>();

        let config = self.config.read().await;
        let current_runtime = config.agent_backend.as_str().to_string();
        let current_provider = config.provider.clone();
        let current_model = config.model.clone();
        let current_cwd = self
            .workspace_root
            .as_ref()
            .map(|path| path.display().to_string());
        drop(config);

        let current_has_zorai_mcp = true;
        let connectors = assets
            .iter()
            .find(|asset| asset.asset_kind == "connectors")
            .map(|asset| {
                asset
                    .payload
                    .get("names")
                    .and_then(|value| value.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.as_str())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        let blockers = assets
            .iter()
            .filter(|asset| {
                asset.severity == ExternalRuntimeReportSeverity::Blocking
                    || asset.bucket == ExternalRuntimeAssetBucket::ManualActionRequired
            })
            .map(|asset| {
                serde_json::json!({
                    "asset_kind": asset.asset_kind,
                    "severity": asset.severity,
                    "reason": asset.reason,
                    "recommended_action": asset.recommended_action,
                })
            })
            .collect::<Vec<_>>();
        let readiness = readiness_score(&assets);

        let prior = self
            .history
            .list_external_runtime_shadow_runs(Some(&runtime), Some(&session_id))
            .await?
            .into_iter()
            .filter_map(|row| parsed_shadow_run(&row))
            .max_by_key(|outcome| outcome.created_at_ms);

        let payload = serde_json::json!({
            "runtime": runtime,
            "isolated": true,
            "guardrails": {
                "isolated": true,
                "side_effects_disabled": true,
                "will_enqueue_tasks": false,
                "will_launch_runner": false,
                "will_spawn_session": false,
                "will_mutate_live_state": false,
            },
            "imported": {
                "runtime": imported.runtime,
                "source_config_path": imported.source_config_path,
                "provider": imported.provider,
                "model": imported.model,
                "cwd": imported.cwd,
                "has_zorai_mcp": imported.has_zorai_mcp,
                "imported_at_ms": imported.imported_at_ms,
                "updated_at": row.updated_at,
                "session_id": session_id,
            },
            "current": {
                "runtime": current_runtime,
                "provider": current_provider,
                "model": current_model,
                "cwd": current_cwd,
                "has_zorai_mcp": current_has_zorai_mcp,
            },
            "comparison": {
                "runtime": compare_optional_str(Some(imported.runtime.as_str()), Some(current_runtime.as_str())),
                "provider": compare_optional_str(imported.provider.as_deref(), Some(current_provider.as_str())),
                "model": compare_optional_str(imported.model.as_deref(), Some(current_model.as_str())),
                "cwd": compare_optional_str(imported.cwd.as_deref(), current_cwd.as_deref()),
                "has_zorai_mcp": compare_bool(imported.has_zorai_mcp, current_has_zorai_mcp),
                "connector_hints": compare_count(connectors.len(), 0),
            },
            "projected_effects": {
                "task_creations": assets.iter().filter(|asset| asset.asset_kind == "routines").count(),
                "connector_calls": connectors.len(),
                "approvals_expected": blockers.len(),
                "missing_dependencies": blockers,
                "stop_reason": if readiness >= 70 { "none" } else { "migration blockers remain" },
            },
            "readiness": {
                "score": readiness,
                "ready": readiness >= 70,
                "blocker_count": assets.iter().filter(|asset| asset.severity == ExternalRuntimeReportSeverity::Blocking || asset.bucket == ExternalRuntimeAssetBucket::ManualActionRequired).count(),
            },
            "previous_outcome": prior,
        });

        let outcome = ExternalRuntimeShadowRunOutcome {
            run_id: format!("shadow-run-{}", Uuid::new_v4()),
            runtime: runtime.clone(),
            session_id: session_id.clone(),
            workflow: "migration_readiness".to_string(),
            readiness_score: readiness,
            blocker_count: assets
                .iter()
                .filter(|asset| {
                    asset.severity == ExternalRuntimeReportSeverity::Blocking
                        || asset.bucket == ExternalRuntimeAssetBucket::ManualActionRequired
                })
                .count() as u32,
            summary: format!(
                "Shadow run for {runtime}: readiness {readiness}/100 with {} blocker(s)",
                assets
                    .iter()
                    .filter(
                        |asset| asset.severity == ExternalRuntimeReportSeverity::Blocking
                            || asset.bucket == ExternalRuntimeAssetBucket::ManualActionRequired
                    )
                    .count()
            ),
            payload: payload.clone(),
            created_at_ms: crate::history::now_ts(),
        };
        self.history
            .upsert_external_runtime_shadow_run(&outcome)
            .await?;

        Ok(payload)
    }
}
