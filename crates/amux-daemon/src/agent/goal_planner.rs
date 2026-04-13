//! Goal run planning — plan generation, step lifecycle, completion, and failure handling.

use super::*;

#[path = "goal_planner/finalization.rs"]
mod finalization_impl;
#[path = "goal_planner/progress.rs"]
mod progress_impl;

impl AgentEngine {
    pub(super) async fn plan_goal_run(&self, goal_run_id: &str) -> Result<()> {
        let goal_run = self
            .get_goal_run(goal_run_id)
            .await
            .context("goal run missing during planning")?;

        let queued = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(current) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                anyhow::bail!("goal run disappeared during planning");
            };
            current.status = GoalRunStatus::Planning;
            current.started_at.get_or_insert(now_millis());
            current.updated_at = now_millis();
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
            current.steps = plan
                .steps
                .into_iter()
                .enumerate()
                .map(|(position, step)| GoalRunStep {
                    id: format!("goal_step_{}", Uuid::new_v4()),
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
            current.status = GoalRunStatus::Running;
            current.updated_at = now;
            current.last_error = None;
            current.failure_cause = None;
            current.awaiting_approval_id = None;
            current.active_task_id = None;
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
        let gate_action = self.plan_confidence_gate(&updated).await;
        if gate_action == super::uncertainty::PlanConfidenceAction::RequireApproval {
            let mut goal_runs = self.goal_runs.lock().await;
            if let Some(current) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) {
                current.status = GoalRunStatus::AwaitingApproval;
                current.updated_at = now_millis();
                current.events.push(make_goal_run_event(
                    "confidence_gate",
                    "plan requires operator approval due to LOW-confidence steps",
                    None,
                ));
            }
            drop(goal_runs);
            self.persist_goal_runs().await;
            self.emit_goal_run_update(
                &updated,
                Some("Plan awaiting approval: LOW-confidence steps detected".into()),
            );
        }

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
        let mut has_low = false;
        let mut low_steps = Vec::new();

        for (i, step) in goal_run.steps.iter().enumerate() {
            if step.title.starts_with("[LOW]") {
                has_low = true;
                low_steps.push(format!("Step {}: {}", i + 1, step.title));
            } else if step.title.starts_with("[MEDIUM]") {
                has_medium = true;
            }
        }

        if has_low {
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

        // If this is a Specialist step, route through the handoff broker
        // instead of the normal task enqueue path.
        let task = if let GoalRunStepKind::Specialist(ref role) = step.kind {
            let thread_id = snapshot.thread_id.clone().unwrap_or_default();
            match self
                .route_handoff(
                    &step.instructions,
                    &[role.clone()],
                    None, // parent_task_id
                    Some(&snapshot.id),
                    &thread_id,
                    &step.success_criteria,
                    0, // depth starts at 0 for goal-originated handoffs
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

        let requires_ack = super::autonomy::requires_acknowledgment(snapshot.autonomy_level);
        let autonomy_acknowledgment_id = requires_ack.then(|| {
            format!(
                "autonomy-ack:{}:{}:{}",
                snapshot.id, snapshot.current_step_index, step.id
            )
        });
        let updated = {
            let mut goal_runs = self.goal_runs.lock().await;
            let mut tasks = self.tasks.lock().await;
            if let Some(current_task) = tasks.iter_mut().find(|entry| entry.id == task.id) {
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
            }
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

#[cfg(test)]
#[path = "tests/goal_planner.rs"]
mod tests;
