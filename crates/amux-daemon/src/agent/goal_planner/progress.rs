use super::*;

const GOAL_COMPLETION_MARKER_REMINDER_LIMIT: u32 = 3;

#[derive(Clone)]
struct GoalCompletionMarkerContext {
    step: GoalRunStep,
    human_step_number: usize,
    total_steps: usize,
    absolute_path: std::path::PathBuf,
}

fn completion_marker_retry_key(goal_run_id: &str, step_id: &str) -> String {
    format!("{goal_run_id}:{step_id}")
}

fn completion_marker_prompt(context: &GoalCompletionMarkerContext) -> String {
    format!(
        "Required completion marker is missing for Step {} of {}.\n\
         Create file: {}\n\
         Do not consider this step complete until that file exists.",
        context.human_step_number,
        context.total_steps,
        context.absolute_path.display(),
    )
}

fn completion_marker_detail(
    context: &GoalCompletionMarkerContext,
    retries_attempted: u32,
    todos_completed: bool,
) -> String {
    format!(
        "Required completion marker missing for Step {} of {} ({}): {}. retries_attempted={retries_attempted}; todos_completed={todos_completed}",
        context.human_step_number,
        context.total_steps,
        context.step.title,
        context.absolute_path.display(),
    )
}

fn current_step_verification_requirements(
    goal_run: &GoalRun,
) -> Option<(GoalRunStep, GoalRoleBinding, Vec<GoalProofCheck>)> {
    let step = goal_run.steps.get(goal_run.current_step_index)?.clone();
    let proof_checks = goal_run
        .dossier
        .as_ref()
        .and_then(|dossier| dossier.units.iter().find(|unit| unit.id == step.id))
        .map(|unit| unit.proof_checks.clone())
        .unwrap_or_default();
    Some((step, default_verification_binding(), proof_checks))
}

fn current_step_completion_marker_context(
    engine: &AgentEngine,
    goal_run: &GoalRun,
) -> Option<GoalCompletionMarkerContext> {
    let step = goal_run.steps.get(goal_run.current_step_index)?.clone();
    Some(GoalCompletionMarkerContext {
        step,
        human_step_number: goal_run.current_step_index.saturating_add(1),
        total_steps: goal_run.steps.len(),
        absolute_path: super::goal_dossier::goal_step_completion_marker_path(
            &engine.data_dir,
            &goal_run.id,
            goal_run.current_step_index,
        ),
    })
}

fn current_step_review_result(task: &AgentTask, thread_summary: Option<&str>) -> Option<String> {
    task.result
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            thread_summary
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            task.last_error
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            task.error
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

fn structured_review_failure_reason(record: &GoalStepReviewRecord) -> String {
    let explanation = record.explanation.trim();
    if explanation.is_empty() {
        "structured reviewer verdict was FAIL without an explanation".to_string()
    } else {
        explanation.to_string()
    }
}

fn legacy_review_failure_reason(review_result: String) -> String {
    if matches!(
        parse_goal_review_verdict(&review_result),
        Some(GoalReviewVerdict::Fail)
    ) {
        review_result
    } else {
        format!(
            "review did not return a pass verdict; reviewer output was: {}",
            review_result
        )
    }
}

impl AgentEngine {
    async fn load_goal_step_review_record(
        &self,
        task_id: &str,
    ) -> Result<Option<GoalStepReviewRecord>> {
        let Some(raw) = self
            .history
            .get_consolidation_state(&goal_step_verdict_state_key(task_id))
            .await?
        else {
            return Ok(None);
        };
        let record = serde_json::from_str::<GoalStepReviewRecord>(&raw)
            .with_context(|| format!("invalid structured goal-step verdict for task {task_id}"))?;
        Ok(Some(record))
    }

    async fn goal_step_review_requires_structured_verdict(&self, task_id: &str) -> Result<bool> {
        Ok(self
            .history
            .get_consolidation_state(&goal_step_verdict_required_state_key(task_id))
            .await?
            .as_deref()
            == Some("true"))
    }

    fn validate_goal_step_review_record(
        task: &AgentTask,
        goal_run_id: &str,
        record: &GoalStepReviewRecord,
    ) -> Result<()> {
        if record.task_id != task.id {
            anyhow::bail!(
                "structured reviewer verdict belongs to task {} but completed task is {}",
                record.task_id,
                task.id
            );
        }
        if record.goal_run_id != goal_run_id {
            anyhow::bail!(
                "structured reviewer verdict belongs to goal {} but completed task is in goal {}",
                record.goal_run_id,
                goal_run_id
            );
        }
        if task.goal_step_id.as_deref() != Some(record.goal_step_id.as_str()) {
            anyhow::bail!(
                "structured reviewer verdict belongs to step {} but completed task is bound to {:?}",
                record.goal_step_id,
                task.goal_step_id
            );
        }
        Ok(())
    }

    async fn current_step_incomplete_todos(
        &self,
        snapshot: &GoalRun,
        task: &AgentTask,
    ) -> Vec<TodoItem> {
        let Some(thread_id) = task.thread_id.as_deref().or(snapshot.thread_id.as_deref()) else {
            return Vec::new();
        };
        self.get_todos(thread_id)
            .await
            .into_iter()
            .filter(|item| {
                (item.step_index == Some(snapshot.current_step_index) || item.step_index.is_none())
                    && item.status != TodoStatus::Completed
            })
            .collect()
    }

    async fn requeue_goal_step_to_pending(
        &self,
        goal_run_id: &str,
        reason: &str,
        task: Option<&AgentTask>,
    ) -> Result<()> {
        let updated = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                anyhow::bail!("goal run missing while re-queuing step");
            };
            if let Some(step) = goal_run.steps.get_mut(goal_run.current_step_index) {
                step.task_id = None;
                step.status = GoalRunStepStatus::Pending;
                step.error = Some(reason.to_string());
                step.completed_at = None;
            }
            goal_run.status = GoalRunStatus::Running;
            goal_run.updated_at = now_millis();
            goal_run.last_error = Some(reason.to_string());
            goal_run.failure_cause = Some(reason.to_string());
            goal_run.awaiting_approval_id = None;
            goal_run.active_task_id = None;
            goal_run.current_step_owner_profile = None;
            goal_run.events.push(make_goal_run_event(
                "review",
                "goal step re-queued after failed completion gate",
                Some(reason.to_string()),
            ));
            if let Some(step_id) = goal_run
                .steps
                .get(goal_run.current_step_index)
                .map(|step| step.id.clone())
            {
                super::goal_dossier::set_goal_unit_verification_state(
                    goal_run,
                    &step_id,
                    GoalProjectionState::Failed,
                    reason.to_string(),
                    vec!["review or completion gate failed".to_string()],
                    Some("goal review failure"),
                    Some(reason.to_string()),
                );
                super::goal_dossier::set_goal_resume_decision(
                    goal_run,
                    GoalResumeAction::Replan,
                    "step_review_failed",
                    Some(reason.to_string()),
                    Vec::new(),
                );
            }
            goal_run.clone()
        };

        self.persist_goal_runs().await;
        self.emit_goal_run_update(&updated, Some("Goal step re-queued".into()));
        if let Some(task) = task {
            self.record_provenance_event(
                "step_requeued",
                "goal step re-queued after failed completion gate",
                serde_json::json!({
                    "goal_run_id": updated.id,
                    "task_id": task.id,
                    "reason": reason,
                }),
                Some(updated.id.as_str()),
                Some(task.id.as_str()),
                updated.thread_id.as_deref(),
                None,
                None,
            )
            .await;
        }
        Ok(())
    }

    async fn requeue_goal_step_for_missing_completion_marker(
        &self,
        snapshot: &GoalRun,
        task: &AgentTask,
        context: &GoalCompletionMarkerContext,
    ) -> Result<()> {
        let retry_key = completion_marker_retry_key(&snapshot.id, &context.step.id);
        let reminder_number = {
            let mut retries = self.goal_step_completion_marker_retries.lock().await;
            let issued = retries.entry(retry_key).or_insert(0);
            if *issued >= GOAL_COMPLETION_MARKER_REMINDER_LIMIT {
                return self
                    .gate_goal_step_missing_completion_marker_for_approval(snapshot, task, context)
                    .await;
            }
            *issued = issued.saturating_add(1);
            *issued
        };

        let todos_completed = if let Some(thread_id) = task.thread_id.as_deref() {
            let todos = self.get_todos(thread_id).await;
            !todos.is_empty()
                && todos
                    .iter()
                    .all(|item| item.status == TodoStatus::Completed)
        } else {
            false
        };
        let detail = completion_marker_detail(context, reminder_number, todos_completed);
        let reminder_prompt = completion_marker_prompt(context);

        let updated_task = {
            let mut tasks = self.tasks.lock().await;
            let Some(current) = tasks.iter_mut().find(|entry| entry.id == task.id) else {
                anyhow::bail!("goal step task disappeared during completion marker retry");
            };
            current.status = TaskStatus::Queued;
            current.progress = current.progress.max(95);
            current.started_at = None;
            current.completed_at = None;
            current.awaiting_approval_id = None;
            current.blocked_reason = Some(detail.clone());
            current.description = reminder_prompt;
            current.error = None;
            current.last_error = None;
            current.logs.push(make_task_log_entry(
                current.retry_count,
                TaskLogLevel::Warn,
                "completion_marker",
                &format!(
                    "required completion marker missing; retry {reminder_number} of {} queued",
                    GOAL_COMPLETION_MARKER_REMINDER_LIMIT
                ),
                Some(detail.clone()),
            ));
            current.clone()
        };

        let updated_goal = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == snapshot.id) else {
                anyhow::bail!("goal run missing while re-queuing completion marker retry");
            };
            goal_run.status = GoalRunStatus::Running;
            goal_run.updated_at = now_millis();
            goal_run.awaiting_approval_id = None;
            goal_run.active_task_id = Some(updated_task.id.clone());
            goal_run.current_step_title = Some(context.step.title.clone());
            goal_run.current_step_kind = Some(context.step.kind.clone());
            goal_run.events.push(make_goal_run_event(
                "completion_marker",
                "required step completion marker missing; retry queued",
                Some(detail),
            ));
            goal_run.clone()
        };

        self.persist_tasks().await;
        self.persist_goal_runs().await;
        self.emit_task_update(
            &updated_task,
            Some(format!(
                "Missing completion marker; retry {reminder_number}/{} queued",
                GOAL_COMPLETION_MARKER_REMINDER_LIMIT
            )),
        );
        self.emit_goal_run_update(
            &updated_goal,
            Some("Goal step waiting for required completion marker".into()),
        );
        Ok(())
    }

    async fn gate_goal_step_missing_completion_marker_for_approval(
        &self,
        snapshot: &GoalRun,
        task: &AgentTask,
        context: &GoalCompletionMarkerContext,
    ) -> Result<()> {
        let approval_id = format!("goal-step-marker-approval-{}", Uuid::new_v4());
        let detail = format!(
            "Required completion marker missing after {} retries for Step {} of {} ({}): {}",
            GOAL_COMPLETION_MARKER_REMINDER_LIMIT,
            context.human_step_number,
            context.total_steps,
            context.step.title,
            context.absolute_path.display(),
        );

        let updated_task = {
            let mut tasks = self.tasks.lock().await;
            let Some(current) = tasks.iter_mut().find(|entry| entry.id == task.id) else {
                anyhow::bail!("goal step task disappeared before approval gating");
            };
            current.status = TaskStatus::AwaitingApproval;
            current.progress = current.progress.max(95);
            current.started_at = None;
            current.completed_at = None;
            current.awaiting_approval_id = Some(approval_id.clone());
            current.blocked_reason = Some(detail.clone());
            current.logs.push(make_task_log_entry(
                current.retry_count,
                TaskLogLevel::Warn,
                "completion_marker",
                "required completion marker missing; human approval required",
                Some(detail.clone()),
            ));
            current.clone()
        };

        let updated_goal = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == snapshot.id) else {
                anyhow::bail!("goal run missing while applying completion marker approval gate");
            };
            goal_run.status = GoalRunStatus::AwaitingApproval;
            goal_run.updated_at = now_millis();
            goal_run.awaiting_approval_id = Some(approval_id.clone());
            goal_run.active_task_id = Some(updated_task.id.clone());
            goal_run.events.push(make_goal_run_event(
                "completion_marker",
                "required step completion marker missing; human approval required",
                Some(detail),
            ));
            goal_run.clone()
        };

        self.persist_tasks().await;
        self.persist_goal_runs().await;
        self.emit_task_update(&updated_task, Some("Task awaiting approval".into()));
        self.emit_goal_run_update(
            &updated_goal,
            Some("Goal step awaiting approval: completion marker missing".into()),
        );
        Ok(())
    }

    async fn enqueue_goal_step_verification(
        &self,
        snapshot: &GoalRun,
        step: &GoalRunStep,
        verification_binding: &GoalRoleBinding,
        proof_checks: &[GoalProofCheck],
        completed_task: &AgentTask,
        implementation_summary: Option<String>,
    ) -> Result<()> {
        let proof_check_lines = if proof_checks.is_empty() {
            "- none specified; perform a strict reviewer pass against the step instructions, success criteria, todos, and completion artifacts".to_string()
        } else {
            proof_checks
                .iter()
                .map(|check| format!("- {}: {}", check.id, check.title))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let binding_label = match verification_binding {
            GoalRoleBinding::Builtin(value) => format!("builtin:{value}"),
            GoalRoleBinding::Subagent(value) => format!("subagent:{value}"),
        };
        let verification_description = format!(
            "Review completed goal step `{}`.\n\n\
             You must call `submit_goal_step_verdict` exactly once before finishing.\n\
             Use verdict `pass` only when the current step satisfies its instructions, success criteria, todos, completion artifacts, and required proof checks.\n\
             Use verdict `fail` with a concrete explanation when the step needs more work; that explanation is sent back to the responsible step agent.\n\n\
             Original instructions:\n{}\n\n\
             Success criteria:\n{}\n\n\
             Implementation summary:\n{}\n\n\
             Verification binding:\n{}\n\n\
             Required proof checks:\n{}",
            step.title,
            step.instructions,
            step.success_criteria,
            implementation_summary
                .clone()
                .unwrap_or_else(|| "step completed without summary".to_string()),
            binding_label,
            proof_check_lines
        );

        let verification_task = self
            .enqueue_task(
                format!("Verify: {}", step.title),
                verification_description,
                task_priority_to_str(snapshot.priority),
                None,
                snapshot.session_id.clone(),
                Vec::new(),
                None,
                GOAL_VERIFICATION_SOURCE,
                Some(snapshot.id.clone()),
                Some(completed_task.id.clone()),
                snapshot.thread_id.clone(),
                None,
            )
            .await;

        let resolved_verification_target = self
            .resolve_goal_target_for_binding(snapshot, step, verification_binding)
            .await;
        let updated_task = {
            let mut tasks = self.tasks.lock().await;
            let Some(task) = tasks
                .iter_mut()
                .find(|task| task.id == verification_task.id)
            else {
                anyhow::bail!("verification task disappeared after enqueue");
            };
            task.goal_run_title = Some(snapshot.title.clone());
            task.goal_step_id = Some(step.id.clone());
            task.goal_step_title = Some(step.title.clone());
            task.thread_id = snapshot.thread_id.clone();
            task.success_criteria = Some(format!(
                "All proof checks pass: {}",
                proof_checks
                    .iter()
                    .map(|check| check.id.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            task.clone()
        };
        self.history
            .set_consolidation_state(
                &goal_step_verdict_required_state_key(&verification_task.id),
                "true",
                now_millis(),
            )
            .await?;
        let updated_task = self
            .apply_goal_resolved_target_to_task(
                verification_task.id.as_str(),
                resolved_verification_target.as_ref(),
            )
            .await
            .unwrap_or(updated_task);
        let current_step_owner_profile = Some(
            self.goal_owner_profile_for_task_target(
                &updated_task,
                resolved_verification_target.as_ref(),
            )
            .await,
        );
        self.persist_tasks().await;
        self.emit_task_update(&updated_task, Some(status_message(&updated_task).into()));

        let updated_goal = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == snapshot.id) else {
                anyhow::bail!("goal run missing while queuing verification");
            };
            if let Some(current_step) = goal_run.steps.get_mut(goal_run.current_step_index) {
                current_step.task_id = Some(updated_task.id.clone());
                current_step.status = GoalRunStepStatus::InProgress;
                current_step.summary = implementation_summary.clone();
                current_step.completed_at = None;
            }
            if !goal_run
                .child_task_ids
                .iter()
                .any(|id| id == &updated_task.id)
            {
                goal_run.child_task_ids.push(updated_task.id.clone());
            }
            goal_run.child_task_count = goal_run.child_task_ids.len() as u32;
            goal_run.status = GoalRunStatus::Running;
            goal_run.updated_at = now_millis();
            goal_run.last_error = None;
            goal_run.failure_cause = None;
            goal_run.awaiting_approval_id = None;
            goal_run.active_task_id = Some(updated_task.id.clone());
            super::super::goal_run_apply_thread_routing(goal_run, updated_task.thread_id.clone());
            goal_run.current_step_owner_profile = current_step_owner_profile;
            goal_run.events.push(make_goal_run_event(
                "verification",
                "goal step verification queued",
                Some(binding_label.clone()),
            ));
            super::goal_dossier::set_goal_unit_verification_state(
                goal_run,
                &step.id,
                GoalProjectionState::InProgress,
                format!("verification queued via {binding_label}"),
                implementation_summary
                    .clone()
                    .map(|summary| vec![format!("implementation summary: {summary}")])
                    .unwrap_or_default(),
                None,
                None,
            );
            goal_run.clone()
        };

        self.persist_goal_runs().await;
        self.emit_goal_run_update(
            &updated_goal,
            Some("Goal step implementation complete; verification queued".into()),
        );
        Ok(())
    }

    pub(in crate::agent) async fn sync_goal_run_with_task(
        &self,
        goal_run_id: &str,
        task: &AgentTask,
    ) {
        let current_step_owner_profile = {
            let goal_runs = self.goal_runs.lock().await;
            goal_runs
                .iter()
                .find(|item| item.id == goal_run_id)
                .and_then(|goal_run| goal_run.current_step_owner_profile.clone())
        };
        let current_step_owner_profile = match current_step_owner_profile {
            Some(profile) => profile,
            None => self.current_step_owner_profile_for_task(task).await,
        };
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
                let prior_thread_routing = (
                    goal_run.thread_id.clone(),
                    goal_run.root_thread_id.clone(),
                    goal_run.active_thread_id.clone(),
                    goal_run.execution_thread_ids.clone(),
                );
                goal_run.status = next_status;
                goal_run.updated_at = now_millis();
                goal_run.awaiting_approval_id = task.awaiting_approval_id.clone();
                goal_run.active_task_id = Some(task.id.clone());
                super::super::goal_run_apply_thread_routing(goal_run, task.thread_id.clone());
                if prior_thread_routing
                    != (
                        goal_run.thread_id.clone(),
                        goal_run.root_thread_id.clone(),
                        goal_run.active_thread_id.clone(),
                        goal_run.execution_thread_ids.clone(),
                    )
                {
                    changed = true;
                }
                let next_current_step_owner_profile = Some(current_step_owner_profile.clone());
                if goal_run.current_step_owner_profile != next_current_step_owner_profile {
                    changed = true;
                }
                goal_run.current_step_owner_profile = next_current_step_owner_profile;
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

                    let validation_failure_reason = match self
                        .resolve_handoff_log_id_by_task_id(&task.id)
                        .await
                    {
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
                                Err(e) => Some(format!("specialist output validation error: {e}")),
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

        let snapshot = self
            .get_goal_run(goal_run_id)
            .await
            .context("goal run missing during step completion")?;
        let incomplete_todos = self.current_step_incomplete_todos(&snapshot, task).await;
        if !incomplete_todos.is_empty() {
            let detail = format!(
                "current step still has incomplete todos: {}",
                incomplete_todos
                    .iter()
                    .map(|todo| todo.content.clone())
                    .collect::<Vec<_>>()
                    .join("; ")
            );
            self.requeue_goal_step_to_pending(goal_run_id, &detail, Some(task))
                .await?;
            return Ok(());
        }
        if task.source != GOAL_VERIFICATION_SOURCE {
            if let Some((step, verification_binding, proof_checks)) =
                current_step_verification_requirements(&snapshot)
            {
                let step_summary = snapshot
                    .steps
                    .get(snapshot.current_step_index)
                    .and_then(|step| step.summary.clone());
                let implementation_summary = task
                    .result
                    .clone()
                    .or(step_summary)
                    .or_else(|| task.last_error.clone())
                    .or_else(|| task.error.clone());
                self.enqueue_goal_step_verification(
                    &snapshot,
                    &step,
                    &verification_binding,
                    &proof_checks,
                    task,
                    implementation_summary,
                )
                .await?;
                return Ok(());
            }
        }
        let Some(marker_context) = current_step_completion_marker_context(self, &snapshot) else {
            return Ok(());
        };
        if tokio::fs::metadata(&marker_context.absolute_path)
            .await
            .is_err()
        {
            self.requeue_goal_step_for_missing_completion_marker(&snapshot, task, &marker_context)
                .await?;
            return Ok(());
        }
        self.goal_step_completion_marker_retries
            .lock()
            .await
            .remove(&completion_marker_retry_key(
                &snapshot.id,
                &marker_context.step.id,
            ));

        let now = now_millis();
        let thread_summary = match task.thread_id.as_deref() {
            Some(thread_id) => self.goal_thread_summary(thread_id).await,
            None => None,
        };
        if task.source == GOAL_VERIFICATION_SOURCE {
            if let Some(record) = self.load_goal_step_review_record(&task.id).await? {
                Self::validate_goal_step_review_record(task, goal_run_id, &record)?;
                if matches!(record.verdict, GoalStepReviewVerdict::Fail) {
                    let failure_reason = structured_review_failure_reason(&record);
                    self.requeue_goal_step_to_pending(goal_run_id, &failure_reason, Some(task))
                        .await?;
                    return Ok(());
                }
            } else if self
                .goal_step_review_requires_structured_verdict(&task.id)
                .await?
            {
                self.requeue_goal_step_to_pending(
                    goal_run_id,
                    "review did not submit a structured verdict via submit_goal_step_verdict",
                    Some(task),
                )
                .await?;
                return Ok(());
            } else {
                let review_result = current_step_review_result(task, thread_summary.as_deref())
                    .unwrap_or_else(|| "review completed without a verdict".to_string());
                if !matches!(
                    parse_goal_review_verdict(&review_result),
                    Some(GoalReviewVerdict::Pass)
                ) {
                    let failure_reason = legacy_review_failure_reason(review_result);
                    self.requeue_goal_step_to_pending(goal_run_id, &failure_reason, Some(task))
                        .await?;
                    return Ok(());
                }
            }
        }
        let updated = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                anyhow::bail!("goal run missing after task completion");
            };
            super::super::goal_run_apply_thread_routing(goal_run, task.thread_id.clone());
            let mut completed_step_id = None;
            let mut completed_summary = None;
            if let Some(step) = goal_run.steps.get_mut(goal_run.current_step_index) {
                step.status = GoalRunStepStatus::Completed;
                step.completed_at = Some(now);
                step.summary = thread_summary
                    .clone()
                    .or_else(|| Some("step completed".into()));
                completed_step_id = Some(step.id.clone());
                completed_summary = step.summary.clone();
            }
            goal_run.current_step_index = goal_run.current_step_index.saturating_add(1);
            let next_step = goal_run.steps.get(goal_run.current_step_index);
            goal_run.current_step_title = next_step.map(|step| step.title.clone());
            goal_run.current_step_kind = next_step.map(|step| step.kind.clone());
            goal_run.current_step_owner_profile = None;
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
            if let Some(step_id) = completed_step_id.as_deref() {
                if task.source == GOAL_VERIFICATION_SOURCE {
                    super::goal_dossier::set_goal_unit_verification_state(
                        goal_run,
                        step_id,
                        GoalProjectionState::Completed,
                        completed_summary.unwrap_or_else(|| "verification completed".to_string()),
                        Vec::new(),
                        Some("verification result"),
                        task.result.clone().or(thread_summary.clone()),
                    );
                } else {
                    super::goal_dossier::set_goal_unit_report(
                        goal_run,
                        step_id,
                        GoalProjectionState::Completed,
                        completed_summary.unwrap_or_else(|| "step completed".to_string()),
                        Vec::new(),
                    );
                }
            }
            super::goal_dossier::set_goal_resume_decision(
                goal_run,
                GoalResumeAction::Advance,
                "step_completed",
                Some("goal step completed successfully".to_string()),
                Vec::new(),
            );
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
        let step_id = {
            let goal_runs = self.goal_runs.lock().await;
            goal_runs
                .iter()
                .find(|item| item.id == goal_run_id)
                .and_then(|goal_run| goal_run.steps.get(goal_run.current_step_index))
                .map(|step| step.id.clone())
        };
        if let Some(step_id) = step_id {
            self.goal_step_completion_marker_retries
                .lock()
                .await
                .remove(&completion_marker_retry_key(goal_run_id, &step_id));
        }
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
            let planner_owner_profile = self.planner_owner_profile().await;
            {
                let mut goal_runs = self.goal_runs.lock().await;
                if let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) {
                    if goal_run.planner_owner_profile.as_ref() != Some(&planner_owner_profile) {
                        goal_run.planner_owner_profile = Some(planner_owner_profile.clone());
                        goal_run.updated_at = now_millis();
                    }
                }
            }
            self.persist_goal_runs().await;

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
                super::super::goal_run_apply_thread_routing(goal_run, task.thread_id.clone());
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
                goal_run.planner_owner_profile = Some(planner_owner_profile.clone());
                goal_run.current_step_owner_profile = None;
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
                if insert_at > 0 {
                    let failed_step_id = goal_run.steps[insert_at - 1].id.clone();
                    if task.source == GOAL_VERIFICATION_SOURCE {
                        super::goal_dossier::set_goal_unit_verification_state(
                            goal_run,
                            &failed_step_id,
                            GoalProjectionState::Failed,
                            failure.clone(),
                            vec!["verification failed and triggered a replan".to_string()],
                            Some("verification failure"),
                            Some(failure.clone()),
                        );
                    } else {
                        super::goal_dossier::set_goal_unit_report(
                            goal_run,
                            &failed_step_id,
                            GoalProjectionState::Failed,
                            failure.clone(),
                            vec!["step failed and triggered a replan".to_string()],
                        );
                    }
                }
                super::goal_dossier::set_goal_resume_decision(
                    goal_run,
                    GoalResumeAction::Replan,
                    "step_failed_replan",
                    Some(failure.clone()),
                    vec![format!("replan_count={}", goal_run.replan_count)],
                );
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

        if task.source == GOAL_VERIFICATION_SOURCE {
            let mut goal_runs = self.goal_runs.lock().await;
            if let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) {
                if let Some(step_id) = goal_run
                    .steps
                    .get(goal_run.current_step_index)
                    .map(|step| step.id.clone())
                {
                    super::goal_dossier::set_goal_unit_verification_state(
                        goal_run,
                        &step_id,
                        GoalProjectionState::Failed,
                        failure.clone(),
                        vec!["verification failed".to_string()],
                        Some("verification failure"),
                        Some(failure.clone()),
                    );
                }
            }
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
        self.fail_goal_run(goal_run_id, &failure, "execution", task.thread_id.clone())
            .await;
        Ok(())
    }
}
