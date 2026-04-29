use super::*;
use zorai_shared::providers::{PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_OPENAI};

fn make_thread(id: &str, title: &str) -> AgentThread {
    AgentThread {
        id: id.into(),
        title: title.into(),
        ..Default::default()
    }
}

#[test]
fn delta_appends_to_streaming_content() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "Test".into(),
    });
    state.reduce(ChatAction::Delta {
        thread_id: "t1".into(),
        content: "Hello".into(),
    });
    state.reduce(ChatAction::Delta {
        thread_id: "t1".into(),
        content: " world".into(),
    });
    assert_eq!(state.streaming_content(), "Hello world");
}

#[test]
fn turn_done_finalizes_streaming_into_message() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "Test".into(),
    });
    state.reduce(ChatAction::Delta {
        thread_id: "t1".into(),
        content: "Hi".into(),
    });
    state.reduce(ChatAction::TurnDone {
        thread_id: "t1".into(),
        input_tokens: 100,
        output_tokens: 50,
        cost: Some(0.01),
        provider: Some(PROVIDER_ID_OPENAI.into()),
        model: Some("gpt-4o".into()),
        tps: Some(45.0),
        generation_ms: Some(1200),
        reasoning: None,
        provider_final_result_json: Some("result_json".to_string()),
    });
    assert_eq!(state.streaming_content(), "");
    let thread = state.active_thread().unwrap();
    let last = thread.messages.last().unwrap();
    assert_eq!(last.content, "Hi");
    assert_eq!(last.role, MessageRole::Assistant);
}

#[test]
fn scroll_up_locks_scroll() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ScrollChat(5));
    assert!(state.scroll_locked());
    assert_eq!(state.scroll_offset(), 5);
}

#[test]
fn scroll_to_zero_unlocks() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ScrollChat(5));
    state.reduce(ChatAction::ScrollChat(-5));
    assert!(!state.scroll_locked());
    assert_eq!(state.scroll_offset(), 0);
}

#[test]
fn locked_scroll_offset_grows_with_streamed_newlines() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "Test".into(),
    });
    state.reduce(ChatAction::ScrollChat(4));

    state.reduce(ChatAction::Delta {
        thread_id: "t1".into(),
        content: "\nnext\nchunk".into(),
    });

    assert_eq!(state.scroll_offset(), 6);
    assert!(state.scroll_locked());
}

#[test]
fn retry_status_replaces_previous_status_for_active_thread() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "Test".into(),
    });
    state.reduce(ChatAction::SetRetryStatus {
        thread_id: "t1".into(),
        phase: RetryPhase::Retrying,
        attempt: 1,
        max_retries: 3,
        delay_ms: 2_000,
        failure_class: "rate_limit".into(),
        message: "429".into(),
        received_at_tick: 10,
    });
    state.reduce(ChatAction::SetRetryStatus {
        thread_id: "t1".into(),
        phase: RetryPhase::Waiting,
        attempt: 3,
        max_retries: 3,
        delay_ms: 30_000,
        failure_class: "rate_limit".into(),
        message: "retrying automatically".into(),
        received_at_tick: 20,
    });

    let status = state.retry_status().expect("retry status should exist");
    assert_eq!(status.phase, RetryPhase::Waiting);
    assert_eq!(status.delay_ms, 30_000);
    assert_eq!(status.received_at_tick, 20);
}

#[test]
fn copied_message_feedback_expires_after_deadline() {
    let mut state = state_with_messages(1);
    state.mark_message_copied(0, 25);

    assert!(state.is_message_recently_copied(0, 24));

    state.clear_expired_copy_feedback(25);

    assert!(!state.is_message_recently_copied(0, 25));
}

#[test]
fn thread_list_received_replaces_threads() {
    let mut state = ChatState::new();
    let threads = vec![
        AgentThread {
            id: "t1".into(),
            title: "First".into(),
            ..Default::default()
        },
        AgentThread {
            id: "t2".into(),
            title: "Second".into(),
            ..Default::default()
        },
    ];
    state.reduce(ChatAction::ThreadListReceived(threads));
    assert_eq!(state.threads().len(), 2);
}

#[test]
fn thread_history_stack_pushes_and_pops_in_order() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadListReceived(vec![
        make_thread("thread-a", "A"),
        make_thread("thread-b", "B"),
        make_thread("thread-c", "C"),
    ]));
    state.reduce(ChatAction::SelectThread("thread-a".into()));

    assert!(state.open_spawned_thread("thread-a", "thread-b"));
    assert!(state.open_spawned_thread("thread-b", "thread-c"));
    assert_eq!(
        state.thread_history_stack(),
        &["thread-a".to_string(), "thread-b".to_string()]
    );

    assert_eq!(state.go_back_thread(), Some("thread-b".to_string()));
    assert_eq!(state.active_thread_id(), Some("thread-b"));
    assert_eq!(state.thread_history_stack(), &["thread-a".to_string()]);

    assert_eq!(state.go_back_thread(), Some("thread-a".to_string()));
    assert_eq!(state.active_thread_id(), Some("thread-a"));
    assert!(state.thread_history_stack().is_empty());
}

#[test]
fn thread_history_stack_suppresses_duplicate_top_entries() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadListReceived(vec![
        make_thread("thread-a", "A"),
        make_thread("thread-b", "B"),
    ]));
    state.reduce(ChatAction::SelectThread("thread-a".into()));

    assert!(state.open_spawned_thread("thread-a", "thread-b"));
    assert!(state.open_spawned_thread("thread-a", "thread-b"));

    assert_eq!(state.thread_history_stack(), &["thread-a".to_string()]);
}

#[test]
fn thread_history_stack_allows_opening_unloaded_spawned_thread() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadListReceived(vec![make_thread(
        "thread-a", "A",
    )]));
    state.reduce(ChatAction::SelectThread("thread-a".into()));

    assert!(state.open_spawned_thread("thread-a", "thread-b"));
    assert_eq!(state.active_thread_id(), Some("thread-b"));
    assert_eq!(state.thread_history_stack(), &["thread-a".to_string()]);
}

#[test]
fn thread_history_stack_survives_ordinary_selection_and_prunes_deleted_threads() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadListReceived(vec![
        make_thread("thread-a", "A"),
        make_thread("thread-b", "B"),
        make_thread("thread-c", "C"),
    ]));
    state.reduce(ChatAction::SelectThread("thread-a".into()));

    assert!(state.open_spawned_thread("thread-a", "thread-b"));
    assert!(state.open_spawned_thread("thread-b", "thread-c"));
    assert_eq!(state.thread_navigation_depth(), 2);

    state.reduce(ChatAction::SelectThread("thread-c".into()));
    assert_eq!(
        state.thread_history_stack(),
        &["thread-a".to_string(), "thread-b".to_string()]
    );
    assert_eq!(state.go_back_thread(), Some("thread-b".to_string()));
    assert_eq!(state.active_thread_id(), Some("thread-b"));

    assert!(state.open_spawned_thread("thread-b", "thread-c"));
    state.reduce(ChatAction::ThreadDeleted {
        thread_id: "thread-c".into(),
    });
    assert!(
        state.thread_history_stack().is_empty(),
        "deleting the active thread should reset spawned-thread history"
    );
    assert_eq!(state.go_back_thread(), None);
}

#[test]
fn thread_history_stack_prunes_missing_threads_on_list_replacement() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadListReceived(vec![
        make_thread("thread-a", "A"),
        make_thread("thread-b", "B"),
        make_thread("thread-c", "C"),
    ]));
    state.reduce(ChatAction::SelectThread("thread-a".into()));

    assert!(state.open_spawned_thread("thread-a", "thread-b"));
    assert_eq!(state.thread_history_stack(), &["thread-a".to_string()]);

    state.reduce(ChatAction::ThreadListReceived(vec![make_thread(
        "thread-c", "C",
    )]));
    assert!(state.thread_history_stack().is_empty());
    assert_eq!(state.go_back_thread(), None);
}

#[test]
fn thread_list_received_preserves_existing_messages_when_summary_is_empty() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "First".into(),
        messages: vec![AgentMessage {
            role: MessageRole::Assistant,
            content: "Existing detail".into(),
            ..Default::default()
        }],
        ..Default::default()
    }));

    state.reduce(ChatAction::ThreadListReceived(vec![AgentThread {
        id: "t1".into(),
        title: "First renamed".into(),
        messages: Vec::new(),
        ..Default::default()
    }]));

    let thread = state.threads().first().expect("thread should exist");
    assert_eq!(thread.title, "First renamed");
    assert_eq!(thread.messages.len(), 1);
    assert_eq!(thread.messages[0].content, "Existing detail");
}

#[test]
fn thread_detail_hydrates_pinned_for_compaction() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Pinned".into(),
        messages: vec![AgentMessage {
            id: Some("m1".into()),
            role: MessageRole::User,
            content: "keep this".into(),
            pinned_for_compaction: true,
            ..Default::default()
        }],
        ..Default::default()
    }));
    state.reduce(ChatAction::SelectThread("t1".into()));

    let thread = state.active_thread().expect("thread should exist");
    assert!(thread.messages[0].pinned_for_compaction);
}

#[test]
fn thread_refresh_updates_pinned_for_compaction_flags() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Pinned".into(),
        messages: vec![AgentMessage {
            id: Some("m1".into()),
            role: MessageRole::User,
            content: "keep this".into(),
            pinned_for_compaction: false,
            ..Default::default()
        }],
        loaded_message_end: 1,
        total_message_count: 1,
        ..Default::default()
    }));
    state.reduce(ChatAction::SelectThread("t1".into()));

    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Pinned".into(),
        messages: vec![AgentMessage {
            id: Some("m1".into()),
            role: MessageRole::User,
            content: "keep this".into(),
            pinned_for_compaction: true,
            ..Default::default()
        }],
        loaded_message_end: 1,
        total_message_count: 1,
        ..Default::default()
    }));

    let thread = state.active_thread().expect("thread should exist");
    assert!(thread.messages[0].pinned_for_compaction);
}

#[test]
fn active_thread_pinned_messages_follow_thread_order() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Pinned".into(),
        messages: vec![
            AgentMessage {
                id: Some("m1".into()),
                role: MessageRole::User,
                content: "unpinned".into(),
                pinned_for_compaction: false,
                ..Default::default()
            },
            AgentMessage {
                id: Some("m2".into()),
                role: MessageRole::Assistant,
                content: "first pin".into(),
                pinned_for_compaction: true,
                ..Default::default()
            },
            AgentMessage {
                id: Some("m3".into()),
                role: MessageRole::User,
                content: "second pin".into(),
                pinned_for_compaction: true,
                ..Default::default()
            },
        ],
        loaded_message_end: 3,
        total_message_count: 3,
        ..Default::default()
    }));
    state.reduce(ChatAction::SelectThread("t1".into()));

    let pinned = state.active_thread_pinned_messages();
    assert_eq!(pinned.len(), 2);
    assert_eq!(pinned[0].absolute_index, 1);
    assert_eq!(pinned[0].message_id, "m2");
    assert_eq!(pinned[1].absolute_index, 2);
    assert_eq!(pinned[1].message_id, "m3");
}

#[test]
fn thread_detail_hydrates_pinned_summaries_outside_loaded_window() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Pinned".into(),
        messages: vec![AgentMessage {
            id: Some("m3".into()),
            role: MessageRole::Assistant,
            content: "latest".into(),
            pinned_for_compaction: false,
            ..Default::default()
        }],
        pinned_messages: vec![PinnedThreadMessage {
            message_id: "m1".into(),
            absolute_index: 0,
            role: MessageRole::User,
            content: "offscreen pin".into(),
        }],
        loaded_message_start: 2,
        loaded_message_end: 3,
        total_message_count: 3,
        ..Default::default()
    }));
    state.reduce(ChatAction::SelectThread("t1".into()));

    let pinned = state.active_thread_pinned_messages();
    assert!(state.active_thread_has_pinned_messages());
    assert_eq!(pinned.len(), 1);
    assert_eq!(pinned[0].message_id, "m1");
    assert_eq!(pinned[0].absolute_index, 0);
    assert_eq!(pinned[0].content, "offscreen pin");
}

#[test]
fn local_unpin_removes_offscreen_pinned_summary_immediately() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Pinned".into(),
        messages: vec![AgentMessage {
            id: Some("m3".into()),
            role: MessageRole::Assistant,
            content: "latest".into(),
            pinned_for_compaction: false,
            ..Default::default()
        }],
        pinned_messages: vec![PinnedThreadMessage {
            message_id: "m1".into(),
            absolute_index: 0,
            role: MessageRole::User,
            content: "offscreen pin".into(),
        }],
        loaded_message_start: 2,
        loaded_message_end: 3,
        total_message_count: 3,
        ..Default::default()
    }));
    state.reduce(ChatAction::SelectThread("t1".into()));

    state.reduce(ChatAction::UnpinMessageForCompaction {
        thread_id: "t1".into(),
        message_id: "m1".into(),
        absolute_index: Some(0),
    });

    assert!(!state.active_thread_has_pinned_messages());
    assert!(state.active_thread_pinned_messages().is_empty());
}

#[test]
fn thread_created_moves_new_thread_to_front() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadListReceived(vec![
        AgentThread {
            id: "older".into(),
            title: "Older".into(),
            updated_at: 10,
            ..Default::default()
        },
        AgentThread {
            id: "oldest".into(),
            title: "Oldest".into(),
            updated_at: 5,
            ..Default::default()
        },
    ]));

    state.reduce(ChatAction::ThreadCreated {
        thread_id: "new".into(),
        title: "Newest".into(),
    });

    let threads = state.threads();
    assert_eq!(threads[0].id, "new");
    assert_eq!(threads[1].id, "older");
    assert_eq!(threads[2].id, "oldest");
    assert_eq!(state.active_thread_id(), Some("new"));
}

#[test]
fn select_thread_with_empty_id_clears_active_selection() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "Test".into(),
    });
    state.reduce(ChatAction::SelectThread("t1".into()));

    state.reduce(ChatAction::SelectThread(String::new()));

    assert_eq!(state.active_thread_id(), None);
    assert!(state.active_thread().is_none());
}

#[test]
fn tool_call_tracks_running_tool() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "Test".into(),
    });
    state.reduce(ChatAction::ToolCall {
        thread_id: "t1".into(),
        call_id: "c1".into(),
        name: "bash_command".into(),
        args: "ls".into(),
        weles_review: None,
    });
    assert_eq!(state.active_tool_calls().len(), 1);
    assert_eq!(state.active_tool_calls()[0].status, ToolCallStatus::Running);
}

#[test]
fn tool_result_updates_status() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "Test".into(),
    });
    state.reduce(ChatAction::ToolCall {
        thread_id: "t1".into(),
        call_id: "c1".into(),
        name: "bash_command".into(),
        args: "ls".into(),
        weles_review: None,
    });
    state.reduce(ChatAction::ToolResult {
        thread_id: "t1".into(),
        call_id: "c1".into(),
        name: "bash_command".into(),
        content: "file.txt".into(),
        is_error: false,
        weles_review: None,
    });
    assert_eq!(state.active_tool_calls()[0].status, ToolCallStatus::Done);
}

#[test]
fn reasoning_and_tool_calls_preserve_transcript_continuity() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "Test".into(),
    });
    state.reduce(ChatAction::SelectThread("t1".into()));
    state.reduce(ChatAction::Reasoning {
        thread_id: "t1".into(),
        content: "First reasoning".into(),
    });
    state.reduce(ChatAction::ToolCall {
        thread_id: "t1".into(),
        call_id: "call-1".into(),
        name: "tool_one".into(),
        args: "{}".into(),
        weles_review: None,
    });
    state.reduce(ChatAction::ToolResult {
        thread_id: "t1".into(),
        call_id: "call-1".into(),
        name: "tool_one".into(),
        content: "done".into(),
        is_error: false,
        weles_review: None,
    });
    state.reduce(ChatAction::Reasoning {
        thread_id: "t1".into(),
        content: "Second reasoning".into(),
    });
    state.reduce(ChatAction::ToolCall {
        thread_id: "t1".into(),
        call_id: "call-2".into(),
        name: "tool_two".into(),
        args: "{}".into(),
        weles_review: None,
    });
    state.reduce(ChatAction::ToolResult {
        thread_id: "t1".into(),
        call_id: "call-2".into(),
        name: "tool_two".into(),
        content: "done".into(),
        is_error: false,
        weles_review: None,
    });
    state.reduce(ChatAction::ToolCall {
        thread_id: "t1".into(),
        call_id: "call-3".into(),
        name: "tool_three".into(),
        args: "{}".into(),
        weles_review: None,
    });
    state.reduce(ChatAction::ToolResult {
        thread_id: "t1".into(),
        call_id: "call-3".into(),
        name: "tool_three".into(),
        content: "done".into(),
        is_error: false,
        weles_review: None,
    });
    state.reduce(ChatAction::Reasoning {
        thread_id: "t1".into(),
        content: "Final reasoning".into(),
    });
    state.reduce(ChatAction::Delta {
        thread_id: "t1".into(),
        content: "Final message".into(),
    });
    state.reduce(ChatAction::TurnDone {
        thread_id: "t1".into(),
        input_tokens: 0,
        output_tokens: 0,
        cost: None,
        provider: None,
        model: None,
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: Some("result_json".to_string()),
    });

    let thread = state.active_thread().expect("thread should exist");
    let transcript: Vec<(MessageRole, Option<&str>, &str)> = thread
        .messages
        .iter()
        .map(|message| {
            (
                message.role,
                message.tool_name.as_deref(),
                message
                    .reasoning
                    .as_deref()
                    .unwrap_or(message.content.as_str()),
            )
        })
        .collect();

    assert_eq!(
        transcript,
        vec![
            (MessageRole::Assistant, None, "First reasoning"),
            (MessageRole::Tool, Some("tool_one"), "done"),
            (MessageRole::Assistant, None, "Second reasoning"),
            (MessageRole::Tool, Some("tool_two"), "done"),
            (MessageRole::Tool, Some("tool_three"), "done"),
            (MessageRole::Assistant, None, "Final reasoning"),
        ]
    );

    let final_message = thread.messages.last().expect("final message should exist");
    assert_eq!(final_message.content, "Final message");
    assert_eq!(final_message.reasoning.as_deref(), Some("Final reasoning"));
}

#[test]
fn tool_messages_store_weles_review_metadata() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "Test".into(),
    });
    let review = WelesReviewMetaVm {
        weles_reviewed: true,
        verdict: "allow".into(),
        reasons: vec!["operator approved".into()],
        audit_id: Some("audit-1".into()),
        security_override_mode: Some("yolo".into()),
    };

    state.reduce(ChatAction::ToolCall {
        thread_id: "t1".into(),
        call_id: "c1".into(),
        name: "bash_command".into(),
        args: "ls".into(),
        weles_review: Some(review.clone()),
    });
    state.reduce(ChatAction::ToolResult {
        thread_id: "t1".into(),
        call_id: "c1".into(),
        name: "bash_command".into(),
        content: "file.txt".into(),
        is_error: false,
        weles_review: Some(review.clone()),
    });

    let thread = state.active_thread().expect("thread should exist");
    let tool_message = thread
        .messages
        .iter()
        .find(|message| message.tool_call_id.as_deref() == Some("c1"))
        .expect("tool message should exist");

    let stored = tool_message
        .weles_review
        .as_ref()
        .expect("weles review should be stored on tool message");
    assert!(stored.weles_reviewed);
    assert_eq!(stored.verdict, "allow");
    assert_eq!(stored.audit_id.as_deref(), Some("audit-1"));
    assert_eq!(stored.security_override_mode.as_deref(), Some("yolo"));
}

#[test]
fn turn_done_uses_final_reasoning_when_no_reasoning_delta_was_streamed() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "Test".into(),
    });
    state.reduce(ChatAction::Delta {
        thread_id: "t1".into(),
        content: "Answer".into(),
    });

    state.reduce(ChatAction::TurnDone {
        thread_id: "t1".into(),
        input_tokens: 100,
        output_tokens: 50,
        cost: Some(0.01),
        provider: Some(PROVIDER_ID_GITHUB_COPILOT.into()),
        model: Some("gpt-5.4".into()),
        tps: Some(45.0),
        generation_ms: Some(1200),
        reasoning: Some("Final reasoning summary".into()),
        provider_final_result_json: Some("result_json".to_string()),
    });

    let thread = state.active_thread().unwrap();
    let last = thread.messages.last().unwrap();
    assert_eq!(last.reasoning.as_deref(), Some("Final reasoning summary"));
}

#[test]
fn new_thread_clears_active() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "Test".into(),
    });
    assert!(state.active_thread_id().is_some());
    state.reduce(ChatAction::NewThread);
    assert!(state.active_thread_id().is_none());
}

#[test]
fn background_delta_after_new_thread_does_not_reselect_previous_thread() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "First".into(),
    });
    state.reduce(ChatAction::NewThread);

    state.reduce(ChatAction::Delta {
        thread_id: "t1".into(),
        content: "background output".into(),
    });

    assert_eq!(state.active_thread_id(), None);
    assert_eq!(state.streaming_content(), "");
}

#[test]
fn select_thread_changes_active() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadListReceived(vec![
        AgentThread {
            id: "t1".into(),
            title: "First".into(),
            ..Default::default()
        },
        AgentThread {
            id: "t2".into(),
            title: "Second".into(),
            ..Default::default()
        },
    ]));
    state.reduce(ChatAction::SelectThread("t2".into()));
    assert_eq!(state.active_thread_id(), Some("t2"));
}

#[test]
fn reselecting_thread_resets_scroll_lock_and_offset() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadListReceived(vec![AgentThread {
        id: "t1".into(),
        title: "First".into(),
        ..Default::default()
    }]));
    state.reduce(ChatAction::SelectThread("t1".into()));
    state.reduce(ChatAction::ScrollChat(5));

    assert!(state.scroll_locked());
    assert_eq!(state.scroll_offset(), 5);

    state.reduce(ChatAction::SelectThread("t1".into()));

    assert!(!state.scroll_locked());
    assert_eq!(state.scroll_offset(), 0);
}

#[test]
fn inactive_thread_streaming_does_not_pollute_selected_thread_view() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadListReceived(vec![
        AgentThread {
            id: "t1".into(),
            title: "First".into(),
            ..Default::default()
        },
        AgentThread {
            id: "t2".into(),
            title: "Second".into(),
            ..Default::default()
        },
    ]));
    state.reduce(ChatAction::SelectThread("t2".into()));

    state.reduce(ChatAction::Delta {
        thread_id: "t1".into(),
        content: "background output".into(),
    });
    state.reduce(ChatAction::Reasoning {
        thread_id: "t1".into(),
        content: "background reasoning".into(),
    });
    state.reduce(ChatAction::ToolCall {
        thread_id: "t1".into(),
        call_id: "call-1".into(),
        name: "bash_command".into(),
        args: "ls".into(),
        weles_review: None,
    });

    assert_eq!(state.active_thread_id(), Some("t2"));
    assert_eq!(state.streaming_content(), "");
    assert_eq!(state.streaming_reasoning(), "");
    assert!(state.active_tool_calls().is_empty());
}

#[test]
fn inactive_thread_done_finalizes_background_stream_on_origin_thread() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadListReceived(vec![
        AgentThread {
            id: "t1".into(),
            title: "First".into(),
            ..Default::default()
        },
        AgentThread {
            id: "t2".into(),
            title: "Second".into(),
            ..Default::default()
        },
    ]));
    state.reduce(ChatAction::SelectThread("t2".into()));

    state.reduce(ChatAction::Delta {
        thread_id: "t1".into(),
        content: "background output".into(),
    });
    state.reduce(ChatAction::Reasoning {
        thread_id: "t1".into(),
        content: "background reasoning".into(),
    });
    state.reduce(ChatAction::TurnDone {
        thread_id: "t1".into(),
        input_tokens: 10,
        output_tokens: 20,
        cost: None,
        provider: None,
        model: None,
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: Some("result_json".to_string()),
    });

    assert_eq!(state.active_thread_id(), Some("t2"));
    assert_eq!(state.streaming_content(), "");
    assert_eq!(state.streaming_reasoning(), "");

    let thread = state
        .threads()
        .iter()
        .find(|thread| thread.id == "t1")
        .expect("origin thread should exist");
    let last = thread
        .messages
        .last()
        .expect("background reply should be recorded");
    assert_eq!(last.role, MessageRole::Assistant);
    assert_eq!(last.content, "background output");
    assert_eq!(last.reasoning.as_deref(), Some("background reasoning"));
}

#[test]
fn thread_detail_keeps_local_messages_with_actions() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "concierge".into(),
        title: "Concierge".into(),
    });
    state.reduce(ChatAction::AppendMessage {
        thread_id: "concierge".into(),
        message: AgentMessage {
            role: MessageRole::Assistant,
            content: "Welcome".into(),
            actions: vec![MessageAction {
                label: "Continue".into(),
                action_type: "continue_session".into(),
                thread_id: Some("t1".into()),
            }],
            is_concierge_welcome: true,
            ..Default::default()
        },
    });

    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "concierge".into(),
        title: "Concierge".into(),
        messages: vec![AgentMessage {
            role: MessageRole::Assistant,
            content: "Welcome".into(),
            ..Default::default()
        }],
        ..Default::default()
    }));

    let thread = state
        .active_thread()
        .expect("concierge thread should exist");
    assert_eq!(thread.messages.len(), 1);
    assert_eq!(thread.messages[0].actions.len(), 1);
}

#[test]
fn thread_detail_preserves_local_messages_in_active_compaction_window() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "Thread".into(),
    });
    state.reduce(ChatAction::SelectThread("t1".into()));
    state.reduce(ChatAction::AppendMessage {
        thread_id: "t1".into(),
        message: AgentMessage {
            role: MessageRole::Assistant,
            content: "rule based".into(),
            message_kind: "compaction_artifact".into(),
            compaction_payload: Some("Older context compacted for continuity".into()),
            timestamp: 10,
            ..Default::default()
        },
    });
    state.reduce(ChatAction::AppendMessage {
        thread_id: "t1".into(),
        message: AgentMessage {
            role: MessageRole::Assistant,
            content: "Local assistant follow-up".into(),
            timestamp: 20,
            ..Default::default()
        },
    });
    state.reduce(ChatAction::AppendMessage {
        thread_id: "t1".into(),
        message: AgentMessage {
            role: MessageRole::User,
            content: "Newest local user prompt".into(),
            timestamp: 30,
            ..Default::default()
        },
    });

    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Thread".into(),
        messages: vec![AgentMessage {
            role: MessageRole::Assistant,
            content: "rule based".into(),
            message_kind: "compaction_artifact".into(),
            compaction_payload: Some("Older context compacted for continuity".into()),
            timestamp: 10,
            ..Default::default()
        }],
        ..Default::default()
    }));

    let thread = state.active_thread().expect("thread should exist");
    assert!(
        thread
            .messages
            .iter()
            .any(|message| message.content == "Local assistant follow-up"),
        "detail merge should preserve local assistant messages after the latest compaction artifact"
    );
    assert!(
        thread
            .messages
            .iter()
            .any(|message| message.content == "Newest local user prompt"),
        "detail merge should preserve local user messages after the latest compaction artifact"
    );
}

#[test]
fn append_message_replaces_previous_concierge_welcome() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "concierge".into(),
        title: "Concierge".into(),
    });
    state.reduce(ChatAction::AppendMessage {
        thread_id: "concierge".into(),
        message: AgentMessage {
            role: MessageRole::Assistant,
            content: "Welcome 1".into(),
            is_concierge_welcome: true,
            ..Default::default()
        },
    });
    state.reduce(ChatAction::AppendMessage {
        thread_id: "concierge".into(),
        message: AgentMessage {
            role: MessageRole::Assistant,
            content: "Welcome 2".into(),
            is_concierge_welcome: true,
            ..Default::default()
        },
    });

    let thread = state
        .active_thread()
        .expect("concierge thread should exist");
    assert_eq!(thread.messages.len(), 1);
    assert_eq!(thread.messages[0].content, "Welcome 2");
    assert!(thread.messages[0].is_concierge_welcome);
}

#[test]
fn append_message_does_not_duplicate_persisted_optimistic_user_tail() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Thread".into(),
        total_message_count: 1,
        loaded_message_start: 0,
        loaded_message_end: 1,
        messages: vec![AgentMessage {
            id: Some("persisted-user".into()),
            role: MessageRole::User,
            content: "same prompt".into(),
            timestamp: 100,
            ..Default::default()
        }],
        ..Default::default()
    }));

    state.reduce(ChatAction::AppendMessage {
        thread_id: "t1".into(),
        message: AgentMessage {
            role: MessageRole::User,
            content: "same prompt".into(),
            timestamp: 101,
            ..Default::default()
        },
    });

    let thread = state
        .threads()
        .iter()
        .find(|thread| thread.id == "t1")
        .expect("thread should exist");
    assert_eq!(
        thread.messages.len(),
        1,
        "optimistic user echo should collapse into the persisted message"
    );
    assert_eq!(thread.total_message_count, 1);
    assert_eq!(thread.messages[0].id.as_deref(), Some("persisted-user"));
}

#[test]
fn append_message_does_not_duplicate_adjacent_optimistic_user_tail() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "Thread".into(),
    });
    state.reduce(ChatAction::AppendMessage {
        thread_id: "t1".into(),
        message: AgentMessage {
            role: MessageRole::User,
            content: "same prompt".into(),
            timestamp: 100,
            ..Default::default()
        },
    });

    state.reduce(ChatAction::AppendMessage {
        thread_id: "t1".into(),
        message: AgentMessage {
            role: MessageRole::User,
            content: "same prompt".into(),
            timestamp: 101,
            ..Default::default()
        },
    });

    let thread = state
        .threads()
        .iter()
        .find(|thread| thread.id == "t1")
        .expect("thread should exist");
    assert_eq!(
        thread.messages.len(),
        1,
        "double-submit optimistic user echo should collapse at the tail"
    );
    assert_eq!(thread.total_message_count, 1);
}

#[test]
fn append_message_does_not_duplicate_persisted_normal_echo_after_optimistic_tail() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "Thread".into(),
    });
    state.reduce(ChatAction::AppendMessage {
        thread_id: "t1".into(),
        message: AgentMessage {
            role: MessageRole::User,
            content: "same prompt".into(),
            timestamp: 100,
            ..Default::default()
        },
    });

    state.reduce(ChatAction::AppendMessage {
        thread_id: "t1".into(),
        message: AgentMessage {
            id: Some("persisted-user".into()),
            role: MessageRole::User,
            content: "same prompt".into(),
            message_kind: "normal".into(),
            timestamp: 101,
            ..Default::default()
        },
    });

    let thread = state
        .threads()
        .iter()
        .find(|thread| thread.id == "t1")
        .expect("thread should exist");
    assert_eq!(
        thread.messages.len(),
        1,
        "persisted normal echo should replace the optimistic user tail"
    );
    assert_eq!(thread.total_message_count, 1);
    assert_eq!(thread.messages[0].id.as_deref(), Some("persisted-user"));
    assert_eq!(thread.messages[0].message_kind, "normal");
}

#[test]
fn dismiss_concierge_welcome_removes_only_welcome_messages() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "concierge".into(),
        title: "Concierge".into(),
    });
    state.reduce(ChatAction::AppendMessage {
        thread_id: "concierge".into(),
        message: AgentMessage {
            role: MessageRole::Assistant,
            content: "Welcome".into(),
            is_concierge_welcome: true,
            ..Default::default()
        },
    });
    state.reduce(ChatAction::AppendMessage {
        thread_id: "concierge".into(),
        message: AgentMessage {
            role: MessageRole::Assistant,
            content: "Follow-up".into(),
            ..Default::default()
        },
    });

    state.reduce(ChatAction::DismissConciergeWelcome);

    let thread = state
        .active_thread()
        .expect("concierge thread should exist");
    assert_eq!(thread.messages.len(), 1);
    assert_eq!(thread.messages[0].content, "Follow-up");
    assert!(!thread.messages[0].is_concierge_welcome);
}

fn state_with_messages(count: usize) -> ChatState {
    let mut state = ChatState::new();
    let messages: Vec<AgentMessage> = (0..count)
        .map(|index| AgentMessage {
            role: MessageRole::User,
            content: format!("msg {}", index),
            ..Default::default()
        })
        .collect();
    let thread = AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        messages,
        ..Default::default()
    };
    state.reduce(ChatAction::ThreadListReceived(vec![thread]));
    state.reduce(ChatAction::SelectThread("t1".into()));
    state
}

#[test]
fn select_next_message_from_none() {
    let mut state = state_with_messages(3);
    assert_eq!(state.selected_message(), None);
    state.select_next_message();
    assert_eq!(state.selected_message(), Some(0));
}

#[test]
fn select_next_message_advances() {
    let mut state = state_with_messages(3);
    state.select_next_message();
    state.select_next_message();
    assert_eq!(state.selected_message(), Some(1));
}

#[test]
fn select_next_message_clamps_at_end() {
    let mut state = state_with_messages(2);
    state.select_message(Some(1));
    state.select_next_message();
    assert_eq!(state.selected_message(), Some(1));
}

#[test]
fn select_prev_message_from_none() {
    let mut state = state_with_messages(3);
    state.select_prev_message();
    assert_eq!(state.selected_message(), Some(2));
}

#[test]
fn select_prev_message_decreases() {
    let mut state = state_with_messages(3);
    state.select_message(Some(2));
    state.select_prev_message();
    assert_eq!(state.selected_message(), Some(1));
}

#[test]
fn thread_detail_latest_page_sets_window_metadata() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 120,
        loaded_message_start: 70,
        loaded_message_end: 120,
        messages: (70..120)
            .map(|index| AgentMessage {
                id: Some(format!("msg-{index}")),
                role: MessageRole::User,
                content: format!("msg {index}"),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }));
    state.reduce(ChatAction::SelectThread("t1".into()));

    let thread = state.active_thread().expect("thread should exist");
    assert_eq!(thread.total_message_count, 120);
    assert_eq!(thread.loaded_message_start, 70);
    assert_eq!(thread.loaded_message_end, 120);
    assert_eq!(thread.messages.len(), 50);
}

#[test]
fn older_thread_page_prepends_into_loaded_window() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 120,
        loaded_message_start: 70,
        loaded_message_end: 120,
        messages: (70..120)
            .map(|index| AgentMessage {
                id: Some(format!("msg-{index}")),
                role: MessageRole::User,
                content: format!("msg {index}"),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }));
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 120,
        loaded_message_start: 20,
        loaded_message_end: 70,
        messages: (20..70)
            .map(|index| AgentMessage {
                id: Some(format!("msg-{index}")),
                role: MessageRole::User,
                content: format!("msg {index}"),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }));
    state.reduce(ChatAction::SelectThread("t1".into()));

    let thread = state.active_thread().expect("thread should exist");
    assert_eq!(thread.loaded_message_start, 20);
    assert_eq!(thread.loaded_message_end, 120);
    assert_eq!(thread.messages.len(), 100);
    assert_eq!(
        thread
            .messages
            .first()
            .and_then(|message| message.id.as_deref()),
        Some("msg-20")
    );
    assert_eq!(
        thread
            .messages
            .last()
            .and_then(|message| message.id.as_deref()),
        Some("msg-119")
    );
}

#[test]
fn thread_detail_refresh_replaces_window_when_total_visible_messages_shrink() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 5,
        loaded_message_start: 0,
        loaded_message_end: 5,
        messages: (0..5)
            .map(|index| AgentMessage {
                id: Some(format!("msg-{index}")),
                role: MessageRole::Assistant,
                content: format!("before {index}"),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }));

    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 2,
        loaded_message_start: 0,
        loaded_message_end: 2,
        messages: vec![
            AgentMessage {
                id: Some("msg-compaction".into()),
                role: MessageRole::Assistant,
                content: "Auto compaction applied".into(),
                message_kind: "compaction_artifact".into(),
                compaction_payload: Some("Older context compacted".into()),
                ..Default::default()
            },
            AgentMessage {
                id: Some("msg-latest".into()),
                role: MessageRole::Assistant,
                content: "Latest visible reply".into(),
                author_agent_id: Some("domowoj".into()),
                author_agent_name: Some("Domowoj".into()),
                ..Default::default()
            },
        ],
        ..Default::default()
    }));

    let thread = state
        .threads()
        .iter()
        .find(|thread| thread.id == "t1")
        .unwrap();
    assert_eq!(thread.total_message_count, 2);
    assert_eq!(thread.loaded_message_start, 0);
    assert_eq!(thread.loaded_message_end, 2);
    assert_eq!(thread.messages.len(), 2);
    assert_eq!(
        thread.messages[0].id.as_deref(),
        Some("msg-compaction"),
        "authoritative refresh should discard stale pre-compaction rows"
    );
    assert_eq!(
        thread.messages[1].author_agent_name.as_deref(),
        Some("Domowoj"),
        "participant-authored metadata from the authoritative refresh should survive"
    );
}

#[test]
fn thread_detail_refresh_preserves_optimistic_local_tail_when_smaller_snapshot_matches() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 2,
        loaded_message_start: 0,
        loaded_message_end: 2,
        messages: vec![
            AgentMessage {
                id: Some("msg-0".into()),
                role: MessageRole::Assistant,
                content: "Earlier reply".into(),
                ..Default::default()
            },
            AgentMessage {
                id: Some("msg-1".into()),
                role: MessageRole::User,
                content: "Previous prompt".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    }));
    state.reduce(ChatAction::AppendMessage {
        thread_id: "t1".into(),
        message: AgentMessage {
            role: MessageRole::User,
            content: "Please inspect this image\n[image attachment]".into(),
            ..Default::default()
        },
    });

    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 2,
        loaded_message_start: 0,
        loaded_message_end: 2,
        messages: vec![
            AgentMessage {
                id: Some("msg-0".into()),
                role: MessageRole::Assistant,
                content: "Earlier reply".into(),
                ..Default::default()
            },
            AgentMessage {
                id: Some("msg-1".into()),
                role: MessageRole::User,
                content: "Previous prompt".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    }));

    let thread = state
        .threads()
        .iter()
        .find(|thread| thread.id == "t1")
        .unwrap();
    assert_eq!(thread.messages.len(), 3);
    assert_eq!(
        thread
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some("Please inspect this image\n[image attachment]"),
        "stale detail refresh should not erase the optimistic local tail"
    );
    assert_eq!(thread.total_message_count, 3);
    assert_eq!(thread.loaded_message_end, 3);
}

#[test]
fn thread_detail_refresh_preserves_finalized_stream_after_stale_user_snapshot() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "Test".into(),
    });
    state.reduce(ChatAction::AppendMessage {
        thread_id: "t1".into(),
        message: AgentMessage {
            role: MessageRole::User,
            content: "Follow up".into(),
            timestamp: 100,
            ..Default::default()
        },
    });
    state.reduce(ChatAction::Delta {
        thread_id: "t1".into(),
        content: "Here is the answer".into(),
    });
    state.reduce(ChatAction::TurnDone {
        thread_id: "t1".into(),
        input_tokens: 1,
        output_tokens: 2,
        cost: None,
        provider: None,
        model: None,
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: None,
    });

    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 1,
        loaded_message_start: 0,
        loaded_message_end: 1,
        messages: vec![AgentMessage {
            id: Some("msg-user".into()),
            role: MessageRole::User,
            content: "Follow up".into(),
            timestamp: 200,
            ..Default::default()
        }],
        ..Default::default()
    }));

    let thread = state.active_thread().unwrap();
    assert_eq!(thread.messages.len(), 2);
    assert_eq!(thread.messages[0].content, "Follow up");
    assert_eq!(
        thread.messages[1].content, "Here is the answer",
        "stale detail refresh should not erase the just-finalized assistant stream"
    );
}

#[test]
fn thread_detail_reload_with_short_tail_does_not_wipe_existing_messages() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 4,
        loaded_message_start: 0,
        loaded_message_end: 4,
        messages: (0..4)
            .map(|index| AgentMessage {
                id: Some(format!("msg-{index}")),
                role: MessageRole::Assistant,
                content: format!("existing {index}"),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }));

    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 1,
        loaded_message_start: 0,
        loaded_message_end: 1,
        messages: vec![AgentMessage {
            id: Some("msg-new".into()),
            role: MessageRole::User,
            content: "new prompt after reload".into(),
            ..Default::default()
        }],
        ..Default::default()
    }));

    let thread = state
        .threads()
        .iter()
        .find(|thread| thread.id == "t1")
        .expect("thread should exist");
    let ids = thread
        .messages
        .iter()
        .filter_map(|message| message.id.as_deref())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec!["msg-0", "msg-1", "msg-2", "msg-3", "msg-new"],
        "short reload snapshots should not replace already loaded history"
    );
}

#[test]
fn empty_thread_detail_does_not_wipe_existing_messages() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Thread".into(),
        total_message_count: 1,
        loaded_message_start: 0,
        loaded_message_end: 1,
        messages: vec![AgentMessage {
            id: Some("msg-0".into()),
            role: MessageRole::Assistant,
            content: "First reply".into(),
            ..Default::default()
        }],
        ..Default::default()
    }));
    state.reduce(ChatAction::AppendMessage {
        thread_id: "t1".into(),
        message: AgentMessage {
            role: MessageRole::User,
            content: "Follow up".into(),
            ..Default::default()
        },
    });

    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Thread".into(),
        agent_name: Some("Dola".into()),
        ..Default::default()
    }));

    let thread = state
        .threads()
        .iter()
        .find(|thread| thread.id == "t1")
        .unwrap();
    assert_eq!(thread.messages.len(), 2);
    assert_eq!(
        thread
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some("Follow up")
    );
    assert_eq!(thread.agent_name.as_deref(), Some("Dola"));
}

#[test]
fn empty_thread_detail_preserves_paged_message_window() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Thread".into(),
        total_message_count: 120,
        loaded_message_start: 70,
        loaded_message_end: 120,
        messages: (70..120)
            .map(|index| AgentMessage {
                id: Some(format!("msg-{index}")),
                role: MessageRole::Assistant,
                content: format!("visible {index}"),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }));

    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Thread".into(),
        profile_model: Some("gpt-5.5".into()),
        ..Default::default()
    }));

    let thread = state
        .threads()
        .iter()
        .find(|thread| thread.id == "t1")
        .unwrap();
    assert_eq!(thread.messages.len(), 50);
    assert_eq!(thread.total_message_count, 120);
    assert_eq!(thread.loaded_message_start, 70);
    assert_eq!(thread.loaded_message_end, 120);
    assert_eq!(thread.profile_model.as_deref(), Some("gpt-5.5"));
    assert_eq!(
        thread
            .messages
            .first()
            .and_then(|message| message.id.as_deref()),
        Some("msg-70")
    );
}

#[test]
fn thread_detail_refresh_replaces_window_when_overlapping_message_ids_shift() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 4,
        loaded_message_start: 0,
        loaded_message_end: 4,
        messages: (0..4)
            .map(|index| AgentMessage {
                id: Some(format!("msg-{index}")),
                role: MessageRole::Assistant,
                content: format!("before {index}"),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }));

    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 4,
        loaded_message_start: 0,
        loaded_message_end: 4,
        messages: vec![
            AgentMessage {
                id: Some("msg-1".into()),
                role: MessageRole::Assistant,
                content: "shifted 1".into(),
                ..Default::default()
            },
            AgentMessage {
                id: Some("msg-2".into()),
                role: MessageRole::Assistant,
                content: "shifted 2".into(),
                ..Default::default()
            },
            AgentMessage {
                id: Some("msg-3".into()),
                role: MessageRole::Assistant,
                content: "shifted 3".into(),
                ..Default::default()
            },
            AgentMessage {
                id: Some("msg-4".into()),
                role: MessageRole::Assistant,
                content: "shifted 4".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    }));

    let thread = state
        .threads()
        .iter()
        .find(|thread| thread.id == "t1")
        .unwrap();
    let ids = thread
        .messages
        .iter()
        .filter_map(|message| message.id.as_deref())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec!["msg-1", "msg-2", "msg-3", "msg-4"],
        "overlapping windows with shifted message IDs should prefer the fresh authoritative mapping"
    );
}

#[test]
fn thread_detail_derives_compaction_boundary_and_preserves_it_after_collapse() {
    let mut state = ChatState::new();
    state.set_history_page_size(2);
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 5,
        loaded_message_start: 0,
        loaded_message_end: 5,
        messages: vec![
            AgentMessage {
                id: Some("msg-0".into()),
                role: MessageRole::User,
                content: "before".into(),
                ..Default::default()
            },
            AgentMessage {
                id: Some("msg-1".into()),
                role: MessageRole::Assistant,
                content: "compacted".into(),
                message_kind: "compaction_artifact".into(),
                compaction_payload: Some("Older context compacted".into()),
                ..Default::default()
            },
            AgentMessage {
                id: Some("msg-2".into()),
                role: MessageRole::Assistant,
                content: "after 1".into(),
                ..Default::default()
            },
            AgentMessage {
                id: Some("msg-3".into()),
                role: MessageRole::User,
                content: "after 2".into(),
                ..Default::default()
            },
            AgentMessage {
                id: Some("msg-4".into()),
                role: MessageRole::Assistant,
                content: "after 3".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    }));
    state.reduce(ChatAction::SelectThread("t1".into()));

    let before = state.active_thread().expect("thread should exist");
    assert_eq!(
        before.active_compaction_window_start,
        Some(1),
        "thread detail should derive the latest compaction boundary from loaded messages"
    );

    state.schedule_history_collapse(0, 0);
    state.maybe_collapse_history(0);

    let after = state.active_thread().expect("thread should exist");
    assert_eq!(after.loaded_message_start, 3);
    assert_eq!(after.messages.len(), 2);
    assert_eq!(
        after.active_compaction_window_start,
        Some(1),
        "history collapse should preserve the absolute compaction boundary even after trimming the artifact"
    );
}

#[test]
fn collapse_history_keeps_latest_page_only() {
    let mut state = ChatState::new();
    state.set_history_page_size(50);
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 120,
        loaded_message_start: 20,
        loaded_message_end: 120,
        messages: (20..120)
            .map(|index| AgentMessage {
                id: Some(format!("msg-{index}")),
                role: MessageRole::User,
                content: format!("msg {index}"),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }));
    state.reduce(ChatAction::SelectThread("t1".into()));

    state.schedule_history_collapse(10, 5);
    state.maybe_collapse_history(14);
    assert_eq!(state.active_thread().expect("thread").messages.len(), 100);

    state.maybe_collapse_history(15);
    let thread = state.active_thread().expect("thread should exist");
    assert_eq!(thread.loaded_message_start, 70);
    assert_eq!(thread.loaded_message_end, 120);
    assert_eq!(thread.messages.len(), 50);
    assert_eq!(
        thread
            .messages
            .first()
            .and_then(|message| message.id.as_deref()),
        Some("msg-70")
    );
}

#[test]
fn collapse_history_waits_when_viewport_is_locked_at_zero_offset() {
    let mut state = ChatState::new();
    state.set_history_page_size(50);
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 120,
        loaded_message_start: 0,
        loaded_message_end: 120,
        messages: (0..120)
            .map(|index| AgentMessage {
                id: Some(format!("msg-{index}")),
                role: MessageRole::User,
                content: format!("msg {index}"),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }));
    state.reduce(ChatAction::SelectThread("t1".into()));
    state.preserve_prepend_scroll_anchor(0);

    state.schedule_history_collapse(10, 5);
    state.maybe_collapse_history(15);

    let thread = state.active_thread().expect("thread should exist");
    assert_eq!(thread.loaded_message_start, 0);
    assert_eq!(thread.loaded_message_end, 120);
    assert_eq!(thread.messages.len(), 120);
    assert_eq!(
        thread
            .messages
            .first()
            .and_then(|message| message.id.as_deref()),
        Some("msg-0")
    );
}

#[test]
fn selected_message_tracks_same_message_when_older_page_is_prepended() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 120,
        loaded_message_start: 70,
        loaded_message_end: 120,
        messages: (70..120)
            .map(|index| AgentMessage {
                id: Some(format!("msg-{index}")),
                role: MessageRole::User,
                content: format!("msg {index}"),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }));
    state.reduce(ChatAction::SelectThread("t1".into()));
    state.select_message(Some(10));

    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 120,
        loaded_message_start: 20,
        loaded_message_end: 70,
        messages: (20..70)
            .map(|index| AgentMessage {
                id: Some(format!("msg-{index}")),
                role: MessageRole::User,
                content: format!("msg {index}"),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }));

    let selected_index = state.selected_message().expect("selection should survive");
    let thread = state.active_thread().expect("thread should exist");
    assert_eq!(selected_index, 60);
    assert_eq!(
        thread.messages[selected_index].id.as_deref(),
        Some("msg-80")
    );
}

#[test]
fn selected_message_tracks_same_message_when_append_trims_latest_window() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 100,
        loaded_message_start: 0,
        loaded_message_end: 100,
        messages: (0..100)
            .map(|index| AgentMessage {
                id: Some(format!("msg-{index}")),
                role: MessageRole::User,
                content: format!("msg {index}"),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }));
    state.reduce(ChatAction::SelectThread("t1".into()));
    state.select_message(Some(80));

    state.reduce(ChatAction::AppendMessage {
        thread_id: "t1".into(),
        message: AgentMessage {
            id: Some("msg-100".into()),
            role: MessageRole::Assistant,
            content: "msg 100".into(),
            ..Default::default()
        },
    });

    let selected_index = state.selected_message().expect("selection should survive");
    let thread = state.active_thread().expect("thread should exist");
    assert_eq!(selected_index, 79);
    assert_eq!(
        thread.messages[selected_index].id.as_deref(),
        Some("msg-80")
    );
}

#[test]
fn resolve_message_ref_ignores_absolute_indexes_before_loaded_window() {
    let thread = AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 8,
        loaded_message_start: 5,
        loaded_message_end: 8,
        messages: (5..8)
            .map(|index| AgentMessage {
                id: Some(format!("msg-{index}")),
                role: MessageRole::User,
                content: format!("msg {index}"),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };

    let message_ref = StoredMessageRef {
        thread_id: "t1".into(),
        message_id: None,
        absolute_index: 2,
    };

    assert_eq!(resolve_message_ref(&thread, &message_ref), None);
}

#[test]
fn expanded_reasoning_and_tools_track_same_messages_across_window_updates() {
    let mut state = ChatState::new();
    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 100,
        loaded_message_start: 0,
        loaded_message_end: 100,
        messages: (0..100)
            .map(|index| AgentMessage {
                id: Some(format!("msg-{index}")),
                role: if index == 80 {
                    MessageRole::Assistant
                } else if index == 90 {
                    MessageRole::Tool
                } else {
                    MessageRole::User
                },
                content: format!("msg {index}"),
                reasoning: (index == 80).then(|| "reasoning".to_string()),
                tool_name: (index == 90).then(|| "bash".to_string()),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }));
    state.reduce(ChatAction::SelectThread("t1".into()));
    state.toggle_reasoning(80);
    state.toggle_tool_expansion(90);

    state.reduce(ChatAction::AppendMessage {
        thread_id: "t1".into(),
        message: AgentMessage {
            id: Some("msg-100".into()),
            role: MessageRole::Assistant,
            content: "msg 100".into(),
            ..Default::default()
        },
    });

    let thread = state.active_thread().expect("thread should exist");
    let expanded_reasoning = state.expanded_reasoning();
    let expanded_tools = state.expanded_tools();
    let reasoning_index = expanded_reasoning
        .iter()
        .copied()
        .next()
        .expect("reasoning expansion should survive");
    let tool_index = expanded_tools
        .iter()
        .copied()
        .next()
        .expect("tool expansion should survive");
    assert_eq!(
        thread.messages[reasoning_index].id.as_deref(),
        Some("msg-80")
    );
    assert_eq!(thread.messages[tool_index].id.as_deref(), Some("msg-90"));

    state.reduce(ChatAction::ThreadDetailReceived(AgentThread {
        id: "t1".into(),
        title: "Test".into(),
        total_message_count: 101,
        loaded_message_start: 0,
        loaded_message_end: 50,
        messages: (0..50)
            .map(|index| AgentMessage {
                id: Some(format!("msg-{index}")),
                role: MessageRole::User,
                content: format!("msg {index}"),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }));

    let thread = state.active_thread().expect("thread should exist");
    let expanded_reasoning = state.expanded_reasoning();
    let expanded_tools = state.expanded_tools();
    let reasoning_index = expanded_reasoning
        .iter()
        .copied()
        .next()
        .expect("reasoning expansion should survive prepend");
    let tool_index = expanded_tools
        .iter()
        .copied()
        .next()
        .expect("tool expansion should survive prepend");
    assert_eq!(
        thread.messages[reasoning_index].id.as_deref(),
        Some("msg-80")
    );
    assert_eq!(thread.messages[tool_index].id.as_deref(), Some("msg-90"));
}

#[test]
fn select_prev_message_clamps_at_zero() {
    let mut state = state_with_messages(3);
    state.select_message(Some(0));
    state.select_prev_message();
    assert_eq!(state.selected_message(), Some(0));
}

#[test]
fn clear_selection() {
    let mut state = state_with_messages(3);
    state.select_message(Some(1));
    state.select_message(None);
    assert_eq!(state.selected_message(), None);
}

#[test]
fn toggle_message_selection_clears_when_same_message_clicked() {
    let mut state = state_with_messages(3);
    state.toggle_message_selection(1);
    assert_eq!(state.selected_message(), Some(1));
    state.toggle_message_selection(1);
    assert_eq!(state.selected_message(), None);
}

#[test]
fn toggle_tool_expansion() {
    let mut state = state_with_messages(1);
    assert!(!state.expanded_tools().contains(&0));
    state.toggle_tool_expansion(0);
    assert!(state.expanded_tools().contains(&0));
    state.toggle_tool_expansion(0);
    assert!(!state.expanded_tools().contains(&0));
}

#[test]
fn toggle_tool_expansion_independent() {
    let mut state = state_with_messages(2);
    state.toggle_tool_expansion(0);
    state.toggle_tool_expansion(1);
    assert!(state.expanded_tools().contains(&0));
    assert!(state.expanded_tools().contains(&1));
    state.toggle_tool_expansion(0);
    assert!(!state.expanded_tools().contains(&0));
    assert!(state.expanded_tools().contains(&1));
}
