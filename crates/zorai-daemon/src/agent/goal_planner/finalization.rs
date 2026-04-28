use super::*;

fn all_goal_steps_completed(goal_run: &GoalRun) -> bool {
    !goal_run.steps.is_empty()
        && goal_run
            .steps
            .iter()
            .all(|step| step.status == GoalRunStepStatus::Completed)
}

fn is_incomplete_goal_step_todo(todo: &TodoItem, goal_run: &GoalRun) -> bool {
    todo.status != TodoStatus::Completed
        && todo
            .step_index
            .is_some_and(|step_index| step_index < goal_run.steps.len())
}

fn final_review_prompt(goal_run: &GoalRun) -> String {
    let planned = goal_run
        .plan_summary
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(goal_run.goal.as_str());
    let completed_steps = goal_run
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| {
            format!(
                "- Step {}: {}\n  Planned: {}\n  Done: {}",
                index + 1,
                step.title,
                step.success_criteria,
                step.summary
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("no completion summary recorded")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Perform the final review for goal `{}`.\n\n\
         Return the first line exactly as `VERDICT: PASS` or `VERDICT: FAIL`.\n\
         PASS only when all planned steps are complete, all goal todos are complete, every step has a completion markdown artifact, and the delivered work matches the plan.\n\
         FAIL when any step is incomplete, any todo remains unchecked, a completion marker is missing, or the delivered work does not satisfy the plan.\n\n\
         Goal:\n{}\n\n\
         Planned summary:\n{}\n\n\
         Completed steps:\n{}",
        goal_run.title, goal_run.goal, planned, completed_steps
    )
}

fn is_active_goal_task_status(status: TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::Queued
            | TaskStatus::InProgress
            | TaskStatus::Blocked
            | TaskStatus::FailedAnalyzing
            | TaskStatus::AwaitingApproval
    )
}

fn final_summary_markdown(goal_run: &GoalRun, reflection_summary: &str) -> String {
    let planned = goal_run
        .plan_summary
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(goal_run.goal.as_str());
    let mut body = vec![
        "# Final Goal Summary".to_string(),
        String::new(),
        format!("- Goal run: {}", goal_run.id),
        format!("- Title: {}", goal_run.title),
        String::new(),
        "## Planned".to_string(),
        planned.to_string(),
        String::new(),
        "## Done".to_string(),
    ];
    for (index, step) in goal_run.steps.iter().enumerate() {
        body.push(format!("### Step {}: {}", index + 1, step.title));
        body.push(format!("Planned: {}", step.success_criteria));
        body.push(format!(
            "Done: {}",
            step.summary
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("no completion summary recorded")
        ));
        body.push(String::new());
    }
    body.push("## Reflection".to_string());
    body.push(reflection_summary.to_string());
    body.push(String::new());
    body.join("\n")
}

async fn persist_reflection_skill_activation_note(
    data_dir: &std::path::Path,
    goal_run: &GoalRun,
    activated_skill: &str,
) -> Result<std::path::PathBuf> {
    let execution_dir =
        crate::agent::goal_dossier::goal_inventory_execution_dir(data_dir, &goal_run.id);
    tokio::fs::create_dir_all(&execution_dir).await?;

    let note_path = execution_dir.join("reflection-skill-activation.md");
    let mut body = format!(
        "# Reflected Skill Activation\n\n- Goal run: {}\n- Activated skill: {}\n- Recorded at: {}\n",
        goal_run.id,
        activated_skill,
        now_millis(),
    );
    if let Some(path) = goal_run.generated_skill_path.as_deref() {
        body.push_str(&format!("- Generated skill artifact: {path}\n"));
    }
    if let Some(summary) = goal_run.reflection_summary.as_deref() {
        body.push_str("\n## Reflection Summary\n");
        body.push_str(summary);
        body.push('\n');
    }

    tokio::fs::write(&note_path, body).await?;
    Ok(note_path)
}

impl AgentEngine {
    async fn active_goal_tasks(&self, goal_run_id: &str) -> Vec<AgentTask> {
        let tasks = self.tasks.lock().await;
        tasks
            .iter()
            .filter(|task| task.goal_run_id.as_deref() == Some(goal_run_id))
            .filter(|task| is_active_goal_task_status(task.status))
            .cloned()
            .collect()
    }

    async fn resume_existing_goal_final_review(
        &self,
        snapshot: &GoalRun,
        review_task: &AgentTask,
    ) -> Result<()> {
        let owner_profile = Some(
            self.goal_owner_profile_for_task_target(review_task, None)
                .await,
        );
        let updated_goal = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == snapshot.id) else {
                anyhow::bail!("goal run missing while resuming final review");
            };
            let already_active =
                goal_run.active_task_id.as_deref() == Some(review_task.id.as_str());
            goal_run.status = GoalRunStatus::Running;
            goal_run.updated_at = now_millis();
            goal_run.awaiting_approval_id = review_task.awaiting_approval_id.clone();
            goal_run.active_task_id = Some(review_task.id.clone());
            goal_run.current_step_owner_profile = owner_profile;
            if !already_active {
                goal_run.events.push(make_goal_run_event(
                    "final_review",
                    "final review already queued",
                    Some(review_task.id.clone()),
                ));
            }
            goal_run.clone()
        };

        self.persist_goal_runs().await;
        self.emit_goal_run_update(&updated_goal, Some("Final review already queued".into()));
        Ok(())
    }

    async fn enqueue_goal_final_review(&self, snapshot: &GoalRun) -> Result<()> {
        let active_tasks = self.active_goal_tasks(&snapshot.id).await;
        if let Some(review_task) = active_tasks
            .iter()
            .find(|task| task.source == GOAL_FINAL_REVIEW_SOURCE)
        {
            self.resume_existing_goal_final_review(snapshot, review_task)
                .await?;
            return Ok(());
        }

        let review_description = final_review_prompt(snapshot);
        let review_task = self
            .enqueue_task(
                format!("Final review: {}", snapshot.title),
                review_description.clone(),
                task_priority_to_str(snapshot.priority),
                None,
                snapshot.session_id.clone(),
                Vec::new(),
                None,
                GOAL_FINAL_REVIEW_SOURCE,
                Some(snapshot.id.clone()),
                None,
                snapshot.thread_id.clone(),
                None,
            )
            .await;

        let synthetic_step = GoalRunStep {
            id: "final-review".to_string(),
            position: snapshot.steps.len(),
            title: "Final review".to_string(),
            instructions: review_description,
            kind: GoalRunStepKind::Reason,
            success_criteria: "Return VERDICT: PASS only when the goal is truly complete."
                .to_string(),
            session_id: snapshot.session_id.clone(),
            status: GoalRunStepStatus::InProgress,
            task_id: Some(review_task.id.clone()),
            summary: None,
            error: None,
            started_at: None,
            completed_at: None,
        };
        let review_binding = default_verification_binding();
        let resolved_target = self
            .resolve_goal_target_for_binding(snapshot, &synthetic_step, &review_binding)
            .await;
        let updated_task = self
            .apply_goal_resolved_target_to_task(review_task.id.as_str(), resolved_target.as_ref())
            .await
            .unwrap_or(review_task);
        let owner_profile = Some(
            self.goal_owner_profile_for_task_target(&updated_task, resolved_target.as_ref())
                .await,
        );

        let updated_goal = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == snapshot.id) else {
                anyhow::bail!("goal run missing while queuing final review");
            };
            goal_run.status = GoalRunStatus::Running;
            goal_run.updated_at = now_millis();
            goal_run.awaiting_approval_id = None;
            goal_run.active_task_id = Some(updated_task.id.clone());
            goal_run.current_step_owner_profile = owner_profile;
            goal_run.events.push(make_goal_run_event(
                "final_review",
                "final review queued",
                Some(updated_task.id.clone()),
            ));
            goal_run.clone()
        };

        self.persist_tasks().await;
        self.persist_goal_runs().await;
        self.emit_task_update(&updated_task, Some(status_message(&updated_task).into()));
        self.emit_goal_run_update(&updated_goal, Some("Final review queued".into()));
        Ok(())
    }

    pub(in crate::agent) async fn handle_goal_run_final_review_completion(
        &self,
        goal_run_id: &str,
        task: &AgentTask,
    ) -> Result<()> {
        let snapshot = self
            .get_goal_run(goal_run_id)
            .await
            .context("goal run missing during final review completion")?;
        let review_thread_output = match task.thread_id.as_deref().or(snapshot.thread_id.as_deref())
        {
            Some(thread_id) => self.goal_thread_latest_assistant_content(thread_id).await,
            None => None,
        };
        let review_output = task
            .result
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| review_thread_output.clone())
            .or_else(|| {
                task.last_error
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "final review completed without a structured verdict".to_string());
        if !matches!(
            parse_goal_review_verdict(&review_output),
            Some(GoalReviewVerdict::Pass)
        ) {
            self.handle_goal_run_final_review_failure(goal_run_id, task)
                .await?;
            return Ok(());
        }

        let marker_path =
            crate::agent::goal_dossier::goal_final_review_marker_path(&self.data_dir, goal_run_id);
        if let Some(parent) = marker_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&marker_path, format!("{review_output}\n")).await?;

        let updated = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                anyhow::bail!("goal run missing while recording final review");
            };
            goal_run.status = GoalRunStatus::Running;
            goal_run.updated_at = now_millis();
            goal_run.awaiting_approval_id = None;
            goal_run.active_task_id = None;
            goal_run.current_step_owner_profile = None;
            goal_run.last_error = None;
            goal_run.failure_cause = None;
            goal_run.events.push(make_goal_run_event(
                "final_review",
                "final review passed",
                Some(review_output.clone()),
            ));
            goal_run.clone()
        };

        self.persist_goal_runs().await;
        if let Some(thread_id) = snapshot.thread_id.as_deref() {
            self.record_file_work_context(
                thread_id,
                None,
                "goal_final_review",
                &marker_path.to_string_lossy(),
            )
            .await;
        }
        self.emit_goal_run_update(&updated, Some("Final review passed".into()));
        self.complete_goal_run(goal_run_id).await
    }

    pub(in crate::agent) async fn handle_goal_run_final_review_failure(
        &self,
        goal_run_id: &str,
        task: &AgentTask,
    ) -> Result<()> {
        let failure = task
            .last_error
            .clone()
            .or_else(|| task.error.clone())
            .or_else(|| task.result.clone())
            .unwrap_or_else(|| "final review did not pass".to_string());
        self.fail_goal_run(
            goal_run_id,
            &failure,
            "final_review",
            task.thread_id.clone(),
        )
        .await;
        Ok(())
    }

    pub(in crate::agent) async fn complete_goal_run(&self, goal_run_id: &str) -> Result<()> {
        let snapshot = self
            .get_goal_run(goal_run_id)
            .await
            .context("goal run missing during completion")?;
        let active_tasks = self.active_goal_tasks(goal_run_id).await;
        if !active_tasks.is_empty() {
            if active_tasks
                .iter()
                .all(|task| task.source == GOAL_FINAL_REVIEW_SOURCE)
            {
                self.resume_existing_goal_final_review(&snapshot, &active_tasks[0])
                    .await?;
                return Ok(());
            }
            anyhow::bail!("goal run still has active child work");
        }
        if !all_goal_steps_completed(&snapshot) {
            anyhow::bail!("goal run completion blocked: not all steps are completed");
        }
        for step_index in 0..snapshot.steps.len() {
            let marker_path = crate::agent::goal_dossier::goal_step_completion_marker_path(
                &self.data_dir,
                goal_run_id,
                step_index,
            );
            if tokio::fs::metadata(&marker_path).await.is_err() {
                anyhow::bail!(
                    "goal run completion blocked: missing step completion marker {}",
                    marker_path.display()
                );
            }
        }
        if let Some(thread_id) = snapshot.thread_id.as_deref() {
            let incomplete_todos = self
                .get_todos(thread_id)
                .await
                .into_iter()
                .filter(|item| is_incomplete_goal_step_todo(item, &snapshot))
                .collect::<Vec<_>>();
            if !incomplete_todos.is_empty() {
                anyhow::bail!(
                    "goal run completion blocked: incomplete todos remain: {}",
                    incomplete_todos
                        .into_iter()
                        .map(|todo| todo.content)
                        .collect::<Vec<_>>()
                        .join("; ")
                );
            }
        }
        let final_review_marker =
            crate::agent::goal_dossier::goal_final_review_marker_path(&self.data_dir, goal_run_id);
        if tokio::fs::metadata(&final_review_marker).await.is_err() {
            self.enqueue_goal_final_review(&snapshot).await?;
            return Ok(());
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
        let activated_skill = reflection
            .activate_skill
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let final_summary_path =
            crate::agent::goal_dossier::goal_final_summary_path(&self.data_dir, goal_run_id);
        if let Some(parent) = final_summary_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(
            &final_summary_path,
            final_summary_markdown(&snapshot, &reflection.summary),
        )
        .await?;

        let cost_summary = {
            let trackers = self.cost_trackers.lock().await;
            trackers.get(goal_run_id).map(|t| t.summary().clone())
        };
        let model_usage = {
            let trackers = self.cost_trackers.lock().await;
            trackers
                .get(goal_run_id)
                .map(|t| t.model_usage())
                .unwrap_or_default()
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
            if let Some(path) = generated_skill_path.clone() {
                goal_run.generated_skill_path = Some(path);
            }
            if let Some(ref summary) = cost_summary {
                goal_run.total_prompt_tokens = summary.total_prompt_tokens;
                goal_run.total_completion_tokens = summary.total_completion_tokens;
                goal_run.estimated_cost_usd = summary.estimated_cost_usd;
            }
            if !model_usage.is_empty() {
                goal_run.model_usage = model_usage;
            }
            goal_run.duration_ms = goal_run
                .started_at
                .zip(goal_run.completed_at)
                .map(|(started_at, completed_at)| completed_at.saturating_sub(started_at));
            super::goal_dossier::set_goal_report(
                goal_run,
                GoalProjectionState::Completed,
                reflection.summary.clone(),
                Vec::new(),
            );
            goal_run.authorship_tag = Some(super::authorship::classify_authorship(true, true));
            goal_run.events.push(make_goal_run_event(
                "reflection",
                "goal run completed",
                goal_run.reflection_summary.clone(),
            ));
            if let Some(skill) = activated_skill.as_deref() {
                goal_run.events.push(make_goal_run_event(
                    "reflection_skill_activation",
                    "goal reflection activated a skill",
                    Some(skill.to_string()),
                ));
            }
            goal_run.clone()
        };

        self.persist_goal_runs().await;
        self.record_generated_skill_work_context(&updated).await;
        if let Some(thread_id) = updated.thread_id.as_deref() {
            self.record_file_work_context(
                thread_id,
                None,
                "goal_final_summary",
                &final_summary_path.to_string_lossy(),
            )
            .await;
        }
        if let Some(skill) = activated_skill.as_deref() {
            if let Some(thread_id) = updated.thread_id.as_deref() {
                if let Some(state) = super::skill_preflight::build_reflection_skill_activation_state(
                    &updated.goal,
                    skill,
                ) {
                    self.set_thread_skill_discovery_state(thread_id, state.clone())
                        .await;
                    self.emit_workflow_notice(
                        thread_id,
                        "goal_reflection_skill_activation",
                        format!("Goal reflection activated skill: {skill}"),
                        serde_json::to_string(&state).ok(),
                    );
                }
            }
            match persist_reflection_skill_activation_note(&self.data_dir, &updated, skill).await {
                Ok(path) => {
                    if let Some(thread_id) = updated.thread_id.as_deref() {
                        self.record_file_work_context(
                            thread_id,
                            None,
                            "goal_reflection_skill_activation",
                            &path.to_string_lossy(),
                        )
                        .await;
                    }
                }
                Err(error) => {
                    tracing::warn!(goal_run_id, skill, error = %error, "failed to persist reflected skill activation note");
                }
            }
        }
        self.settle_goal_skill_consultations(&updated, "success")
            .await;
        self.settle_goal_plan_causal_traces(&updated.id, "success", None)
            .await;
        self.emit_goal_run_update(&updated, Some("Goal completed".into()));
        self.record_provenance_event(
            "goal_completed",
            "goal run completed",
            serde_json::json!({
                "goal_run_id": updated.id,
                "reflection_summary": updated.reflection_summary,
                "generated_skill_path": updated.generated_skill_path,
                "activated_skill": activated_skill,
                "memory_updates": updated.memory_updates,
            }),
            Some(updated.id.as_str()),
            None,
            updated.thread_id.as_deref(),
            None,
            None,
        )
        .await;

        if let Err(e) = self
            .record_goal_episode(&updated, super::episodic::EpisodeOutcome::Success)
            .await
        {
            tracing::warn!(
                "Failed to record episodic memory for completed goal {}: {e}",
                goal_run_id
            );
        }

        {
            let predicted_band = updated.steps.first().and_then(|step| {
                if step.title.starts_with("[HIGH]") {
                    Some(crate::agent::explanation::ConfidenceBand::Confident)
                } else if step.title.starts_with("[MEDIUM]") {
                    Some(crate::agent::explanation::ConfidenceBand::Likely)
                } else if step.title.starts_with("[LOW]") {
                    Some(crate::agent::explanation::ConfidenceBand::Uncertain)
                } else {
                    None
                }
            });
            if let Some(band) = predicted_band {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                self.calibration_tracker
                    .write()
                    .await
                    .record_observation(band, true, now_ms);
                tracing::debug!(goal_run_id, "calibration: recorded successful observation");
            }
        }

        self.cost_trackers.lock().await.remove(goal_run_id);

        Ok(())
    }

    pub(in crate::agent) async fn fail_goal_run(
        &self,
        goal_run_id: &str,
        error: &str,
        phase: &str,
        thread_id: Option<String>,
    ) {
        let cost_summary = {
            let trackers = self.cost_trackers.lock().await;
            trackers.get(goal_run_id).map(|t| t.summary().clone())
        };
        let model_usage = {
            let trackers = self.cost_trackers.lock().await;
            trackers
                .get(goal_run_id)
                .map(|t| t.model_usage())
                .unwrap_or_default()
        };

        let mut maybe_updated = None;
        {
            let mut goal_runs = self.goal_runs.lock().await;
            if let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) {
                super::super::goal_run_apply_thread_routing(goal_run, thread_id);
                goal_run.status = GoalRunStatus::Failed;
                goal_run.completed_at = Some(now_millis());
                goal_run.updated_at = now_millis();
                goal_run.last_error = Some(error.to_string());
                goal_run.failure_cause = Some(error.to_string());
                goal_run.awaiting_approval_id = None;
                goal_run.active_task_id = None;
                if let Some(ref summary) = cost_summary {
                    goal_run.total_prompt_tokens = summary.total_prompt_tokens;
                    goal_run.total_completion_tokens = summary.total_completion_tokens;
                    goal_run.estimated_cost_usd = summary.estimated_cost_usd;
                }
                if !model_usage.is_empty() {
                    goal_run.model_usage = model_usage;
                }
                goal_run.duration_ms = goal_run
                    .started_at
                    .zip(goal_run.completed_at)
                    .map(|(started_at, completed_at)| completed_at.saturating_sub(started_at));
                if let Some(step_id) = goal_run
                    .steps
                    .get(goal_run.current_step_index)
                    .map(|step| step.id.clone())
                {
                    super::goal_dossier::set_goal_unit_report(
                        goal_run,
                        &step_id,
                        GoalProjectionState::Failed,
                        error.to_string(),
                        vec![format!("failure phase: {phase}")],
                    );
                }
                super::goal_dossier::set_goal_report(
                    goal_run,
                    GoalProjectionState::Failed,
                    error.to_string(),
                    vec![format!("failure phase: {phase}")],
                );
                super::goal_dossier::set_goal_resume_decision(
                    goal_run,
                    GoalResumeAction::Stop,
                    "goal_failed",
                    Some(error.to_string()),
                    vec![format!("failure phase: {phase}")],
                );
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
            self.settle_goal_plan_causal_traces(&updated.id, "failure", Some(error))
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

            if let Err(e) = self
                .record_goal_episode(&updated, super::episodic::EpisodeOutcome::Failure)
                .await
            {
                tracing::warn!(
                    "Failed to record episodic memory for failed goal {}: {e}",
                    goal_run_id
                );
            }

            {
                let predicted_band = updated.steps.first().and_then(|step| {
                    if step.title.starts_with("[HIGH]") {
                        Some(crate::agent::explanation::ConfidenceBand::Confident)
                    } else if step.title.starts_with("[MEDIUM]") {
                        Some(crate::agent::explanation::ConfidenceBand::Likely)
                    } else if step.title.starts_with("[LOW]") {
                        Some(crate::agent::explanation::ConfidenceBand::Uncertain)
                    } else {
                        None
                    }
                });
                if let Some(band) = predicted_band {
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    self.calibration_tracker
                        .write()
                        .await
                        .record_observation(band, false, now_ms);
                    tracing::debug!(
                        goal_run_id = updated.id.as_str(),
                        "calibration: recorded failed observation"
                    );
                }
            }
        }

        self.cost_trackers.lock().await.remove(goal_run_id);
    }

    pub(in crate::agent) async fn requeue_goal_run_step(&self, goal_run_id: &str, reason: &str) {
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
                goal_run.current_step_owner_profile = None;
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

    pub(super) async fn auto_checkpoint(
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
