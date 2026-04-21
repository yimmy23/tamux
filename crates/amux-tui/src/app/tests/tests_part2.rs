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
    fn submit_prompt_shows_first_user_message_in_new_local_thread() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.connected = true;

        model.submit_prompt("hello".to_string());

        let thread = model
            .chat
            .active_thread()
            .expect("new prompt should create a local thread");
        assert!(
            thread.id.starts_with("local-"),
            "new prompt should target the optimistic local thread"
        );
        assert_eq!(thread.messages.len(), 1);
        assert_eq!(thread.messages[0].role, chat::MessageRole::User);
        assert_eq!(thread.messages[0].content, "hello");

        let send_command = loop {
            match cmd_rx.try_recv() {
                Ok(DaemonCommand::DismissConciergeWelcome) => continue,
                Ok(command @ DaemonCommand::SendMessage { .. }) => break command,
                other => panic!("expected send-message command, got {:?}", other),
            }
        };

        match send_command {
            DaemonCommand::SendMessage {
                thread_id, content, ..
            } => {
                assert_eq!(thread_id, None);
                assert_eq!(content, "hello");
            }
            other => panic!("expected send-message command, got {:?}", other),
        }
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
                ..
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
        model.connected = true;
        model.agent_config_loaded = true;
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
        model.connected = true;
        model.agent_config_loaded = true;
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
    fn goal_composer_opens_mission_control_preflight_with_main_agent_defaults() {
        let mut model = build_model();
        model.config.provider = "openai".to_string();
        model.config.model = "gpt-5.4".to_string();
        model.config.reasoning_effort = "low".to_string();

        model.open_new_goal_view();

        assert!(matches!(model.main_pane_view, MainPaneView::GoalComposer));
        assert_eq!(model.input.buffer(), "");
        assert_eq!(
            model.goal_mission_control.preset_source_label,
            "Main agent inheritance"
        );
        assert_eq!(
            model.goal_mission_control.main_assignment.provider,
            "openai"
        );
        assert_eq!(model.goal_mission_control.main_assignment.model, "gpt-5.4");
        assert_eq!(
            model.goal_mission_control.main_assignment.reasoning_effort.as_deref(),
            Some("low")
        );
        assert_eq!(model.goal_mission_control.role_assignments.len(), 1);
        assert!(
            !model.goal_mission_control.save_as_default_pending,
            "save-as-default should start disabled"
        );
    }

    #[test]
    fn goal_composer_loads_previous_goal_settings_as_defaults_when_present() {
        let mut model = build_model();
        model.config.provider = "fallback-provider".to_string();
        model.config.model = "fallback-model".to_string();
        model.config.reasoning_effort = "fallback-effort".to_string();
        model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Previous Goal".to_string(),
            updated_at: 42,
            launch_assignment_snapshot: vec![
                task::GoalAgentAssignment {
                    role_id: amux_protocol::AGENT_ID_SWAROG.to_string(),
                    enabled: true,
                    provider: "previous-provider".to_string(),
                    model: "previous-model".to_string(),
                    reasoning_effort: Some("medium".to_string()),
                    inherit_from_main: false,
                },
                task::GoalAgentAssignment {
                    role_id: "planner".to_string(),
                    enabled: true,
                    provider: "planner-provider".to_string(),
                    model: "planner-model".to_string(),
                    reasoning_effort: Some("high".to_string()),
                    inherit_from_main: false,
                },
            ],
            runtime_assignment_list: vec![task::GoalAgentAssignment {
                role_id: "planner".to_string(),
                enabled: true,
                provider: "planner-provider".to_string(),
                model: "planner-model".to_string(),
                reasoning_effort: Some("high".to_string()),
                inherit_from_main: false,
            }],
            ..Default::default()
        }));

        model.open_new_goal_view();

        assert_eq!(
            model.goal_mission_control.preset_source_label,
            "Previous goal snapshot"
        );
        assert_eq!(
            model.goal_mission_control.main_assignment.provider,
            "previous-provider"
        );
        assert_eq!(
            model.goal_mission_control.main_assignment.model,
            "previous-model"
        );
        assert_eq!(
            model.goal_mission_control.main_assignment.reasoning_effort.as_deref(),
            Some("medium")
        );
        assert_eq!(model.goal_mission_control.role_assignments.len(), 2);
        assert_eq!(
            model.goal_mission_control.role_assignments[1].provider,
            "planner-provider"
        );
    }

    #[test]
    fn goal_composer_enter_launches_from_mission_control_preflight_state_instead_of_raw_input_mode() {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();
        let mut model = TuiModel::new(daemon_rx, cmd_tx);
        model.connected = true;
        model.main_pane_view = MainPaneView::GoalComposer;
        model.focus = FocusArea::Input;
        model.goal_mission_control.prompt_text = "Mission Control goal".to_string();
        model.input.set_text("raw input that should be ignored");

        let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(!handled);

        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            other => panic!("expected concierge dismissal before launch, got {:?}", other),
        }
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::StartGoalRun {
                goal,
                thread_id,
                session_id,
                ..
            }) => {
                assert_eq!(goal, "Mission Control goal");
                assert_eq!(thread_id, None);
                assert_eq!(session_id, None);
            }
            other => panic!("expected start-goal-run command, got {:?}", other),
        }
        assert_eq!(
            model.status_line,
            "Starting goal run...",
            "preflight submission should launch the goal run"
        );
    }

    #[test]
    fn goal_command_opens_blank_preflight_even_when_goal_run_is_selected() {
        let mut model = build_model();
        model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Existing Goal".to_string(),
            updated_at: 42,
            launch_assignment_snapshot: vec![
                task::GoalAgentAssignment {
                    role_id: amux_protocol::AGENT_ID_SWAROG.to_string(),
                    enabled: true,
                    provider: "runtime-provider".to_string(),
                    model: "runtime-model".to_string(),
                    reasoning_effort: Some("high".to_string()),
                    inherit_from_main: false,
                },
                task::GoalAgentAssignment {
                    role_id: "planner".to_string(),
                    enabled: true,
                    provider: "planner-provider".to_string(),
                    model: "planner-model".to_string(),
                    reasoning_effort: Some("medium".to_string()),
                    inherit_from_main: false,
                },
            ],
            runtime_assignment_list: vec![task::GoalAgentAssignment {
                role_id: "planner".to_string(),
                enabled: true,
                provider: "planner-provider".to_string(),
                model: "planner-model".to_string(),
                reasoning_effort: Some("medium".to_string()),
                inherit_from_main: false,
            }],
            ..Default::default()
        }));
        model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
            goal_run_id: "goal-1".to_string(),
            step_id: None,
        });

        model.execute_command("new-goal");

        assert!(matches!(model.main_pane_view, MainPaneView::GoalComposer));
        assert_eq!(model.focus, FocusArea::Input);
        assert_eq!(model.goal_mission_control.runtime_goal_run_id, None);
        assert_eq!(model.goal_mission_control.prompt_text, "");
        assert_eq!(
            model.goal_mission_control.preset_source_label,
            "Previous goal snapshot"
        );
    }

    #[test]
    fn goal_composer_prompt_typing_updates_preflight_prompt_text() {
        let mut model = build_model();
        model.execute_command("new-goal");

        let handled_g = model.handle_key(KeyCode::Char('g'), KeyModifiers::NONE);
        let handled_o = model.handle_key(KeyCode::Char('o'), KeyModifiers::NONE);

        assert!(!handled_g);
        assert!(!handled_o);
        assert_eq!(model.input.buffer(), "go");
        assert_eq!(model.goal_mission_control.prompt_text, "go");
    }

    #[test]
    fn goal_composer_escape_cancels_preflight_and_restores_previous_goal_view() {
        let mut model = build_model();
        model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Existing Goal".to_string(),
            updated_at: 42,
            ..Default::default()
        }));
        let previous = SidebarItemTarget::GoalRun {
            goal_run_id: "goal-1".to_string(),
            step_id: None,
        };
        model.main_pane_view = MainPaneView::Task(previous.clone());

        model.execute_command("new-goal");
        let handled = model.handle_key(KeyCode::Esc, KeyModifiers::NONE);

        assert!(!handled);
        assert_eq!(model.focus, FocusArea::Chat);
        match &model.main_pane_view {
            MainPaneView::Task(target) => assert_eq!(target, &previous),
            other => panic!("expected previous goal pane after cancel, got {:?}", other),
        }
    }

    #[test]
    fn goal_composer_arrow_keys_navigate_preflight_role_assignments() {
        let mut model = build_model();
        model.goal_mission_control = goal_mission_control::GoalMissionControlState::from_goal_snapshot(
            vec![
                task::GoalAgentAssignment {
                    role_id: amux_protocol::AGENT_ID_SWAROG.to_string(),
                    enabled: true,
                    provider: "openai".to_string(),
                    model: "gpt-5.4".to_string(),
                    reasoning_effort: Some("low".to_string()),
                    inherit_from_main: false,
                },
                task::GoalAgentAssignment {
                    role_id: "planner".to_string(),
                    enabled: true,
                    provider: "openai".to_string(),
                    model: "gpt-5.4-mini".to_string(),
                    reasoning_effort: Some("medium".to_string()),
                    inherit_from_main: false,
                },
            ],
            task::GoalAgentAssignment::default(),
            "Previous goal snapshot",
        );
        model.main_pane_view = MainPaneView::GoalComposer;
        model.focus = FocusArea::Chat;

        let handled = model.handle_key(KeyCode::Down, KeyModifiers::NONE);

        assert!(!handled);
        assert_eq!(model.goal_mission_control.selected_runtime_assignment_index, 1);
    }

    #[test]
    fn goal_composer_provider_hotkey_opens_picker_for_selected_assignment() {
        let mut model = build_model();
        model.goal_mission_control = goal_mission_control::GoalMissionControlState::from_goal_snapshot(
            vec![task::GoalAgentAssignment {
                role_id: amux_protocol::AGENT_ID_SWAROG.to_string(),
                enabled: true,
                provider: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: Some("low".to_string()),
                inherit_from_main: false,
            }],
            task::GoalAgentAssignment::default(),
            "Main agent inheritance",
        );
        model.main_pane_view = MainPaneView::GoalComposer;
        model.focus = FocusArea::Chat;

        let handled = model.handle_key(KeyCode::Char('p'), KeyModifiers::NONE);

        assert!(!handled);
        assert_eq!(model.modal.top(), Some(modal::ModalKind::ProviderPicker));
        assert_eq!(
            model.goal_mission_control.pending_runtime_edit,
            Some(goal_mission_control::RuntimeAssignmentEditRequest {
                row_index: 0,
                field: goal_mission_control::RuntimeAssignmentEditField::Provider,
            })
        );
    }

    #[test]
    fn goal_view_m_hotkey_opens_runtime_mission_control_editor() {
        let mut model = build_model();
        model.focus = FocusArea::Chat;
        model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Existing Goal".to_string(),
            status: Some(task::GoalRunStatus::Running),
            runtime_assignment_list: vec![task::GoalAgentAssignment {
                role_id: amux_protocol::AGENT_ID_SWAROG.to_string(),
                enabled: true,
                provider: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: Some("low".to_string()),
                inherit_from_main: false,
            }],
            ..Default::default()
        }));
        model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
            goal_run_id: "goal-1".to_string(),
            step_id: None,
        });

        let handled = model.handle_key(KeyCode::Char('m'), KeyModifiers::NONE);

        assert!(!handled);
        assert!(matches!(model.main_pane_view, MainPaneView::GoalComposer));
        assert_eq!(model.focus, FocusArea::Chat);
        assert_eq!(
            model.goal_mission_control.runtime_goal_run_id.as_deref(),
            Some("goal-1")
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
    fn builtin_goal_commands_support_goal_picker_and_new_goal_composer() {
        let mut model = build_model();

        assert!(model.is_builtin_command("new-goal"));
        assert!(model.is_builtin_command("goal"));
        assert!(!model.is_builtin_command("goals"));

        model.execute_command("goal");

        assert_eq!(model.modal.top(), Some(modal::ModalKind::GoalPicker));

        model.execute_command("new-goal");

        assert!(matches!(model.main_pane_view, MainPaneView::GoalComposer));
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
