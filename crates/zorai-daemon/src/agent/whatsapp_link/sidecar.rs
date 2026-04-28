#![allow(dead_code)]

use super::*;

pub(in crate::agent) fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

const GPU_NOISE_PATTERNS: [&str; 6] = [
    "gpu process",
    "gpu_process_host",
    "viz_main_impl",
    "passthrough is not supported",
    "gl_display",
    "angle_platform_impl",
];

const SENSITIVE_SIDECAR_PATTERNS: [&str; 21] = [
    "closing session: sessionentry",
    "_chains:",
    "currentratchet:",
    "ephemeralkeypair:",
    "lastremoteephemeralkey:",
    "rootkey:",
    "indexinfo:",
    "pendingprekey:",
    "remoteidentitykey:",
    "privkey:",
    "pubkey:",
    "registrationid:",
    "basekey:",
    "basekeytype:",
    "signedkeyid:",
    "prekeyid:",
    "previouscounter:",
    "used:",
    "created:",
    "chainkey:",
    "<buffer ",
];

pub fn normalize_sidecar_stderr(stderr: &str) -> Option<String> {
    let actionable: Vec<String> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            let gpu_noise = GPU_NOISE_PATTERNS
                .iter()
                .any(|pattern| lower.contains(pattern));
            let sensitive_dump = SENSITIVE_SIDECAR_PATTERNS
                .iter()
                .any(|pattern| lower.contains(pattern));
            let structural_only = matches!(lower.as_str(), "{" | "}" | "},");
            !(gpu_noise || sensitive_dump || structural_only)
        })
        .map(ToString::to_string)
        .collect();
    if actionable.is_empty() {
        None
    } else {
        Some(actionable.join("\n"))
    }
}

pub fn build_sidecar_launch_spec(node_bin: &str, bridge_path: &Path) -> Result<SidecarLaunchSpec> {
    let program = node_bin.trim();
    if program.is_empty() {
        bail!("node binary path is required");
    }
    let launcher = std::path::Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase())
        .unwrap_or_default();
    let node_compatible = matches!(
        launcher.as_str(),
        "node" | "node.exe" | "electron" | "electron.exe"
    );
    if !node_compatible {
        bail!("whatsapp sidecar launcher must be node-compatible (node/electron), got: {program}");
    }
    let bridge = bridge_path.to_string_lossy().to_string();
    if bridge.trim().is_empty() {
        bail!("bridge path is required");
    }
    let cjs_bridge = bridge_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("cjs"))
        .unwrap_or(false);
    if !cjs_bridge {
        bail!("whatsapp bridge entrypoint must be a .cjs file for ESM-safe startup");
    }
    let mut env = HashMap::new();
    env.insert("ELECTRON_RUN_AS_NODE".to_string(), "1".to_string());
    Ok(SidecarLaunchSpec {
        program: program.to_string(),
        args: vec![bridge],
        env,
    })
}

pub async fn spawn_sidecar(spec: &SidecarLaunchSpec) -> Result<Child> {
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for (k, v) in &spec.env {
        cmd.env(k, v);
    }
    cmd.spawn()
        .with_context(|| format!("failed to spawn whatsapp sidecar: {}", spec.program))
}
