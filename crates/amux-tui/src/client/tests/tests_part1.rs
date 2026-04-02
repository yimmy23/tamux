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

    #[tokio::test]
    async fn done_event_parses_reasoning_payload() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        DaemonClient::dispatch_agent_event(
            serde_json::json!({
                "type": "done",
                "thread_id": "thread-1",
                "input_tokens": 10,
                "output_tokens": 20,
                "provider": "github-copilot",
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
    async fn daemon_agent_error_is_forwarded_to_client_error_event() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        let should_continue = DaemonClient::handle_daemon_message(
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

        let should_continue = DaemonClient::handle_daemon_message(
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

        let should_continue = DaemonClient::handle_daemon_message(
            DaemonMessage::AgentProviderValidation {
                operation_id: Some("op-provider-validation-1".to_string()),
                provider_id: "openai".to_string(),
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
                assert_eq!(provider_id, "openai");
                assert!(!valid);
                assert_eq!(error.as_deref(), Some("bad key"));
            }
            other => panic!("expected provider validation event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn daemon_models_response_with_operation_id_emits_models_fetched_event() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        let should_continue = DaemonClient::handle_daemon_message(
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
            }
            other => panic!("expected models fetched event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn daemon_openai_codex_auth_replies_emit_client_events() {
        let (event_tx, mut event_rx) = mpsc::channel(8);

        let should_continue = DaemonClient::handle_daemon_message(
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

        let should_continue = DaemonClient::handle_daemon_message(
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

        let should_continue = DaemonClient::handle_daemon_message(
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
