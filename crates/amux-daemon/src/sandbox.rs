#![allow(dead_code)]

//! Workspace sandboxing for managed commands.
//! Uses bubblewrap (bwrap) on Linux, sandbox-exec on macOS,
//! or falls back to a passthrough when neither is available.

/// Result of sandbox wrapping a command.
pub struct SandboxedCommand {
    pub program: String,
    pub args: Vec<String>,
}

pub trait Sandbox: Send + Sync {
    fn wrap(&self, command: &str, workspace_root: &str, allow_network: bool) -> SandboxedCommand;
    fn name(&self) -> &'static str;
}

/// Linux: uses bubblewrap (bwrap) for mount namespace isolation.
pub struct BwrapSandbox;

impl Sandbox for BwrapSandbox {
    fn name(&self) -> &'static str {
        "bwrap"
    }

    fn wrap(&self, command: &str, workspace_root: &str, allow_network: bool) -> SandboxedCommand {
        let mut args = vec![
            "--die-with-parent".to_string(),
            "--ro-bind".to_string(),
            "/usr".to_string(),
            "/usr".to_string(),
            "--ro-bind".to_string(),
            "/lib".to_string(),
            "/lib".to_string(),
            "--ro-bind".to_string(),
            "/lib64".to_string(),
            "/lib64".to_string(),
            "--ro-bind".to_string(),
            "/bin".to_string(),
            "/bin".to_string(),
            "--ro-bind".to_string(),
            "/sbin".to_string(),
            "/sbin".to_string(),
            "--ro-bind".to_string(),
            "/etc".to_string(),
            "/etc".to_string(),
            "--proc".to_string(),
            "/proc".to_string(),
            "--dev".to_string(),
            "/dev".to_string(),
            "--tmpfs".to_string(),
            "/tmp".to_string(),
            "--bind".to_string(),
            workspace_root.to_string(),
            workspace_root.to_string(),
            "--chdir".to_string(),
            workspace_root.to_string(),
        ];

        if !allow_network {
            args.push("--unshare-net".to_string());
        }

        // Add the shell command
        args.push("--".to_string());
        args.push("sh".to_string());
        args.push("-c".to_string());
        args.push(command.to_string());

        SandboxedCommand {
            program: "bwrap".to_string(),
            args,
        }
    }
}

/// macOS: uses sandbox-exec with a generated profile.
pub struct SeatbeltSandbox;

impl Sandbox for SeatbeltSandbox {
    fn name(&self) -> &'static str {
        "seatbelt"
    }

    fn wrap(&self, command: &str, workspace_root: &str, allow_network: bool) -> SandboxedCommand {
        let network_rule = if allow_network {
            "(allow network*)"
        } else {
            "(deny network*)"
        };

        let profile = format!(
            r#"(version 1)
(deny default)
(allow process-exec)
(allow process-fork)
(allow file-read* (subpath "/usr") (subpath "/bin") (subpath "/sbin") (subpath "/Library") (subpath "/System") (subpath "/etc") (subpath "/dev") (subpath "/private"))
(allow file-read* file-write* (subpath "{workspace_root}"))
(allow file-read* file-write* (subpath "/tmp"))
(allow file-read* file-write* (subpath "/private/tmp"))
(allow sysctl-read)
(allow mach-lookup)
{network_rule}"#
        );

        SandboxedCommand {
            program: "sandbox-exec".to_string(),
            args: vec![
                "-p".to_string(),
                profile,
                "sh".to_string(),
                "-c".to_string(),
                command.to_string(),
            ],
        }
    }
}

/// No-op fallback when sandbox binaries are unavailable.
pub struct PassthroughSandbox;

impl Sandbox for PassthroughSandbox {
    fn name(&self) -> &'static str {
        "passthrough"
    }

    fn wrap(&self, command: &str, _workspace_root: &str, _allow_network: bool) -> SandboxedCommand {
        SandboxedCommand {
            program: "sh".to_string(),
            args: vec!["-c".to_string(), command.to_string()],
        }
    }
}

/// Detect the best available sandbox for the current platform.
pub fn detect_sandbox() -> Box<dyn Sandbox> {
    #[cfg(target_os = "linux")]
    {
        if which_exists("bwrap") {
            tracing::info!("sandbox: using bubblewrap (bwrap)");
            return Box::new(BwrapSandbox);
        }
    }

    #[cfg(target_os = "macos")]
    {
        if which_exists("sandbox-exec") {
            tracing::info!("sandbox: using macOS seatbelt (sandbox-exec)");
            return Box::new(SeatbeltSandbox);
        }
    }

    tracing::warn!(
        "sandbox: no sandbox binary found, using passthrough (commands run without isolation)"
    );
    Box::new(PassthroughSandbox)
}

fn which_exists(binary: &str) -> bool {
    std::process::Command::new("which")
        .arg(binary)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
