use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use std::time::Duration;

use amux_protocol::{parse_npm_latest_version, TamuxUpdateStatus, TAMUX_NPM_LATEST_URL};
use anyhow::{anyhow, bail, Context, Result};

use crate::commands::common::find_sibling_binary;

const DIRECT_INSTALL_MARKER: &str = ".tamux-install-source";
const INSTALL_SCRIPT_URL: &str =
    "https://raw.githubusercontent.com/mkurman/tamux/main/scripts/install.sh";
const INSTALL_POWERSHELL_URL: &str =
    "https://raw.githubusercontent.com/mkurman/tamux/main/scripts/install.ps1";
const PROCESS_STOP_TIMEOUT: Duration = Duration::from_secs(5);
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(100);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessTarget {
    Daemon,
    Gateway,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessCommandSpec {
    program: String,
    args: Vec<String>,
}

fn process_binary_name(target: ProcessTarget, windows: bool) -> &'static str {
    match (target, windows) {
        (ProcessTarget::Daemon, true) => "tamux-daemon.exe",
        (ProcessTarget::Gateway, true) => "tamux-gateway.exe",
        (ProcessTarget::Daemon, false) => "tamux-daemon",
        (ProcessTarget::Gateway, false) => "tamux-gateway",
    }
}

fn process_label(target: ProcessTarget) -> &'static str {
    match target {
        ProcessTarget::Daemon => "tamux-daemon",
        ProcessTarget::Gateway => "tamux-gateway",
    }
}

fn kill_command_spec(target: ProcessTarget, windows: bool) -> ProcessCommandSpec {
    let process_name = process_binary_name(target, windows);
    if windows {
        ProcessCommandSpec {
            program: "taskkill".to_string(),
            args: vec![
                "/IM".to_string(),
                process_name.to_string(),
                "/F".to_string(),
            ],
        }
    } else {
        ProcessCommandSpec {
            program: "pkill".to_string(),
            args: vec!["-x".to_string(), process_name.to_string()],
        }
    }
}

fn probe_command_spec(target: ProcessTarget, windows: bool) -> ProcessCommandSpec {
    let process_name = process_binary_name(target, windows);
    if windows {
        ProcessCommandSpec {
            program: "tasklist".to_string(),
            args: vec!["/FI".to_string(), format!("IMAGENAME eq {process_name}")],
        }
    } else {
        ProcessCommandSpec {
            program: "pgrep".to_string(),
            args: vec!["-x".to_string(), process_name.to_string()],
        }
    }
}

fn process_is_running(target: ProcessTarget) -> Result<bool> {
    let windows = cfg!(windows);
    let spec = probe_command_spec(target, windows);
    let output = Command::new(&spec.program)
        .args(&spec.args)
        .output()
        .with_context(|| format!("failed to probe {}", process_label(target)))?;

    if windows {
        let stdout = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
        Ok(stdout.contains(&process_binary_name(target, true).to_ascii_lowercase()))
    } else if output.status.success() {
        Ok(true)
    } else if output.status.code() == Some(1) {
        Ok(false)
    } else {
        Err(anyhow!(
            "failed to probe {}: {}",
            process_label(target),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn terminate_process(target: ProcessTarget) -> Result<()> {
    let spec = kill_command_spec(target, cfg!(windows));
    let output = Command::new(&spec.program)
        .args(&spec.args)
        .output()
        .with_context(|| format!("failed to stop {}", process_label(target)))?;

    if output.status.success() || !process_is_running(target)? {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exit status {}", output.status)
    };

    bail!("failed to stop {}: {detail}", process_label(target))
}

fn wait_for_process_exit(target: ProcessTarget, timeout: Duration) -> Result<()> {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if !process_is_running(target)? {
            return Ok(());
        }
        std::thread::sleep(PROCESS_POLL_INTERVAL);
    }

    if process_is_running(target)? {
        bail!("timed out waiting for {} to exit", process_label(target));
    }

    Ok(())
}

fn stop_all_tamux_processes() -> Result<()> {
    for target in [ProcessTarget::Gateway, ProcessTarget::Daemon] {
        terminate_process(target)?;
        wait_for_process_exit(target, PROCESS_STOP_TIMEOUT)?;
    }
    Ok(())
}

fn start_daemon_process() -> Result<()> {
    let mut command = Command::new(find_sibling_binary("tamux-daemon"));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }

    command
        .spawn()
        .context("failed to start tamux-daemon after restart")?;
    Ok(())
}

pub(crate) fn run_stop() -> Result<()> {
    stop_all_tamux_processes()?;
    println!("Stopped tamux daemon and gateway.");
    Ok(())
}

pub(crate) fn run_restart() -> Result<()> {
    stop_all_tamux_processes()?;
    start_daemon_process()?;
    println!("Restarted tamux daemon.");
    Ok(())
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
        reason: format!(
            "unable to determine install source for {}",
            exe_path.display()
        ),
    }
}

fn looks_like_npm_install(exe_path: &Path) -> bool {
    let lowered: Vec<String> = exe_path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect();

    lowered
        .windows(3)
        .any(|window| window[0] == "node_modules" && window[1] == "tamux" && window[2] == "bin")
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
                .context(
                    "failed to launch npm; ensure Node.js and npm are installed and on PATH",
                )?;

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
        std::fs::write(root.path().join("tamux-daemon"), b"binary").expect("write daemon");

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

    #[test]
    fn unix_process_control_uses_exact_process_names() {
        let daemon_kill = kill_command_spec(ProcessTarget::Daemon, false);
        let gateway_kill = kill_command_spec(ProcessTarget::Gateway, false);
        let daemon_probe = probe_command_spec(ProcessTarget::Daemon, false);
        let gateway_probe = probe_command_spec(ProcessTarget::Gateway, false);

        assert_eq!(daemon_kill.program, "pkill");
        assert_eq!(daemon_kill.args, vec!["-x", "tamux-daemon"]);
        assert_eq!(gateway_kill.program, "pkill");
        assert_eq!(gateway_kill.args, vec!["-x", "tamux-gateway"]);

        assert_eq!(daemon_probe.program, "pgrep");
        assert_eq!(daemon_probe.args, vec!["-x", "tamux-daemon"]);
        assert_eq!(gateway_probe.program, "pgrep");
        assert_eq!(gateway_probe.args, vec!["-x", "tamux-gateway"]);
    }

    #[test]
    fn windows_process_control_uses_exact_image_names() {
        let daemon_kill = kill_command_spec(ProcessTarget::Daemon, true);
        let gateway_kill = kill_command_spec(ProcessTarget::Gateway, true);
        let daemon_probe = probe_command_spec(ProcessTarget::Daemon, true);
        let gateway_probe = probe_command_spec(ProcessTarget::Gateway, true);

        assert_eq!(daemon_kill.program, "taskkill");
        assert_eq!(daemon_kill.args, vec!["/IM", "tamux-daemon.exe", "/F"]);
        assert_eq!(gateway_kill.program, "taskkill");
        assert_eq!(gateway_kill.args, vec!["/IM", "tamux-gateway.exe", "/F"]);

        assert_eq!(daemon_probe.program, "tasklist");
        assert_eq!(
            daemon_probe.args,
            vec!["/FI", "IMAGENAME eq tamux-daemon.exe"]
        );
        assert_eq!(gateway_probe.program, "tasklist");
        assert_eq!(
            gateway_probe.args,
            vec!["/FI", "IMAGENAME eq tamux-gateway.exe"]
        );
    }
}
