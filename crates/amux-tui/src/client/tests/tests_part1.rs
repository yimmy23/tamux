use amux_shared::providers::{PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_OPENAI};

    async fn handle_daemon_message_for_test(
        message: DaemonMessage,
        event_tx: &mpsc::Sender<ClientEvent>,
    ) -> bool {
        let mut thread_detail_chunks = None;
        DaemonClient::handle_daemon_message(message, event_tx, &mut thread_detail_chunks).await
    }

    #[test]
    fn whatsapp_link_methods_send_expected_protocol_messages() {
        let (event_tx, _event_rx) = mpsc::channel(8);
        let client = DaemonClient::new(event_tx);
        let mut rx = client.request_rx.lock().unwrap().take().unwrap();

        client.whatsapp_link_start().unwrap();
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentWhatsAppLinkStart
        ));

        client.whatsapp_link_status().unwrap();
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentWhatsAppLinkStatus
        ));

        client.whatsapp_link_subscribe().unwrap();
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentWhatsAppLinkSubscribe
        ));

        client.whatsapp_link_unsubscribe().unwrap();
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentWhatsAppLinkUnsubscribe
        ));

        client.whatsapp_link_reset().unwrap();
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentWhatsAppLinkReset
        ));

        client.whatsapp_link_stop().unwrap();
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentWhatsAppLinkStop
        ));
    }

    #[test]
    fn openai_codex_auth_methods_send_expected_protocol_messages() {
        let (event_tx, _event_rx) = mpsc::channel(8);
        let client = DaemonClient::new(event_tx);
        let mut rx = client.request_rx.lock().unwrap().take().unwrap();

        client.get_openai_codex_auth_status().unwrap();
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentGetOpenAICodexAuthStatus
        ));

        client.login_openai_codex().unwrap();
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentLoginOpenAICodex
        ));

        client.logout_openai_codex().unwrap();
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentLogoutOpenAICodex
        ));
    }

    #[test]
    fn pin_methods_send_expected_protocol_messages() {
        let (event_tx, _event_rx) = mpsc::channel(8);
        let client = DaemonClient::new(event_tx);
        let mut rx = client.request_rx.lock().unwrap().take().unwrap();

        client
            .pin_thread_message_for_compaction("thread-1".to_string(), "message-1".to_string())
            .unwrap();
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentPinThreadMessageForCompaction { thread_id, message_id }
                if thread_id == "thread-1" && message_id == "message-1"
        ));

        client
            .unpin_thread_message_for_compaction("thread-1".to_string(), "message-1".to_string())
            .unwrap();
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentUnpinThreadMessageForCompaction { thread_id, message_id }
                if thread_id == "thread-1" && message_id == "message-1"
        ));
    }

    #[test]
    fn refresh_requests_thread_list_with_internal_threads_included() {
        let (event_tx, _event_rx) = mpsc::channel(8);
        let client = DaemonClient::new(event_tx);
        let mut rx = client.request_rx.lock().unwrap().take().unwrap();

        client.refresh().unwrap();

        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentListThreads {
                limit: None,
                offset: None,
                include_internal: true,
            }
        ));
    }

    #[test]
    fn get_config_requests_agent_config_immediately() {
        let (event_tx, _event_rx) = mpsc::channel(8);
        let client = DaemonClient::new(event_tx);
        let mut rx = client.request_rx.lock().unwrap().take().unwrap();

        client.get_config().unwrap();

        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentGetConfig
        ));
    }

    #[test]
    fn refresh_services_excludes_agent_config_request() {
        let (event_tx, _event_rx) = mpsc::channel(8);
        let client = DaemonClient::new(event_tx);
        let mut rx = client.request_rx.lock().unwrap().take().unwrap();

        client.refresh_services().unwrap();

        assert!(matches!(drain_request(&mut rx), ClientMessage::AgentListTasks));
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentListGoalRuns {
                limit: None,
                offset: None,
            }
        ));
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentHeartbeatGetItems
        ));
        assert!(
            rx.try_recv().is_err(),
            "refresh_services should no longer enqueue AgentGetConfig on the startup-critical lane"
        );
    }

    #[test]
    fn oversized_send_message_is_rejected_before_queueing() {
        let (event_tx, _event_rx) = mpsc::channel(8);
        let client = DaemonClient::new(event_tx);
        let mut rx = client.request_rx.lock().unwrap().take().unwrap();

        let err = client
            .send_message(
                Some("thread-oversized".to_string()),
                "x".repeat(amux_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024),
                None,
                None,
                None,
            )
            .expect_err("oversized message should be rejected locally");

        assert!(err.to_string().contains("too large for IPC"));
        assert!(
            rx.try_recv().is_err(),
            "oversized request should never be queued"
        );
    }

    #[test]
    fn resolve_task_approval_uses_agent_protocol_message() {
        let (event_tx, _event_rx) = mpsc::channel(8);
        let client = DaemonClient::new(event_tx);
        let mut rx = client.request_rx.lock().unwrap().take().unwrap();

        client
            .resolve_task_approval(
                "policy-escalation-thread_abc-123".to_string(),
                "allow_session".to_string(),
            )
            .unwrap();

        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentResolveTaskApproval { approval_id, decision }
                if approval_id == "policy-escalation-thread_abc-123"
                    && decision == "approve-session"
        ));
    }

    #[tokio::test]
    async fn task_list_accepts_budget_exceeded_status() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        let should_continue = handle_daemon_message_for_test(
            DaemonMessage::AgentTaskList {
                tasks_json: serde_json::json!([{
                    "id": "task-budget",
                    "title": "Task budget exceeded",
                    "status": "budget_exceeded"
                }])
                .to_string(),
            },
            &event_tx,
        )
        .await;

        assert!(should_continue);
        match event_rx.recv().await.expect("expected task list event") {
            ClientEvent::TaskList(tasks) => {
                assert_eq!(tasks.len(), 1);
                assert_eq!(
                    tasks[0].status,
                    Some(crate::wire::TaskStatus::BudgetExceeded)
                );
            }
            other => panic!("expected task list event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn goal_run_detail_placeholder_payload_carries_requested_id() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        let should_continue = handle_daemon_message_for_test(
            DaemonMessage::AgentGoalRunDetail {
                goal_run_json: serde_json::json!({
                    "id": "goal-1",
                })
                .to_string(),
            },
            &event_tx,
        )
        .await;

        assert!(should_continue);
        match event_rx.recv().await.expect("expected goal detail event") {
            ClientEvent::GoalRunDetail(Some(goal_run)) => {
                assert_eq!(goal_run.id, "goal-1");
                assert!(goal_run.title.is_empty());
                assert!(goal_run.status.is_none());
            }
            other => panic!("expected goal run detail event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn goal_run_update_placeholder_payload_is_marked_sparse() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        let should_continue = handle_daemon_message_for_test(
            DaemonMessage::AgentEvent {
                event_json: serde_json::json!({
                    "type": "goal_run_update",
                    "goal_run_id": "goal-1",
                    "message": "Goal update",
                    "current_step_index": 4
                })
                .to_string(),
            },
            &event_tx,
        )
        .await;

        assert!(should_continue);
        match event_rx.recv().await.expect("expected goal update event") {
            ClientEvent::GoalRunUpdate(goal_run) => {
                assert_eq!(goal_run.id, "goal-1");
                assert_eq!(goal_run.title, "Goal run update");
                assert!(goal_run.last_error.is_none());
                assert_eq!(goal_run.current_step_index, 4);
                assert!(goal_run.sparse_update);
            }
            other => panic!("expected goal run update event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn todo_update_applies_top_level_step_index_to_items_missing_it() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        DaemonClient::dispatch_agent_event(
            serde_json::json!({
                "type": "todo_update",
                "thread_id": "thread-1",
                "goal_run_id": "goal-1",
                "step_index": 2,
                "items": [
                    {
                        "id": "todo-1",
                        "content": "Verify note contents",
                        "status": "in_progress",
                        "position": 0
                    }
                ]
            }),
            &event_tx,
        )
        .await;

        match event_rx.recv().await.expect("expected thread todos event") {
            ClientEvent::ThreadTodos {
                thread_id,
                goal_run_id,
                step_index,
                items,
            } => {
                assert_eq!(thread_id, "thread-1");
                assert_eq!(goal_run_id.as_deref(), Some("goal-1"));
                assert_eq!(step_index, Some(2));
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].id, "todo-1");
                assert_eq!(items[0].step_index, Some(2));
            }
            other => panic!("expected thread todos event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn checkpoint_list_event_carries_goal_id_when_empty() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        let should_continue = handle_daemon_message_for_test(
            DaemonMessage::AgentCheckpointList {
                goal_run_id: "goal-1".to_string(),
                checkpoints_json: "[]".to_string(),
            },
            &event_tx,
        )
        .await;

        assert!(should_continue);
        match event_rx.recv().await.expect("expected checkpoints event") {
            ClientEvent::GoalRunCheckpoints {
                goal_run_id,
                checkpoints,
            } => {
                assert_eq!(goal_run_id, "goal-1");
                assert!(checkpoints.is_empty());
            }
            other => panic!("expected checkpoints event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn done_event_parses_reasoning_payload() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        DaemonClient::dispatch_agent_event(
            serde_json::json!({
                "type": "done",
                "thread_id": "thread-1",
                "input_tokens": 10,
                "output_tokens": 20,
                "provider": PROVIDER_ID_GITHUB_COPILOT,
                "model": "gpt-5.4",
                "reasoning": "Final reasoning summary"
            }),
            &event_tx,
        )
        .await;

        match event_rx.recv().await.expect("expected done event") {
            ClientEvent::Done {
                thread_id,
                reasoning,
                ..
            } => {
                assert_eq!(thread_id, "thread-1");
                assert_eq!(reasoning.as_deref(), Some("Final reasoning summary"));
            }
            other => panic!("expected done event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn thread_message_pin_result_emits_budget_event() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        handle_daemon_message_for_test(
            DaemonMessage::AgentThreadMessagePinResult {
                result_json: serde_json::json!({
                    "ok": false,
                    "thread_id": "thread-1",
                    "message_id": "message-1",
                    "error": "pinned_budget_exceeded",
                    "current_pinned_chars": 100,
                    "pinned_budget_chars": 120,
                    "candidate_pinned_chars": 160
                })
                .to_string(),
            },
            &event_tx,
        )
        .await;

        match event_rx.recv().await.expect("expected pin result event") {
            ClientEvent::ThreadMessagePinResult(result) => {
                assert!(!result.ok);
                assert_eq!(result.thread_id, "thread-1");
                assert_eq!(result.message_id, "message-1");
                assert_eq!(result.error.as_deref(), Some("pinned_budget_exceeded"));
                assert_eq!(result.current_pinned_chars, 100);
                assert_eq!(result.pinned_budget_chars, 120);
                assert_eq!(result.candidate_pinned_chars, Some(160));
            }
            other => panic!("expected pin result event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn done_event_parses_provider_final_result_payload() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        DaemonClient::dispatch_agent_event(
            serde_json::json!({
                "type": "done",
                "thread_id": "thread-1",
                "input_tokens": 10,
                "output_tokens": 20,
                "provider_final_result": {
                    "provider": "open_ai_responses",
                    "id": "resp_tui_done"
                }
            }),
            &event_tx,
        )
        .await;

        match event_rx.recv().await.expect("expected done event") {
            ClientEvent::Done {
                provider_final_result_json,
                ..
            } => {
                let json = provider_final_result_json.expect("expected provider final result");
                let value: serde_json::Value = serde_json::from_str(&json).expect("parse provider final result json");
                assert_eq!(value.get("provider").and_then(|v| v.as_str()), Some("open_ai_responses"));
                assert_eq!(value.get("id").and_then(|v| v.as_str()), Some("resp_tui_done"));
            }
            other => panic!("expected done event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn daemon_agent_error_is_forwarded_to_client_error_event() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        let should_continue = handle_daemon_message_for_test(
            DaemonMessage::AgentError {
                message: "protected mutation: cannot change WELES name".to_string(),
            },
            &event_tx,
        )
        .await;

        assert!(should_continue);
        match event_rx.recv().await.expect("expected error event") {
            ClientEvent::Error(message) => {
                assert_eq!(message, "protected mutation: cannot change WELES name");
            }
            other => panic!("expected error event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn daemon_operation_accepted_is_ignored_without_error() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        let should_continue = handle_daemon_message_for_test(
            DaemonMessage::OperationAccepted {
                operation_id: "op-tui-1".to_string(),
                kind: "agent_set_sub_agent".to_string(),
                dedup: None,
                revision: 1,
            },
            &event_tx,
        )
        .await;

        assert!(should_continue);
        assert!(
            event_rx.try_recv().is_err(),
            "operation acceptance should not emit a user-visible TUI event"
        );
    }

    #[tokio::test]
    async fn daemon_provider_validation_with_operation_id_emits_provider_validation_event() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        let should_continue = handle_daemon_message_for_test(
            DaemonMessage::AgentProviderValidation {
                operation_id: Some("op-provider-validation-1".to_string()),
                provider_id: PROVIDER_ID_OPENAI.to_string(),
                valid: false,
                error: Some("bad key".to_string()),
                models_json: None,
            },
            &event_tx,
        )
        .await;

        assert!(should_continue);
        match event_rx
            .recv()
            .await
            .expect("expected provider validation event")
        {
            ClientEvent::ProviderValidation {
                provider_id,
                valid,
                error,
            } => {
                assert_eq!(provider_id, PROVIDER_ID_OPENAI);
                assert!(!valid);
                assert_eq!(error.as_deref(), Some("bad key"));
            }
            other => panic!("expected provider validation event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn daemon_models_response_with_operation_id_emits_models_fetched_event() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        let should_continue = handle_daemon_message_for_test(
            DaemonMessage::AgentModelsResponse {
                operation_id: Some("op-fetch-models-1".to_string()),
                models_json:
                    r#"[{"id":"gpt-5.4-mini","name":"GPT-5.4 Mini","provider":"openai"}]"#
                        .to_string(),
            },
            &event_tx,
        )
        .await;

        assert!(should_continue);
        match event_rx.recv().await.expect("expected models fetched event") {
            ClientEvent::ModelsFetched(models) => {
                assert_eq!(models.len(), 1);
                assert_eq!(models[0].id, "gpt-5.4-mini");
                assert_eq!(models[0].name.as_deref(), Some("GPT-5.4 Mini"));
                assert_eq!(models[0].context_window, None);
                assert!(models[0].pricing.is_none());
                assert!(models[0].metadata.is_none());
            }
            other => panic!("expected models fetched event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn agent_status_response_emits_full_status_event_and_diagnostics() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        let should_continue = handle_daemon_message_for_test(
            DaemonMessage::AgentStatusResponse {
                tier: "mission_control".to_string(),
                feature_flags_json: "{}".to_string(),
                activity: "waiting_for_operator".to_string(),
                active_thread_id: Some("thread-1".to_string()),
                active_goal_run_id: Some("goal-1".to_string()),
                active_goal_run_title: Some("Close release gap".to_string()),
                provider_health_json: r#"{"openai":{"can_execute":true,"trip_count":0}}"#.to_string(),
                gateway_statuses_json: r#"{"slack":{"status":"connected"}}"#.to_string(),
                recent_actions_json: r#"[{"action_type":"tool_call","summary":"Ran status","timestamp":1712345678}]"#.to_string(),
                diagnostics_json: r#"{"operator_profile_sync_state":"dirty","operator_profile_sync_dirty":true,"operator_profile_scheduler_fallback":false}"#.to_string(),
            },
            &event_tx,
        )
        .await;

        assert!(should_continue);

        match event_rx.recv().await.expect("expected first event") {
            ClientEvent::StatusSnapshot(AgentStatusSnapshotVm {
                tier,
                activity,
                active_thread_id,
                active_goal_run_title,
                provider_health_json,
                ..
            }) => {
                assert_eq!(tier, "mission_control");
                assert_eq!(activity, "waiting_for_operator");
                assert_eq!(active_thread_id.as_deref(), Some("thread-1"));
                assert_eq!(active_goal_run_title.as_deref(), Some("Close release gap"));
                assert!(provider_health_json.contains("openai"));
            }
            other => panic!("expected status snapshot event, got {:?}", other),
        }

        match event_rx.recv().await.expect("expected second event") {
            ClientEvent::StatusDiagnostics {
                operator_profile_sync_state,
                operator_profile_sync_dirty,
                operator_profile_scheduler_fallback,
                diagnostics_json,
            } => {
                assert_eq!(operator_profile_sync_state, "dirty");
                assert!(operator_profile_sync_dirty);
                assert!(!operator_profile_scheduler_fallback);
                assert!(diagnostics_json.contains("operator_profile_sync_state"));
            }
            other => panic!("expected status diagnostics event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn agent_prompt_inspection_emits_prompt_event() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        let should_continue = handle_daemon_message_for_test(
            DaemonMessage::AgentPromptInspection {
                prompt_json: serde_json::json!({
                    "agent_id": "swarog",
                    "agent_name": "Svarog",
                    "provider_id": "openai",
                    "model": "gpt-5.4-mini",
                    "sections": [{
                        "id": "base_prompt",
                        "title": "Base Prompt",
                        "content": "Custom operator prompt",
                    }],
                    "final_prompt": "Custom operator prompt\n\n## Runtime Identity",
                })
                .to_string(),
            },
            &event_tx,
        )
        .await;

        assert!(should_continue);
        match event_rx.recv().await.expect("expected prompt inspection event") {
            ClientEvent::PromptInspection(prompt) => {
                assert_eq!(prompt.agent_id, "swarog");
                assert_eq!(prompt.agent_name, "Svarog");
                assert_eq!(prompt.sections.len(), 1);
                assert!(prompt.final_prompt.contains("## Runtime Identity"));
            }
            other => panic!("expected prompt inspection event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn daemon_openai_codex_auth_replies_emit_client_events() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        let should_continue = handle_daemon_message_for_test(
            DaemonMessage::AgentOpenAICodexAuthStatus {
                status_json: serde_json::json!({
                    "available": false,
                    "authMode": "chatgpt_subscription",
                    "source": "tamux-daemon",
                    "status": "pending",
                    "authUrl": "https://auth.openai.com/oauth/authorize?code=123"
                })
                .to_string(),
            },
            &event_tx,
        )
        .await;

        assert!(should_continue);
        match event_rx.recv().await.expect("expected auth status event") {
            ClientEvent::OpenAICodexAuthStatus(status) => {
                assert!(!status.available);
                assert_eq!(status.auth_mode.as_deref(), Some("chatgpt_subscription"));
                assert_eq!(status.source.as_deref(), Some("tamux-daemon"));
                assert_eq!(status.status.as_deref(), Some("pending"));
                assert!(status
                    .auth_url
                    .as_deref()
                    .is_some_and(|url| url.starts_with("https://auth.openai.com/oauth/authorize")));
            }
            other => panic!("expected auth status event, got {:?}", other),
        }

        let should_continue = handle_daemon_message_for_test(
            DaemonMessage::AgentOpenAICodexAuthLoginResult {
                result_json: serde_json::json!({
                    "available": false,
                    "authMode": "chatgpt_subscription",
                    "source": "tamux-daemon",
                    "status": "pending",
                    "authUrl": "https://auth.openai.com/oauth/authorize?code=456"
                })
                .to_string(),
            },
            &event_tx,
        )
        .await;

        assert!(should_continue);
        match event_rx.recv().await.expect("expected auth login event") {
            ClientEvent::OpenAICodexAuthLoginResult(status) => {
                assert_eq!(status.status.as_deref(), Some("pending"));
                assert_eq!(status.source.as_deref(), Some("tamux-daemon"));
                assert!(status
                    .auth_url
                    .as_deref()
                    .is_some_and(|url| url.contains("code=456")));
            }
            other => panic!("expected auth login event, got {:?}", other),
        }

        let should_continue = handle_daemon_message_for_test(
            DaemonMessage::AgentOpenAICodexAuthLogoutResult {
                ok: true,
                error: None,
            },
            &event_tx,
        )
        .await;

        assert!(should_continue);
        match event_rx.recv().await.expect("expected auth logout event") {
            ClientEvent::OpenAICodexAuthLogoutResult { ok, error } => {
                assert!(ok);
                assert!(error.is_none());
            }
            other => panic!("expected auth logout event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn weles_health_update_event_parses_degraded_payload() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        DaemonClient::dispatch_agent_event(
            serde_json::json!({
                "type": "weles_health_update",
                "state": "degraded",
                "reason": "WELES review unavailable for guarded actions",
                "checked_at": 321
            }),
            &event_tx,
        )
        .await;

        match event_rx.recv().await.expect("expected weles health event") {
            ClientEvent::WelesHealthUpdate {
                state,
                reason,
                checked_at,
            } => {
                assert_eq!(state, "degraded");
                assert_eq!(checked_at, 321);
                assert_eq!(
                    reason.as_deref(),
                    Some("WELES review unavailable for guarded actions")
                );
            }
            other => panic!("expected weles health update, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn hidden_handoff_thread_reload_event_is_filtered() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        DaemonClient::dispatch_agent_event(
            serde_json::json!({
                "type": "thread_reload_required",
                "thread_id": "handoff:thread-user:handoff-1"
            }),
            &event_tx,
        )
        .await;

        assert!(event_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn internal_dm_thread_reload_event_is_forwarded() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        DaemonClient::dispatch_agent_event(
            serde_json::json!({
                "type": "thread_reload_required",
                "thread_id": "dm:svarog:weles"
            }),
            &event_tx,
        )
        .await;

        match tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv())
            .await
            .expect("internal dm reload event should arrive")
            .expect("expected internal dm reload event")
        {
            ClientEvent::ThreadReloadRequired { thread_id } => {
                assert_eq!(thread_id, "dm:svarog:weles");
            }
            other => panic!("expected thread reload event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn internal_dm_done_event_is_forwarded() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        DaemonClient::dispatch_agent_event(
            serde_json::json!({
                "type": "done",
                "thread_id": "dm:svarog:weles",
                "input_tokens": 1,
                "output_tokens": 2,
                "reasoning": "internal reasoning"
            }),
            &event_tx,
        )
        .await;

        match tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv())
            .await
            .expect("internal dm done event should arrive")
            .expect("expected internal dm done event")
        {
            ClientEvent::Done {
                thread_id,
                input_tokens,
                output_tokens,
                reasoning,
                ..
            } => {
                assert_eq!(thread_id, "dm:svarog:weles");
                assert_eq!(input_tokens, 1);
                assert_eq!(output_tokens, 2);
                assert_eq!(reasoning.as_deref(), Some("internal reasoning"));
            }
            other => panic!("expected done event, got {:?}", other),
        }
    }

    #[test]
    fn list_notifications_sends_agent_event_query() {
        let (event_tx, _event_rx) = mpsc::channel(8);
        let client = DaemonClient::new(event_tx);
        let mut rx = client.request_rx.lock().unwrap().take().unwrap();

        client.list_notifications().unwrap();

        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::ListAgentEvents {
                category: Some(category),
                pane_id: None,
                limit: Some(500),
            } if category == "notification"
        ));
    }

    #[tokio::test]
    async fn notification_inbox_upsert_event_is_forwarded() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        DaemonClient::dispatch_agent_event(
            serde_json::json!({
                "type": "notification_inbox_upsert",
                "notification": {
                    "id": "n1",
                    "source": "plugin_auth",
                    "kind": "plugin_needs_reconnect",
                    "title": "Reconnect Gmail",
                    "body": "Reconnect required.",
                    "subtitle": "gmail",
                    "severity": "warning",
                    "created_at": 10,
                    "updated_at": 20,
                    "read_at": null,
                    "archived_at": null,
                    "deleted_at": null,
                    "actions": [],
                    "metadata_json": null
                }
            }),
            &event_tx,
        )
        .await;

        match event_rx.recv().await.expect("expected notification event") {
            ClientEvent::NotificationUpsert(notification) => {
                assert_eq!(notification.id, "n1");
                assert_eq!(notification.source, "plugin_auth");
                assert_eq!(notification.title, "Reconnect Gmail");
            }
            other => panic!("expected notification upsert, got {:?}", other),
        }
    }
