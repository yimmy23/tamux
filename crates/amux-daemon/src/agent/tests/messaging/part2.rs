use super::*;

#[tokio::test]
async fn delete_thread_messages_drops_incomplete_assistant_tool_turn_after_tool_delete() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_incomplete_tool_turn";

    let tool_result_a_id = "tool-result-a".to_string();
    let tool_result_b_id = "tool-result-b".to_string();
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Incomplete tool turn".to_string(),
                created_at: 1,
                updated_at: 6,
                pinned: false,
                upstream_thread_id: Some("upstream-thread-3".to_string()),
                upstream_transport: Some(ApiTransport::Responses),
                upstream_provider: Some("github-copilot".to_string()),
                upstream_model: Some("gpt-5.4".to_string()),
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![
                    AgentMessage::user("start", 1),
                    AgentMessage {
                        id: "assistant-tool-turn".to_string(),
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
                        response_id: Some("resp_789".to_string()),
                        reasoning: None,
                        message_kind: AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        timestamp: 2,
                    },
                    AgentMessage {
                        id: tool_result_a_id.clone(),
                        role: MessageRole::Tool,
                        content: "result a".to_string(),
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
                        id: tool_result_b_id.clone(),
                        role: MessageRole::Tool,
                        content: "result b".to_string(),
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
                    AgentMessage::user("continue", 5),
                ],
            },
        );
    }
    engine.persist_thread_by_id(thread_id).await;

    engine
        .delete_thread_messages(thread_id, std::slice::from_ref(&tool_result_b_id))
        .await
        .expect("delete should succeed");

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    assert_eq!(thread.messages.len(), 2);
    assert_eq!(thread.messages[0].content, "start");
    assert_eq!(thread.messages[1].content, "continue");
    assert!(thread
        .messages
        .iter()
        .all(|message| message.tool_calls.is_none()));
    assert!(thread
        .messages
        .iter()
        .all(|message| message.role != MessageRole::Tool));
}

#[tokio::test]
async fn delete_thread_messages_emits_thread_reload_event_after_reconciliation() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_reload_event";
    let assistant_id = "assistant-anchor".to_string();
    let mut events = engine.subscribe();

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Reload event".to_string(),
                created_at: 1,
                updated_at: 3,
                pinned: false,
                upstream_thread_id: Some("upstream-thread-4".to_string()),
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
                        response_id: Some("resp_999".to_string()),
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

    while events.try_recv().is_ok() {}

    engine
        .delete_thread_messages(thread_id, std::slice::from_ref(&assistant_id))
        .await
        .expect("delete should succeed");

    let mut saw_reload = false;
    while let Ok(event) = events.try_recv() {
        if let AgentEvent::ThreadReloadRequired {
            thread_id: event_thread_id,
        } = event
        {
            assert_eq!(event_thread_id, thread_id);
            saw_reload = true;
            break;
        }
    }

    assert!(saw_reload, "delete should emit thread reload event");
}
