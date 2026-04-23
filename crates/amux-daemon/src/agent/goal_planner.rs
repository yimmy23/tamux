//! Goal run planning — plan generation, step lifecycle, completion, and failure handling.

use super::*;

#[path = "goal_planner/finalization.rs"]
mod finalization_impl;
#[path = "goal_planner/progress.rs"]
mod progress_impl;

pub(super) const GOAL_FINAL_REVIEW_SOURCE: &str = "goal_final_review";
pub(in crate::agent) const GOAL_VERIFICATION_SOURCE: &str = "goal_verification";
const GOAL_REVIEWER_ROLE_ID: &str = "reviewer";
const GOAL_REVIEW_VERDICT_PASS: &str = "VERDICT: PASS";
const GOAL_REVIEW_VERDICT_FAIL: &str = "VERDICT: FAIL";
const GOAL_STEP_VERDICT_STATE_PREFIX: &str = "goal_step_verdict:";
const GOAL_STEP_VERDICT_REQUIRED_STATE_PREFIX: &str = "goal_step_verdict_required:";

pub(in crate::agent) fn goal_step_verdict_state_key(task_id: &str) -> String {
    format!("{GOAL_STEP_VERDICT_STATE_PREFIX}{task_id}")
}

pub(in crate::agent) fn goal_step_verdict_required_state_key(task_id: &str) -> String {
    format!("{GOAL_STEP_VERDICT_REQUIRED_STATE_PREFIX}{task_id}")
}

fn parse_goal_role_binding(raw: Option<&str>, fallback: GoalRoleBinding) -> GoalRoleBinding {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return fallback;
    };
    if let Some(value) = raw.strip_prefix("builtin:") {
        let value = value.trim();
        if !value.is_empty() {
            return GoalRoleBinding::Builtin(value.to_string());
        }
    }
    if let Some(value) = raw.strip_prefix("subagent:") {
        let value = value.trim();
        if !value.is_empty() {
            return GoalRoleBinding::Subagent(value.to_string());
        }
    }
    fallback
}

fn default_execution_binding(kind: &GoalRunStepKind) -> GoalRoleBinding {
    match kind {
        GoalRunStepKind::Specialist(role) if !role.trim().is_empty() => {
            GoalRoleBinding::Subagent(role.clone())
        }
        _ => GoalRoleBinding::Builtin(crate::agent::agent_identity::MAIN_AGENT_ID.to_string()),
    }
}

fn default_verification_binding() -> GoalRoleBinding {
    GoalRoleBinding::Builtin(GOAL_REVIEWER_ROLE_ID.to_string())
}

fn goal_runtime_owner_profile(
    agent_label: String,
    provider: String,
    model: String,
    reasoning_effort: Option<String>,
) -> GoalRuntimeOwnerProfile {
    GoalRuntimeOwnerProfile {
        agent_label,
        provider,
        model,
        reasoning_effort,
    }
}

fn goal_agent_assignment(
    role_id: String,
    enabled: bool,
    provider: String,
    model: String,
    reasoning_effort: Option<String>,
    inherit_from_main: bool,
) -> GoalAgentAssignment {
    GoalAgentAssignment {
        role_id,
        enabled,
        provider,
        model,
        reasoning_effort,
        inherit_from_main,
    }
}

fn sub_agent_matches_identifier(def: &SubAgentDefinition, identifier: &str) -> bool {
    def.id.eq_ignore_ascii_case(identifier)
        || def.name.eq_ignore_ascii_case(identifier)
        || def
            .id
            .strip_suffix("_builtin")
            .is_some_and(|value| value.eq_ignore_ascii_case(identifier))
}

fn normalize_goal_assignment_role(raw: &str) -> String {
    raw.trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_goal_reviewer_role(role_id: &str) -> bool {
    matches!(
        normalize_goal_assignment_role(role_id).as_str(),
        "reviewer" | "review" | "verifier" | "verify" | "qa"
    )
}

pub(super) enum GoalReviewVerdict {
    Pass,
    Fail,
}

pub(super) fn parse_goal_review_verdict(raw: &str) -> Option<GoalReviewVerdict> {
    let first_line = raw
        .lines()
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    if first_line_matches_goal_verdict(first_line, GOAL_REVIEW_VERDICT_PASS) {
        return Some(GoalReviewVerdict::Pass);
    }
    if first_line_matches_goal_verdict(first_line, GOAL_REVIEW_VERDICT_FAIL) {
        return Some(GoalReviewVerdict::Fail);
    }
    None
}

fn first_line_matches_goal_verdict(first_line: &str, verdict: &str) -> bool {
    let upper_line = first_line.to_ascii_uppercase();
    if upper_line == verdict {
        return true;
    }
    let Some(rest) = upper_line.strip_prefix(verdict) else {
        return false;
    };
    rest.trim_start()
        .chars()
        .next()
        .is_some_and(|separator| matches!(separator, '-' | ':' | '.' | ',' | ';' | '(' | '[' | '{'))
}

impl AgentEngine {
    pub(super) async fn normalized_goal_launch_assignment_snapshot(
        &self,
        assignments: Vec<GoalAgentAssignment>,
    ) -> Vec<GoalAgentAssignment> {
        if assignments
            .iter()
            .any(|assignment| assignment.enabled && is_goal_reviewer_role(&assignment.role_id))
        {
            return assignments;
        }

        let mut assignments = assignments;
        let inherited = assignments
            .iter()
            .find(|assignment| {
                assignment.enabled
                    && assignment.role_id == crate::agent::agent_identity::MAIN_AGENT_ID
            })
            .cloned();
        let reviewer = if let Some(main) = inherited {
            goal_agent_assignment(
                GOAL_REVIEWER_ROLE_ID.to_string(),
                true,
                main.provider,
                main.model,
                main.reasoning_effort,
                true,
            )
        } else {
            let config = self.config.read().await;
            let resolved =
                resolve_active_provider_config(&config).unwrap_or_else(|_| ProviderConfig {
                    base_url: config.base_url.clone(),
                    model: config.model.clone(),
                    api_key: config.api_key.clone(),
                    assistant_id: config.assistant_id.clone(),
                    auth_source: config.auth_source,
                    api_transport: config.api_transport,
                    reasoning_effort: config.reasoning_effort.clone(),
                    context_window_tokens: config.context_window_tokens,
                    response_schema: None,
                    stop_sequences: None,
                    temperature: None,
                    top_p: None,
                    top_k: None,
                    metadata: None,
                    service_tier: None,
                    container: None,
                    inference_geo: None,
                    cache_control: None,
                    max_tokens: None,
                    anthropic_tool_choice: None,
                    output_effort: None,
                });
            goal_agent_assignment(
                GOAL_REVIEWER_ROLE_ID.to_string(),
                true,
                config.provider.clone(),
                resolved.model,
                Some(resolved.reasoning_effort),
                true,
            )
        };
        assignments.push(reviewer);
        assignments
    }

    pub(crate) async fn goal_launch_assignment_snapshot(&self) -> Vec<GoalAgentAssignment> {
        let config = self.config.read().await;
        let resolved = resolve_active_provider_config(&config).unwrap_or_else(|_| ProviderConfig {
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            api_key: config.api_key.clone(),
            assistant_id: config.assistant_id.clone(),
            auth_source: config.auth_source,
            api_transport: config.api_transport,
            reasoning_effort: config.reasoning_effort.clone(),
            context_window_tokens: config.context_window_tokens,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
        });
        let assignments = vec![goal_agent_assignment(
            crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
            true,
            config.provider.clone(),
            resolved.model,
            Some(resolved.reasoning_effort),
            false,
        )];
        drop(config);
        self.normalized_goal_launch_assignment_snapshot(assignments)
            .await
    }

    async fn planner_owner_profile(&self) -> GoalRuntimeOwnerProfile {
        let config = self.config.read().await;
        goal_runtime_owner_profile(
            crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
            config.provider.clone(),
            config.model.clone(),
            Some(config.reasoning_effort.clone()),
        )
    }

    pub(crate) async fn current_step_owner_profile_for_task(
        &self,
        task: &AgentTask,
    ) -> GoalRuntimeOwnerProfile {
        if let Some(identifier) = task.sub_agent_def_id.as_deref() {
            let sub_agents = self.list_sub_agents().await;
            if let Some(def) = sub_agents
                .iter()
                .find(|definition| sub_agent_matches_identifier(definition, identifier))
            {
                return goal_runtime_owner_profile(
                    def.name.clone(),
                    def.provider.clone(),
                    def.model.clone(),
                    def.reasoning_effort.clone(),
                );
            }
        }

        let config = self.config.read().await;
        goal_runtime_owner_profile(
            crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
            task.override_provider
                .clone()
                .unwrap_or_else(|| config.provider.clone()),
            task.override_model
                .clone()
                .unwrap_or_else(|| config.model.clone()),
            Some(config.reasoning_effort.clone()),
        )
    }

    pub(super) async fn plan_goal_run(&self, goal_run_id: &str) -> Result<()> {
        let goal_run = self
            .get_goal_run(goal_run_id)
            .await
            .context("goal run missing during planning")?;
        let planner_owner_profile = self.planner_owner_profile().await;

        let queued = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(current) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                anyhow::bail!("goal run disappeared during planning");
            };
            current.status = GoalRunStatus::Planning;
            current.started_at.get_or_insert(now_millis());
            current.updated_at = now_millis();
            current.planner_owner_profile = Some(planner_owner_profile.clone());
            current.current_step_owner_profile = None;
            current.events.push(make_goal_run_event(
                "planning",
                "building execution plan",
                None,
            ));
            current.clone()
        };
        self.persist_goal_runs().await;
        self.emit_goal_run_update(&queued, Some("Planning goal".into()));

        let plan = self.request_goal_plan(&goal_run).await?;
        self.persist_goal_plan_causal_trace(&goal_run, &plan, None)
            .await;
        let now = now_millis();
        let updated = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(current) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                anyhow::bail!("goal run disappeared after planning");
            };
            let default_session_id = current.session_id.clone();
            if let Some(title) = plan
                .title
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                current.title = title.trim().to_string();
            }
            current.plan_summary = Some(plan.summary.clone());
            let planned_units = plan
                .steps
                .iter()
                .enumerate()
                .map(|(_position, step)| GoalDeliveryUnit {
                    id: format!("goal_step_{}", Uuid::new_v4()),
                    title: step.title.clone(),
                    status: GoalProjectionState::Pending,
                    execution_binding: parse_goal_role_binding(
                        step.execution_binding.as_deref(),
                        default_execution_binding(&step.kind),
                    ),
                    verification_binding: parse_goal_role_binding(
                        step.verification_binding.as_deref(),
                        default_verification_binding(),
                    ),
                    summary: Some(step.success_criteria.clone()),
                    proof_checks: step.proof_checks.clone(),
                    ..Default::default()
                })
                .collect::<Vec<_>>();
            current.steps = plan
                .steps
                .into_iter()
                .enumerate()
                .map(|(position, step)| GoalRunStep {
                    id: planned_units[position].id.clone(),
                    position,
                    title: step.title,
                    instructions: step.instructions,
                    kind: step.kind,
                    success_criteria: step.success_criteria,
                    session_id: step.session_id.or_else(|| default_session_id.clone()),
                    status: GoalRunStepStatus::Pending,
                    task_id: None,
                    summary: None,
                    error: None,
                    started_at: None,
                    completed_at: None,
                })
                .collect();
            current.current_step_index = 0;
            current.current_step_title = current.steps.first().map(|step| step.title.clone());
            current.current_step_kind = current.steps.first().map(|step| step.kind.clone());
            current.planner_owner_profile = Some(planner_owner_profile.clone());
            current.current_step_owner_profile = None;
            current.status = GoalRunStatus::Running;
            current.updated_at = now;
            current.last_error = None;
            current.failure_cause = None;
            current.awaiting_approval_id = None;
            current.active_task_id = None;
            let mut dossier = current.dossier.clone().unwrap_or_default();
            dossier.units = planned_units;
            dossier.summary = current.plan_summary.clone();
            dossier.projection_state = GoalProjectionState::InProgress;
            dossier.projection_error = None;
            current.dossier = Some(dossier);
            current.events.push(make_goal_run_event(
                "planning",
                "goal plan generated",
                current.plan_summary.clone(),
            ));
            current.clone()
        };
        self.persist_goal_runs().await;
        self.emit_goal_run_update(&updated, Some("Goal plan ready".into()));
        self.record_provenance_event(
            "plan_generated",
            "goal plan generated",
            serde_json::json!({
                "goal_run_id": updated.id,
                "step_count": updated.steps.len(),
                "summary": updated.plan_summary,
            }),
            Some(updated.id.as_str()),
            None,
            updated.thread_id.as_deref(),
            None,
            None,
        )
        .await;

        // Check plan confidence and route to approval if needed (UNCR-08)
        let low_confidence_steps = collect_low_confidence_plan_steps(&updated);
        let gate_action = self.plan_confidence_gate(&updated).await;
        if gate_action == super::uncertainty::PlanConfidenceAction::RequireApproval {
            self.gate_low_confidence_plan_for_approval(
                goal_run_id,
                &updated,
                &low_confidence_steps,
            )
            .await?;
        }

        Ok(())
    }

    async fn gate_low_confidence_plan_for_approval(
        &self,
        goal_run_id: &str,
        goal_run: &GoalRun,
        low_confidence_steps: &[String],
    ) -> Result<()> {
        let approval_id = format!("goal-plan-approval-{}", Uuid::new_v4());
        let review_command = "review low-confidence goal plan".to_string();
        let detail = if low_confidence_steps.is_empty() {
            "Plan includes LOW-confidence steps that require operator approval before execution."
                .to_string()
        } else {
            format!(
                "LOW-confidence steps require operator approval before execution: {}",
                low_confidence_steps.join("; ")
            )
        };
        let review_task = self
            .enqueue_task(
                format!("Review plan: {}", goal_run.title),
                detail.clone(),
                "normal",
                Some(review_command.clone()),
                None,
                Vec::new(),
                None,
                "goal_plan_approval",
                Some(goal_run_id.to_string()),
                None,
                None,
                Some("daemon".to_string()),
            )
            .await;

        let updated_task = {
            let mut tasks = self.tasks.lock().await;
            let Some(task) = tasks.iter_mut().find(|entry| entry.id == review_task.id) else {
                anyhow::bail!("goal plan approval task disappeared before gating");
            };
            task.notify_on_complete = false;
            task.notify_channels.clear();
            task.thread_id = goal_run.thread_id.clone();
            task.goal_run_title = Some(goal_run.title.clone());
            task.goal_step_title = goal_run.current_step_title.clone();
            task.status = TaskStatus::AwaitingApproval;
            task.progress = task.progress.max(35);
            task.awaiting_approval_id = Some(approval_id.clone());
            task.blocked_reason = Some(format!("waiting for operator approval: {review_command}"));
            task.logs.push(make_task_log_entry(
                task.retry_count,
                TaskLogLevel::Warn,
                "confidence_gate",
                "goal plan paused for operator approval due to LOW-confidence steps",
                Some(detail.clone()),
            ));
            task.clone()
        };

        let updated_goal = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(current) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                anyhow::bail!("goal run disappeared while applying confidence gate");
            };
            current.status = GoalRunStatus::AwaitingApproval;
            current.updated_at = now_millis();
            current.awaiting_approval_id = Some(approval_id.clone());
            current.active_task_id = Some(updated_task.id.clone());
            current.events.push(make_goal_run_event(
                "confidence_gate",
                "plan requires operator approval due to LOW-confidence steps",
                Some(detail),
            ));
            current.clone()
        };

        self.persist_tasks().await;
        self.persist_goal_runs().await;
        self.emit_task_update(&updated_task, Some("Task awaiting approval".into()));
        self.emit_goal_run_update(
            &updated_goal,
            Some("Plan awaiting approval: LOW-confidence steps detected".into()),
        );
        Ok(())
    }

    /// Check plan confidence and route to appropriate approval flow (UNCR-08).
    ///
    /// All HIGH -> proceed autonomously.
    /// Any MEDIUM -> inform operator via WorkflowNotice.
    /// Any LOW -> require operator approval before proceeding.
    async fn plan_confidence_gate(
        &self,
        goal_run: &GoalRun,
    ) -> super::uncertainty::PlanConfidenceAction {
        let config = self.config.read().await;
        if !config.uncertainty.enabled {
            return super::uncertainty::PlanConfidenceAction::Proceed;
        }
        drop(config);

        let mut has_medium = false;
        let low_steps = collect_low_confidence_plan_steps(goal_run);

        for step in &goal_run.steps {
            if !step.title.starts_with("[LOW]") && step.title.starts_with("[MEDIUM]") {
                has_medium = true;
            }
        }

        if !low_steps.is_empty() {
            let thread_id = goal_run.thread_id.clone().unwrap_or_default();
            let _ = self.event_tx.send(AgentEvent::ConfidenceWarning {
                thread_id: thread_id.clone(),
                action_type: "plan_step".to_string(),
                band: "low".to_string(),
                evidence: low_steps.join("; "),
                domain: "mixed".to_string(),
                blocked: true,
            });
            super::uncertainty::PlanConfidenceAction::RequireApproval
        } else if has_medium {
            let thread_id = goal_run.thread_id.clone().unwrap_or_default();
            let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                thread_id,
                kind: "confidence".to_string(),
                message: "Plan contains MEDIUM-confidence steps. Proceeding with monitoring."
                    .to_string(),
                details: None,
            });
            super::uncertainty::PlanConfidenceAction::Proceed
        } else {
            super::uncertainty::PlanConfidenceAction::Proceed
        }
    }

    pub(super) async fn enqueue_goal_run_step(&self, goal_run_id: &str) -> Result<()> {
        let snapshot = self
            .get_goal_run(goal_run_id)
            .await
            .context("goal run missing while enqueuing step")?;
        if snapshot.current_step_index >= snapshot.steps.len() {
            return Ok(());
        }

        // Auto-checkpoint before step (PreStep)
        {
            let goal_run = {
                let goal_runs = self.goal_runs.lock().await;
                goal_runs.iter().find(|gr| gr.id == goal_run_id).cloned()
            };
            if let Some(goal_run) = goal_run {
                self.auto_checkpoint(
                    &goal_run,
                    crate::agent::liveness::state_layers::CheckpointType::PreStep,
                    "pre_step",
                    Some(goal_run.current_step_index),
                )
                .await;
            }
        }

        let step = snapshot.steps[snapshot.current_step_index].clone();
        let resolved_execution_target = self.resolve_goal_execution_target(&snapshot, &step).await;

        // If this is a Specialist step, route through the handoff broker
        // instead of the normal task enqueue path.
        let task = if let GoalRunStepKind::Specialist(ref role) = step.kind {
            let thread_id = snapshot.thread_id.clone().unwrap_or_default();
            match self
                .route_handoff_to_target(
                    &step.instructions,
                    &[role.clone()],
                    None, // parent_task_id
                    Some(&snapshot.id),
                    &thread_id,
                    &step.success_criteria,
                    0, // depth starts at 0 for goal-originated handoffs
                    resolved_execution_target.as_ref(),
                )
                .await
            {
                Ok(handoff_result) => {
                    // Find the task that was created by route_handoff
                    let tasks = self.tasks.lock().await;
                    tasks
                        .iter()
                        .find(|t| t.id == handoff_result.task_id)
                        .cloned()
                        .expect("handoff-created task missing from queue")
                }
                Err(e) => {
                    tracing::warn!(
                        "specialist handoff failed for step '{}': {e} — falling back to normal enqueue",
                        step.title
                    );
                    self.enqueue_task(
                        step.title.clone(),
                        step.instructions.clone(),
                        task_priority_to_str(snapshot.priority),
                        None,
                        step.session_id
                            .clone()
                            .or_else(|| snapshot.session_id.clone()),
                        Vec::new(),
                        None,
                        "goal_run",
                        Some(snapshot.id.clone()),
                        None,
                        snapshot.thread_id.clone(),
                        None,
                    )
                    .await
                }
            }
        } else if step.kind == GoalRunStepKind::Divergent {
            // Route Divergent steps through start_divergent_session (DIVR-03).
            // The step instructions become the problem statement for parallel framings.
            let thread_id = snapshot.thread_id.clone().unwrap_or_default();
            match self
                .start_divergent_session(
                    &step.instructions,
                    None, // use default framings (analytical + pragmatic)
                    &thread_id,
                    Some(&snapshot.id),
                )
                .await
            {
                Ok(session_id) => {
                    tracing::info!(
                        session_id = %session_id,
                        step = step.title.as_str(),
                        "divergent session started for goal step"
                    );
                    // The divergent session enqueues its own tasks internally.
                    // Create a placeholder task so the goal runner can track the step.
                    self.enqueue_task(
                        format!("Divergent: {}", step.title),
                        format!(
                            "Divergent session {} started for: {}\n\n\
                             Monitor the parallel framings and synthesize tensions when complete.",
                            session_id, step.instructions
                        ),
                        task_priority_to_str(snapshot.priority),
                        None,
                        step.session_id
                            .clone()
                            .or_else(|| snapshot.session_id.clone()),
                        Vec::new(),
                        None,
                        "divergent",
                        Some(snapshot.id.clone()),
                        None,
                        snapshot.thread_id.clone(),
                        None,
                    )
                    .await
                }
                Err(e) => {
                    tracing::warn!(
                        "divergent session failed for step '{}': {e} — falling back to normal enqueue",
                        step.title
                    );
                    self.enqueue_task(
                        step.title.clone(),
                        step.instructions.clone(),
                        task_priority_to_str(snapshot.priority),
                        None,
                        step.session_id
                            .clone()
                            .or_else(|| snapshot.session_id.clone()),
                        Vec::new(),
                        None,
                        "goal_run",
                        Some(snapshot.id.clone()),
                        None,
                        snapshot.thread_id.clone(),
                        None,
                    )
                    .await
                }
            }
        } else if step.kind == GoalRunStepKind::Debate {
            // Route Debate steps through start_debate_session.
            // The step instructions become the debate topic.
            let thread_id = snapshot.thread_id.clone().unwrap_or_default();
            match self
                .start_debate_session(&step.instructions, None, &thread_id, Some(&snapshot.id))
                .await
            {
                Ok(session_id) => {
                    tracing::info!(
                        session_id = %session_id,
                        step = step.title.as_str(),
                        "debate session started for goal step"
                    );
                    self.enqueue_task(
                        format!("Debate: {}", step.title),
                        format!(
                            "Debate session {} started for: {}\n\n\
                             Retrieve the debate state, append arguments, advance rounds, and complete the verdict when ready.",
                            session_id, step.instructions
                        ),
                        task_priority_to_str(snapshot.priority),
                        None,
                        step.session_id
                            .clone()
                            .or_else(|| snapshot.session_id.clone()),
                        Vec::new(),
                        None,
                        "debate",
                        Some(snapshot.id.clone()),
                        None,
                        snapshot.thread_id.clone(),
                        None,
                    )
                    .await
                }
                Err(e) => {
                    tracing::warn!(
                        "debate session failed for step '{}': {e} — falling back to normal enqueue",
                        step.title
                    );
                    self.enqueue_task(
                        step.title.clone(),
                        step.instructions.clone(),
                        task_priority_to_str(snapshot.priority),
                        None,
                        step.session_id
                            .clone()
                            .or_else(|| snapshot.session_id.clone()),
                        Vec::new(),
                        None,
                        "goal_run",
                        Some(snapshot.id.clone()),
                        None,
                        snapshot.thread_id.clone(),
                        None,
                    )
                    .await
                }
            }
        } else {
            self.enqueue_task(
                step.title.clone(),
                step.instructions.clone(),
                task_priority_to_str(snapshot.priority),
                None,
                step.session_id
                    .clone()
                    .or_else(|| snapshot.session_id.clone()),
                Vec::new(),
                None,
                "goal_run",
                Some(snapshot.id.clone()),
                None,
                snapshot.thread_id.clone(),
                None,
            )
            .await
        };
        let task = self
            .apply_goal_resolved_target_to_task(
                task.id.as_str(),
                resolved_execution_target.as_ref(),
            )
            .await
            .unwrap_or(task);

        let requires_ack = super::autonomy::requires_acknowledgment(snapshot.autonomy_level);
        let autonomy_acknowledgment_id = requires_ack.then(|| {
            format!(
                "autonomy-ack:{}:{}:{}",
                snapshot.id, snapshot.current_step_index, step.id
            )
        });
        let current_task_snapshot = {
            let mut tasks = self.tasks.lock().await;
            let Some(current_task) = tasks.iter_mut().find(|entry| entry.id == task.id) else {
                anyhow::bail!("goal step task disappeared after enqueue");
            };
            current_task.goal_run_title = Some(snapshot.title.clone());
            current_task.goal_step_id = Some(step.id.clone());
            current_task.goal_step_title = Some(step.title.clone());
            if let Some(ack_id) = autonomy_acknowledgment_id.as_ref() {
                current_task.status = TaskStatus::AwaitingApproval;
                current_task.awaiting_approval_id = Some(ack_id.clone());
                current_task.blocked_reason =
                    Some("awaiting supervised step acknowledgment".to_string());
                current_task.started_at = None;
                current_task.logs.push(make_task_log_entry(
                    current_task.retry_count,
                    TaskLogLevel::Info,
                    "autonomy_acknowledgment",
                    "supervised step queued and gated pending explicit acknowledgment",
                    Some(ack_id.clone()),
                ));
            }
            current_task.clone()
        };
        let current_step_owner_profile = Some(
            self.goal_owner_profile_for_task_target(
                &current_task_snapshot,
                resolved_execution_target.as_ref(),
            )
            .await,
        );
        let updated = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                anyhow::bail!("goal run disappeared after task enqueue");
            };
            if let Some(current_step) = goal_run.steps.get_mut(goal_run.current_step_index) {
                current_step.task_id = Some(task.id.clone());
                current_step.status = GoalRunStepStatus::InProgress;
                current_step.started_at = Some(now_millis());
            }
            if !goal_run.child_task_ids.iter().any(|id| id == &task.id) {
                goal_run.child_task_ids.push(task.id.clone());
            }
            goal_run.child_task_count = goal_run.child_task_ids.len() as u32;
            goal_run.status = if autonomy_acknowledgment_id.is_some() {
                GoalRunStatus::AwaitingApproval
            } else {
                GoalRunStatus::Running
            };
            goal_run.updated_at = now_millis();
            goal_run.current_step_title = Some(step.title.clone());
            goal_run.current_step_kind = Some(step.kind.clone());
            goal_run.active_task_id = Some(task.id.clone());
            goal_run.current_step_owner_profile = current_step_owner_profile;
            goal_run.awaiting_approval_id = autonomy_acknowledgment_id.clone();
            goal_run.events.push(make_goal_run_event(
                "execution",
                "queued child task for goal step",
                Some(format!("{} -> {}", step.title, task.id)),
            ));
            if let Some(ack_id) = autonomy_acknowledgment_id.as_ref() {
                goal_run.events.push(make_goal_run_event(
                    "autonomy_acknowledgment",
                    &format!(
                        "supervised mode: step queued and awaiting explicit acknowledgment: {}",
                        step.title
                    ),
                    Some(ack_id.clone()),
                ));
            }
            goal_run.clone()
        };

        self.persist_tasks().await;
        self.persist_goal_runs().await;
        self.emit_goal_run_update(&updated, Some(format!("Queued step: {}", step.title)));
        self.record_provenance_event(
            "step_started",
            "goal step queued for execution",
            serde_json::json!({
                "goal_run_id": updated.id,
                "step_index": updated.current_step_index,
                "step_title": step.title,
                "task_id": task.id,
            }),
            Some(updated.id.as_str()),
            Some(task.id.as_str()),
            updated.thread_id.as_deref(),
            None,
            None,
        )
        .await;

        Ok(())
    }
}

fn collect_low_confidence_plan_steps(goal_run: &GoalRun) -> Vec<String> {
    goal_run
        .steps
        .iter()
        .enumerate()
        .filter(|(_, step)| step.title.starts_with("[LOW]"))
        .map(|(index, step)| format!("Step {}: {}", index + 1, step.title))
        .collect()
}

#[cfg(test)]
#[path = "tests/goal_planner.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/goal_planner_structured_fallback.rs"]
mod structured_fallback_tests;
