use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::mpsc::UnboundedSender;
use zorai_protocol::{parse_npm_latest_version, ZoraiUpdateStatus, ZORAI_NPM_LATEST_URL};

use crate::state::DaemonCommand;

pub(crate) fn spawn_update_check(daemon_cmd_tx: UnboundedSender<DaemonCommand>) {
    std::thread::spawn(
        move || match fetch_update_status(env!("CARGO_PKG_VERSION")) {
            Ok(status) => {
                let _ = daemon_cmd_tx.send(DaemonCommand::UpsertNotification(
                    status.into_notification(now_unix_ms()),
                ));
            }
            Err(error) => {
                tracing::debug!(%error, "skipping TUI update notification after npm lookup failure");
            }
        },
    );
}

fn fetch_update_status(current_version: &str) -> Result<ZoraiUpdateStatus, String> {
    let mut response = ureq::get(ZORAI_NPM_LATEST_URL)
        .config()
        .timeout_global(Some(Duration::from_secs(3)))
        .build()
        .call()
        .map_err(|error| error.to_string())?;
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())?;
    let latest_version = parse_npm_latest_version(&body)
        .ok_or_else(|| "npm registry response did not include a valid version".to_string())?;

    ZoraiUpdateStatus::from_versions(current_version, &latest_version)
        .ok_or_else(|| "failed to compare current version against npm @latest".to_string())
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
