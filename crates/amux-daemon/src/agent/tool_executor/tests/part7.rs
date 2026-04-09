fn make_thread(
    id: &str,
    agent_name: Option<&str>,
    title: &str,
    pinned: bool,
    created_at: u64,
    updated_at: u64,
    messages: Vec<crate::agent::types::AgentMessage>,
) -> crate::agent::types::AgentThread {
    crate::agent::types::AgentThread {
        id: id.to_string(),
        agent_name: agent_name.map(ToOwned::to_owned),
        title: title.to_string(),
        messages,
        pinned,
        upstream_thread_id: None,
        upstream_transport: None,
        upstream_provider: None,
        upstream_model: None,
        upstream_assistant_id: None,
        total_input_tokens: 0,
        total_output_tokens: 0,
        created_at,
        updated_at,
    }
}

fn weles_internal_message(ts: u64) -> crate::agent::types::AgentMessage {
    let mut message = crate::agent::types::AgentMessage::user(
        crate::agent::agent_identity::build_weles_persona_prompt(
            crate::agent::agent_identity::WELES_GOVERNANCE_SCOPE,
        ),
        ts,
    );
    message.role = crate::agent::types::MessageRole::Assistant;
    message
}

#[tokio::test]
async fn list_threads_tool_returns_filtered_visible_summaries() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let tools = get_available_tools(&AgentConfig::default(), root.path(), false);
    let list_threads = tools
        .iter()
        .find(|tool| tool.function.name == "list_threads")
        .expect("list_threads tool should be available");
    let properties = list_threads
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("list_threads schema should expose properties");
    for field in [
        "created_after",
        "created_before",
        "updated_after",
        "updated_before",
        "agent_name",
        "title_query",
        "pinned",
        "include_internal",
        "limit",
        "offset",
    ] {
        assert!(
            properties.contains_key(field),
            "list_threads schema should include {field}"
        );
    }

    let mut threads = engine.threads.write().await;
    threads.insert(
        "thread-alpha".to_string(),
        make_thread(
            "thread-alpha",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Alpha project thread",
            true,
            120,
            350,
            vec![crate::agent::types::AgentMessage::user("operator message", 120)],
        ),
    );
    threads.insert(
        "thread-beta".to_string(),
        make_thread(
            "thread-beta",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Beta project thread",
            false,
            121,
            351,
            vec![crate::agent::types::AgentMessage::user("beta message", 121)],
        ),
    );
    threads.insert(
        "thread-hidden".to_string(),
        make_thread(
            "thread-hidden",
            Some(crate::agent::agent_identity::WELES_AGENT_NAME),
            "Alpha internal thread",
            true,
            122,
            352,
            vec![weles_internal_message(122)],
        ),
    );
    drop(threads);

    let tool_call = ToolCall::with_default_weles_review(
        "tool-list-threads".to_string(),
        ToolFunction {
            name: "list_threads".to_string(),
            arguments: serde_json::json!({
                "created_after": 100,
                "created_before": 200,
                "updated_after": 300,
                "updated_before": 400,
                "agent_name": "main-agent",
                "title_query": " alpha ",
                "pinned": true,
                "limit": 10,
                "offset": 0
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-list-threads",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "list_threads should succeed with valid filters: {}",
        result.content
    );
    assert!(result.pending_approval.is_none());

    let payload: serde_json::Value =
        serde_json::from_str(&result.content).expect("list_threads should return JSON");
    let rows = payload
        .as_array()
        .expect("list_threads should return an array");
    assert_eq!(rows.len(), 1, "only the matching visible thread should be returned");
    assert_eq!(rows[0].get("id").and_then(|value| value.as_str()), Some("thread-alpha"));
    assert_eq!(
        rows[0].get("title").and_then(|value| value.as_str()),
        Some("Alpha project thread")
    );
    assert_eq!(rows[0].get("pinned").and_then(|value| value.as_bool()), Some(true));
    assert_eq!(
        rows[0]
            .get("messages")
            .and_then(|value| value.as_array())
            .map(std::vec::Vec::len),
        Some(0),
        "list_threads should return summary rows without message history"
    );
}

#[tokio::test]
async fn ask_questions_tool_waits_for_operator_choice() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let mut operator_events = engine.event_tx.subscribe();

    let tool_call = ToolCall::with_default_weles_review(
        "tool-ask-questions".to_string(),
        ToolFunction {
            name: "ask_questions".to_string(),
            arguments: serde_json::json!({
                "content": "Choose:\nA. Alpha\nB. Beta",
                "options": ["A", "B"]
            })
            .to_string(),
        },
    );

    let engine_for_task = engine.clone();
    let manager_for_task = manager.clone();
    let task = tokio::spawn(async move {
        execute_tool(
            &tool_call,
            &engine_for_task,
            "thread-ask-questions",
            None,
            &manager_for_task,
            None,
            &event_tx,
            root.path(),
            &engine_for_task.http_client,
            None,
        )
        .await
    });

    let question_id = match tokio::time::timeout(
        std::time::Duration::from_secs(2),
        operator_events.recv(),
    )
    .await
    .expect("operator question event should arrive promptly")
    .expect("operator question event")
    {
        AgentEvent::OperatorQuestion { question_id, .. } => question_id,
        other => panic!("expected operator question event, got {other:?}"),
    };

    engine
        .answer_operator_question(&question_id, "B")
        .await
        .expect("operator answer should unblock tool");

    let result = tokio::time::timeout(std::time::Duration::from_secs(2), task)
        .await
        .expect("tool task should finish promptly after the answer")
        .expect("tool task should join");
    assert!(!result.is_error, "ask_questions should succeed: {}", result.content);
    assert_eq!(result.content, "B");
    assert!(result.pending_approval.is_none());
}

#[tokio::test]
async fn discover_skills_tool_returns_discovery_result() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let tool_call = ToolCall::with_default_weles_review(
        "tool-discover-skills".to_string(),
        ToolFunction {
            name: "discover_skills".to_string(),
            arguments: serde_json::json!({
                "query": "debug panic",
                "limit": 3
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-discover-skills",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "discover_skills should succeed once wired: {}",
        result.content
    );
    assert!(result.pending_approval.is_none());

    let payload: serde_json::Value =
        serde_json::from_str(&result.content).expect("discover_skills should return JSON");
    assert_eq!(payload.get("query").and_then(|value| value.as_str()), Some("debug panic"));
    assert!(payload.get("confidence_tier").is_some(), "discovery result should include confidence tier");
    assert!(payload.get("recommended_action").is_some(), "discovery result should include recommended action");
}

#[tokio::test]
async fn list_tools_tool_returns_paginated_catalog() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let tool_call = ToolCall::with_default_weles_review(
        "tool-list-tools".to_string(),
        ToolFunction {
            name: "list_tools".to_string(),
            arguments: serde_json::json!({
                "limit": 5,
                "offset": 0
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-list-tools",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(!result.is_error, "list_tools should succeed: {}", result.content);
    let payload: serde_json::Value =
        serde_json::from_str(&result.content).expect("list_tools should return JSON");
    assert_eq!(payload.get("limit").and_then(|value| value.as_u64()), Some(5));
    assert_eq!(payload.get("offset").and_then(|value| value.as_u64()), Some(0));
    let items = payload
        .get("items")
        .and_then(|value| value.as_array())
        .expect("list_tools should return item array");
    assert!(!items.is_empty(), "list_tools should return at least one tool");
    assert!(
        items.iter().all(|item| item.get("name").is_some() && item.get("description").is_some()),
        "each listed tool should include name and description"
    );
}

#[tokio::test]
async fn tool_search_returns_ranked_matches() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let tool_call = ToolCall::with_default_weles_review(
        "tool-tool-search".to_string(),
        ToolFunction {
            name: "tool_search".to_string(),
            arguments: serde_json::json!({
                "query": "discover_skills",
                "limit": 5,
                "offset": 0
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-tool-search",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(!result.is_error, "tool_search should succeed: {}", result.content);
    let payload: serde_json::Value =
        serde_json::from_str(&result.content).expect("tool_search should return JSON");
    assert_eq!(
        payload.get("query").and_then(|value| value.as_str()),
        Some("discover_skills")
    );
    let items = payload
        .get("items")
        .and_then(|value| value.as_array())
        .expect("tool_search should return item array");
    assert!(!items.is_empty(), "tool_search should return at least one match");
    assert_eq!(
        items[0].get("name").and_then(|value| value.as_str()),
        Some("discover_skills")
    );
}

#[tokio::test]
async fn list_threads_tool_rejects_negative_offset() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let tool_call = ToolCall::with_default_weles_review(
        "tool-list-threads-negative-offset".to_string(),
        ToolFunction {
            name: "list_threads".to_string(),
            arguments: serde_json::json!({ "offset": -1 }).to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-list-threads-negative-offset",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(result.is_error, "negative offset should fail");
    assert!(result.pending_approval.is_none());
    assert!(result.content.contains("'offset' must be a non-negative integer"));
}

#[tokio::test]
async fn list_threads_tool_rejects_negative_limit() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let tool_call = ToolCall::with_default_weles_review(
        "tool-list-threads-negative-limit".to_string(),
        ToolFunction {
            name: "list_threads".to_string(),
            arguments: serde_json::json!({ "limit": -1 }).to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-list-threads-negative-limit",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(result.is_error, "negative limit should fail");
    assert!(result.pending_approval.is_none());
    assert!(result.content.contains("'limit' must be a non-negative integer"));
}

#[tokio::test]
async fn get_thread_tool_returns_truncated_thread_detail() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let tools = get_available_tools(&AgentConfig::default(), root.path(), false);
    let get_thread = tools
        .iter()
        .find(|tool| tool.function.name == "get_thread")
        .expect("get_thread tool should be available");
    let properties = get_thread
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("get_thread schema should expose properties");
    for field in ["thread_id", "limit", "offset", "include_internal"] {
        assert!(
            properties.contains_key(field),
            "get_thread schema should include {field}"
        );
    }

    engine.threads.write().await.insert(
        "thread-detail".to_string(),
        make_thread(
            "thread-detail",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Thread detail",
            false,
            200,
            270,
            vec![
                crate::agent::types::AgentMessage::user("one", 200),
                crate::agent::types::AgentMessage::user("two", 210),
                crate::agent::types::AgentMessage::user("three", 220),
                crate::agent::types::AgentMessage::user("four", 230),
                crate::agent::types::AgentMessage::user("five", 240),
                crate::agent::types::AgentMessage::user("six", 250),
                crate::agent::types::AgentMessage::user("seven", 260),
            ],
        ),
    );

    let tool_call = ToolCall::with_default_weles_review(
        "tool-get-thread".to_string(),
        ToolFunction {
            name: "get_thread".to_string(),
            arguments: serde_json::json!({ "thread_id": "thread-detail" }).to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-get-thread",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "get_thread should succeed with valid arguments: {}",
        result.content
    );
    assert!(result.pending_approval.is_none());

    let payload: serde_json::Value =
        serde_json::from_str(&result.content).expect("get_thread should return JSON");
    assert_eq!(
        payload
            .get("messages_truncated")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        payload
            .get("thread")
            .and_then(|value| value.get("id"))
            .and_then(|value| value.as_str()),
        Some("thread-detail")
    );
    let messages = payload
        .get("thread")
        .and_then(|value| value.get("messages"))
        .and_then(|value| value.as_array())
        .expect("thread detail should include messages");
    assert_eq!(messages.len(), 5);
    let contents = messages
        .iter()
        .map(|message| {
            message
                .get("content")
                .and_then(|value| value.as_str())
                .expect("message content should be present")
        })
        .collect::<Vec<_>>();
    assert_eq!(contents, vec!["three", "four", "five", "six", "seven"]);
}

#[tokio::test]
async fn get_thread_tool_applies_offset_from_most_recent_messages() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    engine.threads.write().await.insert(
        "thread-detail-offset".to_string(),
        make_thread(
            "thread-detail-offset",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Thread detail offset",
            false,
            200,
            260,
            vec![
                crate::agent::types::AgentMessage::user("one", 200),
                crate::agent::types::AgentMessage::user("two", 210),
                crate::agent::types::AgentMessage::user("three", 220),
                crate::agent::types::AgentMessage::user("four", 230),
                crate::agent::types::AgentMessage::user("five", 240),
                crate::agent::types::AgentMessage::user("six", 250),
            ],
        ),
    );

    let tool_call = ToolCall::with_default_weles_review(
        "tool-get-thread-offset".to_string(),
        ToolFunction {
            name: "get_thread".to_string(),
            arguments: serde_json::json!({
                "thread_id": "thread-detail-offset",
                "limit": 2,
                "offset": 1
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-get-thread-offset",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "get_thread offset page should succeed with valid arguments: {}",
        result.content
    );
    assert!(result.pending_approval.is_none());

    let payload: serde_json::Value =
        serde_json::from_str(&result.content).expect("get_thread should return JSON");
    assert_eq!(
        payload
            .get("messages_truncated")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    let messages = payload
        .get("thread")
        .and_then(|value| value.get("messages"))
        .and_then(|value| value.as_array())
        .expect("thread detail should include messages");
    let contents = messages
        .iter()
        .map(|message| {
            message
                .get("content")
                .and_then(|value| value.as_str())
                .expect("message content should be present")
        })
        .collect::<Vec<_>>();
    assert_eq!(contents, vec!["four", "five"]);
}

#[tokio::test]
async fn get_thread_tool_masks_hidden_internal_threads_without_include_internal() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    engine.threads.write().await.insert(
        "weles-hidden".to_string(),
        make_thread(
            "weles-hidden",
            Some(crate::agent::agent_identity::WELES_AGENT_NAME),
            "Weles hidden thread",
            false,
            500,
            550,
            vec![weles_internal_message(500)],
        ),
    );

    let tool_call = ToolCall::with_default_weles_review(
        "tool-get-thread-hidden".to_string(),
        ToolFunction {
            name: "get_thread".to_string(),
            arguments: serde_json::json!({
                "thread_id": "weles-hidden"
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-get-thread-hidden",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(result.is_error, "hidden internal thread should be masked");
    assert!(result.pending_approval.is_none());
    assert!(result.content.contains("thread not found"));
}

#[tokio::test]
async fn get_thread_tool_requires_thread_id_argument() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let tool_call = ToolCall::with_default_weles_review(
        "tool-get-thread-missing-thread-id".to_string(),
        ToolFunction {
            name: "get_thread".to_string(),
            arguments: serde_json::json!({}).to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-get-thread-missing-thread-id",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(result.is_error, "missing thread_id should fail");
    assert!(result.pending_approval.is_none());
    assert!(result.content.contains("missing 'thread_id' argument"));
}