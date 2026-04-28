use super::*;

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
            tracing::warn!(
                "BTRFS backend requested but workspace is not on a BTRFS filesystem; falling back to tar"
            );
        }
        "tar" => {
            tracing::info!("snapshot backend: tar (forced)");
            return Box::new(TarBackend::new().expect("failed to init tar backend"));
        }
        _ => {
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

pub(super) fn detect_zfs_dataset(path: &str) -> Option<String> {
    let output = Command::new("df").arg("-T").arg(path).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().nth(1)?;
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 && parts[1] == "zfs" {
        Some(parts[0].to_string())
    } else {
        None
    }
}

pub(super) fn is_btrfs(path: &str) -> bool {
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
