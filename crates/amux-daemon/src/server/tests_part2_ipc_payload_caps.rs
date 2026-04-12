#[tokio::test]
async fn get_work_context_truncates_oversized_payload_for_ipc() {
    let mut conn = spawn_test_connection().await;
    let thread_id = "thread-work-context-huge";
    let huge_path = "x".repeat(amux_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024);

    conn.agent.thread_work_contexts.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::ThreadWorkContext {
            thread_id: thread_id.to_string(),
            entries: vec![crate::agent::types::WorkContextEntry {
                path: huge_path,
                previous_path: None,
                kind: crate::agent::types::WorkContextEntryKind::Artifact,
                source: "test".to_string(),
                change_kind: None,
                repo_root: None,
                goal_run_id: None,
                step_index: None,
                session_id: None,
                is_text: true,
                updated_at: 1,
            }],
        },
    );

    conn.framed
        .send(ClientMessage::AgentGetWorkContext {
            thread_id: thread_id.to_string(),
        })
        .await
        .expect("request work context");

    match conn.recv().await {
        DaemonMessage::AgentWorkContextDetail {
            thread_id: returned_thread_id,
            context_json,
        } => {
            assert_eq!(returned_thread_id, thread_id);
            let context: crate::agent::types::ThreadWorkContext =
                serde_json::from_str(&context_json).expect("decode work context");
            assert!(
                context.entries.is_empty(),
                "oversized work context entry should be dropped to fit IPC"
            );
        }
        other => panic!("expected work context detail, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn get_thread_streams_oversized_thread_detail_without_truncation() {
    let mut conn = spawn_test_connection().await;
    let thread_id = "thread-detail-huge";
    let huge_message = "x".repeat(amux_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024);
    let expected_huge_message = huge_message.clone();

    conn.agent.threads.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::AgentThread {
            id: thread_id.to_string(),
            agent_name: Some("main".to_string()),
            title: "Huge thread".to_string(),
            messages: vec![
                crate::agent::types::AgentMessage::user(huge_message, 1),
                crate::agent::types::AgentMessage::user("recent tail", 2),
            ],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: 1,
            updated_at: 2,
            total_input_tokens: 0,
            total_output_tokens: 0,
        },
    );

    conn.framed
        .send(ClientMessage::AgentGetThread {
            thread_id: thread_id.to_string(),
            message_limit: None,
            message_offset: None,
        })
        .await
        .expect("request thread detail");

    let mut chunks = Vec::new();
    loop {
        match conn.recv().await {
            DaemonMessage::AgentThreadDetailChunk {
                thread_id: returned_thread_id,
                thread_json_chunk,
                done,
            } => {
                assert_eq!(returned_thread_id, thread_id);
                chunks.extend(thread_json_chunk);
                if done {
                    break;
                }
            }
            other => panic!("expected thread detail chunk, got {other:?}"),
        }
    }

    let thread_json = String::from_utf8(chunks).expect("thread detail chunks should be utf-8");
    let thread: Option<crate::agent::types::AgentThread> =
        serde_json::from_str(&thread_json).expect("decode streamed thread detail");
    let thread = thread.expect("thread detail should exist");
    let contents = thread
        .messages
        .iter()
        .map(|message| message.content.clone())
        .collect::<Vec<_>>();
    assert_eq!(contents, vec![expected_huge_message, "recent tail".to_string()]);

    conn.shutdown().await;
}

#[tokio::test]
async fn get_thread_respects_requested_message_page_and_reports_window_metadata() {
    let mut conn = spawn_test_connection().await;
    let thread_id = "thread-detail-paged";

    conn.agent.threads.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::AgentThread {
            id: thread_id.to_string(),
            agent_name: Some("main".to_string()),
            title: "Paged thread".to_string(),
            messages: (0..120)
                .map(|index| crate::agent::types::AgentMessage::user(format!("msg {index}"), index as u64))
                .collect(),
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: 1,
            updated_at: 120,
            total_input_tokens: 0,
            total_output_tokens: 0,
        },
    );

    conn.framed
        .send(ClientMessage::AgentGetThread {
            thread_id: thread_id.to_string(),
            message_limit: Some(50),
            message_offset: Some(50),
        })
        .await
        .expect("request paged thread detail");

    let thread_json = match conn.recv().await {
        DaemonMessage::AgentThreadDetail { thread_json } => thread_json,
        other => panic!("expected thread detail, got {other:?}"),
    };

    let value: serde_json::Value =
        serde_json::from_str(&thread_json).expect("decode paged thread detail");
    let messages = value["messages"]
        .as_array()
        .expect("messages should be an array");
    let first_content = messages
        .first()
        .and_then(|message| message.get("content"))
        .and_then(serde_json::Value::as_str);
    let last_content = messages
        .last()
        .and_then(|message| message.get("content"))
        .and_then(serde_json::Value::as_str);

    assert_eq!(value["total_message_count"].as_u64(), Some(120));
    assert_eq!(value["loaded_message_start"].as_u64(), Some(20));
    assert_eq!(value["loaded_message_end"].as_u64(), Some(70));
    assert_eq!(messages.len(), 50);
    assert_eq!(first_content, Some("msg 20"));
    assert_eq!(last_content, Some("msg 69"));

    conn.shutdown().await;
}

#[test]
fn cap_scrollback_for_ipc_keeps_recent_tail() {
    let tail = b"recent tail".to_vec();
    let mut data = vec![b'x'; amux_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024];
    data.extend_from_slice(&tail);

    let session_id = uuid::Uuid::nil();
    let (capped, truncated) = super::cap_scrollback_for_ipc(session_id, data);
    assert!(truncated);
    assert!(capped.ends_with(&tail));
    assert!(amux_protocol::daemon_message_fits_ipc(
        &DaemonMessage::Scrollback {
            id: session_id,
            data: capped,
        }
    ));
}

#[test]
fn cap_history_search_result_for_ipc_drops_oversized_hits() {
    let huge_hit = amux_protocol::HistorySearchHit {
        id: "hit-huge".to_string(),
        kind: "session".to_string(),
        title: "Huge".to_string(),
        excerpt: "x".repeat(amux_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024),
        path: None,
        timestamp: 1,
        score: 1.0,
    };
    let small_hit = amux_protocol::HistorySearchHit {
        id: "hit-small".to_string(),
        kind: "session".to_string(),
        title: "Small".to_string(),
        excerpt: "recent excerpt".to_string(),
        path: None,
        timestamp: 2,
        score: 0.9,
    };

    let (summary, hits, truncated) = super::cap_history_search_result_for_ipc(
        "query",
        "summary".to_string(),
        vec![small_hit.clone(), huge_hit],
    );
    assert!(truncated);
    assert_eq!(summary, "summary");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, small_hit.id);
    assert!(amux_protocol::daemon_message_fits_ipc(
        &DaemonMessage::HistorySearchResult {
            query: "query".to_string(),
            summary,
            hits,
        }
    ));
}

#[test]
fn cap_agent_thread_list_for_ipc_drops_oversized_entries() {
    let small_thread = crate::agent::types::AgentThread {
        id: "thread-small".to_string(),
        agent_name: None,
        title: "Small".to_string(),
        messages: Vec::new(),
        pinned: false,
        upstream_thread_id: None,
        upstream_transport: None,
        upstream_provider: None,
        upstream_model: None,
        upstream_assistant_id: None,
        created_at: 1,
        updated_at: 2,
        total_input_tokens: 0,
        total_output_tokens: 0,
    };
    let huge_thread = crate::agent::types::AgentThread {
        id: "thread-huge".to_string(),
        agent_name: None,
        title: "x".repeat(amux_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024),
        messages: Vec::new(),
        pinned: false,
        upstream_thread_id: None,
        upstream_transport: None,
        upstream_provider: None,
        upstream_model: None,
        upstream_assistant_id: None,
        created_at: 1,
        updated_at: 1,
        total_input_tokens: 0,
        total_output_tokens: 0,
    };

    let (threads, truncated) =
        super::cap_agent_thread_list_for_ipc(vec![small_thread.clone(), huge_thread]);
    assert!(truncated);
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, small_thread.id);
    let threads_json = serde_json::to_string(&threads).expect("serialize capped thread list");
    assert!(amux_protocol::daemon_message_fits_ipc(
        &DaemonMessage::AgentThreadList { threads_json }
    ));
}

#[test]
fn cap_plugin_api_call_result_for_ipc_truncates_large_result() {
    let (result, truncated) = super::cap_plugin_api_call_result_for_ipc(
        Some("op-plugin-huge"),
        "plugin",
        "endpoint",
        true,
        "x".repeat(amux_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024),
        None,
    );

    assert!(truncated);
    assert!(amux_protocol::daemon_message_fits_ipc(
        &DaemonMessage::PluginApiCallResult {
            operation_id: Some("op-plugin-huge".to_string()),
            plugin_name: "plugin".to_string(),
            endpoint_name: "endpoint".to_string(),
            success: true,
            result,
            error_type: None,
        }
    ));
}

#[tokio::test]
async fn get_todos_truncates_oversized_payload_for_ipc() {
    let mut conn = spawn_test_connection().await;
    let thread_id = "thread-todos-huge";
    let huge_content = "x".repeat(amux_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024);

    conn.agent.thread_todos.write().await.insert(
        thread_id.to_string(),
        vec![crate::agent::types::TodoItem {
            id: "todo-huge".to_string(),
            content: huge_content,
            status: crate::agent::types::TodoStatus::Pending,
            position: 0,
            step_index: None,
            created_at: 1,
            updated_at: 1,
        }],
    );

    conn.framed
        .send(ClientMessage::AgentGetTodos {
            thread_id: thread_id.to_string(),
        })
        .await
        .expect("request todos");

    match conn.recv().await {
        DaemonMessage::AgentTodoDetail {
            thread_id: returned_thread_id,
            todos_json,
        } => {
            assert_eq!(returned_thread_id, thread_id);
            let todos: Vec<crate::agent::types::TodoItem> =
                serde_json::from_str(&todos_json).expect("decode todo detail");
            assert!(
                todos.is_empty(),
                "oversized todo item should be dropped to fit IPC"
            );
        }
        other => panic!("expected todo detail, got {other:?}"),
    }

    conn.shutdown().await;
}
