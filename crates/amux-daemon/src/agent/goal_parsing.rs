//! Goal plan/reflection response parsing, JSON repair, and goal utility helpers.

use anyhow::Result;
use std::collections::HashSet;
use uuid::Uuid;

use super::types::*;
use super::now_millis;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct GoalPlanResponse {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub steps: Vec<GoalPlanStepResponse>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct GoalPlanStepResponse {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub kind: GoalRunStepKind,
    #[serde(default, alias = "_criteria", alias = "criteria")]
    pub success_criteria: String,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct GoalReflectionResponse {
    pub summary: String,
    #[serde(default)]
    pub stable_memory_update: Option<String>,
    #[serde(default)]
    pub generate_skill: bool,
    #[serde(default)]
    pub skill_title: Option<String>,
}

/// Check a plan for issues that the model should fix. Returns a list of human-readable problems.
pub(super) fn collect_plan_issues(plan: &GoalPlanResponse) -> Vec<String> {
    let mut issues = Vec::new();

    if plan.summary.trim().is_empty() {
        issues.push("Plan summary is empty — provide a brief description of the plan.".into());
    }
    if plan.steps.is_empty() {
        issues.push("Plan has no steps — provide at least 2 steps.".into());
    }
    if plan.steps.len() > 8 {
        issues.push(format!(
            "Plan has {} steps — reduce to 8 or fewer.",
            plan.steps.len()
        ));
    }

    for (i, step) in plan.steps.iter().enumerate() {
        let n = i + 1;
        if step.title.trim().is_empty() {
            issues.push(format!("Step {n}: missing title."));
        }
        if step.instructions.trim().is_empty() {
            issues.push(format!("Step {n}: missing instructions."));
        }
        if step.success_criteria.trim().is_empty() {
            issues.push(format!("Step {n}: missing success_criteria."));
        }
        if step.kind == GoalRunStepKind::Unknown {
            issues.push(format!("Step {n}: kind is empty or unknown — must be one of: command, research, reason, memory, skill."));
        }
    }

    issues
}

/// Apply safe defaults to a plan after all fix attempts are exhausted.
pub(super) fn apply_plan_defaults(plan: &mut GoalPlanResponse) {
    plan.summary = plan.summary.trim().to_string();
    if plan.summary.is_empty() {
        plan.summary = plan
            .title
            .clone()
            .unwrap_or_else(|| "Goal plan".to_string());
    }
    if plan.steps.is_empty() {
        plan.steps.push(GoalPlanStepResponse {
            title: plan.summary.clone(),
            instructions: plan.summary.clone(),
            kind: GoalRunStepKind::Command,
            success_criteria: "Step completed".to_string(),
            session_id: None,
        });
    }
    if plan.steps.len() > 8 {
        plan.steps.truncate(8);
    }
    for (i, step) in plan.steps.iter_mut().enumerate() {
        step.title = step.title.trim().to_string();
        step.instructions = step.instructions.trim().to_string();
        step.success_criteria = step.success_criteria.trim().to_string();
        step.session_id = step
            .session_id
            .take()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        if step.title.is_empty() {
            step.title = format!("Step {}", i + 1);
        }
        if step.instructions.is_empty() {
            step.instructions = step.title.clone();
        }
        if step.success_criteria.is_empty() {
            step.success_criteria = "Step completed successfully".to_string();
        }
        if step.kind == GoalRunStepKind::Unknown {
            step.kind = GoalRunStepKind::Command;
        }
    }
}

pub(super) fn parse_priority_str(value: &str) -> TaskPriority {
    match value {
        "low" => TaskPriority::Low,
        "high" => TaskPriority::High,
        "urgent" => TaskPriority::Urgent,
        _ => TaskPriority::Normal,
    }
}

pub(super) fn task_priority_to_str(value: TaskPriority) -> &'static str {
    match value {
        TaskPriority::Low => "low",
        TaskPriority::Normal => "normal",
        TaskPriority::High => "high",
        TaskPriority::Urgent => "urgent",
    }
}

pub(super) fn summarize_goal_title(goal: &str) -> String {
    let trimmed = goal.trim();
    if trimmed.is_empty() {
        return "Untitled Goal".into();
    }
    summarize_text(trimmed, 72)
}

pub(super) fn normalize_goal_key(goal: &str) -> String {
    goal.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

pub(super) fn normalized_tool_signature(tool_call: &ToolCall) -> String {
    let normalized_args = serde_json::from_str::<serde_json::Value>(&tool_call.function.arguments)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| tool_call.function.arguments.trim().to_string());
    format!("{}:{}", tool_call.function.name, normalized_args)
}

pub(in crate::agent) fn summarize_text(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    let truncated = normalized
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    format!("{truncated}…")
}

pub(super) fn resolve_goal_run_control_step(goal_run: &GoalRun, step_index: Option<usize>) -> usize {
    if goal_run.steps.is_empty() {
        return 0;
    }
    step_index
        .unwrap_or(
            goal_run
                .current_step_index
                .min(goal_run.steps.len().saturating_sub(1)),
        )
        .min(goal_run.steps.len().saturating_sub(1))
}

pub(super) fn reset_goal_run_step(step: &mut GoalRunStep) {
    step.status = GoalRunStepStatus::Pending;
    step.task_id = None;
    step.summary = None;
    step.error = None;
    step.started_at = None;
    step.completed_at = None;
}

pub(super) fn retry_goal_run_step(goal_run: &mut GoalRun, step_index: Option<usize>) -> Result<()> {
    if goal_run.steps.is_empty() {
        anyhow::bail!("goal run has no steps to retry");
    }

    let target_index = resolve_goal_run_control_step(goal_run, step_index);
    let Some(step) = goal_run.steps.get_mut(target_index) else {
        anyhow::bail!("goal run step index out of range");
    };

    reset_goal_run_step(step);
    goal_run.current_step_index = target_index;
    goal_run.completed_at = None;
    goal_run.status = GoalRunStatus::Running;
    goal_run.last_error = None;
    goal_run.failure_cause = None;
    goal_run.current_step_title = goal_run
        .steps
        .get(target_index)
        .map(|step| step.title.clone());
    goal_run.current_step_kind = goal_run.steps.get(target_index).map(|step| step.kind);
    goal_run.awaiting_approval_id = None;
    goal_run.active_task_id = None;
    Ok(())
}

pub(super) fn rerun_goal_run_from_step(goal_run: &mut GoalRun, step_index: Option<usize>) -> Result<()> {
    if goal_run.steps.is_empty() {
        anyhow::bail!("goal run has no steps to rerun");
    }

    let target_index = resolve_goal_run_control_step(goal_run, step_index);
    for step in goal_run.steps.iter_mut().skip(target_index) {
        reset_goal_run_step(step);
    }
    goal_run.current_step_index = target_index;
    goal_run.completed_at = None;
    goal_run.status = GoalRunStatus::Running;
    goal_run.last_error = None;
    goal_run.failure_cause = None;
    goal_run.current_step_title = goal_run
        .steps
        .get(target_index)
        .map(|step| step.title.clone());
    goal_run.current_step_kind = goal_run.steps.get(target_index).map(|step| step.kind);
    goal_run.awaiting_approval_id = None;
    goal_run.active_task_id = None;
    goal_run.reflection_summary = None;
    goal_run.generated_skill_path = None;
    Ok(())
}

pub(super) fn latest_goal_run_failure(goal_run: &GoalRun, tasks: &[AgentTask]) -> Option<String> {
    goal_run
        .last_error
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            goal_run
                .steps
                .iter()
                .rev()
                .find_map(|step| step.error.clone().filter(|value| !value.trim().is_empty()))
        })
        .or_else(|| {
            tasks.iter().rev().find_map(|task| {
                task.last_error
                    .clone()
                    .or_else(|| task.error.clone())
                    .filter(|value| !value.trim().is_empty())
            })
        })
}

pub(super) fn approval_count_for_tasks(tasks: &[AgentTask]) -> u32 {
    tasks
        .iter()
        .flat_map(|task| task.logs.iter())
        .filter(|log| {
            log.phase == "approval"
                && log
                    .message
                    .to_ascii_lowercase()
                    .contains("managed command paused for operator approval")
        })
        .count() as u32
}

pub(super) fn project_goal_run_snapshot(
    mut goal_run: GoalRun,
    related_tasks: &[AgentTask],
    now: u64,
) -> GoalRun {
    goal_run.current_step_title = goal_run
        .steps
        .get(goal_run.current_step_index)
        .map(|step| step.title.clone());
    goal_run.current_step_kind = goal_run
        .steps
        .get(goal_run.current_step_index)
        .map(|step| step.kind);
    goal_run.active_task_id = goal_run
        .steps
        .get(goal_run.current_step_index)
        .and_then(|step| step.task_id.clone());
    goal_run.awaiting_approval_id = related_tasks
        .iter()
        .find_map(|task| task.awaiting_approval_id.clone());
    goal_run.child_task_count = if goal_run.child_task_ids.is_empty() {
        related_tasks.len() as u32
    } else {
        goal_run.child_task_ids.iter().collect::<HashSet<_>>().len() as u32
    };
    goal_run.approval_count = approval_count_for_tasks(related_tasks);
    goal_run.failure_cause = latest_goal_run_failure(&goal_run, related_tasks);
    goal_run.duration_ms = goal_run.started_at.map(|started_at| {
        goal_run
            .completed_at
            .unwrap_or(now)
            .saturating_sub(started_at)
    });
    goal_run
}

pub(super) fn make_goal_run_event(phase: &str, message: &str, details: Option<String>) -> GoalRunEvent {
    make_goal_run_event_with_todos(phase, message, details, None, Vec::new())
}

pub(super) fn make_goal_run_event_with_todos(
    phase: &str,
    message: &str,
    details: Option<String>,
    step_index: Option<usize>,
    todo_snapshot: Vec<TodoItem>,
) -> GoalRunEvent {
    GoalRunEvent {
        id: format!("goal_event_{}", Uuid::new_v4()),
        timestamp: now_millis(),
        phase: phase.to_string(),
        message: message.to_string(),
        details,
        step_index,
        todo_snapshot,
    }
}

pub(super) fn goal_run_status_message(goal_run: &GoalRun) -> &'static str {
    match goal_run.status {
        GoalRunStatus::Queued => "Goal queued",
        GoalRunStatus::Planning => "Goal planning",
        GoalRunStatus::Running => "Goal running",
        GoalRunStatus::AwaitingApproval => "Goal awaiting approval",
        GoalRunStatus::Paused => "Goal paused",
        GoalRunStatus::Completed => "Goal completed",
        GoalRunStatus::Failed => "Goal failed",
        GoalRunStatus::Cancelled => "Goal cancelled",
    }
}

pub(super) fn goal_run_step_status_label(value: GoalRunStepStatus) -> &'static str {
    match value {
        GoalRunStepStatus::Pending => "pending",
        GoalRunStepStatus::InProgress => "in_progress",
        GoalRunStepStatus::Completed => "completed",
        GoalRunStepStatus::Failed => "failed",
        GoalRunStepStatus::Skipped => "skipped",
    }
}

pub(super) fn planner_required_for_message(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lower = trimmed.to_lowercase();
    let word_count = lower.split_whitespace().count();
    if word_count >= 24 || trimmed.len() >= 160 {
        return true;
    }

    if trimmed.lines().count() >= 3 {
        return true;
    }

    if trimmed.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("- ")
            || line.starts_with("* ")
            || (line.len() >= 2
                && line.as_bytes()[0].is_ascii_digit()
                && line.as_bytes()[1] == b'.')
    }) {
        return true;
    }

    [
        " then ",
        " also ",
        " after ",
        " before ",
        " next ",
        " first ",
        " second ",
        " third ",
        " plan ",
        " steps ",
        " todo ",
        " workflow ",
        " investigate ",
        " implement ",
        " migrate ",
        " refactor ",
        " compare ",
        " audit ",
        " analyze ",
        " long-running ",
        " autonomous ",
        " goal ",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// Attempt to repair malformed JSON from LLM output using the jsonrepair crate.
pub(super) fn repair_json(raw: &str) -> String {
    jsonrepair::repair_json(raw, &jsonrepair::Options::default())
        .unwrap_or_else(|_| raw.to_string())
}

/// JSON schema for structured output — forces the API to produce valid GoalPlanResponse.
pub(super) fn goal_plan_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "summary": { "type": "string" },
            "steps": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "instructions": { "type": "string" },
                        "kind": { "type": "string", "enum": ["reason", "command", "research", "memory", "skill"] },
                        "success_criteria": { "type": "string" },
                        "session_id": { "type": ["string", "null"] }
                    },
                    "required": ["title", "instructions", "kind", "success_criteria", "session_id"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["title", "summary", "steps"],
        "additionalProperties": false
    })
}

/// Parse a numbered markdown list into a GoalPlanResponse-compatible JSON value.
pub(super) fn parse_markdown_steps<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T> {
    let mut steps = Vec::new();

    for line in raw.lines() {
        let line = line.trim();
        let content = if let Some(rest) = line.strip_prefix(|c: char| c.is_ascii_digit()) {
            rest.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.')
                .trim()
        } else if let Some(rest) = line.strip_prefix("- ") {
            rest.trim()
        } else {
            continue;
        };

        if content.is_empty() {
            continue;
        }

        let (kind, rest) = if content.starts_with('[') {
            if let Some(close) = content.find(']') {
                let k = &content[1..close];
                let remainder = content[close + 1..].trim();
                (k.to_string(), remainder.to_string())
            } else {
                ("command".to_string(), content.to_string())
            }
        } else {
            ("command".to_string(), content.to_string())
        };

        let (main_part, criteria) = if let Some(pos) = rest.to_lowercase().find("success:") {
            (
                rest[..pos].trim().to_string(),
                rest[pos + 8..].trim().to_string(),
            )
        } else if let Some(pos) = rest.to_lowercase().find("criteria:") {
            (
                rest[..pos].trim().to_string(),
                rest[pos + 9..].trim().to_string(),
            )
        } else {
            (rest.clone(), "Step completed successfully".to_string())
        };

        let (title, instructions) = if let Some(colon) = main_part.find(':') {
            (
                main_part[..colon].trim().to_string(),
                main_part[colon + 1..].trim().to_string(),
            )
        } else {
            (main_part.clone(), main_part)
        };

        steps.push(serde_json::json!({
            "title": title,
            "instructions": instructions,
            "kind": kind,
            "success_criteria": criteria.trim_end_matches('.'),
            "session_id": null,
        }));
    }

    if steps.is_empty() {
        anyhow::bail!("no steps parsed from markdown");
    }

    let plan = serde_json::json!({
        "title": steps.first().and_then(|s| s["title"].as_str()).unwrap_or("Goal plan"),
        "summary": format!("Plan with {} steps parsed from markdown", steps.len()),
        "steps": steps,
    });

    serde_json::from_value::<T>(plan)
        .map_err(|e| anyhow::anyhow!("markdown plan conversion failed: {e}"))
}

pub(super) fn parse_yaml_block<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T> {
    let trimmed = raw.trim();

    if let Ok(parsed) = serde_yaml::from_str::<T>(trimmed) {
        return Ok(parsed);
    }

    let without_fence = trimmed
        .strip_prefix("```yaml")
        .or_else(|| trimmed.strip_prefix("```yml"))
        .or_else(|| trimmed.strip_prefix("```"))
        .map(str::trim)
        .and_then(|v| v.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);

    if let Ok(parsed) = serde_yaml::from_str::<T>(without_fence) {
        return Ok(parsed);
    }

    anyhow::bail!("failed to parse YAML from model output")
}

/// Build a correction prompt when the model fails to return valid JSON.
pub(super) fn build_json_retry_prompt(original_prompt: &str, broken_output: &str) -> String {
    format!(
        "Your previous response was not valid JSON and could not be parsed.\n\
         Here is what you returned:\n\
         ---\n{}\n---\n\n\
         Please try again. Return ONLY the raw JSON object, no markdown fences, no explanation.\n\n\
         Original request:\n{}",
        broken_output.chars().take(2000).collect::<String>(),
        original_prompt
    )
}

pub(super) fn parse_json_block<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T> {
    let trimmed = raw.trim();
    if let Ok(parsed) = serde_json::from_str::<T>(trimmed) {
        return Ok(parsed);
    }

    let without_fence = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(str::trim)
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);

    if let Ok(parsed) = serde_json::from_str::<T>(without_fence) {
        return Ok(parsed);
    }

    let object_candidate = without_fence
        .find('{')
        .zip(without_fence.rfind('}'))
        .and_then(|(start, end)| (start < end).then_some(&without_fence[start..=end]));
    if let Some(candidate) = object_candidate {
        if let Ok(parsed) = serde_json::from_str::<T>(candidate) {
            return Ok(parsed);
        }
    }

    // Try unwrapping {"answer":"..."} wrapper pattern
    if let Some(candidate) = object_candidate {
        if let Ok(wrapper) = serde_json::from_str::<serde_json::Value>(candidate) {
            if let Some(inner) = wrapper.get("answer").and_then(|v| v.as_str()) {
                if let Ok(parsed) = serde_json::from_str::<T>(inner) {
                    tracing::info!("parsed JSON after unwrapping answer wrapper");
                    return Ok(parsed);
                }
                let inner_repaired = repair_json(inner);
                if let Ok(parsed) = serde_json::from_str::<T>(&inner_repaired) {
                    tracing::info!("parsed JSON after unwrapping + repairing answer wrapper");
                    return Ok(parsed);
                }
            }
        }
    }

    // Try repairing the JSON using jsonrepair
    let repaired = repair_json(without_fence);
    if let Ok(parsed) = serde_json::from_str::<T>(&repaired) {
        tracing::info!("parsed JSON after jsonrepair");
        return Ok(parsed);
    }

    tracing::warn!(raw_len = raw.len(), raw_output = %raw, "failed to parse structured JSON from model output");
    anyhow::bail!("failed to parse structured JSON from model output")
}
