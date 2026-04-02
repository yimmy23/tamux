use super::*;

#[test]
fn direct_message_entrypoints_box_large_send_message_futures() {
    let messaging_source =
        fs::read_to_string(repo_root().join("crates/amux-daemon/src/agent/messaging.rs"))
            .expect("read messaging.rs");
    let messaging_production = messaging_source
        .split("\n#[cfg(test)]")
        .next()
        .unwrap_or(messaging_source.as_str());
    let send_message_source = fs::read_to_string(
        repo_root().join("crates/amux-daemon/src/agent/agent_loop/send_message/mod.rs"),
    )
    .expect("read send_message/mod.rs");
    let send_message_production = send_message_source
        .split("\n#[cfg(test)]")
        .next()
        .unwrap_or(send_message_source.as_str());

    for required in [
        "Box::pin(self.send_message_inner(",
        "let target_thread_id = Box::pin(self.send_message_inner(",
    ] {
        assert!(
            messaging_production.contains(required),
            "messaging entrypoint should box oversized future: {required}"
        );
    }

    assert!(
        send_message_production
            .contains("Box::pin(run_with_agent_scope(agent_scope_id, async move {"),
        "send_message_inner should box the oversized agent loop future"
    );
}

#[test]
fn tool_execution_hot_path_boxes_large_futures() {
    let finalize_source = fs::read_to_string(
        repo_root().join("crates/amux-daemon/src/agent/agent_loop/send_message/finalize.rs"),
    )
    .expect("read finalize.rs");
    let finalize_production = finalize_source
        .split("\n#[cfg(test)]")
        .next()
        .unwrap_or(finalize_source.as_str());
    let tool_calls_source = fs::read_to_string(
        repo_root().join("crates/amux-daemon/src/agent/agent_loop/send_message/tool_calls.rs"),
    )
    .expect("read tool_calls.rs");
    let tool_calls_production = tool_calls_source
        .split("\n#[cfg(test)]")
        .next()
        .unwrap_or(tool_calls_source.as_str());
    let execute_tool_source = fs::read_to_string(
        repo_root().join("crates/amux-daemon/src/agent/tool_executor/execute_tool_impl.rs"),
    )
    .expect("read execute_tool_impl.rs");
    let execute_tool_production = execute_tool_source
        .split("\n#[cfg(test)]")
        .next()
        .unwrap_or(execute_tool_source.as_str());

    assert!(
        finalize_production.contains("Box::pin(self.handle_tool_calls_chunk("),
        "tool-call iteration handling should box the large handle_tool_calls_chunk future"
    );
    assert!(
        tool_calls_production.contains("Box::pin(execute_tool("),
        "tool execution callsites should box execute_tool futures"
    );
    assert!(
        execute_tool_production.contains("pub fn execute_tool<'a>("),
        "execute_tool should return an explicitly boxed future instead of an inline async fn"
    );
    assert!(
        execute_tool_production.contains(
            "-> std::pin::Pin<Box<dyn std::future::Future<Output = ToolResult> + Send + 'a>>"
        ),
        "execute_tool should expose a boxed future return type"
    );
    assert!(
        execute_tool_production.contains("Box::pin(async move {"),
        "execute_tool implementation should heap-box its async state machine"
    );
}

#[tokio::test]
async fn delete_thread_messages_updates_live_thread_and_persisted_history() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_test";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                title: "Test".to_string(),
                created_at: 1,
                updated_at: 1,
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![
                    AgentMessage::user("first", 1),
                    AgentMessage::user("second", 2),
                    AgentMessage::user("third", 3),
                ],
            },
        );
    }
    engine.persist_thread_by_id(thread_id).await;

    let msg_id = {
        let threads = engine.threads.read().await;
        threads.get(thread_id).unwrap().messages[1].id.clone()
    };
    let deleted = engine
        .delete_thread_messages(thread_id, &[msg_id])
        .await
        .expect("delete should succeed");
    assert_eq!(deleted, 1);

    let live = engine.threads.read().await;
    let thread = live.get(thread_id).expect("thread should still exist");
    assert_eq!(thread.messages.len(), 2);
    assert_eq!(thread.messages[0].content, "first");
    assert_eq!(thread.messages[1].content, "third");
    drop(live);

    let persisted = engine
        .history
        .list_messages(thread_id, Some(10))
        .await
        .unwrap();
    assert_eq!(persisted.len(), 2);
    assert_eq!(persisted[0].content, "first");
    assert_eq!(persisted[1].content, "third");
}

#[tokio::test]
async fn thread_client_surface_persists_with_thread_metadata() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_surface";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                title: "Surface".to_string(),
                created_at: 1,
                updated_at: 1,
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![AgentMessage::user("hello", 1)],
            },
        );
    }
    engine
        .set_thread_client_surface(thread_id, amux_protocol::ClientSurface::Tui)
        .await;
    engine.persist_thread_by_id(thread_id).await;

    let persisted = engine
        .history
        .get_thread(thread_id)
        .await
        .expect("read thread")
        .expect("thread should persist");
    let metadata = persisted.metadata_json.expect("thread metadata");
    assert!(metadata.contains("\"client_surface\":\"tui\""));

    let manager = SessionManager::new_test(root.path()).await;
    let rehydrated = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    rehydrated.hydrate().await.expect("hydrate");
    assert_eq!(
        rehydrated.get_thread_client_surface(thread_id).await,
        Some(amux_protocol::ClientSurface::Tui)
    );
}

#[tokio::test]
async fn delete_thread_messages_rehydrates_and_clears_invalid_continuation() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_continuation";

    let assistant_id = "assistant-anchor".to_string();
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                title: "Continuation".to_string(),
                created_at: 1,
                updated_at: 4,
                pinned: false,
                upstream_thread_id: Some("upstream-thread-1".to_string()),
                upstream_transport: Some(ApiTransport::Responses),
                upstream_provider: Some("github-copilot".to_string()),
                upstream_model: Some("gpt-5.4".to_string()),
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![
                    AgentMessage::user("first", 1),
                    AgentMessage {
                        id: assistant_id.clone(),
                        role: MessageRole::Assistant,
                        content: "answer".to_string(),
                        tool_calls: None,
                        tool_call_id: None,
                        tool_name: None,
                        tool_arguments: None,
                        tool_status: None,
                        weles_review: None,
                        input_tokens: 0,
                        output_tokens: 0,
                        provider: Some("github-copilot".to_string()),
                        model: Some("gpt-5.4".to_string()),
                        api_transport: Some(ApiTransport::Responses),
                        response_id: Some("resp_123".to_string()),
                        reasoning: None,
                        message_kind: AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        timestamp: 2,
                    },
                    AgentMessage::user("continue", 3),
                ],
            },
        );
    }
    engine.persist_thread_by_id(thread_id).await;

    engine
        .delete_thread_messages(thread_id, std::slice::from_ref(&assistant_id))
        .await
        .expect("delete should succeed");

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    assert_eq!(thread.messages.len(), 2);
    assert!(thread
        .messages
        .iter()
        .all(|message| message.response_id.is_none()));
    assert!(thread.upstream_thread_id.is_none());
    assert!(thread.upstream_transport.is_none());
    assert!(thread.upstream_provider.is_none());
    assert!(thread.upstream_model.is_none());
    drop(threads);

    let persisted = engine
        .history
        .list_messages(thread_id, Some(10))
        .await
        .unwrap();
    assert_eq!(persisted.len(), 2);
    assert!(persisted.iter().all(|message| {
        !message
            .metadata_json
            .as_deref()
            .unwrap_or_default()
            .contains("resp_123")
    }));
}

#[tokio::test]
async fn delete_thread_messages_removes_orphaned_tool_results_during_rebuild() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_orphans";

    let assistant_id = "assistant-tool-turn".to_string();
    let tool_a_id = "tool-a".to_string();
    let tool_b_id = "tool-b".to_string();
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                title: "Orphans".to_string(),
                created_at: 1,
                updated_at: 6,
                pinned: false,
                upstream_thread_id: Some("upstream-thread-2".to_string()),
                upstream_transport: Some(ApiTransport::Responses),
                upstream_provider: Some("github-copilot".to_string()),
                upstream_model: Some("gpt-5.4".to_string()),
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![
                    AgentMessage::user("start", 1),
                    AgentMessage {
                        id: assistant_id.clone(),
                        role: MessageRole::Assistant,
                        content: "checking".to_string(),
                        tool_calls: Some(vec![
                            ToolCall {
                                id: "call-a".to_string(),
                                function: ToolFunction {
                                    name: "tool_a".to_string(),
                                    arguments: "{}".to_string(),
                                },
                                weles_review: None,
                            },
                            ToolCall {
                                id: "call-b".to_string(),
                                function: ToolFunction {
                                    name: "tool_b".to_string(),
                                    arguments: "{}".to_string(),
                                },
                                weles_review: None,
                            },
                        ]),
                        tool_call_id: None,
                        tool_name: None,
                        tool_arguments: None,
                        tool_status: None,
                        weles_review: None,
                        input_tokens: 0,
                        output_tokens: 0,
                        provider: Some("github-copilot".to_string()),
                        model: Some("gpt-5.4".to_string()),
                        api_transport: Some(ApiTransport::Responses),
                        response_id: Some("resp_456".to_string()),
                        reasoning: None,
                        message_kind: AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        timestamp: 2,
                    },
                    AgentMessage {
                        id: tool_a_id.clone(),
                        role: MessageRole::Tool,
                        content: "partial".to_string(),
                        tool_calls: None,
                        tool_call_id: Some("call-a".to_string()),
                        tool_name: Some("tool_a".to_string()),
                        tool_arguments: Some("{}".to_string()),
                        tool_status: Some("done".to_string()),
                        weles_review: None,
                        input_tokens: 0,
                        output_tokens: 0,
                        provider: None,
                        model: None,
                        api_transport: None,
                        response_id: None,
                        reasoning: None,
                        message_kind: AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        timestamp: 3,
                    },
                    AgentMessage {
                        id: tool_b_id.clone(),
                        role: MessageRole::Tool,
                        content: "done".to_string(),
                        tool_calls: None,
                        tool_call_id: Some("call-b".to_string()),
                        tool_name: Some("tool_b".to_string()),
                        tool_arguments: Some("{}".to_string()),
                        tool_status: Some("done".to_string()),
                        weles_review: None,
                        input_tokens: 0,
                        output_tokens: 0,
                        provider: None,
                        model: None,
                        api_transport: None,
                        response_id: None,
                        reasoning: None,
                        message_kind: AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        timestamp: 4,
                    },
                    AgentMessage {
                        id: "assistant-final".to_string(),
                        role: MessageRole::Assistant,
                        content: "final answer".to_string(),
                        tool_calls: None,
                        tool_call_id: None,
                        tool_name: None,
                        tool_arguments: None,
                        tool_status: None,
                        weles_review: None,
                        input_tokens: 0,
                        output_tokens: 0,
                        provider: Some("github-copilot".to_string()),
                        model: Some("gpt-5.4".to_string()),
                        api_transport: Some(ApiTransport::Responses),
                        response_id: None,
                        reasoning: None,
                        message_kind: AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        timestamp: 5,
                    },
                ],
            },
        );
    }
    engine.persist_thread_by_id(thread_id).await;

    engine
        .delete_thread_messages(thread_id, std::slice::from_ref(&assistant_id))
        .await
        .expect("delete should succeed");

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    assert_eq!(thread.messages.len(), 2);
    assert_eq!(thread.messages[0].content, "start");
    assert_eq!(thread.messages[1].content, "final answer");
    assert!(thread
        .messages
        .iter()
        .all(|message| message.role != MessageRole::Tool));
    drop(threads);

    let persisted = engine
        .history
        .list_messages(thread_id, Some(10))
        .await
        .unwrap();
    assert_eq!(persisted.len(), 2);
    assert!(persisted.iter().all(|message| message.role != "tool"));
}
