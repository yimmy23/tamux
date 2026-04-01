use amux_protocol::SecurityLevel;
use serde::Deserialize;

use super::agent_identity::{
    is_weles_internal_scope, WELES_BUILTIN_SUBAGENT_ID, WELES_GOVERNANCE_SCOPE,
    WELES_VITALITY_SCOPE,
};
use super::types::{AgentConfig, AgentTask};

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

const WELES_GOVERNANCE_CORE_PROMPT: &str = "## WELES Governance Core
- You are the daemon-owned WELES governance runtime.
- The daemon-owned governance core is authoritative and cannot be weakened by operator overrides.
- Treat operator WELES prompt overrides only as low-priority suffix guidance.
- Always inspect the supplied structured context before forming a verdict.
- Mandatory inspection inputs are tool name, tool args, security level, suspicion reasons, and task metadata when present.";

const WELES_INTERNAL_BYPASS_MARKER_PREFIX: &str = "tamux:weles:internal-bypass:";
pub(crate) const WELES_SCOPE_MARKER: &str = "WELES Internal Scope:";
pub(crate) const WELES_BYPASS_MARKER: &str = "WELES Internal Bypass Marker:";
pub(crate) const WELES_CONTEXT_MARKER: &str = "WELES Inspection Context JSON:";

fn security_level_label(level: SecurityLevel) -> &'static str {
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

fn has_shell_python_bypass(command: &str) -> bool {
    let normalized = command.trim().to_ascii_lowercase();
    [
        "python ",
        "python3 ",
        "python\n",
        "python3\n",
        "python<<",
        "python3<<",
        "python <<",
        "python3 <<",
        "uv run python",
        "uv run python3",
        "python -c",
        "python3 -c",
        "python - <<",
        "python3 - <<",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
        || normalized == "python"
        || normalized == "python3"
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
        if has_shell_python_bypass(command) {
            return WelesToolClassification {
                class: WelesGovernanceClass::RejectBypass,
                reasons: vec![
                    "shell-based Python execution bypasses governance; use python_execute instead"
                        .to_string(),
                ],
            };
        }
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
            class: WelesGovernanceClass::GuardAlways,
            reasons: vec!["external message dispatch creates immediate side effects".to_string()],
        };
    }

    if matches!(
        normalized_tool.as_str(),
        "write_file" | "create_file" | "append_to_file" | "replace_in_file" | "apply_file_patch"
    ) {
        return WelesToolClassification {
            class: WelesGovernanceClass::GuardIfSuspicious,
            reasons: file_suspicion_reasons(
                tool_args
                    .get("path")
                    .and_then(|value| value.as_str())
                    .or_else(|| tool_args.get("filename").and_then(|value| value.as_str())),
            ),
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

pub(crate) fn is_suspicious_classification(classification: &WelesToolClassification) -> bool {
    matches!(classification.class, WelesGovernanceClass::GuardAlways)
        || matches!(classification.class, WelesGovernanceClass::RejectBypass)
        || !classification.reasons.is_empty()
}

pub(crate) fn should_guard_classification(classification: &WelesToolClassification) -> bool {
    match classification.class {
        WelesGovernanceClass::AllowDirect => false,
        WelesGovernanceClass::GuardIfSuspicious => !classification.reasons.is_empty(),
        WelesGovernanceClass::GuardAlways | WelesGovernanceClass::RejectBypass => true,
    }
}

pub(crate) fn direct_allow_decision(class: WelesGovernanceClass) -> WelesExecutionDecision {
    WelesExecutionDecision {
        class,
        should_execute: true,
        review: super::types::WelesReviewMeta {
            weles_reviewed: false,
            verdict: super::types::WelesVerdict::Allow,
            reasons: vec!["allow_direct: low-risk tool call".to_string()],
            audit_id: None,
            security_override_mode: None,
        },
        block_message: None,
    }
}

pub(crate) fn review_available(config: &AgentConfig) -> bool {
    config
        .extra
        .get("weles_review_available")
        .and_then(|value| value.as_bool())
        .unwrap_or(true)
}

pub(crate) fn bypass_decision(
    classification: &WelesToolClassification,
    security_level: SecurityLevel,
) -> WelesExecutionDecision {
    let yolo = matches!(security_level, SecurityLevel::Yolo);
    let mut reasons = classification.reasons.clone();
    if yolo {
        reasons.push("managed security yolo downgraded bypass rejection to flag_only".to_string());
    }
    WelesExecutionDecision {
        class: classification.class,
        should_execute: yolo,
        review: super::types::WelesReviewMeta {
            weles_reviewed: true,
            verdict: if yolo {
                super::types::WelesVerdict::FlagOnly
            } else {
                super::types::WelesVerdict::Block
            },
            reasons,
            audit_id: Some(format!("weles_{}", uuid::Uuid::new_v4())),
            security_override_mode: if yolo { Some("yolo".to_string()) } else { None },
        },
        block_message: if yolo {
            None
        } else {
            Some(
                "Blocked by WELES governance: shell-based Python execution must use python_execute instead."
                    .to_string(),
            )
        },
    }
}

pub(crate) fn normalize_runtime_verdict_for_classification(
    classification: &WelesToolClassification,
    security_level: SecurityLevel,
    runtime_review: WelesRuntimeReviewPayload,
) -> WelesRuntimeReviewPayload {
    if !matches!(classification.class, WelesGovernanceClass::RejectBypass) {
        return runtime_review;
    }

    WelesRuntimeReviewPayload {
        verdict: if matches!(security_level, SecurityLevel::Yolo) {
            super::types::WelesVerdict::FlagOnly
        } else {
            super::types::WelesVerdict::Block
        },
        reasons: runtime_review.reasons,
        audit_id: runtime_review.audit_id,
    }
}

pub(crate) fn unavailable_review_decision(
    classification: &WelesToolClassification,
    security_level: SecurityLevel,
) -> WelesExecutionDecision {
    let mut reasons = classification.reasons.clone();
    reasons.push("WELES review unavailable for guarded action".to_string());
    let yolo = matches!(security_level, SecurityLevel::Yolo)
        && is_suspicious_classification(classification);
    let block_message = if yolo {
        None
    } else if reasons.is_empty() {
        Some(
            "Blocked by WELES governance: review unavailable; guarded action failed closed."
                .to_string(),
        )
    } else {
        Some(format!(
            "Blocked by WELES governance: {}",
            reasons.join("; ")
        ))
    };
    WelesExecutionDecision {
        class: classification.class,
        should_execute: yolo,
        review: super::types::WelesReviewMeta {
            weles_reviewed: false,
            verdict: if yolo {
                super::types::WelesVerdict::FlagOnly
            } else {
                super::types::WelesVerdict::Block
            },
            reasons,
            audit_id: Some(format!("weles_{}", uuid::Uuid::new_v4())),
            security_override_mode: if yolo { Some("yolo".to_string()) } else { None },
        },
        block_message,
    }
}

pub(crate) fn guarded_fallback_decision(
    classification: &WelesToolClassification,
    security_level: SecurityLevel,
) -> WelesExecutionDecision {
    if matches!(classification.class, WelesGovernanceClass::RejectBypass) {
        bypass_decision(classification, security_level)
    } else {
        unavailable_review_decision(classification, security_level)
    }
}

pub(crate) fn internal_runtime_decision(
    classification: &WelesToolClassification,
    security_level: SecurityLevel,
) -> WelesExecutionDecision {
    if matches!(classification.class, WelesGovernanceClass::RejectBypass) {
        let mut decision = bypass_decision(classification, security_level);
        if !decision.review.reasons.iter().any(|reason| {
            reason == "daemon-owned WELES internal scope skips recursive governance review"
        }) {
            decision.review.reasons.push(
                "daemon-owned WELES internal scope skips recursive governance review".to_string(),
            );
        }
        return decision;
    }

    let mut reasons = classification.reasons.clone();
    reasons.push("daemon-owned WELES internal scope skips recursive governance review".to_string());
    let yolo = matches!(security_level, SecurityLevel::Yolo)
        && is_suspicious_classification(classification);
    let verdict = if yolo {
        super::types::WelesVerdict::FlagOnly
    } else {
        super::types::WelesVerdict::Block
    };
    let block_message = if yolo {
        None
    } else {
        Some(format!(
            "Blocked by WELES governance: {}",
            reasons.join("; ")
        ))
    };

    WelesExecutionDecision {
        class: classification.class,
        should_execute: yolo,
        review: super::types::WelesReviewMeta {
            weles_reviewed: true,
            verdict,
            reasons,
            audit_id: Some(format!("weles_{}", uuid::Uuid::new_v4())),
            security_override_mode: if yolo { Some("yolo".to_string()) } else { None },
        },
        block_message,
    }
}

pub(crate) fn build_weles_runtime_review_message(
    classification: &WelesToolClassification,
    security_level: SecurityLevel,
) -> String {
    let suspicion_summary = if classification.reasons.is_empty() {
        "none".to_string()
    } else {
        classification.reasons.join("; ")
    };
    format!(
        "Review the daemon-supplied WELES inspection context and respond with JSON only. Return a single object like {{\"verdict\":\"allow\"|\"block\",\"reasons\":[\"...\"],\"audit_id\":\"optional\"}}. Security level: {}. Governance class: {:?}. Suspicion summary: {}.",
        security_level_label(security_level),
        classification.class,
        suspicion_summary
    )
}

pub(crate) fn parse_weles_runtime_review_response(
    response: &str,
) -> Option<WelesRuntimeReviewPayload> {
    let trimmed = response.trim();
    serde_json::from_str::<WelesRuntimeReviewPayload>(trimmed)
        .ok()
        .or_else(|| {
            let start = trimmed.find('{')?;
            let end = trimmed.rfind('}')?;
            serde_json::from_str::<WelesRuntimeReviewPayload>(&trimmed[start..=end]).ok()
        })
}

pub(crate) fn reviewed_runtime_decision(
    classification: &WelesToolClassification,
    security_level: SecurityLevel,
    runtime_review: WelesRuntimeReviewPayload,
) -> WelesExecutionDecision {
    let yolo = matches!(security_level, SecurityLevel::Yolo)
        && is_suspicious_classification(classification);
    let mut reasons = if runtime_review.reasons.is_empty() {
        classification.reasons.clone()
    } else {
        runtime_review.reasons
    };
    for reason in &classification.reasons {
        if !reasons.iter().any(|existing| existing == reason) {
            reasons.push(reason.clone());
        }
    }
    if yolo {
        reasons
            .push("managed security yolo requires flag_only for suspicious tool calls".to_string());
    }

    let verdict = if yolo {
        super::types::WelesVerdict::FlagOnly
    } else if matches!(runtime_review.verdict, super::types::WelesVerdict::Block) {
        super::types::WelesVerdict::Block
    } else {
        super::types::WelesVerdict::Allow
    };
    let block_message = if matches!(verdict, super::types::WelesVerdict::Block) {
        Some(if reasons.is_empty() {
            "Blocked by WELES governance before tool execution.".to_string()
        } else {
            format!("Blocked by WELES governance: {}", reasons.join("; "))
        })
    } else {
        None
    };

    WelesExecutionDecision {
        class: classification.class,
        should_execute: !matches!(verdict, super::types::WelesVerdict::Block),
        review: super::types::WelesReviewMeta {
            weles_reviewed: true,
            verdict,
            reasons,
            audit_id: runtime_review
                .audit_id
                .or_else(|| Some(format!("weles_{}", uuid::Uuid::new_v4()))),
            security_override_mode: if yolo { Some("yolo".to_string()) } else { None },
        },
        block_message,
    }
}

fn render_task_metadata(task: Option<&AgentTask>) -> String {
    let Some(task) = task else {
        return "task_metadata: none".to_string();
    };

    let mut lines = vec![
        format!("task_id: {}", task.id),
        format!("task_title: {}", task.title),
        format!("task_description: {}", task.description),
        format!("task_source: {}", task.source),
        format!("task_runtime: {}", task.runtime),
    ];
    if let Some(thread_id) = task.thread_id.as_deref() {
        lines.push(format!("thread_id: {thread_id}"));
    }
    if let Some(session_id) = task.session_id.as_deref() {
        lines.push(format!("session_id: {session_id}"));
    }
    if let Some(goal_run_id) = task.goal_run_id.as_deref() {
        lines.push(format!("goal_run_id: {goal_run_id}"));
    }
    if let Some(goal_run_title) = task.goal_run_title.as_deref() {
        lines.push(format!("goal_run_title: {goal_run_title}"));
    }
    if let Some(goal_step_id) = task.goal_step_id.as_deref() {
        lines.push(format!("goal_step_id: {goal_step_id}"));
    }
    if let Some(goal_step_title) = task.goal_step_title.as_deref() {
        lines.push(format!("goal_step_title: {goal_step_title}"));
    }
    if let Some(parent_task_id) = task.parent_task_id.as_deref() {
        lines.push(format!("parent_task_id: {parent_task_id}"));
    }
    if let Some(parent_thread_id) = task.parent_thread_id.as_deref() {
        lines.push(format!("parent_thread_id: {parent_thread_id}"));
    }
    if let Some(sub_agent_def_id) = task.sub_agent_def_id.as_deref() {
        lines.push(format!("sub_agent_def_id: {sub_agent_def_id}"));
    }
    lines.join("\n")
}

fn task_status_label(status: super::types::TaskStatus) -> &'static str {
    match status {
        super::types::TaskStatus::Queued => "queued",
        super::types::TaskStatus::InProgress => "in_progress",
        super::types::TaskStatus::AwaitingApproval => "awaiting_approval",
        super::types::TaskStatus::Blocked => "blocked",
        super::types::TaskStatus::FailedAnalyzing => "failed_analyzing",
        super::types::TaskStatus::Completed => "completed",
        super::types::TaskStatus::Failed => "failed",
        super::types::TaskStatus::Cancelled => "cancelled",
    }
}

pub(crate) fn build_task_health_signals(task: Option<&AgentTask>) -> serde_json::Value {
    let Some(task) = task else {
        return serde_json::json!({
            "status": "none",
            "progress": 0,
            "retry_count": 0,
            "max_retries": 0,
            "blocked_reason": null,
            "last_error": null,
        });
    };

    serde_json::json!({
        "status": task_status_label(task.status),
        "progress": task.progress,
        "retry_count": task.retry_count,
        "max_retries": task.max_retries,
        "blocked_reason": task.blocked_reason,
        "last_error": task.last_error,
    })
}

fn render_task_health_signals(
    task: Option<&AgentTask>,
    task_health_signals: Option<&serde_json::Value>,
) -> String {
    let signals = task_health_signals
        .cloned()
        .unwrap_or_else(|| build_task_health_signals(task));
    let blocked_reason = signals
        .get("blocked_reason")
        .and_then(|value| value.as_str())
        .unwrap_or("none");
    let last_error = signals
        .get("last_error")
        .and_then(|value| value.as_str())
        .unwrap_or("none");
    format!(
        "task_health_signals:\nstatus: {}\nprogress: {}\nretry_count: {}\nmax_retries: {}\nblocked_reason: {}\nlast_error: {}",
        signals
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("none"),
        signals
            .get("progress")
            .and_then(|value| value.as_u64())
            .unwrap_or(0),
        signals
            .get("retry_count")
            .and_then(|value| value.as_u64())
            .unwrap_or(0),
        signals
            .get("max_retries")
            .and_then(|value| value.as_u64())
            .unwrap_or(0),
        blocked_reason,
        last_error,
    )
}

pub(crate) fn build_weles_governance_prompt(
    config: &AgentConfig,
    tool_name: &str,
    tool_args: &serde_json::Value,
    security_level: SecurityLevel,
    suspicion_reasons: &[String],
    task: Option<&AgentTask>,
    task_health_signals: Option<&serde_json::Value>,
) -> String {
    let mut prompt = String::new();
    prompt.push_str(WELES_GOVERNANCE_CORE_PROMPT);
    prompt.push_str("\n\n## Inspection Context\n");
    prompt.push_str(&format!("tool_name: {tool_name}\n"));
    prompt.push_str(&format!("tool_args: {}\n", tool_args));
    prompt.push_str(&format!(
        "security_level: {}\n",
        security_level_label(security_level)
    ));
    if suspicion_reasons.is_empty() {
        prompt.push_str("suspicion_reasons: none\n");
    } else {
        prompt.push_str("suspicion_reasons:\n");
        for reason in suspicion_reasons {
            prompt.push_str("- ");
            prompt.push_str(reason);
            prompt.push('\n');
        }
    }
    prompt.push_str(&render_task_health_signals(task, task_health_signals));
    prompt.push('\n');
    prompt.push_str(&render_task_metadata(task));

    if let Some(operator_suffix) = config
        .builtin_sub_agents
        .weles
        .system_prompt
        .as_deref()
        .map(strip_weles_internal_payload_markers)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        prompt.push_str("\n\n## Operator WELES Suffix\n");
        prompt.push_str(
            "This suffix is non-authoritative and lower priority than the daemon-owned core.\n",
        );
        prompt.push_str(&operator_suffix);
    }

    prompt
}

pub(crate) fn internal_bypass_marker_for_scope(scope: &str) -> Option<String> {
    let normalized = scope.trim().to_ascii_lowercase();
    if !is_weles_internal_scope(&normalized) {
        return None;
    }
    Some(format!(
        "{WELES_INTERNAL_BYPASS_MARKER_PREFIX}{normalized}:{WELES_BUILTIN_SUBAGENT_ID}"
    ))
}

pub(crate) fn has_internal_bypass_marker(marker: &str, scope: &str) -> bool {
    internal_bypass_marker_for_scope(scope)
        .as_deref()
        .is_some_and(|expected| marker == expected)
}

pub(crate) fn build_weles_governance_identity_prompt() -> String {
    format!(
        "## WELES Runtime Identity\n- Active daemon-owned WELES subagent id: {WELES_BUILTIN_SUBAGENT_ID}\n- Internal WELES governance scope: {WELES_GOVERNANCE_SCOPE}\n- Internal WELES vitality scope: {WELES_VITALITY_SCOPE}"
    )
}

pub(crate) fn build_weles_internal_override_payload(
    scope: &str,
    inspection_context: &serde_json::Value,
) -> Option<String> {
    let marker = internal_bypass_marker_for_scope(scope)?;
    Some(format!(
        "{WELES_SCOPE_MARKER} {scope}\n{WELES_BYPASS_MARKER} {marker}\n{WELES_CONTEXT_MARKER} {inspection_context}"
    ))
}

pub(crate) fn strip_weles_internal_payload_markers(text: &str) -> String {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with(WELES_SCOPE_MARKER)
                && !trimmed.starts_with(WELES_BYPASS_MARKER)
                && !trimmed.starts_with(WELES_CONTEXT_MARKER)
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

pub(crate) fn parse_weles_internal_override_payload(
    override_prompt: &str,
) -> Option<(String, String, serde_json::Value)> {
    let mut scope = None::<String>;
    let mut marker = None::<String>;
    let mut context = None::<serde_json::Value>;

    for line in override_prompt.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix(WELES_SCOPE_MARKER) {
            let value = value.trim();
            if !value.is_empty() {
                scope = Some(value.to_string());
            }
        } else if let Some(value) = trimmed.strip_prefix(WELES_BYPASS_MARKER) {
            let value = value.trim();
            if !value.is_empty() {
                marker = Some(value.to_string());
            }
        } else if let Some(value) = trimmed.strip_prefix(WELES_CONTEXT_MARKER) {
            let value = value.trim();
            if !value.is_empty() {
                context = serde_json::from_str::<serde_json::Value>(value).ok();
            }
        }
    }

    let scope = scope?;
    let marker = marker?;
    let context = context?;
    if !has_internal_bypass_marker(&marker, &scope) {
        return None;
    }
    Some((scope, marker, context))
}
