impl DaemonClient {
    fn is_internal_agent_thread(thread_id: Option<&str>, title: Option<&str>) -> bool {
        let normalized_id = thread_id.unwrap_or_default().trim().to_ascii_lowercase();
        let normalized_title = title.unwrap_or_default().trim().to_ascii_lowercase();
        normalized_id.starts_with("dm:") || normalized_title.starts_with("internal dm")
    }

    fn is_hidden_agent_thread(thread_id: Option<&str>, title: Option<&str>) -> bool {
        let normalized_id = thread_id.unwrap_or_default().trim().to_ascii_lowercase();
        let normalized_title = title.unwrap_or_default().trim().to_ascii_lowercase();
        normalized_id.starts_with("handoff:")
            || normalized_title.starts_with("handoff ")
            || normalized_title == "weles"
            || normalized_title.starts_with("weles ")
    }

    fn parse_weles_review(event: &Value) -> Option<crate::client::WelesReviewMetaVm> {
        let review = event.get("weles_review")?;
        let verdict = review.get("verdict").and_then(Value::as_str)?.to_string();
        let reasons = review
            .get("reasons")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Some(crate::client::WelesReviewMetaVm {
            weles_reviewed: review
                .get("weles_reviewed")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            verdict,
            reasons,
            audit_id: get_string(review, "audit_id"),
            security_override_mode: get_string(review, "security_override_mode"),
        })
    }

    async fn dispatch_agent_event(event: Value, event_tx: &mpsc::Sender<ClientEvent>) {
        let Some(kind) = event.get("type").and_then(Value::as_str) else {
            return;
        };

        match kind {
            "thread_created" => {
                let title =
                    get_string(&event, "title").unwrap_or_else(|| "New Conversation".to_string());
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                let agent_name = get_string(&event, "agent_name");
                if Self::is_hidden_agent_thread(Some(thread_id.as_str()), Some(title.as_str())) {
                    return;
                }
                let _ = event_tx
                    .send(ClientEvent::ThreadCreated {
                        thread_id,
                        title,
                        agent_name,
                    })
                    .await;
            }
            "thread_reload_required" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                if Self::is_hidden_agent_thread(Some(thread_id.as_str()), None) {
                    return;
                }
                let _ = event_tx
                    .send(ClientEvent::ThreadReloadRequired { thread_id })
                    .await;
            }
            "participant_suggestion" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                if Self::is_hidden_agent_thread(Some(thread_id.as_str()), None) {
                    return;
                }
                if let Some(suggestion) = event
                    .get("suggestion")
                    .cloned()
                    .and_then(|raw| serde_json::from_value::<ThreadParticipantSuggestion>(raw).ok())
                {
                    let _ = event_tx
                        .send(ClientEvent::ParticipantSuggestion {
                            thread_id,
                            suggestion,
                        })
                        .await;
                }
            }
            "delta" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                if Self::is_hidden_agent_thread(Some(thread_id.as_str()), None) {
                    return;
                }
                let _ = event_tx
                    .send(ClientEvent::Delta {
                        thread_id,
                        content: get_string(&event, "content").unwrap_or_default(),
                    })
                    .await;
            }
            "reasoning" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                if Self::is_hidden_agent_thread(Some(thread_id.as_str()), None) {
                    return;
                }
                let _ = event_tx
                    .send(ClientEvent::Reasoning {
                        thread_id,
                        content: get_string(&event, "content").unwrap_or_default(),
                    })
                    .await;
            }
            "tool_call" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                if Self::is_hidden_agent_thread(Some(thread_id.as_str()), None) {
                    return;
                }
                let _ = event_tx
                    .send(ClientEvent::ToolCall {
                        thread_id,
                        call_id: get_string(&event, "call_id").unwrap_or_default(),
                        name: get_string(&event, "name").unwrap_or_default(),
                        arguments: get_string_lossy(&event, "arguments"),
                        weles_review: Self::parse_weles_review(&event),
                    })
                    .await;
            }
            "tool_result" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                if Self::is_hidden_agent_thread(Some(thread_id.as_str()), None) {
                    return;
                }
                let _ = event_tx
                    .send(ClientEvent::ToolResult {
                        thread_id,
                        call_id: get_string(&event, "call_id").unwrap_or_default(),
                        name: get_string(&event, "name").unwrap_or_default(),
                        content: get_string_lossy(&event, "content"),
                        is_error: event
                            .get("is_error")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        weles_review: Self::parse_weles_review(&event),
                    })
                    .await;
            }
            "done" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                if Self::is_hidden_agent_thread(Some(thread_id.as_str()), None) {
                    return;
                }
                let _ = event_tx
                    .send(ClientEvent::Done {
                        thread_id,
                        input_tokens: event
                            .get("input_tokens")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                        output_tokens: event
                            .get("output_tokens")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                        cost: event.get("cost").and_then(Value::as_f64),
                        provider: get_string(&event, "provider"),
                        model: get_string(&event, "model"),
                        tps: event.get("tps").and_then(Value::as_f64),
                        generation_ms: event.get("generation_ms").and_then(Value::as_u64),
                        reasoning: get_string(&event, "reasoning"),
                        provider_final_result_json: event
                            .get("provider_final_result")
                            .and_then(|value| serde_json::to_string(value).ok()),
                    })
                    .await;
            }
            "error" => {
                let _ = event_tx
                    .send(ClientEvent::Error(
                        get_string(&event, "message")
                            .unwrap_or_else(|| "Unknown agent error".to_string()),
                    ))
                    .await;
            }
            "workflow_notice" => {
                let _ = event_tx
                    .send(ClientEvent::WorkflowNotice {
                        thread_id: get_string(&event, "thread_id"),
                        kind: get_string(&event, "kind").unwrap_or_default(),
                        message: get_string(&event, "message").unwrap_or_default(),
                        details: get_string(&event, "details"),
                    })
                    .await;
            }
            "operator_question" => {
                let options = event
                    .get("options")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(ToOwned::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let _ = event_tx
                    .send(ClientEvent::OperatorQuestion {
                        question_id: get_string(&event, "question_id").unwrap_or_default(),
                        content: get_string(&event, "content").unwrap_or_default(),
                        options,
                        session_id: get_string(&event, "session_id"),
                        thread_id: get_string(&event, "thread_id"),
                    })
                    .await;
            }
            "operator_question_resolved" => {
                let _ = event_tx
                    .send(ClientEvent::OperatorQuestionResolved {
                        question_id: get_string(&event, "question_id").unwrap_or_default(),
                        answer: get_string(&event, "answer").unwrap_or_default(),
                    })
                    .await;
            }
            "notification_inbox_upsert" => {
                if let Some(notification) = event.get("notification").cloned().and_then(|raw| {
                    serde_json::from_value::<amux_protocol::InboxNotification>(raw).ok()
                }) {
                    let _ = event_tx
                        .send(ClientEvent::NotificationUpsert(notification))
                        .await;
                }
            }
            "weles_health_update" => {
                let _ = event_tx
                    .send(ClientEvent::WelesHealthUpdate {
                        state: get_string(&event, "state").unwrap_or_else(|| "healthy".to_string()),
                        reason: get_string(&event, "reason"),
                        checked_at: event.get("checked_at").and_then(Value::as_u64).unwrap_or(0),
                    })
                    .await;
            }
            "retry_status" => {
                let _ = event_tx
                    .send(ClientEvent::RetryStatus {
                        thread_id: get_string(&event, "thread_id").unwrap_or_default(),
                        phase: get_string(&event, "phase")
                            .unwrap_or_else(|| "retrying".to_string()),
                        attempt: event.get("attempt").and_then(Value::as_u64).unwrap_or(0) as u32,
                        max_retries: event
                            .get("max_retries")
                            .and_then(Value::as_u64)
                            .unwrap_or(0) as u32,
                        delay_ms: event.get("delay_ms").and_then(Value::as_u64).unwrap_or(0),
                        failure_class: get_string(&event, "failure_class")
                            .unwrap_or_else(|| "transient".to_string()),
                        message: get_string(&event, "message").unwrap_or_default(),
                    })
                    .await;
            }
            "anticipatory_update" => {
                let items = event
                    .get("items")
                    .cloned()
                    .and_then(|raw| serde_json::from_value::<Vec<AnticipatoryItem>>(raw).ok())
                    .unwrap_or_default();
                let _ = event_tx.send(ClientEvent::AnticipatoryItems(items)).await;
            }
            "task_update" => {
                let task = event
                    .get("task")
                    .cloned()
                    .and_then(|raw| serde_json::from_value::<AgentTask>(raw).ok())
                    .unwrap_or_else(|| AgentTask {
                        id: get_string(&event, "task_id").unwrap_or_default(),
                        title: get_string(&event, "message")
                            .unwrap_or_else(|| "Task update".to_string()),
                        description: String::new(),
                        thread_id: None,
                        parent_task_id: None,
                        parent_thread_id: None,
                        created_at: 0,
                        status: event
                            .get("status")
                            .cloned()
                            .and_then(|raw| serde_json::from_value::<TaskStatus>(raw).ok()),
                        progress: event.get("progress").and_then(Value::as_u64).unwrap_or(0) as u8,
                        session_id: None,
                        goal_run_id: None,
                        goal_step_title: None,
                        command: None,
                        awaiting_approval_id: None,
                        blocked_reason: get_string(&event, "message"),
                    });

                let _ = event_tx.send(ClientEvent::TaskUpdate(task)).await;
            }
            "goal_run_update" => {
                let goal_run = event
                    .get("goal_run")
                    .cloned()
                    .and_then(|raw| serde_json::from_value::<GoalRun>(raw).ok())
                    .unwrap_or_else(|| GoalRun {
                        id: get_string(&event, "goal_run_id").unwrap_or_default(),
                        title: "Goal run update".to_string(),
                        status: event
                            .get("status")
                            .cloned()
                            .and_then(|raw| serde_json::from_value::<GoalRunStatus>(raw).ok()),
                        current_step_index: event
                            .get("current_step_index")
                            .and_then(Value::as_u64)
                            .unwrap_or(0) as usize,
                        last_error: None,
                        sparse_update: true,
                        ..GoalRun::default()
                    });

                let _ = event_tx.send(ClientEvent::GoalRunUpdate(goal_run)).await;
            }
            "todo_update" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                let goal_run_id = get_string(&event, "goal_run_id");
                let step_index = event
                    .get("step_index")
                    .and_then(Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok());
                let items = event
                    .get("items")
                    .cloned()
                    .and_then(|raw| serde_json::from_value::<Vec<crate::wire::TodoItem>>(raw).ok())
                    .unwrap_or_default()
                    .into_iter()
                    .map(|mut todo| {
                        if todo.step_index.is_none() {
                            todo.step_index = step_index;
                        }
                        todo
                    })
                    .collect();
                let _ = event_tx
                    .send(ClientEvent::ThreadTodos {
                        thread_id,
                        goal_run_id,
                        step_index,
                        items,
                    })
                    .await;
            }
            "work_context_update" => {
                if let Some(context) = event
                    .get("context")
                    .cloned()
                    .and_then(|raw| serde_json::from_value::<ThreadWorkContext>(raw).ok())
                {
                    let _ = event_tx.send(ClientEvent::WorkContext(context)).await;
                }
            }
            "concierge_welcome" => {
                let content = event
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let actions = event
                    .get("actions")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|a| {
                                Some(crate::state::ConciergeActionVm {
                                    label: a.get("label")?.as_str()?.to_string(),
                                    action_type: a.get("action_type")?.as_str()?.to_string(),
                                    thread_id: a
                                        .get("thread_id")
                                        .and_then(|v| v.as_str())
                                        .map(String::from),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let _ = event_tx
                    .send(ClientEvent::ConciergeWelcome { content, actions })
                    .await;
            }
            "heartbeat_digest" => {
                let items: Vec<(u8, String, String, String)> = event
                    .get("items")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|item| {
                                Some((
                                    item.get("priority")?.as_u64()? as u8,
                                    item.get("check_type")?.as_str()?.to_string(),
                                    item.get("title")?.as_str()?.to_string(),
                                    item.get("suggestion")?.as_str()?.to_string(),
                                ))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let explanation = get_string(&event, "explanation");
                let _ = event_tx
                    .send(ClientEvent::HeartbeatDigest {
                        cycle_id: get_string(&event, "cycle_id").unwrap_or_default(),
                        actionable: event
                            .get("actionable")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        digest: get_string(&event, "digest").unwrap_or_default(),
                        items,
                        checked_at: event.get("checked_at").and_then(Value::as_u64).unwrap_or(0),
                        explanation,
                    })
                    .await;
            }
            "audit_action" => {
                let id = get_string(&event, "id").unwrap_or_default();
                let timestamp = event.get("timestamp").and_then(Value::as_u64).unwrap_or(0);
                let action_type = get_string(&event, "action_type").unwrap_or_default();
                let summary = get_string(&event, "summary").unwrap_or_default();
                let explanation = get_string(&event, "explanation");
                let confidence = event.get("confidence").and_then(Value::as_f64);
                let confidence_band = get_string(&event, "confidence_band");
                let causal_trace_id = get_string(&event, "causal_trace_id");
                let thread_id = get_string(&event, "thread_id");
                let _ = event_tx
                    .send(ClientEvent::AuditEntry {
                        id,
                        timestamp,
                        action_type,
                        summary,
                        explanation,
                        confidence,
                        confidence_band,
                        causal_trace_id,
                        thread_id,
                    })
                    .await;
            }
            "escalation_update" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                let from_level = get_string(&event, "from_level").unwrap_or_default();
                let to_level = get_string(&event, "to_level").unwrap_or_default();
                let reason = get_string(&event, "reason").unwrap_or_default();
                let attempts = event.get("attempts").and_then(Value::as_u64).unwrap_or(0) as u32;
                let audit_id = get_string(&event, "audit_id");
                let _ = event_tx
                    .send(ClientEvent::EscalationUpdate {
                        thread_id,
                        from_level,
                        to_level,
                        reason,
                        attempts,
                        audit_id,
                    })
                    .await;
            }
            "gateway_status" => {
                let platform = get_string(&event, "platform").unwrap_or_default();
                let status = get_string(&event, "status").unwrap_or_default();
                let last_error = get_string(&event, "last_error");
                let consecutive_failures = event
                    .get("consecutive_failures")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as u32;
                let _ = event_tx
                    .send(ClientEvent::GatewayStatus {
                        platform,
                        status,
                        last_error,
                        consecutive_failures,
                    })
                    .await;
            }
            "tier_changed" | "tier-changed" => {
                let data = event.get("data").cloned().unwrap_or_else(|| event.clone());
                let new_tier = data
                    .get("new_tier")
                    .or_else(|| data.get("newTier"))
                    .and_then(Value::as_str)
                    .unwrap_or("newcomer")
                    .to_string();
                let _ = event_tx.send(ClientEvent::TierChanged { new_tier }).await;
            }
            _ => {}
        }
    }

    fn send(&self, request: ClientMessage) -> Result<()> {
        amux_protocol::validate_client_message_size(&request)?;
        self.request_tx.send(request)?;
        Ok(())
    }

    pub fn refresh(&self) -> Result<()> {
        self.send(ClientMessage::AgentListThreads {
            limit: None,
            offset: None,
            include_internal: true,
        })
    }

    pub fn get_config(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetConfig)
    }

    pub fn refresh_services(&self) -> Result<()> {
        for request in [
            ClientMessage::AgentListTasks,
            ClientMessage::AgentListGoalRuns {
                limit: None,
                offset: None,
            },
            ClientMessage::AgentHeartbeatGetItems,
        ] {
            self.send(request)?;
        }
        Ok(())
    }

    pub fn list_tasks(&self) -> Result<()> {
        self.send(ClientMessage::AgentListTasks)
    }

    pub fn request_goal_run(&self, goal_run_id: impl Into<String>) -> Result<()> {
        self.send(ClientMessage::AgentGetGoalRun {
            goal_run_id: goal_run_id.into(),
        })
    }

    pub fn request_goal_run_page(
        &self,
        goal_run_id: impl Into<String>,
        step_offset: Option<usize>,
        step_limit: Option<usize>,
        event_offset: Option<usize>,
        event_limit: Option<usize>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentGetGoalRunPage {
            goal_run_id: goal_run_id.into(),
            step_offset,
            step_limit,
            event_offset,
            event_limit,
        })
    }

    pub fn start_goal_run(
        &self,
        goal: String,
        thread_id: Option<String>,
        session_id: Option<String>,
        launch_assignments: Vec<crate::state::task::GoalAgentAssignment>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentStartGoalRun {
            goal,
            title: None,
            thread_id,
            session_id,
            priority: None,
            client_request_id: None,
            launch_assignments: launch_assignments
                .into_iter()
                .map(|assignment| amux_protocol::GoalAgentAssignment {
                    role_id: assignment.role_id,
                    enabled: assignment.enabled,
                    provider: assignment.provider,
                    model: assignment.model,
                    reasoning_effort: assignment.reasoning_effort,
                    inherit_from_main: assignment.inherit_from_main,
                })
                .collect(),
            autonomy_level: None,
            client_surface: Some(amux_protocol::ClientSurface::Tui),
            requires_approval: true,
        })
    }

    pub fn explain_action(&self, action_id: String, step_index: Option<usize>) -> Result<()> {
        self.send(ClientMessage::AgentExplainAction {
            action_id,
            step_index,
        })
    }

    pub fn start_divergent_session(
        &self,
        problem_statement: String,
        thread_id: String,
        goal_run_id: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentStartDivergentSession {
            problem_statement,
            thread_id,
            goal_run_id,
            custom_framings_json: None,
        })
    }

    pub fn get_divergent_session(&self, session_id: String) -> Result<()> {
        self.send(ClientMessage::AgentGetDivergentSession { session_id })
    }

    pub fn request_thread(
        &self,
        thread_id: impl Into<String>,
        message_limit: Option<usize>,
        message_offset: Option<usize>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentGetThread {
            thread_id: thread_id.into(),
            message_limit,
            message_offset,
        })
    }

    pub fn request_todos(&self, thread_id: impl Into<String>) -> Result<()> {
        self.send(ClientMessage::AgentGetTodos {
            thread_id: thread_id.into(),
        })
    }

    pub fn request_work_context(&self, thread_id: impl Into<String>) -> Result<()> {
        self.send(ClientMessage::AgentGetWorkContext {
            thread_id: thread_id.into(),
        })
    }

    pub fn list_notifications(&self) -> Result<()> {
        self.send(ClientMessage::ListAgentEvents {
            category: Some("notification".to_string()),
            pane_id: None,
            limit: Some(500),
        })
    }

    pub fn upsert_notification(
        &self,
        notification: amux_protocol::InboxNotification,
    ) -> Result<()> {
        let event_row = amux_protocol::AgentEventRow {
            id: notification.id.clone(),
            category: "notification".to_string(),
            kind: notification.kind.clone(),
            pane_id: None,
            workspace_id: None,
            surface_id: None,
            session_id: None,
            payload_json: serde_json::to_string(&notification)?,
            timestamp: notification.updated_at,
        };
        self.send(ClientMessage::UpsertAgentEvent {
            event_json: serde_json::to_string(&event_row)?,
        })
    }

    pub fn request_git_diff(
        &self,
        repo_path: impl Into<String>,
        file_path: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::GetGitDiff {
            repo_path: repo_path.into(),
            file_path,
        })
    }

}
