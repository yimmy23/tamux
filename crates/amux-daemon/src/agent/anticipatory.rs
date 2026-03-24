//! Anticipatory runtime surfaces and background pre-warming.

use std::collections::HashMap;

use super::*;

pub(super) const ANTICIPATORY_TICK_SECS: u64 = 30;
const SESSION_RECONNECT_GRACE_MS: u64 = 5 * 60 * 1000;
const PREDICTIVE_HYDRATION_COOLDOWN_MS: u64 = 10 * 60 * 1000;
const RECENT_HEALTH_WINDOW_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Default)]
pub(super) struct AnticipatoryRuntime {
    pub items: Vec<AnticipatoryItem>,
    pub last_presence_at: Option<u64>,
    pub session_start_pending_at: Option<u64>,
    pub last_surface_at: Option<u64>,
    pub active_attention_surface: Option<String>,
    pub active_attention_thread_id: Option<String>,
    pub active_attention_goal_run_id: Option<String>,
    pub active_attention_updated_at: Option<u64>,
    pub hydration_by_thread: HashMap<String, u64>,
}

impl AgentEngine {
    /// Get anticipatory items formatted for heartbeat merge.
    /// Returns (items, is_first_heartbeat) tuple. Per D-07/D-08.
    pub(super) async fn get_anticipatory_for_heartbeat(&self) -> (Vec<AnticipatoryItem>, bool) {
        let runtime = self.anticipatory.read().await;
        let items = runtime.items.clone();
        let is_first = runtime.session_start_pending_at.is_some();
        (items, is_first)
    }

    pub async fn mark_operator_present(&self, _reason: &str) {
        let now = now_millis();
        let mut runtime = self.anticipatory.write().await;
        let start_pending = runtime
            .last_presence_at
            .map(|previous| now.saturating_sub(previous) >= SESSION_RECONNECT_GRACE_MS)
            .unwrap_or(true);
        runtime.last_presence_at = Some(now);
        if start_pending {
            runtime.session_start_pending_at = Some(now);
        }
    }

    pub async fn emit_anticipatory_snapshot(&self) {
        let items = self.anticipatory.read().await.items.clone();
        self.emit_anticipatory_update(items);
    }

    pub(super) fn emit_anticipatory_update(&self, items: Vec<AnticipatoryItem>) {
        let _ = self.event_tx.send(AgentEvent::AnticipatoryUpdate { items });
    }

    pub async fn record_operator_attention(
        &self,
        surface: &str,
        thread_id: Option<&str>,
        goal_run_id: Option<&str>,
    ) -> Result<()> {
        self.record_attention_surface(surface).await?;

        let now = now_millis();
        let mut runtime = self.anticipatory.write().await;
        runtime.active_attention_surface = Some(surface.trim().to_ascii_lowercase());
        runtime.active_attention_thread_id = thread_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        runtime.active_attention_goal_run_id = goal_run_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        runtime.active_attention_updated_at = Some(now);
        Ok(())
    }

    pub(crate) async fn run_anticipatory_tick(&self) {
        let config = self.config.read().await.clone();
        let settings = &config.anticipatory;
        if !settings.enabled {
            let should_clear = !self.anticipatory.read().await.items.is_empty();
            if should_clear {
                self.anticipatory.write().await.items.clear();
                self.emit_anticipatory_update(Vec::new());
            }
            return;
        }

        self.run_predictive_hydration(settings).await;
        let next_items = self.compute_anticipatory_items(settings).await;
        self.refresh_anticipatory_items(next_items, settings).await;
    }

    async fn run_predictive_hydration(&self, settings: &AnticipatoryConfig) {
        if !settings.predictive_hydration {
            return;
        }

        let targets = {
            let goal_runs = self.goal_runs.lock().await;
            goal_runs
                .iter()
                .filter(|goal_run| {
                    matches!(
                        goal_run.status,
                        GoalRunStatus::Planning
                            | GoalRunStatus::Running
                            | GoalRunStatus::AwaitingApproval
                            | GoalRunStatus::Paused
                    )
                })
                .filter_map(|goal_run| {
                    goal_run
                        .thread_id
                        .clone()
                        .map(|thread_id| (thread_id, goal_run.updated_at))
                })
                .collect::<Vec<_>>()
        };

        let now = now_millis();
        let attention_target = self.current_attention_target().await;
        let due_threads = {
            let runtime = self.anticipatory.read().await;
            let mut ordered = Vec::new();
            if let Some(thread_id) = attention_target.as_ref() {
                ordered.push((thread_id.clone(), u64::MAX));
            }
            ordered.extend(targets);
            let mut seen = std::collections::HashSet::new();
            ordered
                .into_iter()
                .filter(|(thread_id, _)| seen.insert(thread_id.clone()))
                .filter(|(thread_id, _)| {
                    runtime
                        .hydration_by_thread
                        .get(thread_id)
                        .map(|last| now.saturating_sub(*last) >= PREDICTIVE_HYDRATION_COOLDOWN_MS)
                        .unwrap_or(true)
                })
                .collect::<Vec<_>>()
        };

        for (thread_id, _) in due_threads {
            self.refresh_thread_repo_context(&thread_id).await;
            self.anticipatory
                .write()
                .await
                .hydration_by_thread
                .insert(thread_id, now);
        }
    }

    async fn compute_anticipatory_items(
        &self,
        settings: &AnticipatoryConfig,
    ) -> Vec<AnticipatoryItem> {
        let mut items = Vec::new();
        let attention_surface = self.current_attention_surface().await;

        if settings.morning_brief {
            if should_surface_anticipatory_kind("morning_brief", attention_surface.as_deref()) {
                if let Some(item) = self.compute_morning_brief(settings).await {
                    items.push(item);
                }
            }
        }

        if settings.stuck_detection {
            if should_surface_anticipatory_kind("stuck_hint", attention_surface.as_deref()) {
                if let Some(item) = self.compute_stuck_hint(settings).await {
                    items.push(item);
                }
            }
        }
        if let Some(item) = self.compute_collaboration_hint(settings).await {
            items.push(item);
        }

        items.sort_by(|left, right| {
            right
                .confidence
                .partial_cmp(&left.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        items
    }

    async fn current_attention_surface(&self) -> Option<String> {
        let runtime = self.anticipatory.read().await;
        let runtime_surface = runtime.active_attention_surface.clone();
        drop(runtime);
        if runtime_surface.is_some() {
            return runtime_surface;
        }
        let model = self.operator_model.read().await;
        model.attention_topology.last_surface.clone()
    }

    async fn current_attention_target(&self) -> Option<String> {
        let runtime = self.anticipatory.read().await;
        if let Some(thread_id) = runtime.active_attention_thread_id.clone() {
            return Some(thread_id);
        }
        let goal_run_id = runtime.active_attention_goal_run_id.clone();
        drop(runtime);

        let goal_run_id = goal_run_id?;
        let goal_runs = self.goal_runs.lock().await;
        goal_runs
            .iter()
            .find(|goal_run| goal_run.id == goal_run_id)
            .and_then(|goal_run| goal_run.thread_id.clone())
    }

    async fn current_attention_focus(&self) -> AttentionFocus {
        let runtime = self.anticipatory.read().await;
        AttentionFocus {
            thread_id: runtime.active_attention_thread_id.clone(),
            goal_run_id: runtime.active_attention_goal_run_id.clone(),
        }
    }

    async fn refresh_anticipatory_items(
        &self,
        next_items: Vec<AnticipatoryItem>,
        settings: &AnticipatoryConfig,
    ) {
        let now = now_millis();
        let mut runtime = self.anticipatory.write().await;
        if runtime.items == next_items {
            return;
        }

        let surface_cooldown_ms = settings.surface_cooldown_seconds.saturating_mul(1000);
        let cooling_down = runtime
            .last_surface_at
            .map(|last| now.saturating_sub(last) < surface_cooldown_ms)
            .unwrap_or(false);

        if cooling_down && !runtime.items.is_empty() && !next_items.is_empty() {
            return;
        }

        runtime.items = next_items.clone();
        if next_items.is_empty() {
            runtime.last_surface_at = None;
        } else {
            runtime.last_surface_at = Some(now);
        }
        drop(runtime);

        self.emit_anticipatory_update(next_items);
    }

    async fn compute_morning_brief(
        &self,
        settings: &AnticipatoryConfig,
    ) -> Option<AnticipatoryItem> {
        let pending_at = self.anticipatory.read().await.session_start_pending_at?;
        let now = now_millis();
        if now.saturating_sub(pending_at)
            > (settings.morning_brief_window_minutes as u64).saturating_mul(60_000)
        {
            self.anticipatory.write().await.session_start_pending_at = None;
            return None;
        }

        let confidence = self.morning_brief_confidence(now).await;
        if confidence < settings.surfacing_min_confidence.max(0.8) {
            return None;
        }

        let attention = self.current_attention_focus().await;
        let unfinished_goals = {
            let goal_runs = self.goal_runs.lock().await;
            let mut runs = goal_runs
                .iter()
                .filter(|goal_run| {
                    matches!(
                        goal_run.status,
                        GoalRunStatus::Queued
                            | GoalRunStatus::Planning
                            | GoalRunStatus::Running
                            | GoalRunStatus::AwaitingApproval
                            | GoalRunStatus::Paused
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            runs.sort_by(|left, right| {
                let left_priority = goal_attention_priority(left, &attention);
                let right_priority = goal_attention_priority(right, &attention);
                right_priority
                    .cmp(&left_priority)
                    .then_with(|| right.updated_at.cmp(&left.updated_at))
            });
            runs.truncate(2);
            runs
        };
        let pending_approvals = self.pending_operator_approvals.read().await.len();
        let recent_health = self
            .history
            .list_health_log(6)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|entry| entry.3 != "healthy")
            .filter(|entry| now.saturating_sub(entry.6) <= RECENT_HEALTH_WINDOW_MS)
            .take(2)
            .collect::<Vec<_>>();

        let mut bullets = Vec::new();
        for goal_run in unfinished_goals {
            let status = format!("{:?}", goal_run.status).to_lowercase();
            let step = goal_run
                .current_step_title
                .clone()
                .unwrap_or_else(|| "next step pending".to_string());
            let mut bullet = format!(
                "Goal \"{}\" is {status}; next focus: {step}",
                goal_run.title
            );
            if goal_attention_priority(&goal_run, &attention) > 0 {
                bullet.push_str(" (currently in your active view)");
            }
            bullets.push(bullet);
        }
        if pending_approvals > 0 {
            bullets.push(format!(
                "{pending_approvals} approval request(s) are still waiting for a decision"
            ));
        }
        for (_, entity_type, entity_id, health_state, _, intervention, _) in recent_health {
            let label = intervention.unwrap_or_else(|| "attention recommended".to_string());
            bullets.push(format!(
                "{entity_type} {entity_id} is {health_state}; {label}"
            ));
        }
        if bullets.is_empty() {
            self.anticipatory.write().await.session_start_pending_at = None;
            return None;
        }

        self.anticipatory.write().await.session_start_pending_at = None;
        Some(AnticipatoryItem {
            id: "morning_brief".to_string(),
            kind: "morning_brief".to_string(),
            title: "Morning Brief".to_string(),
            summary: format!("{} item(s) worth picking up.", bullets.len()),
            bullets,
            confidence,
            goal_run_id: None,
            thread_id: None,
            created_at: now,
            updated_at: now,
        })
    }

    async fn compute_stuck_hint(&self, settings: &AnticipatoryConfig) -> Option<AnticipatoryItem> {
        let now = now_millis();
        let attention = self.current_attention_focus().await;
        let tasks = self.tasks.lock().await;
        let candidate = tasks
            .iter()
            .filter(|task| {
                matches!(
                    task.status,
                    TaskStatus::InProgress
                        | TaskStatus::AwaitingApproval
                        | TaskStatus::Blocked
                        | TaskStatus::FailedAnalyzing
                )
            })
            .max_by(|left, right| {
                let left_priority = task_attention_priority(left, &attention);
                let right_priority = task_attention_priority(right, &attention);
                left_priority
                    .cmp(&right_priority)
                    .then_with(|| left.retry_count.cmp(&right.retry_count))
                    .then_with(|| {
                        left.started_at
                            .unwrap_or(left.created_at)
                            .cmp(&right.started_at.unwrap_or(right.created_at))
                    })
            });

        let mut bullets = Vec::new();
        let mut confidence: f64 = 0.0;
        if let Some(task) = candidate {
            let focused_task = task_attention_priority(task, &attention) > 0;
            if task.status == TaskStatus::Blocked {
                confidence = 0.92;
                if let Some(reason) = task.blocked_reason.as_deref() {
                    bullets.push(format!("Blocked reason: {reason}"));
                }
            } else if task.status == TaskStatus::AwaitingApproval {
                let wait_started_at = task.started_at.unwrap_or(task.created_at);
                let waited_ms = now.saturating_sub(wait_started_at);
                if waited_ms >= settings.stuck_detection_delay_seconds.saturating_mul(1000) {
                    confidence = 0.78;
                    bullets.push("Execution is paused behind operator approval.".to_string());
                }
            }
            if task.retry_count >= 2 {
                confidence = confidence.max(0.84);
                bullets.push(format!(
                    "Task retried {} time(s) without clean completion.",
                    task.retry_count
                ));
            }
            if let Some(error) = task
                .last_error
                .as_deref()
                .or(task.error.as_deref())
                .filter(|value| !value.trim().is_empty())
            {
                confidence = confidence.max(0.8);
                bullets.push(format!("Recent error: {}", truncate_hint(error)));
            }
            if focused_task {
                confidence = (confidence + 0.05).min(0.97);
                bullets
                    .push("This task is in the thread or goal you are currently viewing.".into());
            }

            if confidence >= settings.surfacing_min_confidence && !bullets.is_empty() {
                return Some(AnticipatoryItem {
                    id: format!("stuck_hint_{}", task.id),
                    kind: "stuck_hint".to_string(),
                    title: format!("Task May Be Stuck: {}", task.title),
                    summary: "The daemon sees a live task pattern that usually needs intervention."
                        .to_string(),
                    bullets,
                    confidence,
                    goal_run_id: task.goal_run_id.clone(),
                    thread_id: task.thread_id.clone(),
                    created_at: now,
                    updated_at: now,
                });
            }
        }

        None
    }

    async fn compute_collaboration_hint(
        &self,
        settings: &AnticipatoryConfig,
    ) -> Option<AnticipatoryItem> {
        if !self.config.read().await.collaboration.enabled {
            return None;
        }
        let collaboration = self.collaboration.read().await;
        let candidate = collaboration
            .values()
            .flat_map(|session| {
                session
                    .disagreements
                    .iter()
                    .filter(|item| item.resolution == "pending")
                    .map(move |disagreement| (session, disagreement))
            })
            .max_by(|left, right| {
                right
                    .1
                    .confidence_gap
                    .partial_cmp(&left.1.confidence_gap)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })?;
        let (session, disagreement) = candidate;
        let confidence = (0.78 - disagreement.confidence_gap.min(0.3)).max(0.65);
        if confidence < settings.surfacing_min_confidence {
            return None;
        }
        Some(AnticipatoryItem {
            id: format!("disagreement_{}", disagreement.id),
            kind: "collaboration_disagreement".to_string(),
            title: format!("Subagent Disagreement: {}", disagreement.topic),
            summary: format!(
                "{} subagent(s) disagree and may need arbitration.",
                disagreement.agents.len()
            ),
            bullets: vec![
                format!(
                    "Mission: {}",
                    crate::agent::summarize_text(&session.mission, 100)
                ),
                format!("Positions: {}", disagreement.positions.join(" vs ")),
                if disagreement.confidence_gap < 0.15 {
                    "Weighted confidence is close; operator escalation is recommended.".to_string()
                } else {
                    "Weighted vote is likely recoverable without escalation.".to_string()
                },
            ],
            confidence,
            goal_run_id: session.goal_run_id.clone(),
            thread_id: session.thread_id.clone(),
            created_at: now_millis(),
            updated_at: now_millis(),
        })
    }

    async fn morning_brief_confidence(&self, now: u64) -> f64 {
        let model = self.operator_model.read().await;
        let Some(typical_start_hour_utc) = model.session_rhythm.typical_start_hour_utc else {
            return 0.82;
        };
        let current_hour = current_utc_hour(now);
        let delta = circular_hour_distance(current_hour, typical_start_hour_utc);
        match delta {
            0 => 0.95,
            1 => 0.88,
            2 => 0.8,
            _ => 0.72,
        }
    }
}

fn truncate_hint(value: &str) -> String {
    const MAX_CHARS: usize = 120;
    let trimmed = value.trim();
    if trimmed.chars().count() <= MAX_CHARS {
        return trimmed.to_string();
    }
    let truncated = trimmed.chars().take(MAX_CHARS - 1).collect::<String>();
    format!("{truncated}…")
}

fn current_utc_hour(timestamp_ms: u64) -> u8 {
    ((timestamp_ms / 3_600_000) % 24) as u8
}

fn should_surface_anticipatory_kind(kind: &str, attention_surface: Option<&str>) -> bool {
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

#[derive(Debug, Clone, Default)]
struct AttentionFocus {
    thread_id: Option<String>,
    goal_run_id: Option<String>,
}

fn goal_attention_priority(goal_run: &GoalRun, attention: &AttentionFocus) -> u8 {
    if attention.goal_run_id.as_deref() == Some(goal_run.id.as_str()) {
        2
    } else if attention.thread_id.as_deref() == goal_run.thread_id.as_deref() {
        1
    } else {
        0
    }
}

fn task_attention_priority(task: &AgentTask, attention: &AttentionFocus) -> u8 {
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

fn circular_hour_distance(left: u8, right: u8) -> u8 {
    let forward = left.abs_diff(right);
    forward.min(24 - forward)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_task(id: &str, thread_id: Option<&str>, goal_run_id: Option<&str>) -> AgentTask {
        AgentTask {
            id: id.to_string(),
            title: id.to_string(),
            description: String::new(),
            status: TaskStatus::Queued,
            priority: TaskPriority::Normal,
            progress: 0,
            created_at: 0,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: thread_id.map(str::to_string),
            source: "user".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: goal_run_id.map(str::to_string),
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 3,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: None,
            awaiting_approval_id: None,
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            override_provider: None,
            override_model: None,
            override_system_prompt: None,
            sub_agent_def_id: None,
        }
    }

    fn sample_goal_run(id: &str, thread_id: Option<&str>) -> GoalRun {
        GoalRun {
            id: id.to_string(),
            title: id.to_string(),
            goal: String::new(),
            client_request_id: None,
            status: GoalRunStatus::Queued,
            priority: TaskPriority::Normal,
            created_at: 0,
            updated_at: 0,
            started_at: None,
            completed_at: None,
            thread_id: thread_id.map(str::to_string),
            session_id: None,
            current_step_index: 0,
            current_step_title: None,
            current_step_kind: None,
            replan_count: 0,
            max_replans: 3,
            plan_summary: None,
            reflection_summary: None,
            memory_updates: Vec::new(),
            generated_skill_path: None,
            last_error: None,
            failure_cause: None,
            child_task_ids: Vec::new(),
            child_task_count: 0,
            approval_count: 0,
            awaiting_approval_id: None,
            active_task_id: None,
            duration_ms: None,
            steps: Vec::new(),
            events: Vec::new(),
        }
    }

    #[test]
    fn circular_hour_distance_wraps_across_midnight() {
        assert_eq!(circular_hour_distance(23, 1), 2);
        assert_eq!(circular_hour_distance(5, 5), 0);
    }

    #[test]
    fn truncate_hint_shortens_long_strings() {
        let value = "x".repeat(160);
        let shortened = truncate_hint(&value);
        assert!(shortened.len() < value.len());
        assert!(shortened.ends_with('…'));
    }

    #[test]
    fn attention_surface_suppresses_briefs_in_settings() {
        assert!(!should_surface_anticipatory_kind(
            "morning_brief",
            Some("modal:settings:provider")
        ));
        assert!(!should_surface_anticipatory_kind(
            "stuck_hint",
            Some("modal:approval")
        ));
    }

    #[test]
    fn attention_surface_allows_task_and_conversation_contexts() {
        assert!(should_surface_anticipatory_kind(
            "morning_brief",
            Some("conversation:chat")
        ));
        assert!(should_surface_anticipatory_kind(
            "stuck_hint",
            Some("task:detail")
        ));
    }

    #[test]
    fn task_attention_priority_prefers_goal_then_thread() {
        let attention = AttentionFocus {
            thread_id: Some("thread_1".to_string()),
            goal_run_id: Some("goal_1".to_string()),
        };
        let goal_match = sample_task("task_goal", Some("thread_9"), Some("goal_1"));
        let thread_match = sample_task("task_thread", Some("thread_1"), Some("goal_9"));

        assert_eq!(task_attention_priority(&goal_match, &attention), 2);
        assert_eq!(task_attention_priority(&thread_match, &attention), 1);
    }

    #[test]
    fn goal_attention_priority_prefers_exact_goal_match() {
        let attention = AttentionFocus {
            thread_id: Some("thread_1".to_string()),
            goal_run_id: Some("goal_1".to_string()),
        };
        let exact = sample_goal_run("goal_1", Some("thread_2"));
        let thread_only = sample_goal_run("goal_2", Some("thread_1"));

        assert_eq!(goal_attention_priority(&exact, &attention), 2);
        assert_eq!(goal_attention_priority(&thread_only, &attention), 1);
    }
}
