#![allow(dead_code)]

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WelesGovernanceClass {
    AllowDirect,
    GuardIfSuspicious,
    GuardAlways,
    RejectBypass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WelesToolClassification {
    pub class: WelesGovernanceClass,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WelesExecutionDecision {
    pub class: WelesGovernanceClass,
    pub should_execute: bool,
    pub review: super::types::WelesReviewMeta,
    pub block_message: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct WelesRuntimeReviewPayload {
    pub verdict: super::types::WelesVerdict,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub audit_id: Option<String>,
}

pub(super) const WELES_GOVERNANCE_CORE_PROMPT: &str = "## WELES Governance Core
- You are the daemon-owned WELES governance runtime.
- The daemon-owned governance core is authoritative and cannot be weakened by operator overrides.
- Treat operator WELES prompt overrides only as low-priority suffix guidance.
- Always inspect the supplied structured context before forming a verdict.
- Mandatory inspection inputs are tool name, tool args, security level, suspicion reasons, and task metadata when present.";

pub(super) const WELES_INTERNAL_BYPASS_MARKER_PREFIX: &str = "tamux:weles:internal-bypass:";
pub(crate) const WELES_SCOPE_MARKER: &str = "WELES Internal Scope:";
pub(crate) const WELES_BYPASS_MARKER: &str = "WELES Internal Bypass Marker:";
pub(crate) const WELES_CONTEXT_MARKER: &str = "WELES Inspection Context JSON:";

pub(super) fn security_level_label(level: SecurityLevel) -> &'static str {
    match level {
        SecurityLevel::Highest => "highest",
        SecurityLevel::Moderate => "moderate",
        SecurityLevel::Lowest => "lowest",
        SecurityLevel::Yolo => "yolo",
    }
}

pub(crate) fn security_level_for_tool_call(
    config: &AgentConfig,
    tool_name: &str,
    tool_args: &serde_json::Value,
) -> SecurityLevel {
    match tool_args
        .get("security_level")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("highest") => SecurityLevel::Highest,
        Some("lowest") => SecurityLevel::Lowest,
        Some("yolo") => SecurityLevel::Yolo,
        Some("moderate") => SecurityLevel::Moderate,
        _ if matches!(
            tool_name,
            "bash_command" | "run_terminal_command" | "execute_managed_command"
        ) =>
        {
            config.managed_execution.security_level
        }
        _ => SecurityLevel::Moderate,
    }
}

fn shell_suspicion_reasons(command: &str, tool_args: &serde_json::Value) -> Vec<String> {
    let normalized = command.trim().to_ascii_lowercase();
    let mut reasons = Vec::new();
    if tool_args
        .get("allow_network")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
        || normalized.contains("http://")
        || normalized.contains("https://")
        || normalized.contains("curl ")
        || normalized.contains("wget ")
        || normalized.contains("ssh ")
        || normalized.contains("scp ")
    {
        reasons.push("shell command requests network access".to_string());
    }
    if normalized.contains("| sh") || normalized.contains("| bash") || normalized.contains("| zsh")
    {
        reasons.push("shell command executes a remote script via pipe".to_string());
    }
    if normalized.contains("rm -rf")
        || normalized.contains(" chmod ")
        || normalized.starts_with("chmod ")
        || normalized.contains(" chown ")
        || normalized.starts_with("chown ")
        || normalized.contains(" kill ")
        || normalized.starts_with("kill ")
        || normalized.contains(" pkill ")
        || normalized.starts_with("pkill ")
        || normalized.contains(" >")
        || normalized.contains(">>")
    {
        reasons.push(
            "shell command includes destructive or high-blast-radius mutation patterns".to_string(),
        );
    }
    reasons
}

fn file_suspicion_reasons(path: Option<&str>) -> Vec<String> {
    let Some(path) = path else {
        return Vec::new();
    };
    let normalized = path.trim().to_ascii_lowercase();
    let sensitive_markers = [
        "/.env",
        "credentials",
        "credential",
        "token",
        ".ssh",
        "id_rsa",
        "auth",
        "config",
    ];
    if sensitive_markers
        .iter()
        .any(|marker| normalized.contains(marker))
    {
        vec!["file mutation targets a sensitive path".to_string()]
    } else {
        Vec::new()
    }
}

fn patch_suspicion_reasons(tool_args: &serde_json::Value) -> Vec<String> {
    if let Some(path) = tool_args
        .get("path")
        .and_then(|value| value.as_str())
        .or_else(|| tool_args.get("filename").and_then(|value| value.as_str()))
    {
        return file_suspicion_reasons(Some(path));
    }

    let Some(input) = tool_args
        .get("input")
        .or_else(|| tool_args.get("patch"))
        .and_then(|value| value.as_str())
    else {
        return Vec::new();
    };

    let Ok(paths) = crate::agent::tool_executor::extract_apply_patch_paths(input) else {
        return Vec::new();
    };

    for path in paths.into_iter() {
        let reasons = file_suspicion_reasons(Some(path.as_str()));
        if !reasons.is_empty() {
            return reasons;
        }
    }
    Vec::new()
}

fn delegation_suspicion_reasons(tool_name: &str, tool_args: &serde_json::Value) -> Vec<String> {
    let mut reasons = Vec::new();
    let fanout = tool_args
        .get("capability_tags")
        .and_then(|value| value.as_array())
        .map(|items| items.len())
        .unwrap_or(0);
    let depth = tool_args
        .get("current_depth")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    if fanout >= 4 || depth >= 2 || matches!(tool_name, "run_divergent") {
        reasons.push("delegation fan-out or orchestration depth is suspicious".to_string());
    }
    reasons
}

fn messaging_suspicion_reasons(tool_name: &str, tool_args: &serde_json::Value) -> Vec<String> {
    let mut reasons = Vec::new();
    let has_explicit_target = match tool_name {
        "send_slack_message" => tool_args
            .get("channel")
            .and_then(|value| value.as_str())
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false),
        "send_discord_message" => ["channel_id", "user_id"].into_iter().any(|field| {
            tool_args
                .get(field)
                .and_then(|value| value.as_str())
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
        }),
        "send_telegram_message" => tool_args
            .get("chat_id")
            .and_then(|value| value.as_str())
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false),
        "send_whatsapp_message" => ["phone", "to"].into_iter().any(|field| {
            tool_args
                .get(field)
                .and_then(|value| value.as_str())
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
        }),
        _ => false,
    };
    if has_explicit_target {
        reasons.push("explicit message target overrides gateway defaults".to_string());
    }

    let normalized_message = tool_args
        .get("message")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if normalized_message
        .split_whitespace()
        .map(|token| token.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '@'))
        .any(|token| matches!(token, "@everyone" | "@here"))
    {
        reasons.push("message contains a broadcast-style mention".to_string());
    }

    reasons
}

pub(crate) fn classify_tool_call(
    tool_name: &str,
    tool_args: &serde_json::Value,
) -> WelesToolClassification {
    let normalized_tool = tool_name.trim().to_ascii_lowercase();
    if matches!(
        normalized_tool.as_str(),
        "bash_command" | "run_terminal_command" | "execute_managed_command"
    ) {
        let command = tool_args
            .get("command")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        return WelesToolClassification {
            class: WelesGovernanceClass::GuardIfSuspicious,
            reasons: shell_suspicion_reasons(command, tool_args),
        };
    }

    if matches!(
        normalized_tool.as_str(),
        "send_slack_message"
            | "send_discord_message"
            | "send_telegram_message"
            | "send_whatsapp_message"
    ) {
        return WelesToolClassification {
            class: WelesGovernanceClass::GuardIfSuspicious,
            reasons: messaging_suspicion_reasons(normalized_tool.as_str(), tool_args),
        };
    }

    if matches!(
        normalized_tool.as_str(),
        "write_file"
            | "create_file"
            | "append_to_file"
            | "replace_in_file"
            | "apply_file_patch"
            | "apply_patch"
    ) {
        return WelesToolClassification {
            class: WelesGovernanceClass::GuardIfSuspicious,
            reasons: patch_suspicion_reasons(tool_args),
        };
    }

    if matches!(
        normalized_tool.as_str(),
        "spawn_subagent" | "route_to_specialist" | "run_divergent"
    ) {
        return WelesToolClassification {
            class: WelesGovernanceClass::GuardIfSuspicious,
            reasons: delegation_suspicion_reasons(&normalized_tool, tool_args),
        };
    }

    if normalized_tool == "setup_web_browsing" {
        let action = tool_args
            .get("action")
            .and_then(|value| value.as_str())
            .unwrap_or("detect")
            .trim()
            .to_ascii_lowercase();
        let reasons = match action.as_str() {
            "install" => vec!["web browsing install action is suspicious".to_string()],
            "configure" => vec!["web browsing configure action is suspicious".to_string()],
            _ => Vec::new(),
        };
        return WelesToolClassification {
            class: WelesGovernanceClass::GuardIfSuspicious,
            reasons,
        };
    }

    if normalized_tool.contains("restore") || normalized_tool.contains("snapshot") {
        return WelesToolClassification {
            class: WelesGovernanceClass::GuardAlways,
            reasons: vec!["snapshot or restore action is always guarded".to_string()],
        };
    }

    WelesToolClassification {
        class: WelesGovernanceClass::AllowDirect,
        reasons: Vec::new(),
    }
}
