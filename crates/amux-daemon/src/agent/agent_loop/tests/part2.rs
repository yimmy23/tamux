use super::*;

#[tokio::test]
async fn transport_incompatibility_does_not_mutate_persisted_config_and_emits_notice() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let request_counter = Arc::new(AtomicUsize::new(0));
    let mut config = AgentConfig::default();
    config.provider = "custom".to_string();
    config.base_url = spawn_transport_incompatibility_server(request_counter.clone()).await;
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.providers.insert(
        "custom".to_string(),
        ProviderConfig {
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            api_key: config.api_key.clone(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::Responses,
            reasoning_effort: String::new(),
            context_window_tokens: 0,
            response_schema: None,
        },
    );

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.persist_config().await;
    let mut events = engine.subscribe();

    let _error = match engine
        .send_message_inner(None, "hello", None, None, None, None, None, true)
        .await
    {
        Ok(_) => panic!("transport incompatibility should fail the turn"),
        Err(error) => error,
    };

    let stored_config = engine.config.read().await.clone();
    assert_eq!(stored_config.api_transport, ApiTransport::Responses);
    assert_eq!(
        stored_config
            .providers
            .get("custom")
            .expect("provider entry")
            .api_transport,
        ApiTransport::Responses
    );

    let persisted_items = engine
        .history
        .list_agent_config_items()
        .await
        .expect("list persisted config items");
    let persisted = crate::agent::config::load_config_from_items(persisted_items)
        .expect("decode persisted config");
    assert_eq!(persisted.api_transport, ApiTransport::Responses);
    assert_eq!(
        persisted
            .providers
            .get("custom")
            .expect("persisted provider entry")
            .api_transport,
        ApiTransport::Responses
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut saw_notice = false;
    let mut saw_done = false;
    let mut seen_notice_kinds = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event {
            AgentEvent::WorkflowNotice {
                kind,
                message,
                details,
                ..
            } => {
                seen_notice_kinds.push(kind.clone());
                if kind == "transport-incompatible" || kind == "upstream-error" {
                    saw_notice = true;
                    assert!(
                        message.contains("incompatible")
                            || details
                                .as_deref()
                                .is_some_and(|d| d.contains("transport_incompatible"))
                    );
                    let details = details.expect("notice should include diagnostics");
                    assert!(details.contains("transport_incompatible"));
                    assert!(details.contains("Responses API not supported"));
                }
            }
            AgentEvent::Done { .. } => saw_done = true,
            _ => {}
        }
    }

    assert!(
        saw_notice,
        "expected operator-visible transport incompatibility notice, saw {:?}",
        seen_notice_kinds
    );
    assert!(
        saw_done,
        "expected turn completion event for surfaced error"
    );
    assert_eq!(request_counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn structured_upstream_diagnostics_are_not_persisted_or_streamed_to_user() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let request_counter = Arc::new(AtomicUsize::new(0));
    let mut config = AgentConfig::default();
    config.provider = "custom".to_string();
    config.base_url = spawn_transport_incompatibility_server(request_counter.clone()).await;
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.subscribe();

    let error = match engine
        .send_message_inner(None, "hello", None, None, None, None, None, true)
        .await
    {
        Ok(_) => panic!("structured upstream failure should fail the turn"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains(UPSTREAM_DIAGNOSTICS_MARKER),
        "precondition: returned error still carries structured diagnostics envelope"
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut saw_error_delta = false;
    while let Ok(event) = events.try_recv() {
        if let AgentEvent::Delta { content, .. } = event {
            if content.starts_with("Error: ") {
                saw_error_delta = true;
                assert!(
                    !content.contains(UPSTREAM_DIAGNOSTICS_MARKER),
                    "error delta should not expose structured diagnostics"
                );
            }
        }
    }
    assert!(saw_error_delta, "expected streamed error delta");

    let threads = engine.threads.read().await;
    let thread = threads.values().next().expect("thread should be created");
    let assistant_error = thread
        .messages
        .iter()
        .find(|message| {
            message.role == MessageRole::Assistant && message.content.starts_with("Error: ")
        })
        .expect("assistant error should be persisted");
    assert!(
        !assistant_error
            .content
            .contains(UPSTREAM_DIAGNOSTICS_MARKER),
        "persisted assistant error should not include structured diagnostics"
    );
}

#[tokio::test]
async fn concierge_recovery_fixable_request_invalid_starts_one_background_investigation() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let structured = StructuredUpstreamFailure {
        class: "request_invalid".to_string(),
        summary: "provider rejected the daemon request as invalid: Invalid 'input[12].name': empty string".to_string(),
        diagnostics: serde_json::json!({
            "raw_message": "Invalid 'input[12].name': empty string"
        }),
    };
    let mut attempted = std::collections::HashSet::new();

    let first = engine
        .maybe_recover_fixable_upstream_failure(
            "thread-recovery",
            &structured,
            false,
            false,
            &mut attempted,
        )
        .await
        .expect("recovery evaluation should succeed");
    let second = engine
        .maybe_recover_fixable_upstream_failure(
            "thread-recovery",
            &structured,
            false,
            false,
            &mut attempted,
        )
        .await
        .expect("repeat recovery evaluation should succeed");

    assert!(first.started_investigation);
    assert!(first.retry_attempted);
    assert_eq!(
        first.signature.as_deref(),
        Some("request-invalid-empty-tool-name")
    );
    assert!(!second.started_investigation);
    assert!(!second.retry_attempted);

    let tasks = engine.tasks.lock().await;
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].source, "concierge_recovery");
    assert_eq!(
        tasks[0].parent_thread_id.as_deref(),
        Some("thread-recovery")
    );
}

#[tokio::test]
async fn concierge_recovery_transport_signature_is_blocked_after_committed_output() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let structured = StructuredUpstreamFailure {
        class: "transport_incompatible".to_string(),
        summary: "The selected provider/transport combination is incompatible: request body mismatch from stale thread state".to_string(),
        diagnostics: serde_json::json!({
            "details": "request body mismatch from stale thread state"
        }),
    };
    let mut attempted = std::collections::HashSet::new();

    let disposition = engine
        .maybe_recover_fixable_upstream_failure(
            "thread-visible-output",
            &structured,
            true,
            false,
            &mut attempted,
        )
        .await
        .expect("recovery evaluation should succeed");

    assert!(disposition.started_investigation);
    assert!(!disposition.retry_attempted);

    let tasks = engine.tasks.lock().await;
    assert_eq!(tasks.len(), 1);
}
