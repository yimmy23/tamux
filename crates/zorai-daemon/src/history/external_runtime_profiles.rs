use super::*;

fn map_external_runtime_profile_row(row: &db::Row) -> anyhow::Result<ExternalRuntimeProfileRow> {
    Ok(ExternalRuntimeProfileRow {
        runtime: row.get(0)?,
        profile_json: row.get(1)?,
        session_id: row.get(2)?,
        source_config_path: row.get(3)?,
        source_fingerprint: row.get(4)?,
        updated_at: row.get::<i64>(5)? as u64,
    })
}

fn map_external_runtime_import_session_row(
    row: &db::Row,
) -> anyhow::Result<ExternalRuntimeImportSessionRow> {
    Ok(ExternalRuntimeImportSessionRow {
        session_id: row.get(0)?,
        runtime: row.get(1)?,
        source_config_path: row.get(2)?,
        source_fingerprint: row.get(3)?,
        dry_run: row.get::<i64>(4)? != 0,
        conflict_policy: row.get(5)?,
        source_surface: row.get(6)?,
        session_json: row.get(7)?,
        imported_at_ms: row.get::<i64>(8)? as u64,
        updated_at: row.get::<i64>(9)? as u64,
    })
}

fn map_imported_runtime_asset_row(row: &db::Row) -> anyhow::Result<ImportedRuntimeAssetRow> {
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
        created_at_ms: row.get::<i64>(11)? as u64,
        updated_at: row.get::<i64>(12)? as u64,
    })
}

fn map_external_runtime_shadow_run_row(
    row: &db::Row,
) -> anyhow::Result<ExternalRuntimeShadowRunRow> {
    Ok(ExternalRuntimeShadowRunRow {
        run_id: row.get(0)?,
        runtime: row.get(1)?,
        session_id: row.get(2)?,
        workflow: row.get(3)?,
        readiness_score: row.get::<i64>(4)? as u8,
        blocker_count: row.get::<i64>(5)? as u32,
        summary: row.get(6)?,
        payload_json: row.get(7)?,
        created_at_ms: row.get::<i64>(8)? as u64,
        updated_at: row.get::<i64>(9)? as u64,
    })
}

impl HistoryStore {
    pub async fn list_external_runtime_profiles(&self) -> Result<Vec<ExternalRuntimeProfileRow>> {
        self.list_external_runtime_profiles_filtered(None, None)
            .await
    }

    pub async fn list_external_runtime_profiles_filtered(
        &self,
        runtime: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<ExternalRuntimeProfileRow>> {
        let mut sql = "SELECT runtime, profile_json, session_id, source_config_path, source_fingerprint, updated_at \
                     FROM external_runtime_profiles".to_string();
        let mut values = Vec::<db::Value>::new();
        if let Some(runtime) = runtime {
            sql.push_str(" WHERE runtime = ? COLLATE NOCASE");
            values.push(db::Value::Text(runtime.to_string()));
        }
        sql.push_str(" ORDER BY updated_at DESC, runtime ASC");
        if let Some(limit) = limit {
            sql.push_str(" LIMIT ?");
            values.push(db::Value::Integer(limit.max(1) as i64));
        }
        let rows = self
            .read_db
            .query(&sql, db::Params::Positional(values))
            .await?;
        rows.iter().map(map_external_runtime_profile_row).collect()
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
        let profile_json = serde_json::to_string(profile)?;
        let source_config_path = profile.source_config_path.clone();
        let updated_at = now_ts() as i64;
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO external_runtime_profiles \
                     (runtime, profile_json, session_id, source_config_path, source_fingerprint, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                db::db_params![runtime, profile_json, session_id, source_config_path, source_fingerprint, updated_at],
            )
            .await?;
        Ok(())
    }

    pub async fn get_external_runtime_profile(
        &self,
        runtime: &str,
    ) -> Result<Option<ExternalRuntimeProfileRow>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT runtime, profile_json, session_id, source_config_path, source_fingerprint, updated_at \
                     FROM external_runtime_profiles WHERE runtime = ?1",
                db::db_params![runtime],
            )
            .await?;
        row.map(|row| map_external_runtime_profile_row(&row))
            .transpose()
    }

    pub async fn upsert_external_runtime_import_session(
        &self,
        session: &crate::agent::types::ExternalRuntimeImportSession,
    ) -> Result<()> {
        let session_json = serde_json::to_string(session)?;
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO external_runtime_import_sessions \
                     (session_id, runtime, source_config_path, source_fingerprint, dry_run, conflict_policy, source_surface, session_json, imported_at_ms, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                db::db_params![
                    session.session_id.clone(),
                    session.runtime.clone(),
                    session.source_config_path.clone(),
                    session.source_fingerprint.clone(),
                    session.dry_run as i64,
                    session.conflict_policy.as_str(),
                    session.source_surface.clone(),
                    session_json,
                    session.imported_at_ms as i64,
                    now_ts() as i64,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn get_external_runtime_import_session(
        &self,
        session_id: &str,
    ) -> Result<Option<ExternalRuntimeImportSessionRow>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT session_id, runtime, source_config_path, source_fingerprint, dry_run, conflict_policy, source_surface, session_json, imported_at_ms, updated_at \
                     FROM external_runtime_import_sessions WHERE session_id = ?1",
                db::db_params![session_id],
            )
            .await?;
        row.map(|row| map_external_runtime_import_session_row(&row))
            .transpose()
    }

    pub async fn find_external_runtime_import_session_by_fingerprint(
        &self,
        runtime: &str,
        source_config_path: &str,
        source_fingerprint: &str,
        dry_run: bool,
    ) -> Result<Option<ExternalRuntimeImportSessionRow>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT session_id, runtime, source_config_path, source_fingerprint, dry_run, conflict_policy, source_surface, session_json, imported_at_ms, updated_at \
                     FROM external_runtime_import_sessions \
                     WHERE runtime = ?1 AND source_config_path = ?2 AND source_fingerprint = ?3 AND dry_run = ?4",
                db::db_params![runtime, source_config_path, source_fingerprint, dry_run as i64],
            )
            .await?;
        row.map(|row| map_external_runtime_import_session_row(&row))
            .transpose()
    }

    pub async fn replace_imported_runtime_assets(
        &self,
        session_id: &str,
        assets: &[crate::agent::types::ImportedRuntimeAsset],
    ) -> Result<()> {
        let assets = assets.to_vec();
        let mut txn = self.conn_db.transaction().await?;
        txn.execute(
            "DELETE FROM imported_runtime_assets WHERE session_id = ?1",
            db::db_params![session_id],
        )
        .await?;
        for asset in assets {
            let asset_json = serde_json::to_string(&asset)?;
            txn.execute(
                "INSERT OR REPLACE INTO imported_runtime_assets \
                         (asset_id, session_id, runtime, asset_kind, bucket, severity, recommended_action, source_path, source_fingerprint, conflict_policy, asset_json, created_at_ms, updated_at) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                db::db_params![
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
            )
            .await?;
        }
        txn.commit().await?;
        Ok(())
    }

    pub async fn list_imported_runtime_assets(
        &self,
        runtime: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<Vec<ImportedRuntimeAssetRow>> {
        let mut sql = "SELECT asset_id, session_id, runtime, asset_kind, bucket, severity, recommended_action, source_path, source_fingerprint, conflict_policy, asset_json, created_at_ms, updated_at \
                     FROM imported_runtime_assets".to_string();
        let mut conditions = Vec::new();
        let mut values = Vec::<db::Value>::new();
        if let Some(runtime) = runtime {
            conditions.push("runtime = ? COLLATE NOCASE");
            values.push(db::Value::Text(runtime.to_string()));
        }
        if let Some(session_id) = session_id {
            conditions.push("session_id = ?");
            values.push(db::Value::Text(session_id.to_string()));
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        sql.push_str(" ORDER BY updated_at DESC, asset_kind ASC");
        let rows = self
            .read_db
            .query(&sql, db::Params::Positional(values))
            .await?;
        rows.iter().map(map_imported_runtime_asset_row).collect()
    }

    pub async fn upsert_external_runtime_shadow_run(
        &self,
        outcome: &crate::agent::types::ExternalRuntimeShadowRunOutcome,
    ) -> Result<()> {
        let payload_json = serde_json::to_string(outcome)?;
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO external_runtime_shadow_runs \
                     (run_id, runtime, session_id, workflow, readiness_score, blocker_count, summary, payload_json, created_at_ms, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                db::db_params![
                    outcome.run_id.clone(),
                    outcome.runtime.clone(),
                    outcome.session_id.clone(),
                    outcome.workflow.clone(),
                    outcome.readiness_score as i64,
                    outcome.blocker_count as i64,
                    outcome.summary.clone(),
                    payload_json,
                    outcome.created_at_ms as i64,
                    now_ts() as i64,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn list_external_runtime_shadow_runs(
        &self,
        runtime: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<Vec<ExternalRuntimeShadowRunRow>> {
        let mut sql = "SELECT run_id, runtime, session_id, workflow, readiness_score, blocker_count, summary, payload_json, created_at_ms, updated_at \
                     FROM external_runtime_shadow_runs".to_string();
        let mut conditions = Vec::new();
        let mut values = Vec::<db::Value>::new();
        if let Some(runtime) = runtime {
            conditions.push("runtime = ? COLLATE NOCASE");
            values.push(db::Value::Text(runtime.to_string()));
        }
        if let Some(session_id) = session_id {
            conditions.push("session_id = ?");
            values.push(db::Value::Text(session_id.to_string()));
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        sql.push_str(" ORDER BY created_at_ms DESC, run_id DESC");
        let rows = self
            .read_db
            .query(&sql, db::Params::Positional(values))
            .await?;
        rows.iter()
            .map(map_external_runtime_shadow_run_row)
            .collect()
    }
}
