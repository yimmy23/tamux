    #[test]
    fn clicking_confirm_in_chat_action_confirm_deletes_message() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.width = 100;
        model.height = 40;
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
                id: Some("m1".to_string()),
                role: chat::MessageRole::Assistant,
                content: "Answer".to_string(),
                ..Default::default()
            },
        });

        model.request_delete_message(0);
        let (_, overlay_area) = model
            .current_modal_area()
            .expect("chat action confirm modal should be visible");
        let (confirm_rect, _) = render_helpers::chat_action_confirm_button_bounds(overlay_area)
            .expect("confirm modal should expose button bounds");
        let click_col = confirm_rect.x.saturating_add(1);
        let click_row = confirm_rect.y;

        model.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: click_col,
            row: click_row,
            modifiers: KeyModifiers::NONE,
        });
        model.handle_mouse(MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: click_col,
            row: click_row,
            modifiers: KeyModifiers::NONE,
        });

        let sent = cmd_rx
            .try_recv()
            .expect("confirm click should send a delete command");
        assert!(matches!(sent, DaemonCommand::DeleteMessages { .. }));
        assert_eq!(
            model
                .chat
                .active_thread()
                .map(|thread| thread.messages.len()),
            Some(0),
            "confirm click should delete the message"
        );
    }

    #[test]
    fn resize_clears_drag_snapshots() {
        let mut model = build_model();
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
                content: "drag me".to_string(),
                ..Default::default()
            },
        });
        model.chat_drag_anchor = Some(Position::new(3, 6));
        model.chat_drag_current = Some(Position::new(8, 9));
        model.chat_drag_anchor_point = Some(widgets::chat::SelectionPoint { row: 1, col: 1 });
        model.chat_drag_current_point = Some(widgets::chat::SelectionPoint { row: 2, col: 4 });
        model.chat_selection_snapshot = widgets::chat::build_selection_snapshot(
            Rect::new(0, 3, 80, 12),
            &model.chat,
            &model.theme,
            model.tick_counter,
            model.retry_wait_start_selected,
        );
        model.work_context_drag_anchor = Some(Position::new(1, 1));
        model.work_context_drag_current = Some(Position::new(2, 2));
        model.work_context_drag_anchor_point =
            Some(widgets::chat::SelectionPoint { row: 0, col: 0 });
        model.work_context_drag_current_point =
            Some(widgets::chat::SelectionPoint { row: 0, col: 1 });

        model.handle_resize(100, 24);

        assert!(model.chat_drag_anchor.is_none());
        assert!(model.chat_drag_current.is_none());
        assert!(model.chat_drag_anchor_point.is_none());
        assert!(model.chat_drag_current_point.is_none());
        assert!(model.chat_selection_snapshot.is_none());
        assert!(model.work_context_drag_anchor.is_none());
        assert!(model.work_context_drag_current.is_none());
        assert!(model.work_context_drag_anchor_point.is_none());
        assert!(model.work_context_drag_current_point.is_none());
    }

    #[test]
    fn cleanup_concierge_on_navigate_hides_local_welcome_message() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "concierge".to_string(),
            title: "Concierge".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
        model.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id: "concierge".to_string(),
            message: chat::AgentMessage {
                role: chat::MessageRole::Assistant,
                content: "Welcome".to_string(),
                is_concierge_welcome: true,
                ..Default::default()
            },
        });
        model
            .concierge
            .reduce(crate::state::ConciergeAction::WelcomeReceived {
                content: "Welcome".to_string(),
                actions: vec![crate::state::ConciergeActionVm {
                    label: "Dismiss".to_string(),
                    action_type: "dismiss".to_string(),
                    thread_id: None,
                }],
            });

        model.cleanup_concierge_on_navigate();

        assert!(!model.concierge.welcome_visible);
        assert!(model.ignore_pending_concierge_welcome);
        assert!(
            model.chat.active_actions().is_empty(),
            "dismissed concierge welcome should not leave actionable buttons behind"
        );
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            other => panic!("expected dismiss command, got {:?}", other),
        }
    }

    #[test]
    fn submit_prompt_dismisses_concierge_and_avoids_session_binding() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.connected = true;
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "concierge".to_string(),
            title: "Concierge".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
        model.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id: "concierge".to_string(),
            message: chat::AgentMessage {
                role: chat::MessageRole::Assistant,
                content: "Welcome".to_string(),
                actions: vec![chat::MessageAction {
                    label: "Dismiss".to_string(),
                    action_type: "dismiss".to_string(),
                    thread_id: None,
                }],
                is_concierge_welcome: true,
                ..Default::default()
            },
        });
        model
            .concierge
            .reduce(crate::state::ConciergeAction::WelcomeReceived {
                content: "Welcome".to_string(),
                actions: vec![crate::state::ConciergeActionVm {
                    label: "Dismiss".to_string(),
                    action_type: "dismiss".to_string(),
                    thread_id: None,
                }],
            });
        model.default_session_id = Some("stale-session".to_string());

        model.submit_prompt("hello".to_string());

        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            other => panic!("expected dismiss command first, got {:?}", other),
        }
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::SendMessage {
                thread_id,
                content,
                session_id,
                ..
            }) => {
                assert_eq!(thread_id.as_deref(), Some("concierge"));
                assert_eq!(content, "hello");
                assert_eq!(session_id, None);
            }
            other => panic!("expected send-message command, got {:?}", other),
        }
        assert!(
            model.chat.active_actions().is_empty(),
            "submitting a prompt should hide concierge welcome actions"
        );
    }

    #[test]
    fn start_goal_run_dismisses_concierge_and_avoids_session_binding() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.connected = true;
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "concierge".to_string(),
            title: "Concierge".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
        model.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id: "concierge".to_string(),
            message: chat::AgentMessage {
                role: chat::MessageRole::Assistant,
                content: "Welcome".to_string(),
                actions: vec![chat::MessageAction {
                    label: "Goal".to_string(),
                    action_type: "start_goal_run".to_string(),
                    thread_id: None,
                }],
                is_concierge_welcome: true,
                ..Default::default()
            },
        });
        model
            .concierge
            .reduce(crate::state::ConciergeAction::WelcomeReceived {
                content: "Welcome".to_string(),
                actions: vec![crate::state::ConciergeActionVm {
                    label: "Goal".to_string(),
                    action_type: "start_goal_run".to_string(),
                    thread_id: None,
                }],
            });
        model.default_session_id = Some("stale-session".to_string());

        model.start_goal_run_from_prompt("ship it".to_string());

        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            other => panic!("expected dismiss command first, got {:?}", other),
        }
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::StartGoalRun {
                goal,
                thread_id,
                session_id,
            }) => {
                assert_eq!(goal, "ship it");
                assert_eq!(thread_id, None);
                assert_eq!(session_id, None);
            }
            other => panic!("expected start-goal-run command, got {:?}", other),
        }
    }

    #[test]
    fn start_new_thread_shows_local_landing_and_does_not_request_concierge() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.concierge.loading = false;
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "concierge".to_string(),
            title: "Concierge".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
        model
            .concierge
            .reduce(crate::state::ConciergeAction::WelcomeReceived {
                content: "Welcome".to_string(),
                actions: vec![crate::state::ConciergeActionVm {
                    label: "Search".to_string(),
                    action_type: "search".to_string(),
                    thread_id: None,
                }],
            });

        model.start_new_thread_view();

        assert!(model.should_show_local_landing());
        assert_eq!(model.chat.active_thread_id(), None);
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            other => panic!("expected dismiss command first, got {:?}", other),
        }
        assert!(
            cmd_rx.try_recv().is_err(),
            "unexpected daemon command after /new"
        );
    }

    #[test]
    fn start_new_thread_ignores_replayed_concierge_welcome_events() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "concierge".to_string(),
            title: "Concierge".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
        model
            .concierge
            .reduce(crate::state::ConciergeAction::WelcomeReceived {
                content: "Welcome".to_string(),
                actions: vec![crate::state::ConciergeActionVm {
                    label: "Start new session".to_string(),
                    action_type: "start_new".to_string(),
                    thread_id: None,
                }],
            });

        model.start_new_thread_view();

        model.handle_concierge_welcome_event(
            "Welcome".to_string(),
            vec![crate::state::ConciergeActionVm {
                label: "Start new session".to_string(),
                action_type: "start_new".to_string(),
                thread_id: None,
            }],
        );
        model.handle_concierge_welcome_event(
            "Welcome again".to_string(),
            vec![crate::state::ConciergeActionVm {
                label: "Start new session".to_string(),
                action_type: "start_new".to_string(),
                thread_id: None,
            }],
        );

        assert!(model.should_show_local_landing());
        assert_eq!(model.chat.active_thread_id(), None);
        assert_eq!(model.focus, FocusArea::Input);
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            other => panic!("expected dismiss command first, got {:?}", other),
        }
        assert!(
            cmd_rx.try_recv().is_err(),
            "replayed concierge welcome should not reopen the concierge thread"
        );
    }

    #[test]
    fn concierge_arrow_keys_navigate_visible_actions() {
        let mut model = build_model();
        model.focus = FocusArea::Chat;
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "concierge".to_string(),
            title: "Concierge".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
        model.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id: "concierge".to_string(),
            message: chat::AgentMessage {
                role: chat::MessageRole::Assistant,
                content: "Welcome".to_string(),
                actions: vec![
                    chat::MessageAction {
                        label: "One".to_string(),
                        action_type: "dismiss".to_string(),
                        thread_id: None,
                    },
                    chat::MessageAction {
                        label: "Two".to_string(),
                        action_type: "dismiss".to_string(),
                        thread_id: None,
                    },
                ],
                is_concierge_welcome: true,
                ..Default::default()
            },
        });

        let handled = model.handle_key(KeyCode::Right, KeyModifiers::NONE);

        assert!(!handled);
        assert_eq!(model.concierge.selected_action, 1);
    }

    #[test]
    fn selected_message_arrow_keys_navigate_inline_actions() {
        let mut model = build_model();
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
        model.chat.select_message(Some(0));
        model.chat.select_message_action(0);

        let handled = model.handle_key(KeyCode::Right, KeyModifiers::NONE);

        assert!(!handled);
        assert_eq!(model.chat.selected_message_action(), 1);
    }

    #[test]
    fn retry_wait_keyboard_can_select_yes_and_trigger_immediate_retry() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.focus = FocusArea::Chat;
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "thread-1".to_string(),
            title: "Thread".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
        model.handle_retry_status_event(
            "thread-1".to_string(),
            "waiting".to_string(),
            1,
            1,
            30_000,
            "transport".to_string(),
            "upstream transport error".to_string(),
        );

        let handled = model.handle_key(KeyCode::Left, KeyModifiers::NONE);
        assert!(!handled);

        let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(!handled);
        assert!(
            model.chat.retry_status().is_none(),
            "retry prompt should clear locally once retry-now is triggered"
        );

        match cmd_rx.try_recv() {
            Ok(DaemonCommand::RetryStreamNow { thread_id }) => assert_eq!(thread_id, "thread-1"),
            other => panic!("expected retry-now command, got {:?}", other),
        }
    }

    #[test]
    fn retry_wait_keyboard_can_trigger_from_input_focus_when_input_is_empty() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.focus = FocusArea::Input;
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "thread-1".to_string(),
            title: "Thread".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
        model.handle_retry_status_event(
            "thread-1".to_string(),
            "waiting".to_string(),
            1,
            0,
            30_000,
            "transport".to_string(),
            "upstream transport error".to_string(),
        );

        let handled = model.handle_key(KeyCode::Left, KeyModifiers::NONE);
        assert!(!handled);

        let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(!handled);

        match cmd_rx.try_recv() {
            Ok(DaemonCommand::RetryStreamNow { thread_id }) => assert_eq!(thread_id, "thread-1"),
            other => panic!("expected retry-now command, got {:?}", other),
        }
    }

    #[test]
    fn retry_wait_keyboard_can_trigger_from_input_focus_with_pending_text() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.focus = FocusArea::Input;
        model.input.reduce(input::InputAction::InsertChar('c'));
        model.input.reduce(input::InputAction::InsertChar('o'));
        model.input.reduce(input::InputAction::InsertChar('n'));
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "thread-1".to_string(),
            title: "Thread".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
        model.handle_retry_status_event(
            "thread-1".to_string(),
            "waiting".to_string(),
            1,
            0,
            30_000,
            "transport".to_string(),
            "upstream transport error".to_string(),
        );

        let handled = model.handle_key(KeyCode::Left, KeyModifiers::NONE);
        assert!(!handled);

        let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(!handled);

        match cmd_rx.try_recv() {
            Ok(DaemonCommand::RetryStreamNow { thread_id }) => assert_eq!(thread_id, "thread-1"),
            other => panic!("expected retry-now command, got {:?}", other),
        }
    }

    #[test]
    fn retry_wait_mouse_click_triggers_immediate_retry() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.width = 100;
        model.height = 40;
        model.focus = FocusArea::Input;
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "thread-1".to_string(),
            title: "Thread".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
        model.handle_retry_status_event(
            "thread-1".to_string(),
            "waiting".to_string(),
            1,
            0,
            30_000,
            "transport".to_string(),
            "upstream transport error".to_string(),
        );

        let input_start_row = model.height.saturating_sub(model.input_height() + 1);
        let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
        let retry_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
            .find_map(|row| {
                (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                    let pos = Position::new(column, row);
                    if widgets::chat::hit_test(
                        chat_area,
                        &model.chat,
                        &model.theme,
                        model.tick_counter,
                        pos,
                    ) == Some(chat::ChatHitTarget::RetryStartNow)
                    {
                        Some(pos)
                    } else {
                        None
                    }
                })
            })
            .expect("retry action should expose a clickable yes target");

        model.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: retry_pos.x,
            row: retry_pos.y,
            modifiers: KeyModifiers::NONE,
        });
        model.handle_mouse(MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: retry_pos.x,
            row: retry_pos.y,
            modifiers: KeyModifiers::NONE,
        });
        assert!(
            model.chat.retry_status().is_none(),
            "retry prompt should clear locally after mouse retry-now"
        );

        match cmd_rx.try_recv() {
            Ok(DaemonCommand::RetryStreamNow { thread_id }) => assert_eq!(thread_id, "thread-1"),
            other => panic!("expected retry-now command, got {:?}", other),
        }
    }

    #[test]
    fn retry_wait_mouse_down_triggers_immediate_retry() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.width = 100;
        model.height = 40;
        model.focus = FocusArea::Input;
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "thread-1".to_string(),
            title: "Thread".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
        model.handle_retry_status_event(
            "thread-1".to_string(),
            "waiting".to_string(),
            1,
            0,
            30_000,
            "transport".to_string(),
            "upstream transport error".to_string(),
        );

        let input_start_row = model.height.saturating_sub(model.input_height() + 1);
        let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
        let retry_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
            .find_map(|row| {
                (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                    let pos = Position::new(column, row);
                    if widgets::chat::hit_test(
                        chat_area,
                        &model.chat,
                        &model.theme,
                        model.tick_counter,
                        pos,
                    ) == Some(chat::ChatHitTarget::RetryStartNow)
                    {
                        Some(pos)
                    } else {
                        None
                    }
                })
            })
            .expect("retry action should expose a clickable yes target");

        model.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: retry_pos.x,
            row: retry_pos.y,
            modifiers: KeyModifiers::NONE,
        });
        assert!(
            model.chat.retry_status().is_none(),
            "retry prompt should clear locally after mouse-down retry-now"
        );

        match cmd_rx.try_recv() {
            Ok(DaemonCommand::RetryStreamNow { thread_id }) => assert_eq!(thread_id, "thread-1"),
            other => panic!("expected retry-now command on mouse down, got {:?}", other),
        }
    }
