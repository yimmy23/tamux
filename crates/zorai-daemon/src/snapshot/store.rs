#![allow(dead_code)]

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRetentionConfig {
    pub max_snapshots: usize,
    pub max_total_size_mb: u64,
    pub auto_cleanup: bool,
}

impl Default for SnapshotRetentionConfig {
    fn default() -> Self {
        Self {
            max_snapshots: 0,
            max_total_size_mb: 10_240,
            auto_cleanup: false,
        }
    }
}

impl SnapshotRetentionConfig {
    async fn from_history(history: &HistoryStore) -> Self {
        let mut retention = Self::default();
        let Ok(items) = history.list_agent_config_items().await else {
            return retention;
        };

        for (key_path, value) in items {
            match key_path.as_str() {
                "/snapshot_retention/max_snapshots" => {
                    if let Some(value) = value.as_u64() {
                        retention.max_snapshots = value as usize;
                    }
                }
                "/snapshot_retention/max_total_size_mb" => {
                    if let Some(value) = value.as_u64() {
                        retention.max_total_size_mb = value.max(1);
                    }
                }
                "/snapshot_retention/auto_cleanup" => {
                    if let Some(value) = value.as_bool() {
                        retention.auto_cleanup = value;
                    }
                }
                _ => {}
            }
        }

        retention
    }
}

fn max_snapshot_bytes(config: &SnapshotRetentionConfig) -> u64 {
    config.max_total_size_mb.saturating_mul(1024 * 1024)
}

fn snapshot_exceeds_size_limit(snapshot: &SnapshotInfo, config: &SnapshotRetentionConfig) -> bool {
    std::fs::metadata(&snapshot.path)
        .map(|metadata| metadata.len() > max_snapshot_bytes(config))
        .unwrap_or(false)
}

fn read_settings_root(path: &Path) -> Option<Value> {
    let data = std::fs::read_to_string(path).ok()?;
    let parsed = serde_json::from_str::<Value>(&data).ok()?;
    match parsed.get("settings") {
        Some(settings) if settings.is_object() => Some(settings.clone()),
        _ => Some(parsed),
    }
}

fn effective_snapshot_backend() -> Option<String> {
    let mut backend = zorai_protocol::ZoraiConfig::load().snapshot_backend;
    for path in [zorai_protocol::zorai_data_dir().join("settings.json")] {
        let Some(settings) = read_settings_root(&path) else {
            continue;
        };
        if let Some(value) = settings.get("snapshotBackend").and_then(|v| v.as_str()) {
            backend = Some(value.to_string());
        }
    }
    backend
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotStats {
    pub count: usize,
    pub total_size_bytes: u64,
    pub oldest_timestamp: Option<u64>,
    pub newest_timestamp: Option<u64>,
}

pub async fn enforce_retention(
    history: &HistoryStore,
    config: &SnapshotRetentionConfig,
) -> Result<Vec<String>> {
    let mut entries = history.list_snapshot_index(None).await?;
    let mut removed = Vec::new();
    entries.sort_by_key(|e| e.created_at);

    while entries.len() > config.max_snapshots {
        if let Some(old) = entries.first() {
            let _ = std::fs::remove_file(&old.path);
            let _ = history.delete_snapshot_index(&old.snapshot_id).await;
            removed.push(old.snapshot_id.clone());
            entries.remove(0);
        }
    }

    loop {
        let total_size: u64 = entries
            .iter()
            .filter_map(|e| std::fs::metadata(&e.path).ok())
            .map(|m| m.len())
            .sum();

        if total_size <= max_snapshot_bytes(config) || entries.is_empty() {
            break;
        }

        if let Some(old) = entries.first() {
            let _ = std::fs::remove_file(&old.path);
            let _ = history.delete_snapshot_index(&old.snapshot_id).await;
            removed.push(old.snapshot_id.clone());
            entries.remove(0);
        }
    }

    if !removed.is_empty() {
        tracing::info!(count = removed.len(), ids = ?removed, "snapshot retention: removed old snapshots");
    }

    Ok(removed)
}

pub async fn get_snapshot_stats(history: &HistoryStore) -> Result<SnapshotStats> {
    let mut entries = history.list_snapshot_index(None).await?;
    entries.sort_by_key(|e| e.created_at);

    let total_size: u64 = entries
        .iter()
        .filter_map(|e| std::fs::metadata(&e.path).ok())
        .map(|m| m.len())
        .sum();

    Ok(SnapshotStats {
        count: entries.len(),
        total_size_bytes: total_size,
        oldest_timestamp: entries.first().map(|e| e.created_at.max(0) as u64),
        newest_timestamp: entries.last().map(|e| e.created_at.max(0) as u64),
    })
}

pub async fn delete_snapshot(history: &HistoryStore, snapshot_id: &str) -> Result<bool> {
    let Some(entry) = history.get_snapshot_index(snapshot_id).await? else {
        return Ok(false);
    };
    let _ = std::fs::remove_file(&entry.path);
    history.delete_snapshot_index(snapshot_id).await?;
    tracing::info!(snapshot_id, "deleted snapshot");
    Ok(true)
}

pub async fn cleanup_orphaned_files(history: &HistoryStore) -> Result<usize> {
    let root = zorai_protocol::ensure_zorai_data_dir()?.join("snapshots");
    if !root.exists() {
        return Ok(0);
    }

    let entries = history.list_snapshot_index(None).await?;
    let known_paths: HashSet<String> = entries.iter().map(|e| e.path.clone()).collect();
    let mut removed = 0;
    for dir_entry in std::fs::read_dir(&root)? {
        let dir_entry = dir_entry?;
        let path = dir_entry.path();
        if path.extension().map(|e| e == "gz").unwrap_or(false) {
            let path_str = path.to_string_lossy().to_string();
            if !known_paths.contains(&path_str) {
                let _ = std::fs::remove_file(&path);
                removed += 1;
                tracing::info!(path = %path_str, "removed orphaned snapshot file");
            }
        }
    }
    Ok(removed)
}

#[derive(Clone)]
pub struct SnapshotStore {
    history: HistoryStore,
    retention: SnapshotRetentionConfig,
}

impl SnapshotStore {
    pub fn new_with_history(history: HistoryStore) -> Self {
        Self {
            history,
            retention: SnapshotRetentionConfig::default(),
        }
    }

    pub fn set_retention_config(&mut self, config: SnapshotRetentionConfig) {
        self.retention = config;
    }

    pub fn retention_config(&self) -> &SnapshotRetentionConfig {
        &self.retention
    }

    pub async fn create_snapshot(
        &self,
        workspace_id: Option<WorkspaceId>,
        session_id: Option<SessionId>,
        cwd: Option<&str>,
        command: Option<&str>,
        label: &str,
    ) -> Result<Option<SnapshotInfo>> {
        let Some(cwd) = cwd else {
            return Ok(None);
        };
        let workspace = Path::new(cwd);
        if !workspace.exists() {
            return Ok(None);
        }

        let retention = SnapshotRetentionConfig::from_history(&self.history).await;
        if retention.max_snapshots == 0 {
            return Ok(None);
        }

        let backend = detect_snapshot_backend(cwd, effective_snapshot_backend().as_deref());
        let cwd = cwd.to_string();
        let label = label.to_string();
        let workspace_id = workspace_id;
        let session_id = session_id.map(|id| id.to_string());
        let command = command.map(ToOwned::to_owned);
        let snapshot = tokio::task::spawn_blocking(move || {
            backend.create(
                &cwd,
                &label,
                workspace_id.as_deref(),
                session_id.as_deref(),
                command.as_deref(),
            )
        })
        .await
        .context("snapshot creation task panicked")??;

        if snapshot_exceeds_size_limit(&snapshot, &retention) {
            let size_bytes = std::fs::metadata(&snapshot.path)
                .map(|metadata| metadata.len())
                .unwrap_or_default();
            let _ = std::fs::remove_file(&snapshot.path);
            tracing::warn!(
                snapshot_id = %snapshot.snapshot_id,
                path = %snapshot.path,
                size_bytes,
                limit_bytes = max_snapshot_bytes(&retention),
                "snapshot exceeded size limit and was discarded"
            );
            return Ok(None);
        }

        self.history
            .upsert_snapshot_index(&encode_snapshot(&snapshot))
            .await?;

        if retention.auto_cleanup {
            if let Err(e) = enforce_retention(&self.history, &retention).await {
                tracing::warn!(error = %e, "snapshot retention enforcement failed");
            }
        }

        Ok(Some(snapshot))
    }

    pub async fn list(&self, workspace_id: Option<&str>) -> Result<Vec<SnapshotInfo>> {
        let entries = self.history.list_snapshot_index(workspace_id).await?;
        Ok(entries.into_iter().map(decode_snapshot).collect())
    }

    pub async fn restore(&self, snapshot_id: &str) -> Result<(bool, String)> {
        let Some(entry) = self.history.get_snapshot_index(snapshot_id).await? else {
            return Ok((false, "snapshot not found".to_string()));
        };
        let snapshot = decode_snapshot(entry);
        restore_snapshot_payload(&snapshot)
    }

    pub async fn delete(&self, snapshot_id: &str) -> Result<bool> {
        delete_snapshot(&self.history, snapshot_id).await
    }

    pub async fn stats(&self) -> Result<SnapshotStats> {
        get_snapshot_stats(&self.history).await
    }

    pub async fn cleanup_orphaned(&self) -> Result<usize> {
        cleanup_orphaned_files(&self.history).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;
    use zorai_protocol::SnapshotIndexEntry;

    fn make_snapshot_entry(snapshot_id: &str, path: &Path, created_at: i64) -> SnapshotIndexEntry {
        SnapshotIndexEntry {
            snapshot_id: snapshot_id.to_string(),
            workspace_id: Some("workspace".to_string()),
            session_id: None,
            kind: "filesystem".to_string(),
            label: Some(snapshot_id.to_string()),
            path: path.to_string_lossy().into_owned(),
            created_at,
            details_json: None,
        }
    }

    #[tokio::test]
    async fn enforce_retention_removes_oldest_entries_when_total_size_exceeds_limit() {
        let root = tempdir().expect("tempdir");
        let history = HistoryStore::new_test_store(root.path())
            .await
            .expect("history store");

        let old_path = root.path().join("old.tar.gz");
        let new_path = root.path().join("new.tar.gz");
        std::fs::write(&old_path, vec![b'a'; 700_000]).expect("write old snapshot");
        std::fs::write(&new_path, vec![b'b'; 700_000]).expect("write new snapshot");

        history
            .upsert_snapshot_index(&make_snapshot_entry("old", &old_path, 1))
            .await
            .expect("index old");
        history
            .upsert_snapshot_index(&make_snapshot_entry("new", &new_path, 2))
            .await
            .expect("index new");

        let removed = enforce_retention(
            &history,
            &SnapshotRetentionConfig {
                max_snapshots: 10,
                max_total_size_mb: 1,
                auto_cleanup: true,
            },
        )
        .await
        .expect("enforce retention");

        assert_eq!(removed, vec!["old".to_string()]);
        assert!(!old_path.exists(), "oldest snapshot file should be removed");
        assert!(new_path.exists(), "newest snapshot file should remain");
    }

    #[test]
    fn created_snapshot_is_rejected_when_file_exceeds_size_limit() {
        let root = tempdir().expect("tempdir");
        let path = root.path().join("oversized.tar.gz");
        std::fs::write(&path, vec![b'x'; 1_200_000]).expect("write oversized snapshot");

        let snapshot = SnapshotInfo {
            snapshot_id: "snap_oversized".to_string(),
            workspace_id: Some("workspace".to_string()),
            session_id: None,
            command: None,
            kind: "filesystem".to_string(),
            label: "snapshot".to_string(),
            path: path.to_string_lossy().into_owned(),
            created_at: 1,
            status: "ready".to_string(),
            details: String::new(),
        };

        assert!(
            snapshot_exceeds_size_limit(
                &snapshot,
                &SnapshotRetentionConfig {
                    max_snapshots: 1,
                    max_total_size_mb: 1,
                    auto_cleanup: true,
                },
            ),
            "created snapshot should be rejected when it exceeds the configured size cap"
        );
    }

    #[tokio::test]
    async fn retention_config_reads_db_owned_snapshot_settings() {
        let root = tempdir().expect("tempdir");
        let history = HistoryStore::new_test_store(root.path())
            .await
            .expect("history store");

        history
            .upsert_agent_config_item("/snapshot_retention/max_snapshots", &json!(3))
            .await
            .expect("persist snapshot max count");
        history
            .upsert_agent_config_item("/snapshot_retention/max_total_size_mb", &json!(2048))
            .await
            .expect("persist snapshot size limit");
        history
            .upsert_agent_config_item("/snapshot_retention/auto_cleanup", &json!(true))
            .await
            .expect("persist snapshot auto cleanup");

        let retention = SnapshotRetentionConfig::from_history(&history).await;

        assert_eq!(retention.max_snapshots, 3);
        assert_eq!(retention.max_total_size_mb, 2048);
        assert!(retention.auto_cleanup);
    }

    #[tokio::test]
    async fn create_snapshot_returns_none_when_snapshotting_is_disabled() {
        let root = tempdir().expect("tempdir");
        let history = HistoryStore::new_test_store(root.path())
            .await
            .expect("history store");
        history
            .upsert_agent_config_item("/snapshot_retention/max_snapshots", &json!(0))
            .await
            .expect("persist disabled snapshot max count");

        let store = SnapshotStore::new_with_history(history.clone());
        let workspace = tempdir().expect("workspace tempdir");

        let created = store
            .create_snapshot(
                Some("workspace".to_string()),
                None,
                workspace.path().to_str(),
                Some("echo hi"),
                "pre-execution checkpoint",
            )
            .await
            .expect("snapshot creation should return without error");

        assert!(
            created.is_none(),
            "disabled snapshotting should skip creation"
        );
        assert!(
            history
                .list_snapshot_index(None)
                .await
                .expect("snapshot index should be readable")
                .is_empty(),
            "disabled snapshotting should not persist snapshot metadata"
        );
    }
}
