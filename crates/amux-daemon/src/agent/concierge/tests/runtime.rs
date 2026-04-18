use super::*;

#[tokio::test]
async fn concierge_recovery_deduplicates_inflight_investigations_per_thread_signature() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let first = engine
        .concierge
        .maybe_start_recovery_investigation(
            &engine,
            "thread-recovery",
            "request-invalid-empty-tool-name",
            "request_invalid",
            "invalid request body",
            &serde_json::json!({"raw_message": "Invalid 'input[12].name': empty string"}),
        )
        .await;
    let second = engine
        .concierge
        .maybe_start_recovery_investigation(
            &engine,
            "thread-recovery",
            "request-invalid-empty-tool-name",
            "request_invalid",
            "invalid request body",
            &serde_json::json!({"raw_message": "Invalid 'input[12].name': empty string"}),
        )
        .await;

    assert!(first.is_some());
    assert!(second.is_none());

    let tasks = engine.tasks.lock().await;
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].source, "concierge_recovery");
    assert_eq!(
        tasks[0].parent_thread_id.as_deref(),
        Some("thread-recovery")
    );
    assert_eq!(
        tasks[0].sub_agent_def_id.as_deref(),
        Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
    );
    assert!(
        tasks[0]
            .override_system_prompt
            .as_deref()
            .is_some_and(|prompt| prompt.contains(crate::agent::agent_identity::WELES_AGENT_ID)),
        "recovery investigation should be owned by daemon WELES"
    );
}

#[tokio::test]
async fn prune_welcome_messages_removes_all_concierge_welcomes() {
    let config = Arc::new(RwLock::new(AgentConfig::default()));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let threads = RwLock::new(HashMap::from([(
        CONCIERGE_THREAD_ID.to_string(),
        concierge_thread(vec![
            assistant_message("hello", 1),
            AgentMessage {
                provider: Some("concierge".into()),
                ..assistant_message("welcome 1", 2)
            },
            AgentMessage {
                provider: Some("concierge".into()),
                ..assistant_message("welcome 2", 3)
            },
        ]),
    )]));

    engine.prune_welcome_messages(&threads).await;

    let guard = threads.read().await;
    let thread = guard.get(CONCIERGE_THREAD_ID).unwrap();
    assert_eq!(thread.messages.len(), 1);
    assert_eq!(thread.messages[0].content, "hello");
}

#[tokio::test]
async fn prune_welcome_messages_clears_welcome_cache() {
    let config = Arc::new(RwLock::new(AgentConfig::default()));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let action = ConciergeAction {
        label: "Dismiss".to_string(),
        action_type: ConciergeActionType::DismissWelcome,
        thread_id: None,
    };
    engine.cache_welcome("sig", "cached", &[action]).await;
    assert!(engine.cached_welcome("sig").await.is_some());

    let threads = RwLock::new(HashMap::<String, AgentThread>::new());
    engine.prune_welcome_messages(&threads).await;
    assert!(engine.cached_welcome("sig").await.is_none());
}

#[tokio::test]
async fn generate_welcome_reuses_recent_persisted_welcome_without_new_user_message() {
    let mut config_value = AgentConfig::default();
    config_value.concierge.detail_level = ConciergeDetailLevel::Minimal;
    let config = Arc::new(RwLock::new(config_value));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let threads = RwLock::new(HashMap::from([
        (
            CONCIERGE_THREAD_ID.to_string(),
            concierge_thread(vec![AgentMessage {
                provider: Some("concierge".into()),
                ..assistant_message("persisted welcome", now - 60_000)
            }]),
        ),
        (
            "thread-1".to_string(),
            thread_with_messages(
                "thread-1",
                "Thread One",
                now - 120_000,
                vec![assistant_message("old reply", now - 120_000)],
            ),
        ),
    ]));

    let result = engine
        .generate_welcome(&threads, &Mutex::new(std::collections::VecDeque::new()))
        .await
        .expect("welcome should be returned");
    assert_eq!(result.0, "persisted welcome");
}

#[tokio::test]
async fn generate_welcome_regenerates_when_user_messaged_after_welcome() {
    let mut config_value = AgentConfig::default();
    config_value.concierge.detail_level = ConciergeDetailLevel::Minimal;
    let config = Arc::new(RwLock::new(config_value));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let threads = RwLock::new(HashMap::from([
        (
            CONCIERGE_THREAD_ID.to_string(),
            concierge_thread(vec![AgentMessage {
                provider: Some("concierge".into()),
                ..assistant_message("persisted welcome", now - 60_000)
            }]),
        ),
        (
            "thread-1".to_string(),
            thread_with_messages(
                "thread-1",
                "Thread One",
                now - 30_000,
                vec![AgentMessage::user("new user message", now - 30_000)],
            ),
        ),
    ]));

    let result = engine
        .generate_welcome(&threads, &Mutex::new(std::collections::VecDeque::new()))
        .await
        .expect("welcome should be returned");
    assert_ne!(result.0, "persisted welcome");
}

#[tokio::test]
async fn generate_welcome_reuses_persisted_welcome_when_only_heartbeat_ran_after() {
    let mut config_value = AgentConfig::default();
    config_value.concierge.detail_level = ConciergeDetailLevel::Minimal;
    let config = Arc::new(RwLock::new(config_value));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let threads = RwLock::new(HashMap::from([
        (
            CONCIERGE_THREAD_ID.to_string(),
            concierge_thread(vec![AgentMessage {
                provider: Some("concierge".into()),
                ..assistant_message("persisted welcome", now - 60_000)
            }]),
        ),
        (
            "thread-heartbeat".to_string(),
            thread_with_messages(
                "thread-heartbeat",
                "HEARTBEAT SYNTHESIS",
                now - 10_000,
                vec![
                    user_message(
                        "HEARTBEAT SYNTHESIS\nYou are performing a scheduled heartbeat check for the operator.",
                        now - 10_000,
                    ),
                    assistant_message(
                        "ACTIONABLE: false\nDIGEST: All systems normal.\nITEMS:",
                        now - 9_000,
                    ),
                ],
            ),
        ),
    ]));

    let result = engine
        .generate_welcome(&threads, &Mutex::new(std::collections::VecDeque::new()))
        .await
        .expect("welcome should be returned");
    assert_eq!(result.0, "persisted welcome");
}

#[tokio::test]
async fn generate_welcome_regenerates_when_persisted_welcome_is_stale() {
    let mut config_value = AgentConfig::default();
    config_value.concierge.detail_level = ConciergeDetailLevel::Minimal;
    let config = Arc::new(RwLock::new(config_value));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let threads = RwLock::new(HashMap::from([
        (
            CONCIERGE_THREAD_ID.to_string(),
            concierge_thread(vec![AgentMessage {
                provider: Some("concierge".into()),
                ..assistant_message("persisted welcome", now - WELCOME_REUSE_WINDOW_MS - 1)
            }]),
        ),
        (
            "thread-1".to_string(),
            thread_with_messages(
                "thread-1",
                "Thread One",
                now - 30_000,
                vec![assistant_message("old reply", now - 30_000)],
            ),
        ),
    ]));

    let result = engine
        .generate_welcome(&threads, &Mutex::new(std::collections::VecDeque::new()))
        .await
        .expect("welcome should be returned");
    assert_ne!(result.0, "persisted welcome");
}
