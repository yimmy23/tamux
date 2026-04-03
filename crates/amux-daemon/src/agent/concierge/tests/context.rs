use super::*;

#[tokio::test]
async fn context_summary_gathers_opening_message_recent_messages_and_bounded_tasks() {
    let config = Arc::new(RwLock::new(AgentConfig::default()));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let threads = RwLock::new(HashMap::from([
        (
            "thread-1".to_string(),
            AgentThread {
                id: "thread-1".to_string(),
                agent_name: None,
                title: "Newest".to_string(),
                created_at: 1,
                updated_at: now,
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![
                    AgentMessage::user("kickoff scope", now - 8),
                    assistant_message("reply-1", now - 7),
                    AgentMessage::user("msg-2", now - 6),
                    assistant_message("msg-3", now - 5),
                    AgentMessage::user("msg-4", now - 4),
                    assistant_message("msg-5", now - 3),
                    AgentMessage::user("msg-6", now - 2),
                ],
            },
        ),
        (
            "thread-2".to_string(),
            AgentThread {
                id: "thread-2".to_string(),
                agent_name: None,
                title: "Older".to_string(),
                created_at: 1,
                updated_at: now - 1_000,
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![assistant_message("old", now - 1_000)],
            },
        ),
    ]));
    let tasks = Mutex::new(std::collections::VecDeque::from([
        sample_task("task-1", "oldest-1", now - 600),
        sample_task("task-2", "oldest-2", now - 500),
        sample_task("task-3", "middle", now - 400),
        sample_task("task-4", "newest-3", now - 300),
        sample_task("task-5", "newest-2", now - 200),
        sample_task("task-6", "newest-1", now - 100),
    ]));

    let context = engine
        .gather_context(&threads, &tasks, ConciergeDetailLevel::ContextSummary)
        .await;

    assert_eq!(context.recent_threads.len(), 1);
    assert_eq!(context.recent_threads[0].title, "Newest");
    assert_eq!(
        context.recent_threads[0].opening_message.as_deref(),
        Some("User: kickoff scope")
    );
    assert_eq!(context.recent_threads[0].last_messages.len(), 5);
    assert_eq!(context.pending_task_total, 6);
    assert_eq!(context.pending_tasks.len(), 5);
}

#[tokio::test]
async fn context_summary_prefers_tasks_for_latest_thread_before_global_fallback() {
    let config = Arc::new(RwLock::new(AgentConfig::default()));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let threads = RwLock::new(HashMap::from([(
        "thread-1".to_string(),
        AgentThread {
            id: "thread-1".to_string(),
            agent_name: None,
            title: "Newest".to_string(),
            created_at: 1,
            updated_at: now,
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
            messages: vec![AgentMessage::user("kickoff", now - 5)],
        },
    )]));
    let tasks = Mutex::new(std::collections::VecDeque::from([
        sample_task_for_thread("task-a", "thread task old", now - 250, "thread-1"),
        sample_task_for_thread("task-b", "thread task new", now - 150, "thread-1"),
        sample_task("task-c", "global old", now - 500),
        sample_task("task-d", "global middle", now - 300),
        sample_task("task-e", "global new-2", now - 100),
        sample_task("task-f", "global new-1", now - 10),
    ]));

    let context = engine
        .gather_context(&threads, &tasks, ConciergeDetailLevel::ContextSummary)
        .await;

    assert_eq!(context.pending_task_total, 6);
    assert!(context
        .pending_tasks
        .iter()
        .any(|task| task.contains("thread task old")));
    assert!(context
        .pending_tasks
        .iter()
        .any(|task| task.contains("thread task new")));
    assert!(context
        .pending_tasks
        .iter()
        .any(|task| task.contains("global old")));
    assert!(context
        .pending_tasks
        .iter()
        .any(|task| task.contains("global new-2")));
    assert!(context
        .pending_tasks
        .iter()
        .any(|task| task.contains("global new-1")));
    assert!(!context
        .pending_tasks
        .iter()
        .any(|task| task.contains("global middle")));
}

#[tokio::test]
async fn context_summary_ignores_assistant_only_concierge_like_threads() {
    let config = Arc::new(RwLock::new(AgentConfig::default()));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let threads = RwLock::new(HashMap::from([
        (
            "thread-real".to_string(),
            AgentThread {
                id: "thread-real".to_string(),
                agent_name: None,
                title: "Actual work".to_string(),
                created_at: 1,
                updated_at: now - 100,
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![
                    AgentMessage::user("fix concierge context", now - 120),
                    assistant_message("working on it", now - 110),
                ],
            },
        ),
        (
            "thread-meta".to_string(),
            AgentThread {
                id: "thread-meta".to_string(),
                agent_name: None,
                title: "Concierge".to_string(),
                created_at: 1,
                updated_at: now,
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![assistant_message("welcome back", now - 10)],
            },
        ),
    ]));
    let tasks = Mutex::new(std::collections::VecDeque::new());

    let context = engine
        .gather_context(&threads, &tasks, ConciergeDetailLevel::ContextSummary)
        .await;

    assert_eq!(context.recent_threads.len(), 1);
    assert_eq!(context.recent_threads[0].id, "thread-real");
}

#[tokio::test]
async fn context_summary_excludes_structured_heartbeat_threads() {
    let config = Arc::new(RwLock::new(AgentConfig::default()));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let threads = RwLock::new(HashMap::from([
        (
            "thread-real".to_string(),
            thread_with_messages(
                "thread-real",
                "Actual work",
                now - 100,
                vec![
                    user_message("fix concierge context", now - 120),
                    assistant_message("working on it", now - 110),
                ],
            ),
        ),
        (
            "thread-heartbeat".to_string(),
            thread_with_messages(
                "thread-heartbeat",
                "HEARTBEAT SYNTHESIS",
                now,
                vec![
                    user_message(
                        "HEARTBEAT SYNTHESIS\nYou are performing a scheduled heartbeat check for the operator.",
                        now - 20,
                    ),
                    assistant_message(
                        "ACTIONABLE: false\nDIGEST: All systems normal.\nITEMS:",
                        now - 10,
                    ),
                ],
            ),
        ),
    ]));
    let tasks = Mutex::new(std::collections::VecDeque::new());

    let context = engine
        .gather_context(&threads, &tasks, ConciergeDetailLevel::ContextSummary)
        .await;

    assert_eq!(context.recent_threads.len(), 1);
    assert_eq!(context.recent_threads[0].id, "thread-real");
}

#[tokio::test]
async fn context_summary_hides_weles_internal_threads_and_tasks() {
    let config = Arc::new(RwLock::new(AgentConfig::default()));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let threads = RwLock::new(HashMap::from([
        (
            "thread-real".to_string(),
            thread_with_messages(
                "thread-real",
                "Actual work",
                now - 100,
                vec![
                    user_message("fix concierge context", now - 120),
                    assistant_message("working on it", now - 110),
                ],
            ),
        ),
        (
            "thread-weles".to_string(),
            thread_with_messages(
                "thread-weles",
                "WELES governance runtime thread",
                now,
                vec![
                    user_message(
                        &crate::agent::agent_identity::build_weles_persona_prompt("governance"),
                        now - 20,
                    ),
                    assistant_message("Internal governance review", now - 10),
                ],
            ),
        ),
    ]));
    let mut weles_task = sample_task("task-weles", "WELES", now - 10);
    weles_task.sub_agent_def_id = Some("weles_builtin".to_string());
    let tasks = Mutex::new(std::collections::VecDeque::from([
        sample_task("task-real", "real task", now - 20),
        weles_task,
    ]));

    let context = engine
        .gather_context(&threads, &tasks, ConciergeDetailLevel::ContextSummary)
        .await;

    assert_eq!(context.recent_threads.len(), 1);
    assert_eq!(context.recent_threads[0].id, "thread-real");
    assert_eq!(context.pending_task_total, 1);
    assert_eq!(context.pending_tasks.len(), 1);
    assert!(context.pending_tasks[0].contains("real task"));
}
