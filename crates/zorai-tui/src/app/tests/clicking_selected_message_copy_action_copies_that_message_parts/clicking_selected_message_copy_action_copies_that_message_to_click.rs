use super::super::{build_model, unbounded_channel};
use super::*;
use std::sync::mpsc;
use zorai_protocol::{AgentDbMessage, AgentDbThread};
#[test]
fn clicking_selected_message_copy_action_copies_that_message() {
    let mut model = build_model();
    model.show_sidebar_override = Some(false);
    model.focus = FocusArea::Chat;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::User,
            content: "first".to_string(),
            ..Default::default()
        },
    });
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "second".to_string(),
            ..Default::default()
        },
    });
    model.chat.select_message(Some(1));

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let copy_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::chat::hit_test(
                    chat_area,
                    &model.chat,
                    &model.theme,
                    model.tick_counter,
                    pos,
                ) == Some(chat::ChatHitTarget::CopyMessage(1))
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("selected message should expose a clickable copy action");

    super::conversion::reset_last_copied_text();

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: copy_pos.x,
        row: copy_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: copy_pos.x,
        row: copy_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(
        super::conversion::last_copied_text().as_deref(),
        Some("second")
    );
}

#[test]
fn clicking_selected_message_action_with_small_pointer_shift_still_copies_message() {
    let mut model = build_model();
    model.show_sidebar_override = Some(false);
    model.focus = FocusArea::Chat;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "second".to_string(),
            ..Default::default()
        },
    });
    model.chat.select_message(Some(0));

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let copy_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::chat::hit_test(
                    chat_area,
                    &model.chat,
                    &model.theme,
                    model.tick_counter,
                    pos,
                ) == Some(chat::ChatHitTarget::CopyMessage(0))
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("selected message should expose a clickable copy action");

    super::conversion::reset_last_copied_text();

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: copy_pos.x,
        row: copy_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: copy_pos.x.saturating_add(1),
        row: copy_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(
        super::conversion::last_copied_text().as_deref(),
        Some("second")
    );
}

#[test]
fn assistant_message_feedback_without_id_waits_for_saved_message_id() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Thread".to_string(),
            messages: vec![
                chat::AgentMessage {
                    id: Some("user-1".to_string()),
                    role: chat::MessageRole::User,
                    content: "question".to_string(),
                    ..Default::default()
                },
                chat::AgentMessage {
                    id: None,
                    role: chat::MessageRole::Assistant,
                    content: "answer".to_string(),
                    ..Default::default()
                },
            ],
            total_message_count: 2,
            loaded_message_start: 0,
            loaded_message_end: 2,
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.submit_message_feedback(1, zorai_protocol::Reaction::Up);

    let message = model
        .chat
        .active_thread()
        .and_then(|thread| thread.messages.get(1))
        .expect("assistant message should exist");
    assert_eq!(message.feedback, None);
    assert_eq!(
        model.status_line,
        "Feedback is available after the message is saved"
    );
    assert!(cmd_rx.try_recv().is_err());
}

#[test]
fn fork_message_creates_new_thread_with_history_through_selected_message() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "parent-thread".to_string(),
            agent_name: Some("Swarog".to_string()),
            profile_provider: Some("openai".to_string()),
            profile_model: Some("gpt-5.4".to_string()),
            profile_context_window_tokens: Some(400_000),
            title: "Parent".to_string(),
            messages: vec![
                chat::AgentMessage {
                    id: Some("parent-0".to_string()),
                    role: chat::MessageRole::User,
                    content: "first".to_string(),
                    timestamp: 10,
                    input_tokens: 3,
                    ..Default::default()
                },
                chat::AgentMessage {
                    id: Some("parent-1".to_string()),
                    role: chat::MessageRole::Assistant,
                    content: "answer".to_string(),
                    reasoning: Some("because".to_string()),
                    timestamp: 20,
                    output_tokens: 5,
                    ..Default::default()
                },
                chat::AgentMessage {
                    id: Some("parent-2".to_string()),
                    role: chat::MessageRole::User,
                    content: "after fork point".to_string(),
                    timestamp: 30,
                    ..Default::default()
                },
            ],
            total_message_count: 3,
            loaded_message_start: 0,
            loaded_message_end: 3,
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("parent-thread".to_string()));

    model.fork_message(1);

    let active = model
        .chat
        .active_thread()
        .expect("forked thread should be selected");
    assert!(active.id.starts_with("fork-parent-thread-"));
    assert_eq!(active.title, "Fork: answer");
    assert_eq!(active.messages.len(), 2);
    assert_eq!(active.messages[0].content, "first");
    assert_eq!(active.messages[1].content, "answer");
    assert_eq!(active.profile_context_window_tokens, Some(400_000));

    let command = cmd_rx.try_recv().expect("fork should persist via daemon");
    let DaemonCommand::ForkThread {
        thread_id,
        thread_json,
        messages_json,
        refresh_message_limit,
    } = command
    else {
        panic!("expected ForkThread command");
    };
    assert_eq!(thread_id, active.id);
    assert_eq!(messages_json.len(), 2);
    assert!(refresh_message_limit >= 2);

    let db_thread: AgentDbThread =
        serde_json::from_str(&thread_json).expect("thread payload should parse");
    assert_eq!(db_thread.id, active.id);
    assert_eq!(db_thread.message_count, 2);
    assert_eq!(db_thread.agent_name.as_deref(), Some("Swarog"));
    let metadata: serde_json::Value = serde_json::from_str(
        db_thread
            .metadata_json
            .as_deref()
            .expect("fork metadata should exist"),
    )
    .expect("fork metadata should parse");
    assert_eq!(metadata["upstream_thread_id"], "parent-thread");
    assert_eq!(metadata["forked_message_index"], 1);
    assert_eq!(metadata["forked_message_id"], "parent-1");

    let db_message: AgentDbMessage =
        serde_json::from_str(&messages_json[1]).expect("message payload should parse");
    assert_eq!(db_message.thread_id, active.id);
    assert_eq!(db_message.role, "assistant");
    assert_eq!(db_message.reasoning.as_deref(), Some("because"));
    assert_ne!(db_message.id, "parent-1");
}

#[test]
fn fork_message_persists_runtime_profile_and_responder_identity() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "subagent-thread".to_string(),
            agent_name: Some("Design Reviewer".to_string()),
            title: "Subagent".to_string(),
            messages: vec![chat::AgentMessage {
                id: Some("assistant-1".to_string()),
                role: chat::MessageRole::Assistant,
                content: "design answer".to_string(),
                author_agent_id: Some("design-reviewer".to_string()),
                author_agent_name: Some("Design Reviewer".to_string()),
                timestamp: 10,
                ..Default::default()
            }],
            total_message_count: 1,
            loaded_message_start: 0,
            loaded_message_end: 1,
            runtime_provider: Some("groq".to_string()),
            runtime_model: Some("llama-3.3-70b-versatile".to_string()),
            runtime_reasoning_effort: Some("high".to_string()),
            ..Default::default()
        }));
    model.chat.reduce(chat::ChatAction::SelectThread(
        "subagent-thread".to_string(),
    ));

    model.fork_message(0);

    let command = cmd_rx.try_recv().expect("fork should persist via daemon");
    let DaemonCommand::ForkThread { thread_json, .. } = command else {
        panic!("expected ForkThread command");
    };
    let db_thread: AgentDbThread =
        serde_json::from_str(&thread_json).expect("thread payload should parse");
    let metadata: serde_json::Value = serde_json::from_str(
        db_thread
            .metadata_json
            .as_deref()
            .expect("fork metadata should exist"),
    )
    .expect("fork metadata should parse");

    assert_eq!(metadata["execution_profile"]["provider"], "groq");
    assert_eq!(
        metadata["execution_profile"]["model"],
        "llama-3.3-70b-versatile"
    );
    assert_eq!(metadata["execution_profile"]["reasoning_effort"], "high");
    assert_eq!(metadata["active_agent_id"], "design-reviewer");
    assert_eq!(
        metadata["handoff_stack"][0]["agent_name"],
        "Design Reviewer"
    );
}

#[test]
fn fork_message_from_partial_history_opens_thread_immediately() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "parent-thread".to_string(),
            title: "Parent".to_string(),
            messages: vec![
                chat::AgentMessage {
                    id: Some("parent-2".to_string()),
                    role: chat::MessageRole::User,
                    content: "loaded second user".to_string(),
                    timestamp: 30,
                    ..Default::default()
                },
                chat::AgentMessage {
                    id: Some("parent-3".to_string()),
                    role: chat::MessageRole::Assistant,
                    content: "fork point".to_string(),
                    timestamp: 40,
                    ..Default::default()
                },
            ],
            total_message_count: 4,
            loaded_message_start: 2,
            loaded_message_end: 4,
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("parent-thread".to_string()));

    model.fork_message(1);

    let active = model
        .chat
        .active_thread()
        .expect("forked thread should be selected immediately");
    assert!(active.id.starts_with("fork-parent-thread-"));
    assert_eq!(
        active
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["loaded second user", "fork point"]
    );

    let command = cmd_rx.try_recv().expect("immediate fork should persist");
    let DaemonCommand::ForkThread { messages_json, .. } = command else {
        panic!("expected ForkThread command");
    };
    assert_eq!(messages_json.len(), 2);
}

#[test]
fn clicking_reasoning_action_with_small_pointer_shift_still_toggles_reasoning() {
    let mut model = build_model();
    model.show_sidebar_override = Some(false);
    model.focus = FocusArea::Chat;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "Answer".to_string(),
            reasoning: Some("Think".to_string()),
            ..Default::default()
        },
    });
    model.chat.select_message(Some(0));

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let expand_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::chat::hit_test(
                    chat_area,
                    &model.chat,
                    &model.theme,
                    model.tick_counter,
                    pos,
                ) == Some(chat::ChatHitTarget::ReasoningToggle(0))
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("selected reasoning message should expose a clickable expand action");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: expand_pos.x,
        row: expand_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: expand_pos.x.saturating_add(1),
        row: expand_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert!(
        model.chat.expanded_reasoning().contains(&0),
        "reasoning should toggle even when the pointer shifts within the action button"
    );
}

#[test]
fn clicking_tool_action_with_small_pointer_shift_still_toggles_tool_details() {
    let mut model = build_model();
    model.show_sidebar_override = Some(false);
    model.focus = FocusArea::Chat;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Tool,
            content: "result".to_string(),
            tool_name: Some("read_file".to_string()),
            tool_status: Some("done".to_string()),
            ..Default::default()
        },
    });
    model.chat.select_message(Some(0));

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let expand_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::chat::hit_test(
                    chat_area,
                    &model.chat,
                    &model.theme,
                    model.tick_counter,
                    pos,
                ) == Some(chat::ChatHitTarget::ToolToggle(0))
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("selected tool message should expose a clickable expand action");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: expand_pos.x,
        row: expand_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: expand_pos.x.saturating_add(1),
        row: expand_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert!(
        model.chat.expanded_tools().contains(&0),
        "tool details should toggle even when the pointer shifts within the action button"
    );
}

#[test]
fn pressing_enter_executes_selected_inline_message_action() {
    let mut model = build_model();
    model.show_sidebar_override = Some(false);
    model.focus = FocusArea::Chat;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "second".to_string(),
            ..Default::default()
        },
    });
    model.chat.select_message(Some(0));
    model.chat.select_message_action(0);
    super::conversion::reset_last_copied_text();

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(
        super::conversion::last_copied_text().as_deref(),
        Some("second")
    );
}

#[test]
fn operator_question_event_uses_keyboard_submission_from_inline_actions() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.show_sidebar_override = Some(false);
    model.focus = FocusArea::Chat;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.handle_client_event(ClientEvent::OperatorQuestion {
        question_id: "oq-1".to_string(),
        content: "Approve this slice?\nA - proceed\nB - revise".to_string(),
        options: vec!["A".to_string(), "B".to_string()],
        session_id: None,
        thread_id: Some("thread-1".to_string()),
    });
    assert_eq!(model.modal.top(), None);
    let thread = model.chat.active_thread().expect("thread should exist");
    let message = thread
        .messages
        .last()
        .expect("question message should exist");
    assert!(message.is_operator_question);
    assert_eq!(message.operator_question_id.as_deref(), Some("oq-1"));
    assert_eq!(message.actions[0].label, "A");
    assert_eq!(message.actions[1].label, "B");
    model.chat.select_message(Some(0));

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!handled);
    let sent = cmd_rx
        .try_recv()
        .expect("pressing Enter should answer the operator question");
    match sent {
        DaemonCommand::AnswerOperatorQuestion {
            question_id,
            answer,
        } => {
            assert_eq!(question_id, "oq-1");
            assert_eq!(answer, "A");
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn click_without_drag_uses_press_location_for_message_selection() {
    let mut model = build_model();
    model.show_sidebar_override = Some(false);
    model.focus = FocusArea::Chat;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::User,
            content: "first".to_string(),
            ..Default::default()
        },
    });
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "second".to_string(),
            ..Default::default()
        },
    });

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let message1_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::chat::hit_test(
                    chat_area,
                    &model.chat,
                    &model.theme,
                    model.tick_counter,
                    pos,
                ) == Some(chat::ChatHitTarget::Message(1))
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("second message should expose a clickable body position");
    let message0_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::chat::hit_test(
                    chat_area,
                    &model.chat,
                    &model.theme,
                    model.tick_counter,
                    pos,
                ) == Some(chat::ChatHitTarget::Message(0))
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("first message should expose a clickable body position");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: message1_pos.x,
        row: message1_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: message0_pos.x,
        row: message0_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.chat.selected_message(), Some(1));
}
