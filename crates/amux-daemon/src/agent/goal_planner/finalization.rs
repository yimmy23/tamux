use super::*;

impl AgentEngine {
    pub(in crate::agent) async fn complete_goal_run(&self, goal_run_id: &str) -> Result<()> {
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

        let cost_summary = {
            let trackers = self.cost_trackers.lock().await;
            trackers.get(goal_run_id).map(|t| t.summary().clone())
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
            if let Some(ref summary) = cost_summary {
                goal_run.total_prompt_tokens = summary.total_prompt_tokens;
                goal_run.total_completion_tokens = summary.total_completion_tokens;
                goal_run.estimated_cost_usd = summary.estimated_cost_usd;
            }
            goal_run.authorship_tag = Some(super::authorship::classify_authorship(true, true));
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
    ) {
        let cost_summary = {
            let trackers = self.cost_trackers.lock().await;
            trackers.get(goal_run_id).map(|t| t.summary().clone())
        };

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
                if let Some(ref summary) = cost_summary {
                    goal_run.total_prompt_tokens = summary.total_prompt_tokens;
                    goal_run.total_completion_tokens = summary.total_completion_tokens;
                    goal_run.estimated_cost_usd = summary.estimated_cost_usd;
                }
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
