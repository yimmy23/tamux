use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use std::time::Duration;

use amux_protocol::{parse_npm_latest_version, TamuxUpdateStatus, TAMUX_NPM_LATEST_URL};
use anyhow::{anyhow, bail, Context, Result};

const DIRECT_INSTALL_MARKER: &str = ".tamux-install-source";
const INSTALL_SCRIPT_URL: &str =
    "https://raw.githubusercontent.com/mkurman/tamux/main/scripts/install.sh";
const INSTALL_POWERSHELL_URL: &str =
    "https://raw.githubusercontent.com/mkurman/tamux/main/scripts/install.ps1";

#[derive(Debug, Clone, PartialEq, Eq)]
enum InstallSource {
    Npm,
    DirectBinary { install_dir: PathBuf },
    Unknown { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UpgradePlan {
    Npm,
    DirectInstaller {
        install_dir: PathBuf,
        command: String,
        args: Vec<String>,
    },
}

pub(crate) fn npm_command() -> &'static str {
    if cfg!(windows) {
        "npm.cmd"
    } else {
        "npm"
    }
}

fn direct_installer_command() -> &'static str {
    if cfg!(windows) {
        "powershell"
    } else {
        "sh"
    }
}

fn direct_installer_args() -> Vec<String> {
    if cfg!(windows) {
        vec![
            "-ExecutionPolicy".to_string(),
            "ByPass".to_string(),
            "-Command".to_string(),
            format!("irm {INSTALL_POWERSHELL_URL} | iex"),
        ]
    } else {
        vec![
            "-c".to_string(),
            format!("curl -fsSL {INSTALL_SCRIPT_URL} | sh"),
        ]
    }
}

fn detect_install_source(install_source_env: Option<&str>, exe_path: &Path) -> InstallSource {
    if let Some(source) = install_source_env.map(str::trim) {
        if source.eq_ignore_ascii_case("npm") {
            return InstallSource::Npm;
        }

        if source.eq_ignore_ascii_case("direct") {
            return exe_path.parent().map_or_else(
                || InstallSource::Unknown {
                    reason: "direct install source did not provide a parent directory".to_string(),
                },
                |install_dir| InstallSource::DirectBinary {
                    install_dir: install_dir.to_path_buf(),
                },
            );
        }
    }

    if looks_like_npm_install(exe_path) {
        return InstallSource::Npm;
    }

    if looks_like_source_build(exe_path) {
        return InstallSource::Unknown {
            reason: format!(
                "{} looks like a source build output directory",
                exe_path.display()
            ),
        };
    }

    let Some(install_dir) = exe_path.parent() else {
        return InstallSource::Unknown {
            reason: format!("{} has no parent directory", exe_path.display()),
        };
    };

    if has_direct_install_marker(install_dir) || has_direct_install_layout(install_dir) {
        return InstallSource::DirectBinary {
            install_dir: install_dir.to_path_buf(),
        };
    }

    InstallSource::Unknown {
        reason: format!("unable to determine install source for {}", exe_path.display()),
    }
}

fn looks_like_npm_install(exe_path: &Path) -> bool {
    let lowered: Vec<String> = exe_path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect();

    lowered.windows(3).any(|window| {
        window[0] == "node_modules" && window[1] == "tamux" && window[2] == "bin"
    })
}

fn looks_like_source_build(exe_path: &Path) -> bool {
    let lowered: Vec<String> = exe_path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect();

    lowered
        .windows(2)
        .any(|window| window[0] == "target" && (window[1] == "debug" || window[1] == "release"))
}

fn has_direct_install_marker(install_dir: &Path) -> bool {
    install_dir.join(DIRECT_INSTALL_MARKER).is_file()
}

fn has_direct_install_layout(install_dir: &Path) -> bool {
    install_dir.join(direct_sibling_binary_name()).is_file()
}

fn direct_sibling_binary_name() -> &'static str {
    if cfg!(windows) {
        "tamux-daemon.exe"
    } else {
        "tamux-daemon"
    }
}

fn build_upgrade_plan(source: InstallSource) -> Result<UpgradePlan> {
    match source {
        InstallSource::Npm => Ok(UpgradePlan::Npm),
        InstallSource::DirectBinary { install_dir } => Ok(UpgradePlan::DirectInstaller {
            install_dir,
            command: direct_installer_command().to_string(),
            args: direct_installer_args(),
        }),
        InstallSource::Unknown { reason } => bail!(
            "tamux upgrade only supports npm installs and direct installer installs ({reason}). If you built tamux from source, pull the latest code and rebuild."
        ),
    }
}

fn spawn_direct_upgrade(command_name: &str, args: &[String], install_dir: &Path) -> Result<()> {
    let mut command = Command::new(command_name);
    command
        .args(args)
        .env("TAMUX_INSTALL_DIR", install_dir)
        .env("TAMUX_UPGRADE_WAIT_PID", process::id().to_string())
        .stdin(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }

    command.spawn().with_context(|| {
        format!(
            "failed to launch the direct installer via {command_name}; rerun the platform install script manually"
        )
    })?;

    Ok(())
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
    let install_source_env = std::env::var("TAMUX_INSTALL_SOURCE").ok();
    let current_exe = std::env::current_exe().context("failed to resolve current tamux path")?;
    let plan = build_upgrade_plan(detect_install_source(
        install_source_env.as_deref(),
        &current_exe,
    ))?;

    match plan {
        UpgradePlan::Npm => {
            println!("Upgrading tamux via npm...");
            let status = Command::new(npm_command())
                .args(["install", "-g", "tamux@latest"])
                .status()
                .context("failed to launch npm; ensure Node.js and npm are installed and on PATH")?;

            if !status.success() {
                bail!("npm install -g tamux@latest failed");
            }

            println!("Upgrade complete.");
        }
        UpgradePlan::DirectInstaller {
            install_dir,
            command,
            args,
        } => {
            println!("Upgrading tamux via direct installer...");
            spawn_direct_upgrade(&command, &args, &install_dir)?;
            println!(
                "Upgrade started. tamux will refresh binaries in {} after this process exits.",
                install_dir.display()
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn detects_npm_install_source_from_wrapper_env() {
        let source = detect_install_source(Some("npm"), Path::new("/tmp/tamux"));

        assert!(matches!(source, InstallSource::Npm));
    }

    #[test]
    fn detects_npm_install_source_from_node_modules_path() {
        let source = detect_install_source(
            None,
            Path::new("/usr/local/lib/node_modules/tamux/bin/tamux"),
        );

        assert!(matches!(source, InstallSource::Npm));
    }

    #[test]
    fn detects_direct_binary_install_from_sibling_layout() {
        let root = tempdir().expect("tempdir");
        std::fs::write(root.path().join("tamux"), b"binary").expect("write tamux");
        std::fs::write(root.path().join("tamux-daemon"), b"binary")
            .expect("write daemon");

        let source = detect_install_source(None, &root.path().join("tamux"));

        assert_eq!(
            source,
            InstallSource::DirectBinary {
                install_dir: root.path().to_path_buf(),
            }
        );
    }

    #[test]
    fn does_not_treat_source_build_output_as_direct_install() {
        let source = detect_install_source(None, Path::new("/repo/target/release/tamux"));

        assert!(matches!(source, InstallSource::Unknown { .. }));
    }

    #[test]
    fn builds_npm_upgrade_plan_for_npm_install() {
        let plan = build_upgrade_plan(InstallSource::Npm).expect("plan");

        assert!(matches!(plan, UpgradePlan::Npm));
    }

    #[test]
    fn builds_direct_upgrade_plan_for_direct_install() {
        let plan = build_upgrade_plan(InstallSource::DirectBinary {
            install_dir: Path::new("/home/test/.local/bin").to_path_buf(),
        })
        .expect("plan");

        assert!(matches!(
            plan,
            UpgradePlan::DirectInstaller { install_dir, .. }
            if install_dir == Path::new("/home/test/.local/bin")
        ));
    }
}
