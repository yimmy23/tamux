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
            max_snapshots: 10,
            max_total_size_mb: 51_200,
            auto_cleanup: true,
        }
    }
}

impl SnapshotRetentionConfig {
    fn from_sources() -> Self {
        let config = amux_protocol::AmuxConfig::load();
        let mut retention = Self {
            max_snapshots: config.snapshot_max_count.max(1),
            max_total_size_mb: config.snapshot_max_total_size_mb.max(1024),
            auto_cleanup: config.snapshot_auto_cleanup,
        };

        for path in [amux_protocol::amux_data_dir().join("settings.json")] {
            let Some(settings) = read_settings_root(&path) else {
                continue;
            };

            if let Some(value) = settings.get("snapshotMaxCount").and_then(|v| v.as_u64()) {
                retention.max_snapshots = value.max(1) as usize;
            }
            if let Some(value) = settings.get("snapshotMaxSizeMb").and_then(|v| v.as_u64()) {
                retention.max_total_size_mb = value.max(1024);
            }
            if let Some(value) = settings
                .get("snapshotAutoCleanup")
                .and_then(|v| v.as_bool())
            {
                retention.auto_cleanup = value;
            }
        }

        retention
    }
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
    let mut backend = amux_protocol::AmuxConfig::load().snapshot_backend;
    for path in [amux_protocol::amux_data_dir().join("settings.json")] {
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
        let total_mb = total_size / (1024 * 1024);

        if total_mb <= config.max_total_size_mb || entries.is_empty() {
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
    let root = amux_protocol::ensure_amux_data_dir()?.join("snapshots");
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
            retention: SnapshotRetentionConfig::from_sources(),
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

        let backend = detect_snapshot_backend(cwd, effective_snapshot_backend().as_deref());
        let retention = SnapshotRetentionConfig::from_sources();
        let snapshot = backend.create(
            cwd,
            label,
            workspace_id.as_deref(),
            session_id.as_ref().map(|id| id.to_string()).as_deref(),
            command,
        )?;

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
