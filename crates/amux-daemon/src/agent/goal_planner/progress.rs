use super::*;

impl AgentEngine {
    pub(in crate::agent) async fn sync_goal_run_with_task(
        &self,
        goal_run_id: &str,
        task: &AgentTask,
    ) {
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

    pub(in crate::agent) async fn handle_goal_run_step_completion(
        &self,
        goal_run_id: &str,
        task: &AgentTask,
    ) -> Result<()> {
        if task.source == "handoff" {
            let snapshot = self
                .get_goal_run(goal_run_id)
                .await
                .context("goal run missing during specialist validation")?;
            let current_step = snapshot.steps.get(snapshot.current_step_index);
            if let Some(step) = current_step {
                if let GoalRunStepKind::Specialist(_) = &step.kind {
                    let criteria = super::handoff::AcceptanceCriteria {
                        description: step.success_criteria.clone(),
                        structural_checks: vec!["non_empty".to_string()],
                        require_llm_validation: false,
                    };

                    let validation_failure_reason =
                        match self.resolve_handoff_log_id_by_task_id(&task.id).await {
                            Ok(Some(handoff_log_id)) => {
                                match self
                                    .validate_specialist_output(&handoff_log_id, &task.id, &criteria)
                                    .await
                                {
                                    Ok(result) if result.passed => None,
                                    Ok(result) => Some(format!(
                                        "specialist output validation failed: {}",
                                        if result.failures.is_empty() {
                                            "unknown validation failure".to_string()
                                        } else {
                                            result.failures.join("; ")
                                        }
                                    )),
                                    Err(e) => Some(format!(
                                        "specialist output validation error: {e}"
                                    )),
                                }
                            }
                            Ok(None) => Some(format!(
                                "specialist validation blocked: no persisted handoff_log linkage for task {}",
                                task.id
                            )),
                            Err(e) => Some(format!(
                                "specialist validation blocked: failed to resolve handoff linkage: {e}"
                            )),
                        };

                    if let Some(reason) = validation_failure_reason {
                        tracing::warn!(
                            task_id = task.id.as_str(),
                            goal_run_id,
                            reason = %reason,
                            "specialist validation gate failed; routing through failure handler"
                        );
                        let mut failed_task = task.clone();
                        failed_task.last_error = Some(reason.clone());
                        failed_task.error = Some(reason);
                        self.handle_goal_run_step_failure(goal_run_id, &failed_task)
                            .await?;
                        return Ok(());
                    }
                }
            }
        }

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
            goal_run.current_step_kind = next_step.map(|step| step.kind.clone());
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

    pub(in crate::agent) async fn handle_goal_run_step_failure(
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

        if let Some(step) = snapshot.steps.get(snapshot.current_step_index) {
            if let GoalRunStepKind::Specialist(ref role) = step.kind {
                let broker = self.handoff_broker.read().await;
                if let Some(profile) = broker.profiles.iter().find(|p| p.role == *role) {
                    if !profile.escalation_chain.is_empty() {
                        let consecutive_failures = task.retry_count;
                        let elapsed_secs = task
                            .started_at
                            .map(|started| {
                                let now = now_millis();
                                now.saturating_sub(started) / 1000
                            })
                            .unwrap_or(0);
                        let confidence_band = if step.title.starts_with("[HIGH]") {
                            "high"
                        } else if step.title.starts_with("[MEDIUM]") {
                            "medium"
                        } else if step.title.starts_with("[LOW]") {
                            "low"
                        } else {
                            "medium"
                        };

                        if let Some(action) =
                            super::handoff::escalation::evaluate_escalation_triggers(
                                &profile.escalation_chain,
                                consecutive_failures,
                                elapsed_secs,
                                confidence_band,
                            )
                        {
                            tracing::info!(
                                goal_run_id = snapshot.id.as_str(),
                                role,
                                ?action,
                                "escalation trigger fired for specialist step"
                            );
                            self.record_provenance_event(
                                "escalation_triggered",
                                &format!("specialist escalation: {:?}", action),
                                serde_json::json!({
                                    "goal_run_id": snapshot.id,
                                    "specialist_role": role,
                                    "action": format!("{:?}", action),
                                    "consecutive_failures": consecutive_failures,
                                    "elapsed_secs": elapsed_secs,
                                }),
                                Some(snapshot.id.as_str()),
                                Some(task.id.as_str()),
                                snapshot.thread_id.as_deref(),
                                None,
                                None,
                            )
                            .await;
                        }
                    }
                }
                drop(broker);
            }
        }

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
                goal_run.current_step_kind = next_step.map(|step| step.kind.clone());
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
}
