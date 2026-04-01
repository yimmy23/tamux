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
