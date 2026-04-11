use super::*;

impl AgentEngine {
    pub async fn add_task(
        &self,
        title: String,
        description: String,
        priority: &str,
        command: Option<String>,
        session_id: Option<String>,
        dependencies: Vec<String>,
    ) -> String {
        self.enqueue_task(
            title,
            description,
            priority,
            command,
            session_id,
            dependencies,
            None,
            "user",
            None,
            None,
            None,
            None,
        )
        .await
        .id
    }

    pub async fn enqueue_task(
        &self,
        title: String,
        description: String,
        priority: &str,
        command: Option<String>,
        session_id: Option<String>,
        dependencies: Vec<String>,
        scheduled_at: Option<u64>,
        source: &str,
        goal_run_id: Option<String>,
        parent_task_id: Option<String>,
        parent_thread_id: Option<String>,
        runtime: Option<String>,
    ) -> AgentTask {
        let id = format!("task_{}", Uuid::new_v4());
        let now = now_millis();
        let initial_schedule_reason = scheduled_at
            .filter(|deadline| *deadline > now)
            .map(describe_scheduled_time);
        let task = AgentTask {
            id: id.clone(),
            title,
            description,
            status: if initial_schedule_reason.is_some() {
                TaskStatus::Blocked
            } else {
                TaskStatus::Queued
            },
            priority: parse_priority_str(priority),
            progress: 0,
            created_at: now,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: None,
            source: source.into(),
            notify_on_complete: true,
            notify_channels: vec!["in-app".into()],
            dependencies,
            command,
            session_id,
            goal_run_id,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id,
            parent_thread_id,
            runtime: runtime.unwrap_or_else(|| "daemon".to_string()),
            retry_count: 0,
            max_retries: self.config.read().await.max_retries.max(1),
            next_retry_at: None,
            scheduled_at,
            blocked_reason: initial_schedule_reason.clone(),
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: None,
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
            logs: vec![make_task_log_entry(
                0,
                TaskLogLevel::Info,
                "queue",
                if initial_schedule_reason.is_some() {
                    "task scheduled"
                } else {
                    "task enqueued"
                },
                initial_schedule_reason,
            )],
        };

        self.tasks.lock().await.push_back(task);
        self.persist_tasks().await;

        let task = self
            .tasks
            .lock()
            .await
            .iter()
            .find(|task| task.id == id)
            .cloned()
            .expect("enqueued task missing from queue");
        self.emit_task_update(&task, Some(status_message(&task).into()));

        task
    }

    pub async fn cancel_task(&self, task_id: &str) -> bool {
        let mut tasks = self.tasks.lock().await;
        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
            if matches!(
                task.status,
                TaskStatus::Queued
                    | TaskStatus::InProgress
                    | TaskStatus::Blocked
                    | TaskStatus::FailedAnalyzing
                    | TaskStatus::AwaitingApproval
            ) {
                let thread_to_stop = task.thread_id.clone();
                let session_to_interrupt = task.session_id.clone();
                task.status = TaskStatus::Cancelled;
                task.completed_at = Some(now_millis());
                task.lane_id = None;
                task.blocked_reason = None;
                task.awaiting_approval_id = None;
                task.logs.push(make_task_log_entry(
                    task.retry_count,
                    TaskLogLevel::Warn,
                    "queue",
                    "task cancelled by user",
                    None,
                ));
                let updated = task.clone();
                drop(tasks);
                self.persist_tasks().await;
                if let Some(thread_id) = thread_to_stop {
                    let _ = self.stop_stream(&thread_id).await;
                }
                if let Some(session_id) =
                    session_to_interrupt.and_then(|value| Uuid::parse_str(&value).ok())
                {
                    let _ = self.session_manager.write_input(session_id, &[3]).await;
                }
                self.emit_task_update(&updated, Some("Cancelled by user".into()));
                self.settle_task_skill_consultations(&updated, "cancelled")
                    .await;
                self.record_collaboration_outcome(&updated, "cancelled")
                    .await;
                self.record_provenance_event(
                    "step_cancelled",
                    "task cancelled by operator",
                    serde_json::json!({
                        "task_id": updated.id,
                        "title": updated.title,
                        "source": updated.source,
                    }),
                    updated.goal_run_id.as_deref(),
                    Some(updated.id.as_str()),
                    updated.thread_id.as_deref(),
                    None,
                    None,
                )
                .await;
                return true;
            }
        }
        false
    }

    pub async fn handle_task_approval_resolution(
        &self,
        approval_id: &str,
        decision: amux_protocol::ApprovalDecision,
    ) -> bool {
        let handoff_task = {
            let tasks = self.tasks.lock().await;
            tasks
                .iter()
                .find(|task| {
                    task.awaiting_approval_id.as_deref() == Some(approval_id)
                        && task.source == "thread_handoff"
                })
                .cloned()
        };
        if let Some(task) = handoff_task {
            let handoff_task_id = task.id.clone();
            let request = task.command.as_deref().and_then(|value| {
                serde_json::from_str::<PendingThreadHandoffActivation>(value).ok()
            });
            let Some(request) = request else {
                return false;
            };

            match decision {
                amux_protocol::ApprovalDecision::ApproveOnce
                | amux_protocol::ApprovalDecision::ApproveSession => {
                    let activation = self
                        .apply_thread_handoff_activation(&request, Some(approval_id.to_string()))
                        .await;
                    let updated = {
                        let mut tasks = self.tasks.lock().await;
                        let Some(task) = tasks.iter_mut().find(|task| task.id == handoff_task_id)
                        else {
                            return false;
                        };
                        task.awaiting_approval_id = None;
                        task.blocked_reason = None;
                        task.started_at = None;
                        task.completed_at = Some(now_millis());
                        match activation {
                            Ok(event) => {
                                task.status = TaskStatus::Completed;
                                task.progress = 100;
                                task.result = Some(format!(
                                    "handoff activated: {} -> {}",
                                    canonical_agent_name(&event.from_agent_id),
                                    canonical_agent_name(&event.to_agent_id)
                                ));
                                task.error = None;
                                task.last_error = None;
                                task.logs.push(make_task_log_entry(
                                    task.retry_count,
                                    TaskLogLevel::Info,
                                    "approval",
                                    "operator approved thread handoff; activation completed",
                                    None,
                                ));
                            }
                            Err(error) => {
                                let message = error.to_string();
                                task.status = TaskStatus::Failed;
                                task.error = Some(message.clone());
                                task.last_error = Some(message.clone());
                                task.blocked_reason = Some(message.clone());
                                task.logs.push(make_task_log_entry(
                                    task.retry_count,
                                    TaskLogLevel::Error,
                                    "approval",
                                    "operator approved thread handoff but activation failed",
                                    Some(message),
                                ));
                            }
                        }
                        task.clone()
                    };
                    self.persist_tasks().await;
                    self.emit_task_update(&updated, Some(status_message(&updated).into()));
                    self.record_provenance_event(
                        "approval_granted",
                        "operator approved thread handoff",
                        serde_json::json!({
                            "approval_id": approval_id,
                            "task_id": updated.id,
                            "title": updated.title,
                            "decision": format!("{decision:?}").to_lowercase(),
                            "source": updated.source,
                        }),
                        updated.goal_run_id.as_deref(),
                        Some(updated.id.as_str()),
                        updated.thread_id.as_deref(),
                        Some(approval_id),
                        None,
                    )
                    .await;
                    return true;
                }
                amux_protocol::ApprovalDecision::Deny => {
                    self.clear_pending_thread_handoff_approval(&request.thread_id, approval_id)
                        .await;
                    let updated = {
                        let mut tasks = self.tasks.lock().await;
                        let Some(task) = tasks.iter_mut().find(|task| task.id == handoff_task_id)
                        else {
                            return false;
                        };
                        let reason = "operator denied thread handoff approval".to_string();
                        task.status = TaskStatus::Failed;
                        task.started_at = None;
                        task.completed_at = Some(now_millis());
                        task.awaiting_approval_id = None;
                        task.blocked_reason = Some(reason.clone());
                        task.error = Some(reason.clone());
                        task.last_error = Some(reason.clone());
                        task.logs.push(make_task_log_entry(
                            task.retry_count,
                            TaskLogLevel::Error,
                            "approval",
                            "operator denied thread handoff; task failed",
                            Some(reason),
                        ));
                        task.clone()
                    };
                    self.persist_tasks().await;
                    self.emit_task_update(&updated, Some(status_message(&updated).into()));
                    self.record_provenance_event(
                        "approval_denied",
                        "operator denied thread handoff",
                        serde_json::json!({
                            "approval_id": approval_id,
                            "task_id": updated.id,
                            "title": updated.title,
                            "decision": format!("{decision:?}").to_lowercase(),
                            "source": updated.source,
                        }),
                        updated.goal_run_id.as_deref(),
                        Some(updated.id.as_str()),
                        updated.thread_id.as_deref(),
                        Some(approval_id),
                        None,
                    )
                    .await;
                    let correction_desc = format!("Denied approval for task: {}", updated.title);
                    let thread_id = updated.thread_id.clone().unwrap_or_default();
                    self.update_counter_who_on_correction(&thread_id, &correction_desc)
                        .await;
                    return true;
                }
            }
        }

        let updated = {
            let mut tasks = self.tasks.lock().await;
            let Some(task) = tasks
                .iter_mut()
                .find(|task| task.awaiting_approval_id.as_deref() == Some(approval_id))
            else {
                return false;
            };

            match decision {
                amux_protocol::ApprovalDecision::ApproveOnce
                | amux_protocol::ApprovalDecision::ApproveSession => {
                    task.status = TaskStatus::Queued;
                    task.started_at = None;
                    task.awaiting_approval_id = None;
                    task.blocked_reason = None;
                    task.logs.push(make_task_log_entry(
                        task.retry_count,
                        TaskLogLevel::Info,
                        "approval",
                        "operator approved managed command; task re-queued",
                        None,
                    ));
                }
                amux_protocol::ApprovalDecision::Deny => {
                    let reason = "operator denied managed command approval".to_string();
                    task.status = TaskStatus::Failed;
                    task.started_at = None;
                    task.completed_at = Some(now_millis());
                    task.awaiting_approval_id = None;
                    task.blocked_reason = Some(reason.clone());
                    task.error = Some(reason.clone());
                    task.last_error = Some(reason.clone());
                    task.logs.push(make_task_log_entry(
                        task.retry_count,
                        TaskLogLevel::Error,
                        "approval",
                        "operator denied managed command; task failed",
                        Some(reason),
                    ));
                }
            }

            task.clone()
        };

        self.persist_tasks().await;
        if matches!(
            decision,
            amux_protocol::ApprovalDecision::ApproveOnce
                | amux_protocol::ApprovalDecision::ApproveSession
        ) {
            if let Some(thread_id) = updated.thread_id.as_deref() {
                let _ = self
                    .record_thread_skill_approval_resolution(thread_id, approval_id)
                    .await;
            }
        } else if let Some(thread_id) = updated.thread_id.as_deref() {
            let _ = self
                .record_thread_skill_approval_denial(thread_id, approval_id)
                .await;
        }
        self.emit_task_update(&updated, Some(status_message(&updated).into()));
        self.record_provenance_event(
            match decision {
                amux_protocol::ApprovalDecision::ApproveOnce
                | amux_protocol::ApprovalDecision::ApproveSession => "approval_granted",
                amux_protocol::ApprovalDecision::Deny => "approval_denied",
            },
            match decision {
                amux_protocol::ApprovalDecision::ApproveOnce
                | amux_protocol::ApprovalDecision::ApproveSession => {
                    "operator approved managed command"
                }
                amux_protocol::ApprovalDecision::Deny => "operator denied managed command",
            },
            serde_json::json!({
                "approval_id": approval_id,
                "task_id": updated.id,
                "title": updated.title,
                "decision": format!("{decision:?}").to_lowercase(),
            }),
            updated.goal_run_id.as_deref(),
            Some(updated.id.as_str()),
            updated.thread_id.as_deref(),
            Some(approval_id),
            None,
        )
        .await;

        if matches!(decision, amux_protocol::ApprovalDecision::Deny) {
            let correction_desc = format!("Denied approval for task: {}", updated.title);
            let thread_id = updated.thread_id.clone().unwrap_or_default();
            self.update_counter_who_on_correction(&thread_id, &correction_desc)
                .await;
        }

        true
    }

    pub(super) async fn snapshot_tasks(&self) -> Vec<AgentTask> {
        let sessions = self.session_manager.list().await;
        let config = self.config.read().await.clone();
        let mut tasks = self.tasks.lock().await;
        let changed = refresh_task_queue_state(&mut tasks, now_millis(), &sessions, &config);
        let snapshot = tasks
            .iter()
            .cloned()
            .map(|mut task| {
                crate::agent::persistence::sanitize_task_for_external_view(&mut task);
                task
            })
            .collect();
        drop(tasks);

        if !changed.is_empty() {
            self.persist_tasks().await;
            for task in changed {
                self.emit_task_update(&task, Some(status_message(&task).into()));
            }
        }

        snapshot
    }

    pub async fn list_tasks(&self) -> Vec<AgentTask> {
        self.snapshot_tasks().await
    }

    pub async fn list_runs(&self) -> Vec<AgentRun> {
        let tasks = self.snapshot_tasks().await;
        let sessions = self.session_manager.list().await;
        let mut runs = project_task_runs(&tasks, &sessions);
        runs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        runs
    }

    pub async fn get_run(&self, run_id: &str) -> Option<AgentRun> {
        let tasks = self.snapshot_tasks().await;
        let sessions = self.session_manager.list().await;
        project_task_runs(&tasks, &sessions)
            .into_iter()
            .find(|run| run.id == run_id)
    }
}
