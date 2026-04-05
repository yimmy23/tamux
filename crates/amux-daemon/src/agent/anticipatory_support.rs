use super::*;

#[derive(Debug, Clone, Default)]
pub(super) struct AttentionFocus {
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct AnticipatoryRoutingHint {
    pub preferred_client_surface: Option<String>,
    pub preferred_attention_surface: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct StuckHintAssessment {
    pub confidence: f64,
    pub bullets: Vec<String>,
}

pub(super) fn truncate_hint(value: &str) -> String {
    const MAX_CHARS: usize = 120;
    let trimmed = value.trim();
    if trimmed.chars().count() <= MAX_CHARS {
        return trimmed.to_string();
    }
    let truncated = trimmed.chars().take(MAX_CHARS - 1).collect::<String>();
    format!("{truncated}…")
}

pub(super) fn anticipatory_current_utc_hour(timestamp_ms: u64) -> u8 {
    ((timestamp_ms / 3_600_000) % 24) as u8
}

pub(super) fn should_surface_anticipatory_kind(
    kind: &str,
    attention_surface: Option<&str>,
) -> bool {
    let Some(surface) = attention_surface else {
        return true;
    };
    let in_settings = surface.starts_with("modal:settings:");
    let in_provider_setup =
        surface == "conversation:onboarding" || surface == "modal:provider_picker";
    let in_help = surface == "modal:help" || surface == "modal:command_palette";
    let in_auth = surface == "modal:openai_auth";
    let in_approval = surface == "modal:approval";

    match kind {
        "morning_brief" => !(in_settings || in_provider_setup || in_help || in_auth),
        "stuck_hint" => !(in_settings || in_provider_setup || in_help || in_auth || in_approval),
        _ => true,
    }
}

pub(super) fn goal_attention_priority(goal_run: &GoalRun, attention: &AttentionFocus) -> u8 {
    if attention.goal_run_id.as_deref() == Some(goal_run.id.as_str()) {
        2
    } else if attention.thread_id.as_deref() == goal_run.thread_id.as_deref() {
        1
    } else {
        0
    }
}

pub(super) fn task_attention_priority(task: &AgentTask, attention: &AttentionFocus) -> u8 {
    if attention.goal_run_id.as_deref() == task.goal_run_id.as_deref() {
        2
    } else if attention.thread_id.as_deref() == task.thread_id.as_deref()
        || attention.thread_id.as_deref() == task.parent_thread_id.as_deref()
    {
        1
    } else {
        0
    }
}

pub(super) fn circular_hour_distance(left: u8, right: u8) -> u8 {
    let forward = left.abs_diff(right);
    forward.min(24 - forward)
}

pub(super) fn operator_idle_ms(
    last_presence_at: Option<u64>,
    active_attention_updated_at: Option<u64>,
    now: u64,
) -> Option<u64> {
    last_presence_at
        .into_iter()
        .chain(active_attention_updated_at)
        .max()
        .map(|last_signal| now.saturating_sub(last_signal))
}

pub(super) fn collect_session_start_prewarm_threads(
    attention_target: Option<String>,
    goal_runs: &[GoalRun],
    tasks: &std::collections::VecDeque<AgentTask>,
) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(thread_id) = attention_target {
        candidates.push((thread_id, 3u8, u64::MAX));
    }
    candidates.extend(goal_runs.iter().filter_map(|goal_run| {
        goal_run.thread_id.clone().map(|thread_id| {
            let priority = match goal_run.status {
                GoalRunStatus::Running | GoalRunStatus::AwaitingApproval => 2,
                GoalRunStatus::Planning | GoalRunStatus::Paused => 1,
                _ => 0,
            };
            (thread_id, priority, goal_run.updated_at)
        })
    }));
    candidates.extend(tasks.iter().filter_map(|task| {
        let priority = match task.status {
            TaskStatus::InProgress | TaskStatus::AwaitingApproval | TaskStatus::Blocked => 2,
            TaskStatus::FailedAnalyzing => 1,
            _ => 0,
        };
        if priority == 0 {
            return None;
        }
        task.thread_id
            .clone()
            .or_else(|| task.parent_thread_id.clone())
            .map(|thread_id| {
                (
                    thread_id,
                    priority,
                    task.started_at.unwrap_or(task.created_at),
                )
            })
    }));

    candidates.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| right.2.cmp(&left.2)));
    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter_map(|(thread_id, _, _)| seen.insert(thread_id.clone()).then_some(thread_id))
        .take(3)
        .collect()
}

pub(super) fn assess_stuck_task(
    task: &AgentTask,
    attention: &AttentionFocus,
    now: u64,
    operator_idle_ms: Option<u64>,
    settings: &AnticipatoryConfig,
) -> Option<StuckHintAssessment> {
    let focused_task = task_attention_priority(task, attention) > 0;
    let started_at = task.started_at.unwrap_or(task.created_at);
    let active_runtime_ms = now.saturating_sub(started_at);
    let delay_ms = settings.stuck_detection_delay_seconds.saturating_mul(1000);

    let mut bullets = Vec::new();
    let mut confidence: f64 = 0.0;
    if task.status == TaskStatus::Blocked {
        confidence = 0.92;
        if let Some(reason) = task.blocked_reason.as_deref() {
            bullets.push(format!("Blocked reason: {reason}"));
        }
    } else if task.status == TaskStatus::AwaitingApproval && active_runtime_ms >= delay_ms {
        confidence = 0.78;
        bullets.push("Execution is paused behind operator approval.".to_string());
    } else if matches!(
        task.status,
        TaskStatus::InProgress | TaskStatus::FailedAnalyzing
    ) && active_runtime_ms >= delay_ms
    {
        confidence = 0.74;
        bullets.push(format!(
            "Execution has stayed active for {}s without a clean completion signal.",
            active_runtime_ms / 1000
        ));
    }

    if task.retry_count >= 2 {
        confidence = confidence.max(0.84);
        bullets.push(format!(
            "Task retried {} time(s) without clean completion.",
            task.retry_count
        ));
    }

    if let Some(error) = latest_task_error(task) {
        confidence = confidence.max(0.8);
        bullets.push(format!("Recent error: {}", truncate_hint(error)));
    }

    if let Some(idle_ms) = operator_idle_ms.filter(|value| *value >= delay_ms) {
        confidence = confidence.max(0.82);
        bullets.push(format!(
            "Operator attention has been idle for {}s while this task stays active.",
            idle_ms / 1000
        ));
    }

    if focused_task {
        confidence = (confidence + 0.05).min(0.97);
        bullets.push("This task is in the thread or goal you are currently viewing.".into());
    }

    (confidence >= settings.surfacing_min_confidence && !bullets.is_empty()).then_some(
        StuckHintAssessment {
            confidence,
            bullets,
        },
    )
}

pub(super) fn client_surface_label(surface: amux_protocol::ClientSurface) -> &'static str {
    match surface {
        amux_protocol::ClientSurface::Tui => "tui",
        amux_protocol::ClientSurface::Electron => "electron",
    }
}

pub(super) fn preferred_attention_surface(
    kind: &str,
    active_surface: Option<String>,
    deep_focus_surface: Option<String>,
    dominant_surface: Option<String>,
) -> Option<String> {
    active_surface
        .filter(|surface| should_surface_anticipatory_kind(kind, Some(surface)))
        .or_else(|| {
            deep_focus_surface
                .filter(|surface| should_surface_anticipatory_kind(kind, Some(surface)))
        })
        .or_else(|| {
            dominant_surface.filter(|surface| should_surface_anticipatory_kind(kind, Some(surface)))
        })
}

fn latest_task_error(task: &AgentTask) -> Option<&str> {
    task.last_error
        .as_deref()
        .or(task.error.as_deref())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            task.logs
                .iter()
                .rev()
                .find(|entry| entry.level == TaskLogLevel::Error)
                .map(|entry| entry.message.as_str())
                .filter(|value| !value.trim().is_empty())
        })
}

impl AgentEngine {
    pub async fn emit_anticipatory_snapshot(&self) {
        let items = self.anticipatory.read().await.items.clone();
        self.emit_anticipatory_update(items);
    }

    pub(super) fn emit_anticipatory_update(&self, items: Vec<AnticipatoryItem>) {
        let _ = self.event_tx.send(AgentEvent::AnticipatoryUpdate { items });
    }

    pub(super) async fn morning_brief_confidence(&self, now: u64) -> f64 {
        let model = self.operator_model.read().await;
        let Some(typical_start_hour_utc) = model.session_rhythm.typical_start_hour_utc else {
            return 0.82;
        };
        let current_hour = anticipatory_current_utc_hour(now);
        let delta = circular_hour_distance(current_hour, typical_start_hour_utc);
        match delta {
            0 => 0.95,
            1 => 0.88,
            2 => 0.8,
            _ => 0.72,
        }
    }
}
