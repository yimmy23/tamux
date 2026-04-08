use super::*;

#[test]
fn direct_message_entrypoints_box_large_send_message_futures() {
    let messaging_production = [
        repo_root().join("crates/amux-daemon/src/agent/messaging.rs"),
        repo_root().join("crates/amux-daemon/src/agent/messaging/direct_messages.rs"),
    ]
    .into_iter()
    .map(|path| fs::read_to_string(path).expect("read messaging source"))
    .collect::<Vec<_>>()
    .join("\n");
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
        "let outcome = Box::pin(self.send_message_inner(",
        "Some(\n                Box::pin(self.send_message_inner(",
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
    let subagents_source = fs::read_to_string(
        repo_root().join("crates/amux-daemon/src/agent/tool_executor/subagents.rs"),
    )
    .expect("read subagents.rs");
    let subagents_production = subagents_source
        .split("\n#[cfg(test)]")
        .next()
        .unwrap_or(subagents_source.as_str());

    assert!(
        finalize_production.contains("Box::pin(self.handle_tool_calls_chunk("),
        "tool-call iteration handling should box the large handle_tool_calls_chunk future"
    );
    assert!(
        finalize_production.contains(
            "Box::pin(self.engine\n            .maybe_auto_send_gateway_thread_response(&self.tid))"
        ) || finalize_production.contains(
            "Box::pin(self.engine.maybe_auto_send_gateway_thread_response(&self.tid))"
        ) || finalize_production.contains(
            "Box::pin(\n            self.engine\n                .maybe_auto_send_gateway_thread_response(&self.tid),\n        )"
        ),
        "done-chunk finalization should box gateway auto-send futures"
    );
    assert!(
        tool_calls_production.contains("Box::pin(execute_tool("),
        "tool execution callsites should box execute_tool futures"
    );
    assert!(
        tool_calls_production.contains(
            "Box::pin(self.engine\n            .maybe_auto_send_gateway_thread_response(&self.tid))"
        ) || tool_calls_production.contains(
            "Box::pin(self.engine.maybe_auto_send_gateway_thread_response(&self.tid))"
        ) || tool_calls_production.contains(
            "Box::pin(\n            self.engine\n                .maybe_auto_send_gateway_thread_response(&self.tid),\n        )"
        ),
        "tool-call chunk handling should box gateway auto-send futures"
    );
    assert!(
        subagents_production.contains(
            "Box::pin(agent.send_internal_agent_message(&sender, target, message, preferred_session_hint.as_deref()))"
        ) || subagents_production.contains(
            "Box::pin(agent\n        .send_internal_agent_message(&sender, target, message, preferred_session_hint.as_deref()))"
        ) || subagents_production.contains(
            "let result = Box::pin(agent.send_internal_agent_message(\n        &sender,\n        target,\n        message,\n        preferred_session_hint.as_deref(),\n    ))"
        ),
        "message_agent should box the oversized internal-agent send future"
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
    use amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT;

    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_test";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
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
async fn get_or_create_thread_with_target_sets_requested_initial_responder() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let (thread_id, created) = engine
        .get_or_create_thread_with_target(None, "Talk to Weles", Some("weles"))
        .await;

    assert!(created, "new target-scoped thread should be created");
    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert_eq!(thread.agent_name.as_deref(), Some("Weles"));

    let handoff_state = engine
        .thread_handoff_state(&thread_id)
        .await
        .expect("handoff state should exist");
    assert_eq!(handoff_state.active_agent_id, WELES_AGENT_ID);
    assert_eq!(
        handoff_state
            .responder_stack
            .last()
            .map(|frame| frame.agent_name.as_str()),
        Some(WELES_AGENT_NAME)
    );
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
                agent_name: Some(MAIN_AGENT_NAME.to_string()),
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
async fn thread_handoff_state_persists_and_restores_active_agent_identity() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_handoff_state";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string()),
                title: "Handoff".to_string(),
                created_at: 1,
                updated_at: 2,
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![AgentMessage::user("operator asks for audit", 1)],
            },
        );
    }

    engine
        .set_thread_handoff_state(
            thread_id,
            ThreadHandoffState {
                origin_agent_id: MAIN_AGENT_ID.to_string(),
                active_agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                responder_stack: vec![
                    ThreadResponderFrame {
                        agent_id: MAIN_AGENT_ID.to_string(),
                        agent_name: MAIN_AGENT_NAME.to_string(),
                        entered_at: 1,
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    },
                    ThreadResponderFrame {
                        agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                        agent_name: crate::agent::agent_identity::WELES_AGENT_NAME.to_string(),
                        entered_at: 2,
                        entered_via_handoff_event_id: Some("handoff-1".to_string()),
                        linked_thread_id: Some("dm:svarog:weles".to_string()),
                    },
                ],
                events: vec![ThreadHandoffEvent {
                    id: "handoff-1".to_string(),
                    kind: ThreadHandoffKind::Push,
                    from_agent_id: MAIN_AGENT_ID.to_string(),
                    to_agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                    requested_by: ThreadHandoffRequestedBy::Agent,
                    reason: "Security review needed".to_string(),
                    summary: "Asked Weles to inspect risky changes.".to_string(),
                    linked_thread_id: Some("dm:svarog:weles".to_string()),
                    approval_id: None,
                    stack_depth_before: 1,
                    stack_depth_after: 2,
                    created_at: 2,
                    approved_at: None,
                    completed_at: Some(2),
                    failed_at: None,
                    failure_reason: None,
                }],
                pending_approval_id: Some("approval-handoff-1".to_string()),
            },
        )
        .await;
    engine.persist_thread_by_id(thread_id).await;

    let persisted = engine
        .history
        .get_thread(thread_id)
        .await
        .expect("read thread")
        .expect("thread should persist");
    assert_eq!(persisted.agent_name.as_deref(), Some("Weles"));
    let metadata = persisted.metadata_json.expect("thread metadata");
    assert!(metadata.contains("\"active_agent_id\":\"weles\""));
    assert!(metadata.contains("\"pending_handoff_approval_id\":\"approval-handoff-1\""));

    let manager = SessionManager::new_test(root.path()).await;
    let rehydrated = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    rehydrated.hydrate().await.expect("hydrate");

    let restored = rehydrated
        .thread_handoff_state(thread_id)
        .await
        .expect("handoff state should restore");
    assert_eq!(restored.origin_agent_id, MAIN_AGENT_ID);
    assert_eq!(
        restored.active_agent_id,
        crate::agent::agent_identity::WELES_AGENT_ID
    );
    assert_eq!(restored.responder_stack.len(), 2);

    let restored_thread = rehydrated
        .threads
        .read()
        .await
        .get(thread_id)
        .cloned()
        .expect("thread should reload");
    assert_eq!(restored_thread.agent_name.as_deref(), Some("Weles"));
}

#[tokio::test]
async fn operator_turns_route_through_active_thread_responder_scope() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_active_responder_scope";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string()),
                title: "Active responder".to_string(),
                created_at: 1,
                updated_at: 2,
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![AgentMessage::user("continue from Weles", 1)],
            },
        );
    }
    engine
        .set_thread_handoff_state(
            thread_id,
            ThreadHandoffState {
                origin_agent_id: MAIN_AGENT_ID.to_string(),
                active_agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                responder_stack: vec![
                    ThreadResponderFrame {
                        agent_id: MAIN_AGENT_ID.to_string(),
                        agent_name: MAIN_AGENT_NAME.to_string(),
                        entered_at: 1,
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    },
                    ThreadResponderFrame {
                        agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                        agent_name: crate::agent::agent_identity::WELES_AGENT_NAME.to_string(),
                        entered_at: 2,
                        entered_via_handoff_event_id: Some("handoff-1".to_string()),
                        linked_thread_id: Some(
                            "handoff:thread_active_responder_scope:handoff-1".to_string(),
                        ),
                    },
                ],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    assert_eq!(
        engine.agent_scope_id_for_turn(Some(thread_id), None).await,
        crate::agent::agent_identity::WELES_AGENT_ID,
        "operator turns should run under the active responder scope"
    );
    assert_eq!(
        engine.agent_scope_id_for_turn(None, None).await,
        MAIN_AGENT_ID,
        "new threads should still default to the main agent scope"
    );
}

#[tokio::test]
async fn handoff_activation_clears_thread_continuation_state_for_new_responder_stream() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_handoff_clears_continuity";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(MAIN_AGENT_NAME.to_string()),
                title: "Handoff continuity".to_string(),
                created_at: 1,
                updated_at: 3,
                pinned: false,
                upstream_thread_id: Some("legacy-upstream-thread".to_string()),
                upstream_transport: Some(ApiTransport::Responses),
                upstream_provider: Some(
                    amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT.to_string(),
                ),
                upstream_model: Some("gpt-5.4".to_string()),
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![
                    AgentMessage::user("first", 1),
                    AgentMessage {
                        id: "assistant-anchor".to_string(),
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
                        provider: Some(
                            amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT.to_string(),
                        ),
                        model: Some("gpt-5.4".to_string()),
                        api_transport: Some(ApiTransport::Responses),
                        response_id: Some("resp_123".to_string()),
                        upstream_message: None,
                        provider_final_result: None,
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

    let request = crate::agent::PendingThreadHandoffActivation {
        thread_id: thread_id.to_string(),
        kind: crate::agent::ThreadHandoffKind::Push,
        target_agent_id: Some("weles".to_string()),
        requested_by: crate::agent::ThreadHandoffRequestedBy::User,
        reason: "Operator asked to talk to Weles".to_string(),
        summary: "Switch active responder to Weles".to_string(),
    };

    engine
        .apply_thread_handoff_activation(&request, None)
        .await
        .expect("handoff activation should succeed");

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    assert!(thread.upstream_thread_id.is_none());
    assert!(thread.upstream_transport.is_none());
    assert!(thread.upstream_provider.is_none());
    assert!(thread.upstream_model.is_none());
    assert!(thread
        .messages
        .iter()
        .all(|message| message.response_id.is_none()));
}

#[tokio::test]
async fn handoff_activation_emits_thread_reload_event_for_visible_thread() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_handoff_reload_event";
    let mut events = engine.subscribe();

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(MAIN_AGENT_NAME.to_string()),
                title: "Reload me after handoff".to_string(),
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
                messages: vec![AgentMessage::user("switch me", 1)],
            },
        );
    }

    while events.try_recv().is_ok() {}

    let request = crate::agent::PendingThreadHandoffActivation {
        thread_id: thread_id.to_string(),
        kind: crate::agent::ThreadHandoffKind::Push,
        target_agent_id: Some("weles".to_string()),
        requested_by: crate::agent::ThreadHandoffRequestedBy::User,
        reason: "Operator asked to talk to Weles".to_string(),
        summary: "Switch active responder to Weles".to_string(),
    };

    engine
        .apply_thread_handoff_activation(&request, None)
        .await
        .expect("handoff activation should succeed");

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

    assert!(
        saw_reload,
        "handoff activation should emit a thread reload event"
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
                agent_name: None,
                title: "Continuation".to_string(),
                created_at: 1,
                updated_at: 4,
                pinned: false,
                upstream_thread_id: Some("upstream-thread-1".to_string()),
                upstream_transport: Some(ApiTransport::Responses),
                upstream_provider: Some(
                    amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT.to_string(),
                ),
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
                        provider: Some(
                            amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT.to_string(),
                        ),
                        model: Some("gpt-5.4".to_string()),
                        api_transport: Some(ApiTransport::Responses),
                        response_id: Some("resp_123".to_string()),
                        upstream_message: None,
                        provider_final_result: None,
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
                agent_name: None,
                title: "Orphans".to_string(),
                created_at: 1,
                updated_at: 6,
                pinned: false,
                upstream_thread_id: Some("upstream-thread-2".to_string()),
                upstream_transport: Some(ApiTransport::Responses),
                upstream_provider: Some(
                    amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT.to_string(),
                ),
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
                        provider: Some(
                            amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT.to_string(),
                        ),
                        model: Some("gpt-5.4".to_string()),
                        api_transport: Some(ApiTransport::Responses),
                        response_id: Some("resp_456".to_string()),
                        upstream_message: None,
                        provider_final_result: None,
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
                        upstream_message: None,
                        provider_final_result: None,
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
                        upstream_message: None,
                        provider_final_result: None,
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
                        provider: Some(
                            amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT.to_string(),
                        ),
                        model: Some("gpt-5.4".to_string()),
                        api_transport: Some(ApiTransport::Responses),
                        response_id: None,
                        upstream_message: None,
                        provider_final_result: None,
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

#[tokio::test]
async fn thread_metadata_round_trips_latest_skill_discovery_state() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let seed_engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_skill_discovery_metadata";

    seed_engine
        .history
        .create_thread(&amux_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(MAIN_AGENT_NAME.to_string()),
            title: "Discovery metadata".to_string(),
            created_at: 1,
            updated_at: 2,
            message_count: 1,
            total_tokens: 0,
            last_preview: "debug panic".to_string(),
            metadata_json: Some(
                serde_json::json!({
                    "client_surface": "tui",
                    "latest_skill_discovery_state": {
                        "query": "debug panic",
                        "confidence_tier": "strong",
                        "recommended_skill": "systematic-debugging",
                        "recommended_action": "read_skill systematic-debugging",
                        "read_skill_identifier": "systematic-debugging",
                        "skip_rationale": null,
                        "compliant": false,
                        "updated_at": 123
                    }
                })
                .to_string(),
            ),
        })
        .await
        .expect("seed thread row");
    seed_engine
        .history
        .add_message(&amux_protocol::AgentDbMessage {
            id: "seed-message-1".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1,
            role: "user".to_string(),
            content: "debug panic".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("seed thread message");

    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    engine.hydrate().await.expect("hydrate");
    engine.persist_thread_by_id(thread_id).await;

    let persisted = engine
        .history
        .get_thread(thread_id)
        .await
        .expect("read thread")
        .expect("thread should persist");
    let metadata = persisted.metadata_json.expect("thread metadata");
    assert!(
        metadata.contains("\"latest_skill_discovery_state\""),
        "expected latest skill discovery state to survive hydrate + persist: {metadata}"
    );
    assert!(metadata.contains("\"query\":\"debug panic\""));
    assert!(metadata.contains("\"recommended_skill\":\"systematic-debugging\""));
    assert!(metadata.contains("\"compliant\":false"));
}
