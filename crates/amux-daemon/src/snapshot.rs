use amux_protocol::{SessionId, SnapshotIndexEntry, SnapshotInfo, WorkspaceId};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::history::HistoryStore;

// ---------------------------------------------------------------------------
// Snapshot backend trait
// ---------------------------------------------------------------------------

/// Pluggable backend for creating, restoring and listing workspace snapshots.
pub trait SnapshotBackend: Send + Sync {
    fn create(
        &self,
        workspace_root: &str,
        label: &str,
        workspace_id: Option<&str>,
        session_id: Option<&str>,
        command: Option<&str>,
    ) -> Result<SnapshotInfo>;

    fn restore(&self, snapshot_id: &str) -> Result<(bool, String)>;

    fn list(&self, workspace_id: Option<&str>) -> Result<Vec<SnapshotInfo>>;

    fn name(&self) -> &'static str;
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SnapshotIndex {
    snapshots: Vec<SnapshotInfo>,
}

fn load_index(index_path: &Path) -> Result<SnapshotIndex> {
    if !index_path.exists() {
        return Ok(SnapshotIndex::default());
    }

    let data = std::fs::read_to_string(index_path)
        .with_context(|| format!("failed to read {}", index_path.display()))?;
    Ok(serde_json::from_str(&data).unwrap_or_default())
}

/// Parse a session ID string back into a `Uuid`.
fn parse_session_id(s: &str) -> Option<SessionId> {
    Uuid::parse_str(s).ok()
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SnapshotDetails {
    command: Option<String>,
    status: Option<String>,
    details: Option<String>,
}

fn encode_snapshot(snapshot: &SnapshotInfo) -> SnapshotIndexEntry {
    SnapshotIndexEntry {
        snapshot_id: snapshot.snapshot_id.clone(),
        workspace_id: snapshot.workspace_id.clone(),
        session_id: snapshot.session_id.map(|id| id.to_string()),
        kind: snapshot.kind.clone(),
        label: Some(snapshot.label.clone()),
        path: snapshot.path.clone(),
        created_at: snapshot.created_at as i64,
        details_json: serde_json::to_string(&SnapshotDetails {
            command: snapshot.command.clone(),
            status: Some(snapshot.status.clone()),
            details: Some(snapshot.details.clone()),
        })
        .ok(),
    }
}

fn decode_snapshot(entry: SnapshotIndexEntry) -> SnapshotInfo {
    let details = entry
        .details_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<SnapshotDetails>(raw).ok())
        .unwrap_or_default();
    SnapshotInfo {
        snapshot_id: entry.snapshot_id,
        workspace_id: entry.workspace_id,
        session_id: entry.session_id.as_deref().and_then(parse_session_id),
        command: details.command,
        kind: entry.kind,
        label: entry.label.unwrap_or_else(|| "snapshot".to_string()),
        path: entry.path,
        created_at: entry.created_at.max(0) as u64,
        status: details.status.unwrap_or_else(|| "ready".to_string()),
        details: details.details.unwrap_or_default(),
    }
}

fn restore_snapshot_payload(snapshot: &SnapshotInfo) -> Result<(bool, String)> {
    match snapshot.kind.as_str() {
        "zfs" => {
            let status = Command::new("zfs")
                .arg("rollback")
                .arg(&snapshot.path)
                .status();
            match status {
                Ok(result) if result.success() => Ok((
                    true,
                    format!("rolled back to ZFS snapshot {}", snapshot.path),
                )),
                Ok(result) => Ok((false, format!("zfs rollback exited with status {result}"))),
                Err(error) => Ok((false, format!("zfs rollback failed: {error}"))),
            }
        }
        "btrfs" => {
            let Some(workspace_id) = snapshot.workspace_id.as_deref() else {
                return Ok((false, "snapshot has no workspace association".to_string()));
            };
            let target_root = amux_protocol::ensure_amux_data_dir()?
                .join("restores")
                .join(workspace_id);
            if target_root.exists() {
                let _ = Command::new("btrfs")
                    .arg("subvolume")
                    .arg("delete")
                    .arg(&target_root)
                    .status();
            }
            let status = Command::new("btrfs")
                .arg("subvolume")
                .arg("snapshot")
                .arg(&snapshot.path)
                .arg(&target_root)
                .status();
            match status {
                Ok(result) if result.success() => Ok((
                    true,
                    format!("restored BTRFS snapshot into {}", target_root.display()),
                )),
                Ok(result) => Ok((
                    false,
                    format!("btrfs snapshot restore exited with status {result}"),
                )),
                Err(error) => Ok((false, format!("btrfs restore failed: {error}"))),
            }
        }
        _ => {
            let Some(workspace_id) = snapshot.workspace_id.as_deref() else {
                return Ok((false, "snapshot has no workspace association".to_string()));
            };
            let target_root = amux_protocol::ensure_amux_data_dir()?
                .join("restores")
                .join(workspace_id);
            std::fs::create_dir_all(&target_root)?;
            let status = Command::new("tar")
                .arg("-xzf")
                .arg(&snapshot.path)
                .arg("-C")
                .arg(&target_root)
                .status();
            match status {
                Ok(result) if result.success() => Ok((
                    true,
                    format!("restored snapshot into {}", target_root.display()),
                )),
                Ok(result) => Ok((false, format!("restore tar exited with status {result}"))),
                Err(error) => Ok((false, format!("restore failed: {error}"))),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TarBackend — the original tar.gz implementation
// ---------------------------------------------------------------------------

pub struct TarBackend {
    root: PathBuf,
    index_path: PathBuf,
}

impl TarBackend {
    pub fn new() -> Result<Self> {
        let root = amux_protocol::ensure_amux_data_dir()?.join("snapshots");
        std::fs::create_dir_all(&root)?;
        let index_path = root.join("index.json");
        Ok(Self { root, index_path })
    }
}

impl SnapshotBackend for TarBackend {
    fn name(&self) -> &'static str {
        "tar"
    }

    fn create(
        &self,
        workspace_root: &str,
        label: &str,
        workspace_id: Option<&str>,
        session_id: Option<&str>,
        command: Option<&str>,
    ) -> Result<SnapshotInfo> {
        let timestamp = now_ts();
        let snapshot_id = format!("snap_{}", uuid::Uuid::new_v4());
        let archive_path = self.root.join(format!("{snapshot_id}.tar.gz"));

        let tar_status = Command::new("tar")
            .arg("-czf")
            .arg(&archive_path)
            .arg("-C")
            .arg(workspace_root)
            .arg(".")
            .status();

        let (status, details) = match tar_status {
            Ok(result) if result.success() => (
                "ready".to_string(),
                format!("workspace checkpoint created at {}", archive_path.display()),
            ),
            Ok(result) => (
                "degraded".to_string(),
                format!("tar exited with status {result}; metadata checkpoint only"),
            ),
            Err(error) => (
                "degraded".to_string(),
                format!("tar unavailable: {error}; metadata checkpoint only"),
            ),
        };

        let snapshot = SnapshotInfo {
            snapshot_id: snapshot_id.clone(),
            workspace_id: workspace_id.map(ToOwned::to_owned),
            session_id: session_id.and_then(parse_session_id),
            command: command.map(ToOwned::to_owned),
            kind: "filesystem".to_string(),
            label: label.to_string(),
            path: archive_path.to_string_lossy().into_owned(),
            created_at: timestamp,
            status,
            details,
        };

        Ok(snapshot)
    }

    fn restore(&self, snapshot_id: &str) -> Result<(bool, String)> {
        let index = load_index(&self.index_path)?;
        let Some(snapshot) = index
            .snapshots
            .iter()
            .find(|entry| entry.snapshot_id == snapshot_id)
        else {
            return Ok((false, "snapshot not found".to_string()));
        };

        let Some(workspace_id) = snapshot.workspace_id.as_deref() else {
            return Ok((false, "snapshot has no workspace association".to_string()));
        };

        let target_root = amux_protocol::ensure_amux_data_dir()?
            .join("restores")
            .join(workspace_id);
        std::fs::create_dir_all(&target_root)?;

        let status = Command::new("tar")
            .arg("-xzf")
            .arg(&snapshot.path)
            .arg("-C")
            .arg(&target_root)
            .status();

        match status {
            Ok(result) if result.success() => Ok((
                true,
                format!("restored snapshot into {}", target_root.display()),
            )),
            Ok(result) => Ok((false, format!("restore tar exited with status {result}"))),
            Err(error) => Ok((false, format!("restore failed: {error}"))),
        }
    }

    fn list(&self, workspace_id: Option<&str>) -> Result<Vec<SnapshotInfo>> {
        let index = load_index(&self.index_path)?;
        Ok(index
            .snapshots
            .into_iter()
            .filter(|snapshot| {
                workspace_id.is_none() || snapshot.workspace_id.as_deref() == workspace_id
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// ZfsBackend — ZFS snapshot support
// ---------------------------------------------------------------------------

pub struct ZfsBackend {
    /// The ZFS dataset that corresponds to the workspace (e.g. "pool/data").
    dataset: String,
    index_path: PathBuf,
}

impl ZfsBackend {
    pub fn new(dataset: String) -> Result<Self> {
        let root = amux_protocol::ensure_amux_data_dir()?.join("snapshots");
        std::fs::create_dir_all(&root)?;
        let index_path = root.join("index.json");
        Ok(Self {
            dataset,
            index_path,
        })
    }
}

impl SnapshotBackend for ZfsBackend {
    fn name(&self) -> &'static str {
        "zfs"
    }

    fn create(
        &self,
        _workspace_root: &str,
        label: &str,
        workspace_id: Option<&str>,
        session_id: Option<&str>,
        command: Option<&str>,
    ) -> Result<SnapshotInfo> {
        let timestamp = now_ts();
        let safe_label = label.replace(' ', "-");
        let snap_tag = format!("amux-{safe_label}-{timestamp}");
        let full_snap = format!("{}@{}", self.dataset, snap_tag);

        let zfs_status = Command::new("zfs").arg("snapshot").arg(&full_snap).status();

        let (status, details) = match zfs_status {
            Ok(result) if result.success() => (
                "ready".to_string(),
                format!("ZFS snapshot created: {full_snap}"),
            ),
            Ok(result) => (
                "degraded".to_string(),
                format!("zfs snapshot exited with status {result}"),
            ),
            Err(error) => (
                "degraded".to_string(),
                format!("zfs command failed: {error}"),
            ),
        };

        let snapshot_id = format!("snap_{}", uuid::Uuid::new_v4());

        let snapshot = SnapshotInfo {
            snapshot_id: snapshot_id.clone(),
            workspace_id: workspace_id.map(ToOwned::to_owned),
            session_id: session_id.and_then(parse_session_id),
            command: command.map(ToOwned::to_owned),
            kind: "zfs".to_string(),
            label: label.to_string(),
            path: full_snap,
            created_at: timestamp,
            status,
            details,
        };

        Ok(snapshot)
    }

    fn restore(&self, snapshot_id: &str) -> Result<(bool, String)> {
        let index = load_index(&self.index_path)?;
        let Some(snapshot) = index
            .snapshots
            .iter()
            .find(|entry| entry.snapshot_id == snapshot_id)
        else {
            return Ok((false, "snapshot not found".to_string()));
        };

        // snapshot.path holds the full ZFS snapshot name (e.g. "pool/data@amux-label-123")
        let status = Command::new("zfs")
            .arg("rollback")
            .arg(&snapshot.path)
            .status();

        match status {
            Ok(result) if result.success() => Ok((
                true,
                format!("rolled back to ZFS snapshot {}", snapshot.path),
            )),
            Ok(result) => Ok((false, format!("zfs rollback exited with status {result}"))),
            Err(error) => Ok((false, format!("zfs rollback failed: {error}"))),
        }
    }

    fn list(&self, workspace_id: Option<&str>) -> Result<Vec<SnapshotInfo>> {
        let index = load_index(&self.index_path)?;
        Ok(index
            .snapshots
            .into_iter()
            .filter(|snapshot| {
                snapshot.kind == "zfs"
                    && (workspace_id.is_none() || snapshot.workspace_id.as_deref() == workspace_id)
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// BtrfsBackend — BTRFS subvolume snapshot support
// ---------------------------------------------------------------------------

pub struct BtrfsBackend {
    /// Directory where BTRFS snapshots are stored.
    snapshot_dir: PathBuf,
    index_path: PathBuf,
}

impl BtrfsBackend {
    pub fn new() -> Result<Self> {
        let root = amux_protocol::ensure_amux_data_dir()?.join("snapshots");
        std::fs::create_dir_all(&root)?;
        let snapshot_dir = root.join("btrfs");
        std::fs::create_dir_all(&snapshot_dir)?;
        let index_path = root.join("index.json");
        Ok(Self {
            snapshot_dir,
            index_path,
        })
    }
}

impl SnapshotBackend for BtrfsBackend {
    fn name(&self) -> &'static str {
        "btrfs"
    }

    fn create(
        &self,
        workspace_root: &str,
        label: &str,
        workspace_id: Option<&str>,
        session_id: Option<&str>,
        command: Option<&str>,
    ) -> Result<SnapshotInfo> {
        let timestamp = now_ts();
        let snapshot_id = format!("snap_{}", uuid::Uuid::new_v4());
        let safe_label = label.replace(' ', "-");
        let dest = self
            .snapshot_dir
            .join(format!("{snapshot_id}-{safe_label}-{timestamp}"));

        let btrfs_status = Command::new("btrfs")
            .arg("subvolume")
            .arg("snapshot")
            .arg(workspace_root)
            .arg(&dest)
            .status();

        let (status, details) = match btrfs_status {
            Ok(result) if result.success() => (
                "ready".to_string(),
                format!("BTRFS snapshot created at {}", dest.display()),
            ),
            Ok(result) => (
                "degraded".to_string(),
                format!("btrfs subvolume snapshot exited with status {result}"),
            ),
            Err(error) => (
                "degraded".to_string(),
                format!("btrfs command failed: {error}"),
            ),
        };

        let snapshot = SnapshotInfo {
            snapshot_id: snapshot_id.clone(),
            workspace_id: workspace_id.map(ToOwned::to_owned),
            session_id: session_id.and_then(parse_session_id),
            command: command.map(ToOwned::to_owned),
            kind: "btrfs".to_string(),
            label: label.to_string(),
            path: dest.to_string_lossy().into_owned(),
            created_at: timestamp,
            status,
            details,
        };

        Ok(snapshot)
    }

    fn restore(&self, snapshot_id: &str) -> Result<(bool, String)> {
        let index = load_index(&self.index_path)?;
        let Some(snapshot) = index
            .snapshots
            .iter()
            .find(|entry| entry.snapshot_id == snapshot_id)
        else {
            return Ok((false, "snapshot not found".to_string()));
        };

        let Some(workspace_id) = snapshot.workspace_id.as_deref() else {
            return Ok((false, "snapshot has no workspace association".to_string()));
        };

        // Restore by deleting the current subvolume and re-snapshotting.
        let target_root = amux_protocol::ensure_amux_data_dir()?
            .join("restores")
            .join(workspace_id);

        // Delete existing restore target if it is a subvolume.
        if target_root.exists() {
            let _ = Command::new("btrfs")
                .arg("subvolume")
                .arg("delete")
                .arg(&target_root)
                .status();
        }

        let status = Command::new("btrfs")
            .arg("subvolume")
            .arg("snapshot")
            .arg(&snapshot.path)
            .arg(&target_root)
            .status();

        match status {
            Ok(result) if result.success() => Ok((
                true,
                format!("restored BTRFS snapshot into {}", target_root.display()),
            )),
            Ok(result) => Ok((
                false,
                format!("btrfs snapshot restore exited with status {result}"),
            )),
            Err(error) => Ok((false, format!("btrfs restore failed: {error}"))),
        }
    }

    fn list(&self, workspace_id: Option<&str>) -> Result<Vec<SnapshotInfo>> {
        let index = load_index(&self.index_path)?;
        Ok(index
            .snapshots
            .into_iter()
            .filter(|snapshot| {
                snapshot.kind == "btrfs"
                    && (workspace_id.is_none() || snapshot.workspace_id.as_deref() == workspace_id)
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Backend detection
// ---------------------------------------------------------------------------

/// Detect the best available snapshot backend for the given workspace root.
///
/// When `preference` is `None` or `Some("auto")`, detection order is:
///   1. ZFS (if workspace is on a ZFS dataset)
///   2. BTRFS (if workspace is on a BTRFS filesystem)
///   3. tar.gz (always available)
///
/// Explicit preferences: `"zfs"`, `"btrfs"`, `"tar"`.
pub fn detect_snapshot_backend(
    workspace_root: &str,
    preference: Option<&str>,
) -> Box<dyn SnapshotBackend> {
    let pref = preference.unwrap_or("auto");

    match pref {
        "zfs" => {
            if let Some(dataset) = detect_zfs_dataset(workspace_root) {
                tracing::info!(dataset = %dataset, "snapshot backend: ZFS (forced)");
                return Box::new(ZfsBackend::new(dataset).expect("failed to init ZFS backend"));
            }
            tracing::warn!(
                "ZFS backend requested but workspace is not on a ZFS dataset; falling back to tar"
            );
        }
        "btrfs" => {
            if is_btrfs(workspace_root) {
                tracing::info!("snapshot backend: BTRFS (forced)");
                return Box::new(BtrfsBackend::new().expect("failed to init BTRFS backend"));
            }
            tracing::warn!("BTRFS backend requested but workspace is not on a BTRFS filesystem; falling back to tar");
        }
        "tar" => {
            tracing::info!("snapshot backend: tar (forced)");
            return Box::new(TarBackend::new().expect("failed to init tar backend"));
        }
        _ => {
            // "auto" or unknown — try detection
            if let Some(dataset) = detect_zfs_dataset(workspace_root) {
                tracing::info!(dataset = %dataset, "snapshot backend: ZFS (auto-detected)");
                return Box::new(ZfsBackend::new(dataset).expect("failed to init ZFS backend"));
            }
            if is_btrfs(workspace_root) {
                tracing::info!("snapshot backend: BTRFS (auto-detected)");
                return Box::new(BtrfsBackend::new().expect("failed to init BTRFS backend"));
            }
        }
    }

    tracing::info!("snapshot backend: tar (default)");
    Box::new(TarBackend::new().expect("failed to init tar backend"))
}

/// Try to find the ZFS dataset for a given path using `df -T`.
fn detect_zfs_dataset(path: &str) -> Option<String> {
    let output = Command::new("df").arg("-T").arg(path).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // df -T output: second line, first column is filesystem, second column is type
    let line = stdout.lines().nth(1)?;
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 && parts[1] == "zfs" {
        // First column is the dataset name
        Some(parts[0].to_string())
    } else {
        None
    }
}

/// Check if the given path resides on a BTRFS filesystem.
fn is_btrfs(path: &str) -> bool {
    let output = Command::new("stat")
        .arg("-f")
        .arg("-c")
        .arg("%T")
        .arg(path)
        .output();

    match output {
        Ok(result) if result.status.success() => {
            let fs_type = String::from_utf8_lossy(&result.stdout).trim().to_string();
            fs_type == "btrfs"
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Snapshot retention, cleanup, stats
// ---------------------------------------------------------------------------

/// Configuration for snapshot retention limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRetentionConfig {
    /// Maximum number of snapshots to keep.
    pub max_snapshots: usize,
    /// Maximum total size of all snapshots in megabytes.
    pub max_total_size_mb: u64,
    /// Whether to automatically enforce retention after each create().
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

/// Aggregate statistics about stored snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotStats {
    pub count: usize,
    pub total_size_bytes: u64,
    pub oldest_timestamp: Option<u64>,
    pub newest_timestamp: Option<u64>,
}

/// Remove oldest snapshots to stay within retention limits.
/// Returns list of removed snapshot IDs.
///
/// Only operates on snapshots tracked in the `HistoryStore` (SQLite index).
pub async fn enforce_retention(
    history: &HistoryStore,
    config: &SnapshotRetentionConfig,
) -> Result<Vec<String>> {
    let mut entries = history.list_snapshot_index(None).await?;
    let mut removed = Vec::new();

    // Sort by created_at ascending (oldest first)
    entries.sort_by_key(|e| e.created_at);

    // Remove oldest if exceeding max count
    while entries.len() > config.max_snapshots {
        if let Some(old) = entries.first() {
            let _ = std::fs::remove_file(&old.path);
            let _ = history.delete_snapshot_index(&old.snapshot_id).await;
            removed.push(old.snapshot_id.clone());
            entries.remove(0);
        }
    }

    // Remove oldest if exceeding total size
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
        tracing::info!(
            count = removed.len(),
            ids = ?removed,
            "snapshot retention: removed old snapshots"
        );
    }

    Ok(removed)
}

/// Compute aggregate statistics for all tracked snapshots.
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

/// Delete a single snapshot by ID. Returns `true` if a snapshot was found and removed.
pub async fn delete_snapshot(history: &HistoryStore, snapshot_id: &str) -> Result<bool> {
    let Some(entry) = history.get_snapshot_index(snapshot_id).await? else {
        return Ok(false);
    };
    let _ = std::fs::remove_file(&entry.path);
    history.delete_snapshot_index(snapshot_id).await?;
    tracing::info!(snapshot_id, "deleted snapshot");
    Ok(true)
}

/// Scan the snapshots directory and remove `.tar.gz` files not tracked in the index.
/// Returns the number of orphaned files removed.
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

// ---------------------------------------------------------------------------
// SnapshotStore — public facade (preserves existing API)
// ---------------------------------------------------------------------------

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

    /// Update the retention configuration (e.g. when the user changes settings).
    pub fn set_retention_config(&mut self, config: SnapshotRetentionConfig) {
        self.retention = config;
    }

    /// Get the current retention configuration.
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
            .upsert_snapshot_index(&encode_snapshot(&snapshot)).await?;

        // Enforce retention limits after creating a new snapshot
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

    /// Delete a single snapshot by ID.
    pub async fn delete(&self, snapshot_id: &str) -> Result<bool> {
        delete_snapshot(&self.history, snapshot_id).await
    }

    /// Get aggregate stats for all tracked snapshots.
    pub async fn stats(&self) -> Result<SnapshotStats> {
        get_snapshot_stats(&self.history).await
    }

    /// Remove orphaned .tar.gz files not tracked in the index.
    pub async fn cleanup_orphaned(&self) -> Result<usize> {
        cleanup_orphaned_files(&self.history).await
    }
}
