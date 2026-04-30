use super::*;

impl HistoryStore {
    pub async fn list_external_runtime_profiles(&self) -> Result<Vec<ExternalRuntimeProfileRow>> {
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT runtime, profile_json, session_id, source_config_path, source_fingerprint, updated_at \
                     FROM external_runtime_profiles ORDER BY updated_at DESC, runtime ASC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(ExternalRuntimeProfileRow {
                        runtime: row.get(0)?,
                        profile_json: row.get(1)?,
                        session_id: row.get(2)?,
                        source_config_path: row.get(3)?,
                        source_fingerprint: row.get(4)?,
                        updated_at: row.get::<_, i64>(5)? as u64,
                    })
                })?;
                Ok(rows.filter_map(|row| row.ok()).collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_external_runtime_profile(
        &self,
        runtime: &str,
        profile: &crate::agent::types::ExternalRuntimeProfile,
    ) -> Result<()> {
        self.upsert_external_runtime_profile_with_provenance(runtime, profile, None, None)
            .await
    }

    pub async fn upsert_external_runtime_profile_with_provenance(
        &self,
        runtime: &str,
        profile: &crate::agent::types::ExternalRuntimeProfile,
        session_id: Option<&str>,
        source_fingerprint: Option<&str>,
    ) -> Result<()> {
        let runtime = runtime.to_string();
        let profile_json = serde_json::to_string(profile)?;
        let session_id = session_id.map(str::to_string);
        let source_config_path = profile.source_config_path.clone();
        let source_fingerprint = source_fingerprint.map(str::to_string);
        let updated_at = now_ts() as i64;
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO external_runtime_profiles \
                     (runtime, profile_json, session_id, source_config_path, source_fingerprint, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![runtime, profile_json, session_id, source_config_path, source_fingerprint, updated_at],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_external_runtime_profile(
        &self,
        runtime: &str,
    ) -> Result<Option<ExternalRuntimeProfileRow>> {
        let runtime = runtime.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT runtime, profile_json, session_id, source_config_path, source_fingerprint, updated_at \
                     FROM external_runtime_profiles WHERE runtime = ?1",
                    params![runtime],
                    |row| {
                        Ok(ExternalRuntimeProfileRow {
                            runtime: row.get(0)?,
                            profile_json: row.get(1)?,
                            session_id: row.get(2)?,
                            source_config_path: row.get(3)?,
                            source_fingerprint: row.get(4)?,
                            updated_at: row.get::<_, i64>(5)? as u64,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_external_runtime_import_session(
        &self,
        session: &crate::agent::types::ExternalRuntimeImportSession,
    ) -> Result<()> {
        let row = ExternalRuntimeImportSessionRow {
            session_id: session.session_id.clone(),
            runtime: session.runtime.clone(),
            source_config_path: session.source_config_path.clone(),
            source_fingerprint: session.source_fingerprint.clone(),
            dry_run: session.dry_run,
            conflict_policy: session.conflict_policy.as_str().to_string(),
            source_surface: session.source_surface.clone(),
            session_json: serde_json::to_string(session)?,
            imported_at_ms: session.imported_at_ms,
            updated_at: now_ts(),
        };
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO external_runtime_import_sessions \
                     (session_id, runtime, source_config_path, source_fingerprint, dry_run, conflict_policy, source_surface, session_json, imported_at_ms, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        row.session_id,
                        row.runtime,
                        row.source_config_path,
                        row.source_fingerprint,
                        row.dry_run as i64,
                        row.conflict_policy,
                        row.source_surface,
                        row.session_json,
                        row.imported_at_ms as i64,
                        row.updated_at as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_external_runtime_import_session(
        &self,
        session_id: &str,
    ) -> Result<Option<ExternalRuntimeImportSessionRow>> {
        let session_id = session_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT session_id, runtime, source_config_path, source_fingerprint, dry_run, conflict_policy, source_surface, session_json, imported_at_ms, updated_at \
                     FROM external_runtime_import_sessions WHERE session_id = ?1",
                    params![session_id],
                    |row| {
                        Ok(ExternalRuntimeImportSessionRow {
                            session_id: row.get(0)?,
                            runtime: row.get(1)?,
                            source_config_path: row.get(2)?,
                            source_fingerprint: row.get(3)?,
                            dry_run: row.get::<_, i64>(4)? != 0,
                            conflict_policy: row.get(5)?,
                            source_surface: row.get(6)?,
                            session_json: row.get(7)?,
                            imported_at_ms: row.get::<_, i64>(8)? as u64,
                            updated_at: row.get::<_, i64>(9)? as u64,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn find_external_runtime_import_session_by_fingerprint(
        &self,
        runtime: &str,
        source_config_path: &str,
        source_fingerprint: &str,
        dry_run: bool,
    ) -> Result<Option<ExternalRuntimeImportSessionRow>> {
        let runtime = runtime.to_string();
        let source_config_path = source_config_path.to_string();
        let source_fingerprint = source_fingerprint.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT session_id, runtime, source_config_path, source_fingerprint, dry_run, conflict_policy, source_surface, session_json, imported_at_ms, updated_at \
                     FROM external_runtime_import_sessions \
                     WHERE runtime = ?1 AND source_config_path = ?2 AND source_fingerprint = ?3 AND dry_run = ?4",
                    params![runtime, source_config_path, source_fingerprint, dry_run as i64],
                    |row| {
                        Ok(ExternalRuntimeImportSessionRow {
                            session_id: row.get(0)?,
                            runtime: row.get(1)?,
                            source_config_path: row.get(2)?,
                            source_fingerprint: row.get(3)?,
                            dry_run: row.get::<_, i64>(4)? != 0,
                            conflict_policy: row.get(5)?,
                            source_surface: row.get(6)?,
                            session_json: row.get(7)?,
                            imported_at_ms: row.get::<_, i64>(8)? as u64,
                            updated_at: row.get::<_, i64>(9)? as u64,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn replace_imported_runtime_assets(
        &self,
        session_id: &str,
        assets: &[crate::agent::types::ImportedRuntimeAsset],
    ) -> Result<()> {
        let session_id = session_id.to_string();
        let assets = assets.to_vec();
        self.conn
            .call(move |conn| {
                let tx = conn.transaction()?;
                tx.execute(
                    "DELETE FROM imported_runtime_assets WHERE session_id = ?1",
                    params![session_id],
                )?;
                for asset in assets {
                    let asset_json = serde_json::to_string(&asset)
                        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
                    tx.execute(
                        "INSERT OR REPLACE INTO imported_runtime_assets \
                         (asset_id, session_id, runtime, asset_kind, bucket, severity, recommended_action, source_path, source_fingerprint, conflict_policy, asset_json, created_at_ms, updated_at) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                        params![
                            asset.asset_id,
                            asset.session_id,
                            asset.runtime,
                            asset.asset_kind,
                            asset.bucket.as_str(),
                            asset.severity.as_str(),
                            asset.recommended_action,
                            asset.source_path,
                            asset.source_fingerprint,
                            asset.conflict_policy.as_str(),
                            asset_json,
                            asset.created_at_ms as i64,
                            now_ts() as i64,
                        ],
                    )?;
                }
                tx.commit()?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_imported_runtime_assets(
        &self,
        runtime: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<Vec<ImportedRuntimeAssetRow>> {
        let runtime = runtime.map(str::to_string);
        let session_id = session_id.map(str::to_string);
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT asset_id, session_id, runtime, asset_kind, bucket, severity, recommended_action, source_path, source_fingerprint, conflict_policy, asset_json, created_at_ms, updated_at \
                     FROM imported_runtime_assets ORDER BY updated_at DESC, asset_kind ASC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(ImportedRuntimeAssetRow {
                        asset_id: row.get(0)?,
                        session_id: row.get(1)?,
                        runtime: row.get(2)?,
                        asset_kind: row.get(3)?,
                        bucket: row.get(4)?,
                        severity: row.get(5)?,
                        recommended_action: row.get(6)?,
                        source_path: row.get(7)?,
                        source_fingerprint: row.get(8)?,
                        conflict_policy: row.get(9)?,
                        asset_json: row.get(10)?,
                        created_at_ms: row.get::<_, i64>(11)? as u64,
                        updated_at: row.get::<_, i64>(12)? as u64,
                    })
                })?;
                let mut items = Vec::new();
                for row in rows {
                    let row = row?;
                    if runtime
                        .as_deref()
                        .is_some_and(|value| !row.runtime.eq_ignore_ascii_case(value))
                    {
                        continue;
                    }
                    if session_id
                        .as_deref()
                        .is_some_and(|value| row.session_id != value)
                    {
                        continue;
                    }
                    items.push(row);
                }
                Ok(items)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_external_runtime_shadow_run(
        &self,
        outcome: &crate::agent::types::ExternalRuntimeShadowRunOutcome,
    ) -> Result<()> {
        let row = ExternalRuntimeShadowRunRow {
            run_id: outcome.run_id.clone(),
            runtime: outcome.runtime.clone(),
            session_id: outcome.session_id.clone(),
            workflow: outcome.workflow.clone(),
            readiness_score: outcome.readiness_score,
            blocker_count: outcome.blocker_count,
            summary: outcome.summary.clone(),
            payload_json: serde_json::to_string(outcome)?,
            created_at_ms: outcome.created_at_ms,
            updated_at: now_ts(),
        };
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO external_runtime_shadow_runs \
                     (run_id, runtime, session_id, workflow, readiness_score, blocker_count, summary, payload_json, created_at_ms, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        row.run_id,
                        row.runtime,
                        row.session_id,
                        row.workflow,
                        row.readiness_score as i64,
                        row.blocker_count as i64,
                        row.summary,
                        row.payload_json,
                        row.created_at_ms as i64,
                        row.updated_at as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_external_runtime_shadow_runs(
        &self,
        runtime: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<Vec<ExternalRuntimeShadowRunRow>> {
        let runtime = runtime.map(str::to_string);
        let session_id = session_id.map(str::to_string);
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT run_id, runtime, session_id, workflow, readiness_score, blocker_count, summary, payload_json, created_at_ms, updated_at \
                     FROM external_runtime_shadow_runs ORDER BY created_at_ms DESC, run_id DESC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(ExternalRuntimeShadowRunRow {
                        run_id: row.get(0)?,
                        runtime: row.get(1)?,
                        session_id: row.get(2)?,
                        workflow: row.get(3)?,
                        readiness_score: row.get::<_, i64>(4)? as u8,
                        blocker_count: row.get::<_, i64>(5)? as u32,
                        summary: row.get(6)?,
                        payload_json: row.get(7)?,
                        created_at_ms: row.get::<_, i64>(8)? as u64,
                        updated_at: row.get::<_, i64>(9)? as u64,
                    })
                })?;
                let mut items = Vec::new();
                for row in rows {
                    let row = row?;
                    if runtime
                        .as_deref()
                        .is_some_and(|value| !row.runtime.eq_ignore_ascii_case(value))
                    {
                        continue;
                    }
                    if session_id
                        .as_deref()
                        .is_some_and(|value| row.session_id != value)
                    {
                        continue;
                    }
                    items.push(row);
                }
                Ok(items)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
