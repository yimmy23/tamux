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
    let mut operator_events = event_tx.subscribe();
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
async fn read_skill_uses_workspace_root_when_session_is_absent() {
    let _cwd_lock = current_dir_test_lock().lock().expect("cwd lock");
    let original_cwd = std::env::current_dir().expect("current dir");

    let root = tempdir().expect("tempdir");
    let frontend_root = tempdir().expect("tempdir frontend");
    let agent_data_dir = root.path().join("agent");
    fs::create_dir_all(&agent_data_dir).expect("create agent data dir");
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='workspace-root'\nversion='0.1.0'\n[dependencies]\ntokio='1'\n",
    )
    .expect("write cargo workspace");
    fs::write(
        frontend_root.path().join("package.json"),
        r#"{"name":"frontend-root","dependencies":{"react":"19.0.0"}}"#,
    )
    .expect("write package json");

    let generated_dir = root.path().join("skills").join("generated");
    fs::create_dir_all(&generated_dir).expect("create generated dir");
    fs::write(
        generated_dir.join("build-pipeline.md"),
        "# Build pipeline\nRun cargo build for rust workspaces.\n",
    )
    .expect("write rust skill");
    fs::write(
        generated_dir.join("build-pipeline--frontend.md"),
        "# Frontend build pipeline\nUse react build checks.\n",
    )
    .expect("write frontend skill");

    std::env::set_current_dir(frontend_root.path()).expect("set frontend cwd");

    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let tool_call = ToolCall::with_default_weles_review(
        "tool-read-skill-workspace-root".to_string(),
        ToolFunction {
            name: "read_skill".to_string(),
            arguments: serde_json::json!({
                "skill": "build-pipeline",
                "max_lines": 50
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-read-skill-workspace-root",
        None,
        &manager,
        None,
        &event_tx,
        &agent_data_dir,
        &engine.http_client,
        None,
    )
    .await;

    std::env::set_current_dir(&original_cwd).expect("restore cwd");

    assert!(!result.is_error, "read_skill should succeed: {}", result.content);
    assert!(
        result.content.contains("build-pipeline.md"),
        "read_skill should resolve the rust workspace variant from workspace_root fallback: {}",
        result.content
    );
    assert!(
        !result.content.contains("build-pipeline--frontend.md"),
        "read_skill should not resolve from process cwd when workspace_root is available: {}",
        result.content
    );
}

#[tokio::test]
async fn read_skill_clears_stale_variant_gate_when_same_skill_family_is_read() {
    let root = tempdir().expect("tempdir");
    let agent_data_dir = root.path().join("agent");
    fs::create_dir_all(&agent_data_dir).expect("create agent data dir");
    let generated_dir = root.path().join("skills").join("generated");
    fs::create_dir_all(&generated_dir).expect("create generated dir");
    fs::write(
        generated_dir.join("systematic-debugging.md"),
        "# Systematic debugging\nUse this workflow to debug panic in rust services.\n",
    )
    .expect("write skill");

    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let thread_id = "thread-read-skill-stale-variant";
    engine
        .set_thread_skill_discovery_state(
            thread_id,
            crate::agent::types::LatestSkillDiscoveryState {
                query: "debug panic".to_string(),
                confidence_tier: "strong".to_string(),
                recommended_skill: Some("systematic-debugging".to_string()),
                recommended_action: "read_skill systematic-debugging".to_string(),
                mesh_next_step: Some(crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill),
                mesh_requires_approval: false,
                mesh_approval_id: None,
                read_skill_identifier: Some("stale-variant-id".to_string()),
                skip_rationale: None,
                discovery_pending: false,
                skill_read_completed: false,
                compliant: false,
                updated_at: 1,
            },
        )
        .await;
    let (event_tx, _) = broadcast::channel(8);

    let tool_call = ToolCall::with_default_weles_review(
        "tool-read-skill-stale-variant".to_string(),
        ToolFunction {
            name: "read_skill".to_string(),
            arguments: serde_json::json!({
                "skill": "systematic-debugging",
                "max_lines": 50
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        &agent_data_dir,
        &engine.http_client,
        None,
    )
    .await;

    assert!(!result.is_error, "read_skill should succeed: {}", result.content);
    let state = engine
        .get_thread_skill_discovery_state(thread_id)
        .await
        .expect("thread skill state should remain present");
    assert!(state.skill_read_completed);
    assert!(state.compliant);
}

#[tokio::test]
async fn read_skill_resolves_nested_skill_by_frontmatter_name() {
    let root = tempdir().expect("tempdir");
    let agent_data_dir = root.path().join("agent");
    fs::create_dir_all(&agent_data_dir).expect("create agent data dir");
    let skill_path = root
        .path()
        .join("skills")
        .join("development")
        .join("superpowers")
        .join("alias-dir")
        .join("SKILL.md");
    fs::create_dir_all(skill_path.parent().expect("skill directory"))
        .expect("create skill directory");
    fs::write(
        &skill_path,
        "---\nname: subagent-driven-development\ndescription: Execute implementation work through subagents.\n---\n# Subagent-Driven Development\n",
    )
    .expect("write skill");

    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let tool_call = ToolCall::with_default_weles_review(
        "tool-read-skill-frontmatter-name".to_string(),
        ToolFunction {
            name: "read_skill".to_string(),
            arguments: serde_json::json!({
                "skill": "subagent-driven-development",
                "max_lines": 50
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-read-skill-frontmatter-name",
        None,
        &manager,
        None,
        &event_tx,
        &agent_data_dir,
        &engine.http_client,
        None,
    )
    .await;

    assert!(!result.is_error, "read_skill should succeed: {}", result.content);
    assert!(
        result.content.contains("alias-dir/SKILL.md"),
        "read_skill should resolve the nested skill entrypoint: {}",
        result.content
    );
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

#[test]
fn default_offload_threshold_is_50kb() {
    assert_eq!(
        AgentConfig::default().offload_tool_result_threshold_bytes,
        50 * 1024
    );
}

#[tokio::test]
async fn read_offloaded_payload_tool_reads_canonical_path_even_if_metadata_storage_path_is_tampered() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let thread_id = "thread-offloaded-read";
    let payload_id = "payload-read-123";
    let raw_payload = "first line\nAuthorization: Bearer super_secret_token_123\nthird line";
    let payload_path = root
        .path()
        .join("offloaded-payloads")
        .join(thread_id)
        .join(format!("{payload_id}.txt"));
    std::fs::create_dir_all(payload_path.parent().expect("payload parent"))
        .expect("create payload directory");
    std::fs::write(&payload_path, raw_payload).expect("write raw offloaded payload");
    engine
        .history
        .upsert_offloaded_payload_metadata(
            payload_id,
            thread_id,
            "bash_command",
            Some("tool-call-read"),
            "text/plain",
            raw_payload.len() as u64,
            "summary placeholder",
            1_700_000_000,
        )
        .await
        .expect("store offloaded payload metadata");

    let tampered_path = root.path().join("outside.txt");
    std::fs::write(&tampered_path, "tampered path payload")
        .expect("write tampered payload outside canonical path");
    let tampered_storage_path = tampered_path.to_string_lossy().into_owned();
    let payload_id_for_update = payload_id.to_string();
    engine
        .history
        .conn
        .call(move |conn| {
            conn.execute(
                "UPDATE offloaded_payloads SET storage_path = ?1 WHERE payload_id = ?2",
                rusqlite::params![tampered_storage_path, payload_id_for_update],
            )?;
            Ok(())
        })
        .await
        .expect("update tampered storage path");

    let tools = get_available_tools(&AgentConfig::default(), root.path(), false);
    let read_offloaded_payload = tools
        .iter()
        .find(|tool| tool.function.name == "read_offloaded_payload")
        .expect("read_offloaded_payload tool should be available");
    let properties = read_offloaded_payload
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("read_offloaded_payload schema should expose properties");
    assert!(
        properties.contains_key("payload_id"),
        "read_offloaded_payload schema should include payload_id"
    );

    let tool_call = ToolCall::with_default_weles_review(
        "tool-read-offloaded-payload".to_string(),
        ToolFunction {
            name: "read_offloaded_payload".to_string(),
            arguments: serde_json::json!({ "payload_id": payload_id }).to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
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
        "read_offloaded_payload should succeed with valid payload_id: {}",
        result.content
    );
    assert!(result.pending_approval.is_none());
    assert_eq!(result.content, raw_payload);
}

#[tokio::test]
async fn read_offloaded_payload_tool_rejects_cross_thread_metadata_reads() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let owner_thread_id = "thread-offloaded-owner";
    let caller_thread_id = "thread-offloaded-other";
    let payload_id = "payload-cross-thread-123";
    let raw_payload = "cross-thread payload should stay private";
    let payload_path = root
        .path()
        .join("offloaded-payloads")
        .join(owner_thread_id)
        .join(format!("{payload_id}.txt"));
    std::fs::create_dir_all(payload_path.parent().expect("payload parent"))
        .expect("create payload directory");
    std::fs::write(&payload_path, raw_payload).expect("write raw offloaded payload");

    engine
        .history
        .upsert_offloaded_payload_metadata(
            payload_id,
            owner_thread_id,
            "bash_command",
            Some("tool-call-cross-thread"),
            "text/plain",
            raw_payload.len() as u64,
            "summary placeholder",
            1_700_000_002,
        )
        .await
        .expect("store offloaded payload metadata");

    let tool_call = ToolCall::with_default_weles_review(
        "tool-read-offloaded-payload-cross-thread".to_string(),
        ToolFunction {
            name: "read_offloaded_payload".to_string(),
            arguments: serde_json::json!({ "payload_id": payload_id }).to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        caller_thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(result.is_error, "cross-thread read should fail safely");
    assert!(result.pending_approval.is_none());
    assert!(
        result.content.contains("offloaded payload not found"),
        "expected thread-scoped rejection, got: {}",
        result.content
    );
}

#[tokio::test]
async fn read_offloaded_payload_tool_rejects_paths_that_escape_the_daemon_root() {
    let root = tempdir().expect("tempdir");
    let escaped_root = tempfile::tempdir_in(root.path().parent().expect("root parent"))
        .expect("external tempdir");
    std::fs::create_dir_all(root.path().join("offloaded-payloads"))
        .expect("create daemon offloaded payload root");

    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let payload_id = "payload-escape-123";
    let escaped_component = escaped_root
        .path()
        .file_name()
        .and_then(|value| value.to_str())
        .expect("external tempdir basename");
    let thread_id = format!("../../{escaped_component}");
    let escaped_payload_path = escaped_root.path().join(format!("{payload_id}.txt"));
    std::fs::write(&escaped_payload_path, "outside daemon root")
        .expect("write escaped payload file");

    engine
        .history
        .upsert_offloaded_payload_metadata(
            payload_id,
            &thread_id,
            "bash_command",
            Some("tool-call-escape"),
            "text/plain",
            19,
            "summary placeholder",
            1_700_000_001,
        )
        .await
        .expect("store escaped payload metadata");

    let tool_call = ToolCall::with_default_weles_review(
        "tool-read-offloaded-payload-escape".to_string(),
        ToolFunction {
            name: "read_offloaded_payload".to_string(),
            arguments: serde_json::json!({ "payload_id": payload_id }).to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        &thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(result.is_error, "escaped path should fail safely");
    assert!(result.pending_approval.is_none());
    assert!(
        result.content.contains("outside daemon-owned root"),
        "expected containment failure, got: {}",
        result.content
    );
}

#[tokio::test]
async fn read_offloaded_payload_tool_rejects_payload_ids_that_escape_the_caller_thread_subtree() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let caller_thread_id = "thread-offloaded-owner";
    let sibling_thread_id = "thread-offloaded-sibling";
    let payload_leaf = "payload-cross-scope-123";
    let payload_id = format!("../{sibling_thread_id}/{payload_leaf}");
    let raw_payload = "sibling-thread payload should stay private";

    let sibling_payload_path = root
        .path()
        .join("offloaded-payloads")
        .join(sibling_thread_id)
        .join(format!("{payload_leaf}.txt"));
    std::fs::create_dir_all(sibling_payload_path.parent().expect("payload parent"))
        .expect("create sibling payload directory");
    std::fs::write(&sibling_payload_path, raw_payload).expect("write sibling payload");
    std::fs::create_dir_all(
        root.path()
            .join("offloaded-payloads")
            .join(caller_thread_id),
    )
    .expect("create caller thread directory");

    engine
        .history
        .upsert_offloaded_payload_metadata(
            &payload_id,
            caller_thread_id,
            "bash_command",
            Some("tool-call-payload-traversal"),
            "text/plain",
            raw_payload.len() as u64,
            "summary placeholder",
            1_700_000_003,
        )
        .await
        .expect("store traversal metadata row");

    let tool_call = ToolCall::with_default_weles_review(
        "tool-read-offloaded-payload-thread-subtree".to_string(),
        ToolFunction {
            name: "read_offloaded_payload".to_string(),
            arguments: serde_json::json!({ "payload_id": payload_id }).to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        caller_thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(result.is_error, "payload subtree escape should fail safely");
    assert!(result.pending_approval.is_none());
    assert!(
        result.content.contains("offloaded payload not found"),
        "expected thread-scoped rejection, got: {}",
        result.content
    );
}

#[tokio::test]
async fn large_tool_result_is_offloaded_and_thread_message_keeps_summary() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.offload_tool_result_threshold_bytes = 64;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let raw_payload = "tool output line\n".repeat(16);
    let prepared = crate::agent::agent_loop::send_message::tool_results::prepare_tool_result_thread_message(
        &engine,
        "thread-offload-large",
        &ToolResult {
            tool_call_id: "tool-call-large".to_string(),
            name: "bash_command".to_string(),
            content: raw_payload.clone(),
            is_error: false,
            weles_review: None,
            pending_approval: None,
        },
        1_700_000_123,
    )
    .await;

    let payload_id = prepared
        .offloaded_payload_id
        .clone()
        .expect("large tool result should be offloaded");
    let expected_summary = format!(
        "Tool result offloaded\n- tool: bash_command\n- status: done\n- bytes: {}\n- payload_id: {}\n- key findings:\n  - tool output line\n  - tool output line\n  - tool output line",
        raw_payload.len(), payload_id
    );
    assert_eq!(prepared.content, expected_summary);

    let metadata = engine
        .history
        .get_offloaded_payload_metadata(&payload_id)
        .await
        .expect("metadata lookup should succeed")
        .expect("metadata row should exist for offloaded payload");
    assert_eq!(metadata.thread_id, "thread-offload-large");
    assert_eq!(metadata.tool_name, "bash_command");
    assert_eq!(metadata.tool_call_id.as_deref(), Some("tool-call-large"));
    assert_eq!(metadata.content_type, "text/plain");
    assert_eq!(metadata.byte_size, raw_payload.len() as u64);
    assert_eq!(metadata.summary, expected_summary);

    let payload_path = root
        .path()
        .join("offloaded-payloads")
        .join("thread-offload-large")
        .join(format!("{payload_id}.txt"));
    assert_eq!(
        std::fs::read_to_string(&payload_path).expect("offloaded payload file should exist"),
        raw_payload
    );
}

#[tokio::test]
async fn large_read_offloaded_payload_result_stays_inline_in_thread_messages() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.offload_tool_result_threshold_bytes = 64;
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let thread_id = "thread-read-inline";
    let payload_id = "payload-read-inline-123";
    let raw_payload = "retrieved payload line\n".repeat(16);
    let payload_path = root
        .path()
        .join("offloaded-payloads")
        .join(thread_id)
        .join(format!("{payload_id}.txt"));
    std::fs::create_dir_all(payload_path.parent().expect("payload parent"))
        .expect("create payload directory");
    std::fs::write(&payload_path, &raw_payload).expect("write raw offloaded payload");

    engine
        .history
        .upsert_offloaded_payload_metadata(
            payload_id,
            thread_id,
            "bash_command",
            Some("tool-call-read-inline"),
            "text/plain",
            raw_payload.len() as u64,
            "summary placeholder",
            1_700_000_124,
        )
        .await
        .expect("store offloaded payload metadata");

    let tool_call = ToolCall::with_default_weles_review(
        "tool-read-offloaded-payload-inline".to_string(),
        ToolFunction {
            name: "read_offloaded_payload".to_string(),
            arguments: serde_json::json!({ "payload_id": payload_id }).to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
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
        "read_offloaded_payload should succeed for inline-preservation regression: {}",
        result.content
    );
    assert_eq!(result.content, raw_payload);

    let prepared = crate::agent::agent_loop::send_message::tool_results::prepare_tool_result_thread_message(
        &engine,
        thread_id,
        &result,
        1_700_000_125,
    )
    .await;

    assert_eq!(prepared.content, raw_payload);
    assert_eq!(prepared.offloaded_payload_id, None);

    let metadata = engine
        .history
        .list_offloaded_payload_metadata_for_thread(thread_id)
        .await
        .expect("metadata lookup should succeed");
    assert_eq!(metadata.len(), 1, "read tool result should not create a second offloaded payload row");
    assert_eq!(metadata[0].payload_id, payload_id);
}

#[tokio::test]
async fn offloaded_tool_result_falls_back_to_inline_content_when_persist_fails() {
    let root = tempdir().expect("tempdir");
    std::fs::write(root.path().join("offloaded-payloads"), "blocked")
        .expect("block offloaded payload directory creation");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.offload_tool_result_threshold_bytes = 8;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let raw_payload = "payload that should have been offloaded".to_string();
    let prepared = crate::agent::agent_loop::send_message::tool_results::prepare_tool_result_thread_message(
        &engine,
        "thread-inline-fallback",
        &ToolResult {
            tool_call_id: "tool-call-inline-fallback".to_string(),
            name: "bash_command".to_string(),
            content: raw_payload.clone(),
            is_error: false,
            weles_review: None,
            pending_approval: None,
        },
        1_700_000_456,
    )
    .await;

    assert_eq!(prepared.content, raw_payload);
    assert_eq!(prepared.offloaded_payload_id, None);

    let metadata = engine
        .history
        .list_offloaded_payload_metadata_for_thread("thread-inline-fallback")
        .await
        .expect("metadata lookup should succeed");
    assert!(
        metadata.is_empty(),
        "failed offload should not persist metadata rows"
    );
}

#[tokio::test]
async fn offloaded_tool_result_cleans_up_payload_file_when_metadata_write_fails() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.offload_tool_result_threshold_bytes = 8;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .history
        .conn
        .call(|conn| {
            conn.execute("DROP TABLE offloaded_payloads", [])?;
            Ok(())
        })
        .await
        .expect("drop offloaded payload metadata table");

    let raw_payload = "payload that should be cleaned up after metadata failure".to_string();
    let prepared = crate::agent::agent_loop::send_message::tool_results::prepare_tool_result_thread_message(
        &engine,
        "thread-inline-cleanup",
        &ToolResult {
            tool_call_id: "tool-call-inline-cleanup".to_string(),
            name: "bash_command".to_string(),
            content: raw_payload.clone(),
            is_error: false,
            weles_review: None,
            pending_approval: None,
        },
        1_700_000_789,
    )
    .await;

    assert_eq!(prepared.content, raw_payload);
    assert_eq!(prepared.offloaded_payload_id, None);

    let payload_dir = root
        .path()
        .join("offloaded-payloads")
        .join("thread-inline-cleanup");
    let remaining_files = if payload_dir.exists() {
        std::fs::read_dir(&payload_dir)
            .expect("read payload cleanup directory")
            .count()
    } else {
        0
    };
    assert_eq!(remaining_files, 0, "metadata write failure should clean up payload file");
}
