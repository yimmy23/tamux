use std::process::Command;
use std::time::Duration;

use amux_protocol::{parse_npm_latest_version, TamuxUpdateStatus, TAMUX_NPM_LATEST_URL};
use anyhow::{anyhow, bail, Context, Result};

pub(crate) fn npm_command() -> &'static str {
    if cfg!(windows) {
        "npm.cmd"
    } else {
        "npm"
    }
}

pub(crate) async fn fetch_update_status(current_version: &str) -> Result<TamuxUpdateStatus> {
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?
        .get(TAMUX_NPM_LATEST_URL)
        .header(
            reqwest::header::USER_AGENT,
            format!("tamux-cli/{}", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .context("failed to query npm registry")?
        .error_for_status()
        .context("npm registry returned an error status")?;

    let body = response
        .text()
        .await
        .context("failed to read npm response")?;
    let latest_version = parse_npm_latest_version(&body)
        .ok_or_else(|| anyhow!("npm registry response did not include a valid version"))?;

    TamuxUpdateStatus::from_versions(current_version, &latest_version)
        .ok_or_else(|| anyhow!("failed to compare current version against npm @latest"))
}

pub(crate) async fn print_upgrade_notice_if_available(current_version: &str) {
    if std::env::var_os("TAMUX_DISABLE_UPDATE_CHECK").is_some() {
        tracing::debug!("skipping update notice because TAMUX_DISABLE_UPDATE_CHECK is set");
        return;
    }

    let current_version = current_version.to_string();
    tokio::spawn(async move {
        match fetch_update_status(&current_version).await {
            Ok(status) => {
                if let Some(notice) = status.cli_notice() {
                    eprintln!("{notice}");
                }
            }
            Err(error) => {
                tracing::debug!(%error, "skipping update notice after npm lookup failure");
            }
        }
    });
}

pub(crate) fn run_upgrade() -> Result<()> {
    println!("Upgrading tamux via npm...");
    let status = Command::new(npm_command())
        .args(["install", "-g", "tamux@latest"])
        .status()
        .context("failed to launch npm; ensure Node.js and npm are installed and on PATH")?;

    if !status.success() {
        bail!("npm install -g tamux@latest failed");
    }

    println!("Upgrade complete.");
    Ok(())
}
