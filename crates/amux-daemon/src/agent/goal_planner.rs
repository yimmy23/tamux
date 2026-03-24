//! Goal run planning — plan generation, step lifecycle, completion, and failure handling.

use super::*;

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
            current.current_step_kind = current.steps.first().map(|step| step.kind);
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
        Ok(())
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
        let task = self
            .enqueue_task(
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
            .await;

        let updated = {
            let mut goal_runs = self.goal_runs.lock().await;
            let mut tasks = self.tasks.lock().await;
            if let Some(current_task) = tasks.iter_mut().find(|entry| entry.id == task.id) {
                current_task.goal_run_title = Some(snapshot.title.clone());
                current_task.goal_step_id = Some(step.id.clone());
                current_task.goal_step_title = Some(step.title.clone());
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
            goal_run.status = GoalRunStatus::Running;
            goal_run.updated_at = now_millis();
            goal_run.current_step_title = Some(step.title.clone());
            goal_run.current_step_kind = Some(step.kind);
            goal_run.active_task_id = Some(task.id.clone());
            goal_run.awaiting_approval_id = None;
            goal_run.events.push(make_goal_run_event(
                "execution",
                "queued child task for goal step",
                Some(format!("{} -> {}", step.title, task.id)),
            ));
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

    pub(super) async fn sync_goal_run_with_task(&self, goal_run_id: &str, task: &AgentTask) {
        let mut maybe_updated = None;
        {
            let mut goal_runs = self.goal_runs.lock().await;
            if let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) {
                let prior_status = goal_run.status;
                let next_status = if task.status == TaskStatus::AwaitingApproval {
                    GoalRunStatus::AwaitingApproval
                } else {
                    GoalRunStatus::Running
                };
                let mut changed = goal_run.status != next_status;
                goal_run.status = next_status;
                goal_run.updated_at = now_millis();
                goal_run.awaiting_approval_id = task.awaiting_approval_id.clone();
                goal_run.active_task_id = Some(task.id.clone());
                if let Some(step) = goal_run.steps.get_mut(goal_run.current_step_index) {
                    if step.status != GoalRunStepStatus::InProgress {
                        step.status = GoalRunStepStatus::InProgress;
                        step.started_at.get_or_insert(now_millis());
                        changed = true;
                    }
                }
                if next_status == GoalRunStatus::AwaitingApproval
                    && prior_status != GoalRunStatus::AwaitingApproval
                {
                    goal_run.events.push(make_goal_run_event(
                        "approval",
                        "goal step awaiting operator approval",
                        task.awaiting_approval_id.clone(),
                    ));
                    changed = true;
                }
                if changed {
                    maybe_updated = Some(goal_run.clone());
                }
            }
        }

        if let Some(updated) = maybe_updated {
            self.persist_goal_runs().await;
            self.emit_goal_run_update(&updated, Some(goal_run_status_message(&updated).into()));
        }
    }

    pub(super) async fn handle_goal_run_step_completion(
        &self,
        goal_run_id: &str,
        task: &AgentTask,
    ) -> Result<()> {
        let now = now_millis();
        let thread_summary = match task.thread_id.as_deref() {
            Some(thread_id) => self.goal_thread_summary(thread_id).await,
            None => None,
        };
        let updated = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                anyhow::bail!("goal run missing after task completion");
            };
            if let Some(thread_id) = task.thread_id.clone() {
                goal_run.thread_id = Some(thread_id);
            }
            if let Some(step) = goal_run.steps.get_mut(goal_run.current_step_index) {
                step.status = GoalRunStepStatus::Completed;
                step.completed_at = Some(now);
                step.summary = thread_summary
                    .clone()
                    .or_else(|| Some("step completed".into()));
            }
            goal_run.current_step_index = goal_run.current_step_index.saturating_add(1);
            let next_step = goal_run.steps.get(goal_run.current_step_index);
            goal_run.current_step_title = next_step.map(|step| step.title.clone());
            goal_run.current_step_kind = next_step.map(|step| step.kind);
            goal_run.status = GoalRunStatus::Running;
            goal_run.updated_at = now;
            goal_run.last_error = None;
            goal_run.failure_cause = None;
            goal_run.awaiting_approval_id = None;
            goal_run.active_task_id = None;
            goal_run.events.push(make_goal_run_event(
                "execution",
                "goal step completed",
                thread_summary.clone(),
            ));
            goal_run.clone()
        };

        self.persist_goal_runs().await;
        self.emit_goal_run_update(&updated, Some("Goal step completed".into()));
        self.record_provenance_event(
            "step_completed",
            "goal step completed",
            serde_json::json!({
                "goal_run_id": updated.id,
                "completed_step_index": updated.current_step_index.saturating_sub(1),
                "task_id": task.id,
                "summary": thread_summary,
            }),
            Some(updated.id.as_str()),
            Some(task.id.as_str()),
            updated.thread_id.as_deref(),
            None,
            None,
        )
        .await;

        // Auto-checkpoint after step completion (PostStep)
        self.auto_checkpoint(
            &updated,
            crate::agent::liveness::state_layers::CheckpointType::PostStep,
            "post_step",
            Some(updated.current_step_index.saturating_sub(1)),
        )
        .await;

        if updated.current_step_index >= updated.steps.len() {
            self.complete_goal_run(goal_run_id).await?;
        }

        Ok(())
    }

    pub(super) async fn handle_goal_run_step_failure(
        &self,
        goal_run_id: &str,
        task: &AgentTask,
    ) -> Result<()> {
        let snapshot = self
            .get_goal_run(goal_run_id)
            .await
            .context("goal run missing during failure handling")?;
        let failure = task
            .last_error
            .clone()
            .or_else(|| task.error.clone())
            .unwrap_or_else(|| format!("child task {} failed", task.id));

        if snapshot.replan_count < snapshot.max_replans
            && snapshot.current_step_index < snapshot.steps.len()
        {
            let revised = self.request_goal_replan(&snapshot, &failure).await?;
            self.persist_goal_plan_causal_trace(&snapshot, &revised, Some(&failure))
                .await;
            self.persist_recovery_near_miss_trace(&snapshot, task, &failure, &revised)
                .await;
            let updated = {
                let mut goal_runs = self.goal_runs.lock().await;
                let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id)
                else {
                    anyhow::bail!("goal run disappeared during replan");
                };
                let default_session_id = goal_run.session_id.clone();
                if let Some(step) = goal_run.steps.get_mut(goal_run.current_step_index) {
                    step.status = GoalRunStepStatus::Failed;
                    step.completed_at = Some(now_millis());
                    step.error = Some(failure.clone());
                }
                let insert_at = goal_run.current_step_index.saturating_add(1);
                goal_run.steps.truncate(insert_at);
                for (offset, step) in revised.steps.into_iter().enumerate() {
                    goal_run.steps.push(GoalRunStep {
                        id: format!("goal_step_{}", Uuid::new_v4()),
                        position: insert_at + offset,
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
                    });
                }
                for (position, step) in goal_run.steps.iter_mut().enumerate() {
                    step.position = position;
                }
                goal_run.current_step_index = insert_at;
                let next_step = goal_run.steps.get(goal_run.current_step_index);
                goal_run.current_step_title = next_step.map(|step| step.title.clone());
                goal_run.current_step_kind = next_step.map(|step| step.kind);
                goal_run.replan_count = goal_run.replan_count.saturating_add(1);
                goal_run.status = GoalRunStatus::Running;
                goal_run.updated_at = now_millis();
                goal_run.last_error = Some(failure.clone());
                goal_run.failure_cause = Some(failure.clone());
                goal_run.reflection_summary = Some(revised.summary.clone());
                goal_run.awaiting_approval_id = None;
                goal_run.active_task_id = None;
                goal_run.events.push(make_goal_run_event(
                    "replan",
                    "goal plan revised after failed step",
                    Some(failure.clone()),
                ));
                goal_run.clone()
            };
            self.persist_goal_runs().await;
            self.emit_goal_run_update(&updated, Some("Goal replanned after failure".into()));
            self.record_provenance_event(
                "replan_triggered",
                "goal replan triggered after failed step",
                serde_json::json!({
                    "goal_run_id": updated.id,
                    "task_id": task.id,
                    "failure": failure,
                    "replan_count": updated.replan_count,
                }),
                Some(updated.id.as_str()),
                Some(task.id.as_str()),
                updated.thread_id.as_deref(),
                None,
                None,
            )
            .await;
            self.record_provenance_event(
                "recovery_triggered",
                "goal recovery path recorded",
                serde_json::json!({
                    "goal_run_id": updated.id,
                    "task_id": task.id,
                    "failure": failure,
                    "mode": "replan_after_failure",
                }),
                Some(updated.id.as_str()),
                Some(task.id.as_str()),
                updated.thread_id.as_deref(),
                None,
                None,
            )
            .await;
            return Ok(());
        }

        self.record_provenance_event(
            "step_failed",
            "goal step failed",
            serde_json::json!({
                "goal_run_id": snapshot.id,
                "task_id": task.id,
                "failure": failure,
            }),
            Some(snapshot.id.as_str()),
            Some(task.id.as_str()),
            snapshot.thread_id.as_deref(),
            None,
            None,
        )
        .await;
        self.fail_goal_run(goal_run_id, &failure, "execution").await;
        Ok(())
    }

    pub(super) async fn complete_goal_run(&self, goal_run_id: &str) -> Result<()> {
        let snapshot = self
            .get_goal_run(goal_run_id)
            .await
            .context("goal run missing during completion")?;
        if self.goal_run_has_active_tasks(goal_run_id).await {
            anyhow::bail!("goal run still has active child work");
        }
        let reflection = self.request_goal_reflection(&snapshot).await?;
        let mut applied_memory_update = None;
        if let Some(update) = reflection.stable_memory_update.clone() {
            match self.append_goal_memory_update(goal_run_id, &update).await {
                Ok(()) => applied_memory_update = Some(update),
                Err(error) => {
                    tracing::warn!(goal_run_id, error = %error, "failed to persist reflected memory update");
                }
            }
        }
        let generated_skill_path = if reflection.generate_skill {
            let skill_title = reflection
                .skill_title
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(snapshot.title.as_str());
            self.history
                .generate_skill(Some(snapshot.goal.as_str()), Some(skill_title))
                .await
                .ok()
                .map(|(_, path)| path)
        } else {
            None
        };

        let updated = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                anyhow::bail!("goal run missing while finalizing");
            };
            goal_run.status = GoalRunStatus::Completed;
            goal_run.completed_at = Some(now_millis());
            goal_run.updated_at = now_millis();
            goal_run.reflection_summary = Some(reflection.summary.clone());
            goal_run.current_step_title = None;
            goal_run.current_step_kind = None;
            goal_run.awaiting_approval_id = None;
            goal_run.active_task_id = None;
            if let Some(update) = applied_memory_update {
                goal_run.memory_updates.push(update);
            }
            if let Some(path) = generated_skill_path {
                goal_run.generated_skill_path = Some(path);
            }
            goal_run.events.push(make_goal_run_event(
                "reflection",
                "goal run completed",
                goal_run.reflection_summary.clone(),
            ));
            goal_run.clone()
        };

        self.persist_goal_runs().await;
        self.record_generated_skill_work_context(&updated).await;
        self.settle_goal_skill_consultations(&updated, "success")
            .await;
        self.emit_goal_run_update(&updated, Some("Goal completed".into()));
        self.record_provenance_event(
            "goal_completed",
            "goal run completed",
            serde_json::json!({
                "goal_run_id": updated.id,
                "reflection_summary": updated.reflection_summary,
                "generated_skill_path": updated.generated_skill_path,
                "memory_updates": updated.memory_updates,
            }),
            Some(updated.id.as_str()),
            None,
            updated.thread_id.as_deref(),
            None,
            None,
        )
        .await;
        Ok(())
    }

    pub(super) async fn fail_goal_run(&self, goal_run_id: &str, error: &str, phase: &str) {
        let mut maybe_updated = None;
        {
            let mut goal_runs = self.goal_runs.lock().await;
            if let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) {
                goal_run.status = GoalRunStatus::Failed;
                goal_run.completed_at = Some(now_millis());
                goal_run.updated_at = now_millis();
                goal_run.last_error = Some(error.to_string());
                goal_run.failure_cause = Some(error.to_string());
                goal_run.awaiting_approval_id = None;
                goal_run.active_task_id = None;
                goal_run.events.push(make_goal_run_event(
                    phase,
                    "goal run failed",
                    Some(error.to_string()),
                ));
                maybe_updated = Some(goal_run.clone());
            }
        }
        if let Some(updated) = maybe_updated {
            self.persist_goal_runs().await;
            self.settle_goal_skill_consultations(&updated, "failure")
                .await;
            self.emit_goal_run_update(&updated, Some(format!("Goal failed: {error}")));
            self.record_provenance_event(
                "goal_failed",
                "goal run failed",
                serde_json::json!({
                    "goal_run_id": updated.id,
                    "phase": phase,
                    "error": error,
                }),
                Some(updated.id.as_str()),
                None,
                updated.thread_id.as_deref(),
                None,
                None,
            )
            .await;
        }
    }

    pub(super) async fn requeue_goal_run_step(&self, goal_run_id: &str, reason: &str) {
        let mut maybe_updated = None;
        {
            let mut goal_runs = self.goal_runs.lock().await;
            if let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) {
                if let Some(step) = goal_run.steps.get_mut(goal_run.current_step_index) {
                    step.task_id = None;
                    step.status = GoalRunStepStatus::Pending;
                    step.error = Some(reason.to_string());
                }
                goal_run.status = GoalRunStatus::Running;
                goal_run.updated_at = now_millis();
                goal_run.awaiting_approval_id = None;
                goal_run.active_task_id = None;
                goal_run.events.push(make_goal_run_event(
                    "execution",
                    "goal step returned to pending",
                    Some(reason.to_string()),
                ));
                maybe_updated = Some(goal_run.clone());
            }
        }
        if let Some(updated) = maybe_updated {
            self.persist_goal_runs().await;
            self.emit_goal_run_update(&updated, Some("Goal step re-queued".into()));
        }
    }

    async fn auto_checkpoint(
        &self,
        goal_run: &GoalRun,
        checkpoint_type: crate::agent::liveness::state_layers::CheckpointType,
        label: &str,
        step_index: Option<usize>,
    ) {
        let goal_run_id = &goal_run.id;
        let tasks_snapshot: Vec<_> = self
            .tasks
            .lock()
            .await
            .iter()
            .filter(|t| t.goal_run_id.as_deref() == Some(goal_run_id.as_str()))
            .cloned()
            .collect();
        let todos = self
            .thread_todos
            .read()
            .await
            .get(goal_run.thread_id.as_deref().unwrap_or(""))
            .cloned()
            .unwrap_or_default();
        let now = now_millis();
        let checkpoint = crate::agent::liveness::checkpoint::checkpoint_save(
            checkpoint_type,
            goal_run,
            &tasks_snapshot,
            goal_run.thread_id.as_deref(),
            None,
            None,
            None,
            &todos,
            now,
        );
        if let Err(e) =
            crate::agent::liveness::checkpoint::checkpoint_store(&self.history, &checkpoint).await
        {
            tracing::warn!(goal_run_id, "failed to store checkpoint: {e}");
        }
        let _ = self.event_tx.send(AgentEvent::CheckpointCreated {
            checkpoint_id: checkpoint.id,
            goal_run_id: goal_run_id.clone(),
            checkpoint_type: label.to_string(),
            step_index,
        });
    }
}
