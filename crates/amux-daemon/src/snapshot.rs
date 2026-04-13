#![allow(dead_code)]

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

mod detect;
mod store;

use detect::detect_snapshot_backend;
pub use store::SnapshotStore;

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
