use std::fs;
use std::path::{Path, PathBuf};

fn make_temp_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tamux-tui-tab-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("temporary directory should be creatable");
    dir
}

struct CurrentDirGuard(PathBuf);

impl CurrentDirGuard {
    fn enter(dir: &Path) -> Self {
        let previous = std::env::current_dir().expect("current dir should be readable");
        std::env::set_current_dir(dir).expect("temporary dir should be settable");
        Self(previous)
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

#[test]
fn sidebar_arrow_keys_follow_todos_first_tab_order() {
    let mut model = build_model();
    model.focus = FocusArea::Sidebar;

    assert_eq!(model.sidebar.active_tab(), SidebarTab::Todos);

    let handled = model.handle_key(KeyCode::Left, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.sidebar.active_tab(), SidebarTab::Todos);

    let handled = model.handle_key(KeyCode::Right, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.sidebar.active_tab(), SidebarTab::Files);
}

#[test]
fn typing_in_files_sidebar_filters_entries_and_escape_clears_query() {
    let mut model = build_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.tasks.reduce(task::TaskAction::WorkContextReceived(
        task::ThreadWorkContext {
            thread_id: "thread-1".to_string(),
            entries: vec![
                task::WorkContextEntry {
                    path: "/tmp/readme.md".to_string(),
                    is_text: true,
                    ..Default::default()
                },
                task::WorkContextEntry {
                    path: "/tmp/runtime.rs".to_string(),
                    is_text: true,
                    ..Default::default()
                },
                task::WorkContextEntry {
                    path: "/tmp/schema.sql".to_string(),
                    is_text: true,
                    ..Default::default()
                },
            ],
        },
    ));
    model
        .sidebar
        .reduce(SidebarAction::SwitchTab(SidebarTab::Files));
    model.focus = FocusArea::Sidebar;

    for ch in "runtime".chars() {
        let handled = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!handled);
    }

    assert_eq!(model.focus, FocusArea::Sidebar);
    assert_eq!(model.sidebar_item_count(), 1);
    assert_eq!(model.sidebar.files_filter(), "runtime");

    let handled = model.handle_key(KeyCode::Esc, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Sidebar);
    assert_eq!(model.sidebar.files_filter(), "");
    assert_eq!(model.sidebar_item_count(), 3);

    for ch in "runtime".chars() {
        let handled = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!handled);
    }

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(!handled);
    assert!(matches!(model.main_pane_view, MainPaneView::WorkContext));
    assert_eq!(
        model.tasks.selected_work_path("thread-1"),
        Some("/tmp/runtime.rs")
    );
}

#[test]
fn mouse_wheel_over_sidebar_moves_file_selection() {
    let mut model = build_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.tasks.reduce(task::TaskAction::WorkContextReceived(
        task::ThreadWorkContext {
            thread_id: "thread-1".to_string(),
            entries: (0..12)
                .map(|idx| task::WorkContextEntry {
                    path: format!("/tmp/file-{idx}.rs"),
                    is_text: true,
                    ..Default::default()
                })
                .collect(),
        },
    ));
    model
        .sidebar
        .reduce(SidebarAction::SwitchTab(SidebarTab::Files));
    model.focus = FocusArea::Sidebar;

    let sidebar_area = model
        .pane_layout()
        .sidebar
        .expect("default layout should include a sidebar");
    let mouse = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: sidebar_area.x.saturating_add(2),
        row: sidebar_area.y.saturating_add(3),
        modifiers: KeyModifiers::NONE,
    };

    model.handle_mouse(mouse);

    assert_eq!(model.focus, FocusArea::Sidebar);
    assert_eq!(model.sidebar.selected_item(), 3);

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(
        model.tasks.selected_work_path("thread-1"),
        Some("/tmp/file-3.rs")
    );
}

#[test]
fn deleting_message_keeps_sidebar_visible_when_thread_still_has_pins() {
    let mut model = build_model();
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Pinned".to_string(),
            messages: vec![
                chat::AgentMessage {
                    id: Some("message-1".to_string()),
                    role: chat::MessageRole::Assistant,
                    content: "Pinned one".to_string(),
                    pinned_for_compaction: true,
                    ..Default::default()
                },
                chat::AgentMessage {
                    id: Some("message-2".to_string()),
                    role: chat::MessageRole::Assistant,
                    content: "Pinned two".to_string(),
                    pinned_for_compaction: true,
                    ..Default::default()
                },
            ],
            loaded_message_end: 2,
            total_message_count: 2,
            ..Default::default()
        },
    ));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model
        .sidebar
        .reduce(SidebarAction::SwitchTab(SidebarTab::Pinned));

    assert!(
        model.pane_layout().sidebar.is_some(),
        "pinned threads should keep the sidebar visible"
    );

    model.delete_message(0);

    assert!(
        model.pane_layout().sidebar.is_some(),
        "remaining pins should keep the sidebar visible after deletion"
    );
    assert_eq!(model.sidebar.active_tab(), SidebarTab::Pinned);
}

#[test]
fn pinned_summary_only_thread_keeps_sidebar_visible() {
    let mut model = build_model();
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Pinned".to_string(),
            messages: vec![chat::AgentMessage {
                id: Some("message-2".to_string()),
                role: chat::MessageRole::Assistant,
                content: "Latest visible".to_string(),
                ..Default::default()
            }],
            pinned_messages: vec![chat::PinnedThreadMessage {
                message_id: "message-1".to_string(),
                absolute_index: 0,
                role: chat::MessageRole::User,
                content: "Pinned offscreen".to_string(),
            }],
            loaded_message_start: 1,
            loaded_message_end: 2,
            total_message_count: 2,
            ..Default::default()
        },
    ));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    assert!(
        model.pane_layout().sidebar.is_some(),
        "summary pins should keep the sidebar visible even when the loaded page has no pinned rows"
    );
}

#[test]
fn ctrl_k_then_j_jumps_to_selected_pinned_message_from_input_focus() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, _cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Pinned".to_string(),
            messages: vec![
                chat::AgentMessage {
                    id: Some("message-1".to_string()),
                    role: chat::MessageRole::User,
                    content: "Pinned user message".to_string(),
                    pinned_for_compaction: true,
                    ..Default::default()
                },
                chat::AgentMessage {
                    id: Some("message-2".to_string()),
                    role: chat::MessageRole::Assistant,
                    content: "Later reply".to_string(),
                    ..Default::default()
                },
            ],
            loaded_message_end: 2,
            total_message_count: 2,
            ..Default::default()
        },
    ));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model
        .sidebar
        .reduce(SidebarAction::SwitchTab(SidebarTab::Pinned));
    model.focus = FocusArea::Input;
    model.input.set_text("draft");

    let handled = model.handle_key(KeyCode::Char('k'), KeyModifiers::CONTROL);
    assert!(!handled);

    let handled = model.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Chat);
    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert_eq!(model.chat.selected_message(), Some(0));
    assert_eq!(model.input.buffer(), "draft");
}

#[test]
fn ctrl_k_then_u_unpins_selected_pinned_message_from_input_focus() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Pinned".to_string(),
            messages: vec![chat::AgentMessage {
                id: Some("message-1".to_string()),
                role: chat::MessageRole::Assistant,
                content: "Pinned content".to_string(),
                pinned_for_compaction: true,
                ..Default::default()
            }],
            loaded_message_end: 1,
            total_message_count: 1,
            ..Default::default()
        },
    ));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model
        .sidebar
        .reduce(SidebarAction::SwitchTab(SidebarTab::Pinned));
    model.focus = FocusArea::Input;
    model.input.set_text("draft");

    let handled = model.handle_key(KeyCode::Char('k'), KeyModifiers::CONTROL);
    assert!(!handled);

    let handled = model.handle_key(KeyCode::Char('u'), KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.input.buffer(), "draft");
    let command = cmd_rx
        .try_recv()
        .expect("Ctrl+K then U should unpin the selected message");
    assert!(matches!(
        command,
        DaemonCommand::UnpinThreadMessageForCompaction {
            thread_id,
            message_id
        } if thread_id == "thread-1" && message_id == "message-1"
    ));
}

#[test]
fn submit_operator_profile_answer_allows_empty_input_when_question_is_optional() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "nickname".to_string(),
        field_key: "nickname".to_string(),
        prompt: "Nickname?".to_string(),
        input_kind: "text".to_string(),
        optional: true,
    });

    assert!(model.submit_operator_profile_answer());
    assert!(
        model.operator_profile.loading,
        "optional empty answer should begin submission"
    );
    assert!(
        model.operator_profile.question.is_none(),
        "question should clear when submission starts"
    );

    let sent = cmd_rx
        .try_recv()
        .expect("submitting optional empty answer should emit daemon command");
    match sent {
        DaemonCommand::SubmitOperatorProfileAnswer {
            session_id,
            question_id,
            answer_json,
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(question_id, "nickname");
            assert_eq!(answer_json, "null");
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn submit_operator_profile_answer_blocks_empty_input_when_question_is_required() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "name".to_string(),
        field_key: "name".to_string(),
        prompt: "What should I call you?".to_string(),
        input_kind: "text".to_string(),
        optional: false,
    });

    assert!(model.submit_operator_profile_answer());
    assert!(
        !model.operator_profile.loading,
        "required empty answer should not start submission"
    );
    assert!(
        model.operator_profile.question.is_some(),
        "question should remain while awaiting required answer"
    );
    assert!(
        cmd_rx.try_recv().is_err(),
        "required empty answer should not emit daemon command"
    );
}

#[test]
fn skip_operator_profile_question_clears_stale_question_immediately() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "name".to_string(),
        field_key: "name".to_string(),
        prompt: "What should I call you?".to_string(),
        input_kind: "text".to_string(),
        optional: false,
    });

    assert!(model.skip_operator_profile_question());
    assert!(model.operator_profile.loading);
    assert!(
        model.operator_profile.question.is_none(),
        "question should clear when skip starts"
    );

    let sent = cmd_rx.try_recv().expect("skip should emit daemon command");
    match sent {
        DaemonCommand::SkipOperatorProfileQuestion {
            session_id,
            question_id,
            reason,
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(question_id, "name");
            assert_eq!(reason.as_deref(), Some("tui_skip_shortcut"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn defer_operator_profile_question_clears_stale_question_immediately() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "name".to_string(),
        field_key: "name".to_string(),
        prompt: "What should I call you?".to_string(),
        input_kind: "text".to_string(),
        optional: false,
    });

    assert!(model.defer_operator_profile_question());
    assert!(model.operator_profile.loading);
    assert!(
        model.operator_profile.question.is_none(),
        "question should clear when defer starts"
    );

    let sent = cmd_rx.try_recv().expect("defer should emit daemon command");
    match sent {
        DaemonCommand::DeferOperatorProfileQuestion {
            session_id,
            question_id,
            defer_until_unix_ms,
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(question_id, "name");
            assert!(defer_until_unix_ms.is_some());
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn clicking_bottom_action_bar_submits_operator_question_answer() {
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
    model.chat.select_message(Some(0));

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let concierge_height = if model.chat.active_actions().is_empty() {
        0
    } else {
        1
    };
    let concierge_area = Rect::new(
        0,
        input_start_row.saturating_sub(concierge_height),
        model.width,
        concierge_height,
    );
    let action_pos = (concierge_area.y..concierge_area.y.saturating_add(concierge_area.height))
        .find_map(|row| {
            (concierge_area.x..concierge_area.x.saturating_add(concierge_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::concierge::hit_test(
                    concierge_area,
                    model.chat.active_actions(),
                    model.concierge.selected_action,
                    pos,
                ) == Some(widgets::concierge::ConciergeHitTarget::Action(0)) {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("operator question should expose a clickable concierge action bar");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: action_pos.x,
        row: action_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: action_pos.x,
        row: action_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    let sent = cmd_rx
        .try_recv()
        .expect("clicking the action bar should answer the operator question");
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
fn tab_completes_active_file_reference_instead_of_changing_focus() {
    let mut model = build_model();
    let cwd = make_temp_dir();
    let docs_dir = cwd.join("docs");
    fs::create_dir_all(&docs_dir).expect("docs directory should be creatable");
    fs::write(docs_dir.join("notes.txt"), "hello").expect("file should be writable");
    let reference = format!("@{}/do", cwd.display());
    model.input.set_text(&reference);

    let handled = model.handle_key(KeyCode::Tab, KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Input);
    assert_eq!(model.input.buffer(), format!("@{}/docs/", cwd.display()));
}

#[test]
fn tab_focus_cycles_when_not_inside_file_reference() {
    let mut model = build_model();
    model.focus = FocusArea::Input;
    model.input.set_text("hello world");

    let handled = model.handle_key(KeyCode::Tab, KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Chat);
    assert_eq!(model.input.buffer(), "hello world");
}

#[test]
fn tab_inside_unmatched_file_reference_keeps_input_focus() {
    let mut model = build_model();
    let cwd = make_temp_dir();
    let _guard = CurrentDirGuard::enter(&cwd);
    model.focus = FocusArea::Input;
    model.input.set_text("@missing");

    let handled = model.handle_key(KeyCode::Tab, KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Input);
    assert_eq!(model.input.buffer(), "@missing");
    assert!(
        model.status_line.contains("No matches"),
        "unmatched completion should surface a notice"
    );
}

#[test]
fn leading_agent_directive_supports_internal_delegate() {
    let known = vec!["weles".to_string()];
    let directive = crate::state::input_refs::parse_leading_agent_directive("!weles check X", &known)
        .expect("directive should parse");

    assert_eq!(
        directive.kind,
        crate::state::input_refs::LeadingAgentDirectiveKind::InternalDelegate
    );
}

#[test]
fn leading_agent_directive_supports_deactivate_phrases() {
    let known = vec!["weles".to_string()];

    for phrase in ["stop", "leave", "done", "return"] {
        let directive = crate::state::input_refs::parse_leading_agent_directive(
            &format!("@weles {phrase}"),
            &known,
        )
        .expect("directive should parse");

        assert_eq!(
            directive.kind,
            crate::state::input_refs::LeadingAgentDirectiveKind::ParticipantDeactivate
        );
    }
}

#[test]
fn leading_agent_directive_is_case_insensitive() {
    let known = vec!["weles".to_string()];
    let directive = crate::state::input_refs::parse_leading_agent_directive("!WeLeS check X", &known)
        .expect("directive should parse");

    assert_eq!(
        directive.kind,
        crate::state::input_refs::LeadingAgentDirectiveKind::InternalDelegate
    );
}

#[test]
fn leading_agent_directive_unknown_alias_falls_back() {
    let known = vec!["weles".to_string()];
    let directive =
        crate::state::input_refs::parse_leading_agent_directive("@unknown inspect @foo", &known);

    assert!(directive.is_none());
}

#[test]
fn leading_agent_directive_preserves_file_refs() {
    let known = vec!["weles".to_string()];
    let directive = crate::state::input_refs::parse_leading_agent_directive(
        "@weles inspect @foo/bar",
        &known,
    )
    .expect("directive should parse");

    assert_eq!(directive.body, "inspect @foo/bar");
}

fn sample_collaboration_sessions() -> Vec<crate::state::CollaborationSessionVm> {
    vec![crate::state::CollaborationSessionVm {
        id: "session-1".to_string(),
        parent_task_id: Some("task-1".to_string()),
        parent_thread_id: None,
        agent_count: 2,
        disagreement_count: 1,
        consensus_summary: None,
        escalation: None,
        disagreements: vec![crate::state::CollaborationDisagreementVm {
            id: "disagreement-1".to_string(),
            topic: "deployment strategy".to_string(),
            positions: vec!["roll forward".to_string(), "roll back".to_string()],
            vote_count: 0,
            resolution: None,
        }],
    }]
}

#[test]
fn collaboration_tab_cycles_between_navigator_detail_and_input() {
    let mut model = build_model();
    model.main_pane_view = MainPaneView::Collaboration;
    model.focus = FocusArea::Chat;
    model
        .collaboration
        .reduce(crate::state::CollaborationAction::SessionsLoaded(
            sample_collaboration_sessions(),
        ));

    let handled = model.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Chat);
    assert_eq!(
        model.collaboration.focus(),
        crate::state::CollaborationPaneFocus::Detail
    );

    let handled = model.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Input);

    let handled = model.handle_key(KeyCode::BackTab, KeyModifiers::SHIFT);
    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Chat);
    assert_eq!(
        model.collaboration.focus(),
        crate::state::CollaborationPaneFocus::Detail
    );
}

#[test]
fn collaboration_arrow_keys_navigate_rows_and_detail_actions() {
    let mut model = build_model();
    model.main_pane_view = MainPaneView::Collaboration;
    model.focus = FocusArea::Chat;
    model
        .collaboration
        .reduce(crate::state::CollaborationAction::SessionsLoaded(
            sample_collaboration_sessions(),
        ));

    let handled = model.handle_key(KeyCode::Down, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.collaboration.selected_row_index(), 1);

    let handled = model.handle_key(KeyCode::Right, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(
        model.collaboration.focus(),
        crate::state::CollaborationPaneFocus::Detail
    );

    let handled = model.handle_key(KeyCode::Right, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.collaboration.selected_detail_action_index(), 1);

    let handled = model.handle_key(KeyCode::Left, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.collaboration.selected_detail_action_index(), 0);

    let handled = model.handle_key(KeyCode::Left, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(
        model.collaboration.focus(),
        crate::state::CollaborationPaneFocus::Navigator
    );
}

#[test]
fn collaboration_enter_in_detail_sends_vote_command() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.main_pane_view = MainPaneView::Collaboration;
    model.focus = FocusArea::Chat;
    model
        .collaboration
        .reduce(crate::state::CollaborationAction::SessionsLoaded(
            sample_collaboration_sessions(),
        ));
    model
        .collaboration
        .reduce(crate::state::CollaborationAction::SelectRow(1));
    model.collaboration.reduce(crate::state::CollaborationAction::SetFocus(
        crate::state::CollaborationPaneFocus::Detail,
    ));

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(!handled);

    match cmd_rx
        .try_recv()
        .expect("expected collaboration vote command from detail enter")
    {
        DaemonCommand::VoteOnCollaborationDisagreement {
            parent_task_id,
            disagreement_id,
            task_id,
            position,
            confidence,
        } => {
            assert_eq!(parent_task_id, "task-1");
            assert_eq!(disagreement_id, "disagreement-1");
            assert_eq!(task_id, "operator");
            assert_eq!(position, "roll forward");
            assert_eq!(confidence, Some(1.0));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn collaboration_mouse_clicks_select_rows_and_vote_actions() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.main_pane_view = MainPaneView::Collaboration;
    model.focus = FocusArea::Chat;
    model
        .collaboration
        .reduce(crate::state::CollaborationAction::SessionsLoaded(
            sample_collaboration_sessions(),
        ));

    let chat_area = rendered_chat_area(&model);
    let left_x = chat_area.x + 3;
    let top_y = chat_area.y + 2;

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: left_x,
        row: top_y,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(model.collaboration.selected_row_index(), 0);

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: left_x,
        row: top_y + 1,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(model.collaboration.selected_row_index(), 1);

    let right_x = chat_area.x + (chat_area.width / 2);
    let action_y = chat_area.y + 6;
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: right_x,
        row: action_y,
        modifiers: KeyModifiers::NONE,
    });

    match cmd_rx
        .try_recv()
        .expect("expected collaboration vote command from mouse action")
    {
        DaemonCommand::VoteOnCollaborationDisagreement { position, .. } => {
            assert_eq!(position, "roll forward");
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn done_event_stores_provider_final_result_on_final_message() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, _cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::Delta {
        thread_id: "thread-1".to_string(),
        content: "Answer".to_string(),
    });

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-1".to_string(),
        input_tokens: 10,
        output_tokens: 20,
        cost: None,
        provider: Some(amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT.to_string()),
        model: Some("gpt-5.4".to_string()),
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: Some(
            r#"{"provider":"open_ai_responses","id":"resp_tui_done"}"#.to_string(),
        ),
    });

    let thread = model.chat.active_thread().expect("thread should exist");
    let last = thread
        .messages
        .last()
        .expect("assistant message should exist");
    let json = last
        .provider_final_result_json
        .as_deref()
        .expect("provider final result should be stored");
    let value: serde_json::Value = serde_json::from_str(json).expect("parse provider final result json");
    assert_eq!(value.get("provider").and_then(|v| v.as_str()), Some("open_ai_responses"));
    assert_eq!(value.get("id").and_then(|v| v.as_str()), Some("resp_tui_done"));
}

#[test]
fn submit_prompt_appends_referenced_files_footer() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    let cwd = make_temp_dir();
    let source_dir = cwd.join("src");
    fs::create_dir_all(&source_dir).expect("source directory should be creatable");
    let main_rs = source_dir.join("main.rs");
    fs::write(&main_rs, "fn main() {}\n").expect("fixture file should be writable");

    model.submit_prompt(format!(
        "Please inspect @{} before editing",
        main_rs.display()
    ));

    let expected = format!(
        "Please inspect @{} before editing\n\nReferenced files: {}\nInspect these with read_file before making assumptions.",
        main_rs.display(),
        main_rs.display()
    );

    match cmd_rx.try_recv() {
        Ok(DaemonCommand::SendMessage { content, .. }) => {
            assert_eq!(content, expected);
        }
        other => panic!("expected send-message command, got {:?}", other),
    }

    assert_eq!(
        model
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.last())
            .map(|message| message.content.as_str()),
        Some(expected.as_str())
    );
}

#[test]
fn submit_prompt_does_not_inline_referenced_file_contents() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;

    let cwd = make_temp_dir();
    let notes = cwd.join("notes.txt");
    fs::write(&notes, "top secret contents\n").expect("fixture file should be writable");

    model.submit_prompt(format!("Review @{}", notes.display()));

    let sent_content = match cmd_rx.try_recv() {
        Ok(DaemonCommand::SendMessage { content, .. }) => content,
        other => panic!("expected send-message command, got {:?}", other),
    };

    assert!(
        sent_content.contains("Referenced files:"),
        "resolved references should append footer metadata"
    );
    assert!(
        sent_content.contains(&notes.display().to_string()),
        "footer should include the normalized absolute path"
    );
    assert!(
        !sent_content.contains("top secret contents"),
        "submit_prompt should not inline referenced file contents"
    );
    assert!(
        !sent_content.contains("<attached_file"),
        "file references should not create synthetic attachments"
    );
}

#[test]
fn submit_prompt_deduplicates_referenced_files() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;

    let cwd = make_temp_dir();
    let lib_rs = cwd.join("lib.rs");
    fs::write(&lib_rs, "pub fn demo() {}\n").expect("fixture file should be writable");

    model.submit_prompt(format!(
        "Check @{0} and again @{0}",
        lib_rs.display()
    ));

    let sent_content = match cmd_rx.try_recv() {
        Ok(DaemonCommand::SendMessage { content, .. }) => content,
        other => panic!("expected send-message command, got {:?}", other),
    };

    let footer = format!("Referenced files: {}", lib_rs.display());
    assert_eq!(sent_content.matches(&footer).count(), 1);
}

#[test]
fn submit_prompt_ignores_nonexistent_referenced_files() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;

    let missing = make_temp_dir().join("missing.txt");
    let prompt = format!("Investigate @{} later", missing.display());

    model.submit_prompt(prompt.clone());

    let sent_content = match cmd_rx.try_recv() {
        Ok(DaemonCommand::SendMessage { content, .. }) => content,
        other => panic!("expected send-message command, got {:?}", other),
    };

    assert_eq!(sent_content, prompt);
    assert!(
        !sent_content.contains("Referenced files:"),
        "nonexistent references should remain plain text without footer metadata"
    );
}
