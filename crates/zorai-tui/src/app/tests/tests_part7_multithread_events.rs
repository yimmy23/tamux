fn make_model_with_daemon_rx_for_multithread_tests() -> (
    TuiModel,
    tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) {
    let (_daemon_tx, daemon_rx) = std::sync::mpsc::channel();
    let (cmd_tx, cmd_rx) = unbounded_channel();
    (TuiModel::new(daemon_rx, cmd_tx), cmd_rx)
}

fn seed_two_visible_threads(model: &mut TuiModel) {
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-other".to_string(),
        title: "Other Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));
}

fn drain_daemon_commands(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Vec<DaemonCommand> {
    let mut commands = Vec::new();
    while let Ok(command) = daemon_rx.try_recv() {
        commands.push(command);
    }
    commands
}

#[test]
fn background_delta_isolated_to_origin_thread_until_switch() {
    let mut model = build_model();
    seed_two_visible_threads(&mut model);

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-other".to_string(),
        content: "background partial".to_string(),
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert!(model.footer_activity_text().is_none());
    assert_eq!(model.chat.streaming_content(), "");

    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-other".to_string()));

    assert_eq!(model.footer_activity_text().as_deref(), Some("writing"));
    assert_eq!(model.chat.streaming_content(), "background partial");
}

#[test]
fn background_reasoning_isolated_to_origin_thread_until_switch() {
    let mut model = build_model();
    seed_two_visible_threads(&mut model);

    model.handle_client_event(ClientEvent::Reasoning {
        thread_id: "thread-other".to_string(),
        content: "background reasoning".to_string(),
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert!(model.footer_activity_text().is_none());
    assert_eq!(model.chat.streaming_reasoning(), "");

    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-other".to_string()));

    assert_eq!(model.footer_activity_text().as_deref(), Some("reasoning"));
    assert_eq!(model.chat.streaming_reasoning(), "background reasoning");
}

#[test]
fn background_tool_events_do_not_clear_selected_thread_activity() {
    let mut model = build_model();
    seed_two_visible_threads(&mut model);

    model.handle_client_event(ClientEvent::ToolCall {
        thread_id: "thread-user".to_string(),
        call_id: "user-call".to_string(),
        name: "bash_command".to_string(),
        arguments: "{\"command\":\"pwd\"}".to_string(),
        weles_review: None,
    });
    assert_eq!(model.chat.active_tool_calls().len(), 1);

    model.handle_client_event(ClientEvent::ToolCall {
        thread_id: "thread-other".to_string(),
        call_id: "other-call".to_string(),
        name: "bash_command".to_string(),
        arguments: "{\"command\":\"ls\"}".to_string(),
        weles_review: None,
    });
    model.handle_client_event(ClientEvent::ToolResult {
        thread_id: "thread-other".to_string(),
        call_id: "other-call".to_string(),
        name: "bash_command".to_string(),
        content: "Cargo.toml".to_string(),
        is_error: false,
        weles_review: None,
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert_eq!(model.chat.active_tool_calls().len(), 1);
    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("⚙  bash_command")
    );

    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-other".to_string()));

    assert!(!model.chat.has_running_tool_calls());
    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("⚙  bash_command ✓")
    );
}

#[test]
fn background_retry_status_isolated_to_origin_thread_until_switch() {
    let mut model = build_model();
    seed_two_visible_threads(&mut model);

    model.handle_client_event(ClientEvent::RetryStatus {
        thread_id: "thread-other".to_string(),
        phase: "waiting".to_string(),
        attempt: 2,
        max_retries: 3,
        delay_ms: 5_000,
        failure_class: "provider".to_string(),
        message: "retrying after disconnect".to_string(),
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert!(model.footer_activity_text().is_none());
    assert!(model.chat.retry_status().is_none());

    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-other".to_string()));

    assert_eq!(model.footer_activity_text().as_deref(), Some("retry wait"));
    assert!(model.chat.retry_status().is_some());
}

#[test]
fn background_done_finalizes_origin_thread_without_clearing_selected_thread_busy_state() {
    let mut model = build_model();
    seed_two_visible_threads(&mut model);
    model.set_active_thread_activity("thinking");

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-other".to_string(),
        content: "background answer".to_string(),
    });
    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-other".to_string(),
        input_tokens: 11,
        output_tokens: 17,
        cost: None,
        provider: Some("openai".to_string()),
        model: Some("gpt-5.4".to_string()),
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: None,
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));

    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-other".to_string()));

    let thread = model
        .chat
        .active_thread()
        .expect("background thread should exist");
    let last_message = thread.messages.last().expect("done should append assistant");
    assert_eq!(last_message.content, "background answer");
    assert!(model.footer_activity_text().is_none());
}

#[test]
fn active_thread_reload_required_preserves_thinking_when_waiting_for_first_response() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx_for_multithread_tests();
    model.config.tui_chat_history_page_size = 123;
    model.connected = true;
    seed_two_visible_threads(&mut model);
    model.submit_prompt("inspect the race".to_string());
    drain_daemon_commands(&mut daemon_rx);

    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));

    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "thread-user".to_string(),
    });

    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("thinking"),
        "reload before the first assistant event should preserve pending thinking"
    );
    assert!(
        model.assistant_busy(),
        "reload before the first assistant event should keep the thread marked busy"
    );

    let commands = drain_daemon_commands(&mut daemon_rx);
    assert!(
        commands.iter().any(|command| matches!(
            command,
            DaemonCommand::RequestThread {
                thread_id,
                message_limit: Some(123),
                message_offset: Some(0),
            } if thread_id == "thread-user"
        )),
        "reload should request fresh thread detail"
    );
    assert!(
        commands.iter().any(|command| matches!(
            command,
            DaemonCommand::RequestThreadTodos(thread_id) if thread_id == "thread-user"
        )),
        "reload should request fresh todos"
    );
    assert!(
        commands.iter().any(|command| matches!(
            command,
            DaemonCommand::RequestThreadWorkContext(thread_id) if thread_id == "thread-user"
        )),
        "reload should request fresh work context"
    );
}

#[test]
fn active_thread_reload_required_clears_stale_busy_state_without_pending_first_response() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx_for_multithread_tests();
    model.config.tui_chat_history_page_size = 123;
    model.connected = true;
    seed_two_visible_threads(&mut model);
    model.set_active_thread_activity("thinking");

    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "thread-user".to_string(),
    });

    assert!(
        model.footer_activity_text().is_none(),
        "reload fallback for the selected thread must clear stale busy UI"
    );
    assert!(
        !model.assistant_busy(),
        "reload fallback should not leave the selected thread busy forever"
    );

    let commands = drain_daemon_commands(&mut daemon_rx);
    assert!(
        commands.iter().any(|command| matches!(
            command,
            DaemonCommand::RequestThread {
                thread_id,
                message_limit: Some(123),
                message_offset: Some(0),
            } if thread_id == "thread-user"
        )),
        "reload should request fresh thread detail"
    );
}

#[test]
fn background_workflow_notice_isolated_to_origin_thread_until_switch() {
    let mut model = build_model();
    seed_two_visible_threads(&mut model);
    model.set_active_thread_activity("thinking");

    model.handle_client_event(ClientEvent::WorkflowNotice {
        thread_id: Some("thread-other".to_string()),
        kind: "skill-gate".to_string(),
        message: "background skill gate".to_string(),
        details: Some(r#"{"recommended_skill":"onecontext"}"#.to_string()),
    });

    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));

    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-other".to_string()));

    assert_eq!(model.footer_activity_text().as_deref(), Some("skill gate"));
}

#[test]
fn background_todos_and_work_context_stay_attached_to_origin_thread() {
    let mut model = build_model();
    seed_two_visible_threads(&mut model);

    model.handle_client_event(ClientEvent::ThreadTodos {
        thread_id: "thread-other".to_string(),
        goal_run_id: None,
        step_index: None,
        items: vec![crate::wire::TodoItem {
            id: "todo-1".to_string(),
            content: "Check routed done event".to_string(),
            status: Some(crate::wire::TodoStatus::InProgress),
            position: 0,
            step_index: None,
            created_at: 1,
            updated_at: 1,
        }],
    });
    model.handle_client_event(ClientEvent::WorkContext(crate::wire::ThreadWorkContext {
        thread_id: "thread-other".to_string(),
        entries: vec![crate::wire::WorkContextEntry {
            path: "crates/zorai-tui/src/app/events.rs".to_string(),
            kind: Some(crate::wire::WorkContextEntryKind::RepoChange),
            source: "test".to_string(),
            is_text: true,
            updated_at: 1,
            ..Default::default()
        }],
    }));

    assert!(model.tasks.todos_for_thread("thread-user").is_empty());
    assert_eq!(model.tasks.todos_for_thread("thread-other").len(), 1);
    assert_eq!(
        model
            .tasks
            .work_context_for_thread("thread-other")
            .expect("background work context should be stored")
            .entries
            .len(),
        1
    );
}

#[test]
fn background_participant_suggestion_keeps_selected_thread_activity_and_origin_thread_binding() {
    let mut model = build_model();
    seed_two_visible_threads(&mut model);
    model.set_active_thread_activity("thinking");

    model.handle_client_event(ClientEvent::ParticipantSuggestion {
        thread_id: "thread-other".to_string(),
        suggestion: crate::wire::ThreadParticipantSuggestion {
            id: "suggestion-1".to_string(),
            target_agent_id: "weles".to_string(),
            target_agent_name: "Weles".to_string(),
            instruction: "check the race".to_string(),
            suggestion_kind: "prepared_message".to_string(),
            force_send: false,
            status: "queued".to_string(),
            created_at: 1,
            updated_at: 1,
            auto_send_at: None,
            source_message_timestamp: None,
            error: None,
        },
    });

    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));
    assert_eq!(model.queued_prompts.len(), 1);
    assert_eq!(
        model.queued_prompts[0].thread_id.as_deref(),
        Some("thread-other")
    );
    assert_eq!(
        model.queued_prompts[0].participant_agent_name.as_deref(),
        Some("Weles")
    );
}
