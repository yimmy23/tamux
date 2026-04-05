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
    pub session_start_prewarmed_at: Option<u64>,
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
            runtime.session_start_prewarmed_at = None;
        }
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

        self.run_session_start_prewarm(settings).await;
        self.run_predictive_hydration(settings).await;
        let next_items = self.compute_anticipatory_items(settings).await;
        self.refresh_anticipatory_items(next_items, settings).await;
    }

    async fn run_session_start_prewarm(&self, _settings: &AnticipatoryConfig) {
        let pending_at = {
            let runtime = self.anticipatory.read().await;
            match runtime.session_start_pending_at {
                Some(pending_at) if runtime.session_start_prewarmed_at != Some(pending_at) => {
                    Some(pending_at)
                }
                _ => None,
            }
        };
        if pending_at.is_none() {
            return;
        }

        let attention_target = self.current_attention_target().await;
        let goal_runs = {
            let goal_runs = self.goal_runs.lock().await;
            goal_runs.iter().cloned().collect::<Vec<_>>()
        };
        let tasks = {
            let tasks = self.tasks.lock().await;
            tasks.clone()
        };
        let threads = collect_session_start_prewarm_threads(attention_target, &goal_runs, &tasks);
        let now = now_millis();
        for thread_id in threads {
            self.refresh_thread_repo_context(&thread_id).await;
            self.anticipatory
                .write()
                .await
                .hydration_by_thread
                .insert(thread_id, now);
        }
        if let Some(pending_at) = pending_at {
            self.anticipatory.write().await.session_start_prewarmed_at = Some(pending_at);
        }
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

    async fn resolve_anticipatory_route(
        &self,
        kind: &str,
        goal_run_id: Option<&str>,
        thread_id: Option<&str>,
    ) -> AnticipatoryRoutingHint {
        let preferred_client_surface = if let Some(goal_run_id) = goal_run_id {
            self.get_goal_run_client_surface(goal_run_id)
                .await
                .map(client_surface_label)
                .map(ToOwned::to_owned)
        } else {
            None
        };
        let preferred_client_surface = match (preferred_client_surface, thread_id) {
            (Some(surface), _) => Some(surface),
            (None, Some(thread_id)) => self
                .get_thread_client_surface(thread_id)
                .await
                .map(client_surface_label)
                .map(ToOwned::to_owned),
            (None, None) => None,
        };

        let active_surface = self.current_attention_surface().await;
        let (deep_focus_surface, dominant_surface) = {
            let model = self.operator_model.read().await;
            (
                model.attention_topology.deep_focus_surface.clone(),
                model.attention_topology.dominant_surface.clone(),
            )
        };

        AnticipatoryRoutingHint {
            preferred_client_surface,
            preferred_attention_surface: preferred_attention_surface(
                kind,
                active_surface,
                deep_focus_surface,
                dominant_surface,
            ),
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
        let primary_goal = unfinished_goals.first().cloned();
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
        let routing = self
            .resolve_anticipatory_route(
                "morning_brief",
                primary_goal.as_ref().map(|goal| goal.id.as_str()),
                primary_goal
                    .as_ref()
                    .and_then(|goal| goal.thread_id.as_deref()),
            )
            .await;
        Some(AnticipatoryItem {
            id: "morning_brief".to_string(),
            kind: "morning_brief".to_string(),
            title: "Morning Brief".to_string(),
            summary: format!("{} item(s) worth picking up.", bullets.len()),
            bullets,
            confidence,
            goal_run_id: primary_goal.as_ref().map(|goal| goal.id.clone()),
            thread_id: primary_goal
                .as_ref()
                .and_then(|goal| goal.thread_id.clone()),
            preferred_client_surface: routing.preferred_client_surface,
            preferred_attention_surface: routing.preferred_attention_surface,
            created_at: now,
            updated_at: now,
        })
    }

    async fn compute_stuck_hint(&self, settings: &AnticipatoryConfig) -> Option<AnticipatoryItem> {
        let now = now_millis();
        let attention = self.current_attention_focus().await;
        let operator_idle_ms = {
            let runtime = self.anticipatory.read().await;
            operator_idle_ms(
                runtime.last_presence_at,
                runtime.active_attention_updated_at,
                now,
            )
        };
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
                ) && !crate::agent::concierge::is_user_hidden_task(task)
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
        if let Some(task) = candidate {
            let assessment = assess_stuck_task(task, &attention, now, operator_idle_ms, settings)?;
            let route = self
                .resolve_anticipatory_route(
                    "stuck_hint",
                    task.goal_run_id.as_deref(),
                    task.thread_id
                        .as_deref()
                        .or(task.parent_thread_id.as_deref()),
                )
                .await;
            return Some(AnticipatoryItem {
                id: format!("stuck_hint_{}", task.id),
                kind: "stuck_hint".to_string(),
                title: format!("Task May Be Stuck: {}", task.title),
                summary: "The daemon sees a live task pattern that usually needs intervention."
                    .to_string(),
                bullets: assessment.bullets,
                confidence: assessment.confidence,
                goal_run_id: task.goal_run_id.clone(),
                thread_id: task.thread_id.clone().or(task.parent_thread_id.clone()),
                preferred_client_surface: route.preferred_client_surface,
                preferred_attention_surface: route.preferred_attention_surface,
                created_at: now,
                updated_at: now,
            });
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
        let route = self
            .resolve_anticipatory_route(
                "collaboration_disagreement",
                session.goal_run_id.as_deref(),
                session.thread_id.as_deref(),
            )
            .await;
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
            preferred_client_surface: route.preferred_client_surface,
            preferred_attention_surface: route.preferred_attention_surface,
            created_at: now_millis(),
            updated_at: now_millis(),
        })
    }
}

#[cfg(test)]
#[path = "tests/anticipatory.rs"]
mod tests;
