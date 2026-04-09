    #[test]
    fn provider_onboarding_requires_loaded_auth_state() {
        let mut model = build_model();
        model.connected = true;
        model.auth.entries = vec![unauthenticated_entry()];

        assert!(!model.should_show_provider_onboarding());
    }

    #[test]
    fn provider_onboarding_shows_when_no_provider_is_configured() {
        let mut model = build_model();
        model.connected = true;
        model.auth.loaded = true;
        model.auth.entries = vec![unauthenticated_entry()];

        assert!(model.should_show_provider_onboarding());
    }

    #[test]
    fn provider_onboarding_hides_when_provider_is_configured() {
        let mut model = build_model();
        model.connected = true;
        model.auth.loaded = true;
        let mut entry = unauthenticated_entry();
        entry.authenticated = true;
        model.auth.entries = vec![entry];

        assert!(!model.should_show_provider_onboarding());
    }

    #[test]
    fn local_landing_shows_only_for_empty_conversation_state() {
        let mut model = build_model();

        assert!(model.should_show_local_landing());

        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "thread-1".to_string(),
            title: "Thread".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
        assert!(!model.should_show_local_landing());

        model
            .chat
            .reduce(chat::ChatAction::SelectThread(String::new()));
        model.chat.reduce(chat::ChatAction::Delta {
            thread_id: "stream".to_string(),
            content: "hello".to_string(),
        });
        assert!(!model.should_show_local_landing());

        model.chat.reduce(chat::ChatAction::ResetStreaming);
        model
            .chat
            .reduce(chat::ChatAction::ThreadListReceived(Vec::new()));
        model.connected = true;
        model.auth.loaded = true;
        model.auth.entries = vec![unauthenticated_entry()];
        assert!(!model.should_show_local_landing());
    }

    #[test]
    fn local_landing_yields_to_concierge_loading() {
        let mut model = build_model();
        model.concierge.loading = true;

        assert!(model.should_show_concierge_hero_loading());
        assert!(
            !model.should_show_local_landing(),
            "local landing should not hide concierge loading animation"
        );
    }

    #[test]
    fn thread_loading_placeholder_shows_for_selected_empty_thread() {
        let mut model = build_model();
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "thread-1".to_string(),
            title: "Thread".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
        model.thread_loading_id = Some("thread-1".to_string());

        assert!(model.should_show_thread_loading());
        assert!(!model.should_show_local_landing());
    }

    #[test]
    fn anticipatory_banner_is_suppressed_while_concierge_welcome_is_active() {
        let mut model = build_model();
        model
            .anticipatory
            .reduce(crate::state::AnticipatoryAction::Replace(vec![crate::wire::AnticipatoryItem {
                id: "digest-1".to_string(),
                ..Default::default()
            }]));
        model.concierge.reduce(crate::state::ConciergeAction::WelcomeReceived {
            content: "Welcome back".to_string(),
            actions: vec![crate::state::ConciergeActionVm {
                label: "Continue".to_string(),
                action_type: "continue_session".to_string(),
                thread_id: Some("thread-1".to_string()),
            }],
        });

        assert_eq!(model.anticipatory_banner_height(), 0);
    }

    #[test]
    fn local_landing_full_render_does_not_panic_at_width_100() {
        let mut model = build_model();
        model.width = 100;
        model.height = 40;
        model.focus = FocusArea::Input;

        let backend = TestBackend::new(model.width, model.height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| model.render(frame))
            .expect("local landing render should not panic at width 100");
    }

    #[test]
    fn local_landing_full_render_does_not_panic_at_width_80() {
        let mut model = build_model();
        model.width = 80;
        model.height = 24;
        model.focus = FocusArea::Input;

        let backend = TestBackend::new(model.width, model.height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| model.render(frame))
            .expect("local landing render should not panic at width 80");
    }

    #[test]
    fn render_uses_frame_area_even_when_model_size_is_stale() {
        let mut model = build_model();
        model.width = 120;
        model.height = 40;
        model.focus = FocusArea::Input;

        let backend = TestBackend::new(100, 40);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| model.render(frame))
            .expect("render should honor the live frame size, not stale model dimensions");
    }

    #[test]
    fn concierge_loading_uses_frame_area_even_when_model_size_is_stale() {
        let mut model = build_model();
        model.width = 120;
        model.height = 40;
        model.concierge.loading = true;
        model.focus = FocusArea::Chat;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| model.render(frame))
            .expect("concierge loading should render within the live frame size");
    }

    #[test]
    fn render_syncs_model_dimensions_to_live_frame_area() {
        let mut model = build_model();
        model.width = 120;
        model.height = 40;

        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| model.render(frame))
            .expect("render should succeed against the live frame area");

        assert_eq!(model.width, 100);
        assert_eq!(model.height, 24);
    }

    #[test]
    fn copy_message_formats_reasoning_and_content_with_separator() {
        let mut model = build_model();
        conversion::reset_last_copied_text();
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
                reasoning: Some("Private chain".to_string()),
                content: "Public answer".to_string(),
                ..Default::default()
            },
        });

        model.copy_message(0);

        assert_eq!(
            conversion::last_copied_text().as_deref(),
            Some("Reasoning:\nPrivate chain\n\n-------\n\nContent:\nPublic answer")
        );
    }

    #[test]
    fn copy_message_shows_copied_label_until_timeout() {
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
                content: "Public answer".to_string(),
                ..Default::default()
            },
        });
        model.chat.select_message(Some(0));

        model.copy_message(0);

        let copied_actions = widgets::chat::message_action_targets(
            &model.chat,
            0,
            model
                .chat
                .active_thread()
                .and_then(|thread| thread.messages.first())
                .expect("message should exist"),
            model.tick_counter,
        );
        assert_eq!(copied_actions[0].0, "[Copied]");

        for _ in 0..100 {
            model.on_tick();
        }

        let reverted_actions = widgets::chat::message_action_targets(
            &model.chat,
            0,
            model
                .chat
                .active_thread()
                .and_then(|thread| thread.messages.first())
                .expect("message should exist"),
            model.tick_counter,
        );
        assert_eq!(reverted_actions[0].0, "[Copy]");
    }

    #[test]
    fn regenerate_message_requires_confirmation_before_sending() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.connected = true;
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
                content: "Original prompt".to_string(),
                ..Default::default()
            },
        });
        model.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id: "thread-1".to_string(),
            message: chat::AgentMessage {
                role: chat::MessageRole::Assistant,
                content: "Answer".to_string(),
                ..Default::default()
            },
        });

        model.request_regenerate_message(1);

        assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
        assert!(
            cmd_rx.try_recv().is_err(),
            "regenerate should wait for confirmation"
        );

        let quit = model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::ChatActionConfirm,
        );
        assert!(!quit);

        let mut saw_send = false;
        while let Ok(command) = cmd_rx.try_recv() {
            if matches!(command, DaemonCommand::SendMessage { .. }) {
                saw_send = true;
                break;
            }
        }
        assert!(
            saw_send,
            "confirmation should eventually send the regenerated prompt"
        );
    }

    #[test]
    fn delete_message_requires_confirmation_before_removing_message() {
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

        assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
        assert_eq!(
            model
                .chat
                .active_thread()
                .map(|thread| thread.messages.len()),
            Some(1),
            "message should remain until deletion is confirmed"
        );
        assert!(
            cmd_rx.try_recv().is_err(),
            "delete should wait for confirmation"
        );

        let quit = model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::ChatActionConfirm,
        );
        assert!(!quit);

        let sent = cmd_rx
            .try_recv()
            .expect("confirmation should send delete command");
        assert!(matches!(sent, DaemonCommand::DeleteMessages { .. }));
        assert_eq!(
            model
                .chat
                .active_thread()
                .map(|thread| thread.messages.len()),
            Some(0),
            "message should be removed after deletion is confirmed"
        );
    }

    #[test]
    fn clicking_cancel_in_chat_action_confirm_does_not_delete_message() {
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
        let (_, cancel_rect) = render_helpers::chat_action_confirm_button_bounds(overlay_area)
            .expect("confirm modal should expose button bounds");
        let click_col = cancel_rect.x.saturating_add(1);
        let click_row = cancel_rect.y;

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

        assert_eq!(
            model.modal.top(),
            None,
            "cancel click should close the modal"
        );
        assert_eq!(
            model
                .chat
                .active_thread()
                .map(|thread| thread.messages.len()),
            Some(1),
            "cancel click must not delete the message"
        );
        assert!(
            cmd_rx.try_recv().is_err(),
            "cancel click must not send a delete command"
        );
    }

