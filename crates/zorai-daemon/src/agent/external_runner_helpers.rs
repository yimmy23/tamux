use super::*;

pub(super) fn find_executable(agent_type: &str) -> Option<String> {
    let name = match agent_type {
        "hermes" => "hermes",
        "openclaw" => "openclaw",
        _ => agent_type,
    };

    match which::which(name) {
        Ok(path) => {
            let path_str = path.to_string_lossy().to_string();
            tracing::info!(
                agent = %agent_type,
                path = %path_str,
                "external agent: executable found"
            );
            Some(path_str)
        }
        Err(_) => {
            tracing::debug!(
                agent = %agent_type,
                name = %name,
                "external agent: executable not found on PATH"
            );
            None
        }
    }
}

pub(super) fn build_one_shot_args(agent_type: &str, prompt: &str) -> Vec<String> {
    match agent_type {
        "hermes" => vec![
            "chat".to_string(),
            "-q".to_string(),
            prompt.to_string(),
            "-Q".to_string(),
        ],
        "openclaw" => vec![
            "agent".to_string(),
            "--agent".to_string(),
            "main".to_string(),
            "-m".to_string(),
            prompt.to_string(),
            "--json".to_string(),
        ],
        _ => vec!["--".to_string(), prompt.to_string()],
    }
}

pub(super) fn build_gateway_args(agent_type: &str) -> Vec<String> {
    match agent_type {
        "hermes" => vec!["gateway".to_string()],
        "openclaw" => vec!["gateway".to_string()],
        _ => vec!["gateway".to_string()],
    }
}

pub(super) fn strip_ansi(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if ('\x40'..='\x7e').contains(&next) {
                        break;
                    }
                }
            } else if chars.peek() == Some(&']') {
                chars.next();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == '\x07' {
                        break;
                    }
                    if next == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            } else {
                chars.next();
            }
        } else if c == '\r' {
            result.clear();
        } else {
            result.push(c);
        }
    }

    result
}

pub(super) fn is_tui_noise(line: &str) -> bool {
    let s = line.trim();
    if s.is_empty() {
        return true;
    }
    if s.starts_with('╭')
        || s.starts_with('╰')
        || s.starts_with('│')
        || s.starts_with('┌')
        || s.starts_with('└')
    {
        return true;
    }
    if s.ends_with('╮')
        || s.ends_with('╯')
        || s.ends_with('│')
        || s.ends_with('┐')
        || s.ends_with('┘')
    {
        return true;
    }
    if s.chars().all(|c| "─═╌╍┈┉━".contains(c) || c == ' ') && s.len() > 3 {
        return true;
    }
    if s.contains("██") || s.contains("╗") || s.contains("╚") || s.contains("╔") {
        return true;
    }
    if s.chars().any(|c| ('\u{2800}'..='\u{28FF}').contains(&c)) {
        return true;
    }
    if s.contains("...)") && (s.contains("(") && s.contains("s)")) {
        return true;
    }
    if s.contains("processing...")
        || s.contains("mulling...")
        || s.contains("thinking...")
        || s.contains("pondering...")
        || s.contains("reasoning...")
        || s.contains("working...")
        || s.contains("computing...")
        || s.contains("deliberating...")
        || s.contains("analyzing...")
        || s.contains("formulating...")
    {
        return true;
    }
    if s.starts_with("Query:")
        || s.contains("Hermes Agent v")
        || s.contains("Available Tools")
        || s.contains("Available Skills")
    {
        return true;
    }
    if s.contains("Session:") && s.len() < 60 {
        return true;
    }
    if (s.contains("browser_") || s.contains("file_tools:") || s.contains("code_execution:"))
        && !s.starts_with('{')
    {
        return true;
    }
    if s.contains("more toolsets") || s.contains("tools ·") || s.contains("/help for") {
        return true;
    }
    if s.contains("· Nous") || s.contains("· OpenRouter") || s.contains("· OpenAI") {
        return true;
    }
    if s.contains("error, retrying") || s.contains("API retry") {
        return true;
    }
    if s.starts_with("⚠️") || s.starts_with("❌") || s.starts_with("⏳") {
        return true;
    }
    if s.starts_with("⏱️") || s.starts_with("📝") || s.starts_with("📊") {
        return true;
    }
    if s == "> assistant" || s == "> user" {
        return true;
    }
    if s.starts_with("Resume this session with:") || s.starts_with("hermes --resume") {
        return true;
    }
    if s.starts_with("Duration:")
        || s.starts_with("Messages:")
        || s.starts_with("Session:")
        || s.starts_with("session_id:")
    {
        return true;
    }

    false
}

pub(super) struct ParsedResponse {
    pub(super) text: String,
    pub(super) input_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) provider: Option<String>,
    pub(super) model: Option<String>,
}

pub(super) fn parse_structured_response(agent_type: &str, raw: &str) -> ParsedResponse {
    let trimmed = raw.trim();
    if agent_type == "openclaw" {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let text = json
                .pointer("/result/payloads")
                .and_then(|payloads| payloads.as_array())
                .map(|payloads| {
                    payloads
                        .iter()
                        .filter_map(|payload| payload.get("text").and_then(|text| text.as_str()))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();

            let usage = json.pointer("/result/meta/agentMeta/usage");
            let input_tokens = usage
                .and_then(|usage| usage.get("input"))
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            let output_tokens = usage
                .and_then(|usage| usage.get("output"))
                .and_then(|value| value.as_u64())
                .unwrap_or(0);

            let provider = json
                .pointer("/result/meta/agentMeta/provider")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string());
            let model = json
                .pointer("/result/meta/agentMeta/model")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string());

            if !text.is_empty() {
                return ParsedResponse {
                    text,
                    input_tokens,
                    output_tokens,
                    provider,
                    model,
                };
            }
        }
    }

    ParsedResponse {
        text: trimmed.to_string(),
        input_tokens: 0,
        output_tokens: 0,
        provider: None,
        model: None,
    }
}

fn find_zorai_mcp_binary() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("zorai-mcp");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    if let Ok(path) = which::which("zorai-mcp") {
        return Some(path);
    }
    None
}

pub(super) fn check_zorai_mcp_configured(agent_type: &str) -> bool {
    match agent_type {
        "hermes" => check_hermes_mcp_config(),
        "openclaw" => check_openclaw_mcp_config(),
        _ => false,
    }
}

fn check_hermes_mcp_config() -> bool {
    let config_path = dirs::home_dir()
        .map(|home| home.join(".hermes/config.yaml"))
        .unwrap_or_default();
    let content = match std::fs::read_to_string(&config_path) {
        Ok(content) => content,
        Err(_) => return false,
    };
    content.contains("mcp_servers:") && content.contains("zorai")
}

fn check_openclaw_mcp_config() -> bool {
    match std::process::Command::new("mcporter")
        .args(["config", "get", "zorai", "--json"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}

pub fn ensure_zorai_mcp_configured(agent_type: &str) -> bool {
    if check_zorai_mcp_configured(agent_type) {
        tracing::info!(agent = %agent_type, "zorai-mcp already configured");
        return false;
    }
    let mcp_binary = match find_zorai_mcp_binary() {
        Some(path) => path,
        None => {
            tracing::warn!(
                agent = %agent_type,
                "zorai-mcp binary not found — cannot auto-inject MCP config"
            );
            return false;
        }
    };

    let mcp_path = mcp_binary.to_string_lossy().to_string();
    match agent_type {
        "hermes" => inject_hermes_mcp_config(&mcp_path),
        "openclaw" => inject_openclaw_mcp_config(&mcp_path),
        _ => false,
    }
}

fn inject_hermes_mcp_config(mcp_path: &str) -> bool {
    let config_path = match dirs::home_dir() {
        Some(home) => home.join(".hermes/config.yaml"),
        None => return false,
    };

    let content = match std::fs::read_to_string(&config_path) {
        Ok(content) => content,
        Err(error) => {
            tracing::warn!(error = %error, "failed to read hermes config");
            return false;
        }
    };

    let zorai_entry =
        format!("\nmcp_servers:\n  zorai:\n    command: \"{mcp_path}\"\n    args: []\n");

    let new_content = if content.contains("mcp_servers:") {
        content.replacen(
            "mcp_servers:",
            &format!("mcp_servers:\n  zorai:\n    command: \"{mcp_path}\"\n    args: []"),
            1,
        )
    } else {
        format!("{content}\n{zorai_entry}")
    };

    match std::fs::write(&config_path, &new_content) {
        Ok(()) => {
            tracing::info!(
                path = %config_path.display(),
                mcp_binary = %mcp_path,
                "injected zorai-mcp into hermes config"
            );
            true
        }
        Err(error) => {
            tracing::error!(error = %error, "failed to write hermes config");
            false
        }
    }
}

fn inject_openclaw_mcp_config(mcp_path: &str) -> bool {
    match std::process::Command::new("mcporter")
        .args([
            "config",
            "add",
            "zorai",
            "--command",
            mcp_path,
            "--scope",
            "home",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .status()
    {
        Ok(status) if status.success() => {
            tracing::info!(
                mcp_binary = %mcp_path,
                "injected zorai-mcp into mcporter config for openclaw"
            );
            true
        }
        Ok(status) => {
            tracing::warn!(exit = %status, "mcporter config add failed");
            false
        }
        Err(error) => {
            tracing::warn!(
                error = %error,
                "mcporter not found — install mcporter for OpenClaw MCP integration"
            );
            false
        }
    }
}
