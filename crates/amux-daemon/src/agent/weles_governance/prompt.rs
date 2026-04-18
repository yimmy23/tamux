use super::*;

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

fn task_status_label(status: crate::agent::types::TaskStatus) -> &'static str {
    match status {
        crate::agent::types::TaskStatus::Queued => "queued",
        crate::agent::types::TaskStatus::InProgress => "in_progress",
        crate::agent::types::TaskStatus::AwaitingApproval => "awaiting_approval",
        crate::agent::types::TaskStatus::Blocked => "blocked",
        crate::agent::types::TaskStatus::FailedAnalyzing => "failed_analyzing",
        crate::agent::types::TaskStatus::BudgetExceeded => "budget_exceeded",
        crate::agent::types::TaskStatus::Completed => "completed",
        crate::agent::types::TaskStatus::Failed => "failed",
        crate::agent::types::TaskStatus::Cancelled => "cancelled",
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
