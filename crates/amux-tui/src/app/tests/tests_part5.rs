#[test]
fn drag_selection_copies_expected_text_after_autoscroll() {
    let mut model = build_model();
    model.show_sidebar_override = Some(false);
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
            content: (1..=80)
                .map(|idx| format!("line {idx}"))
                .collect::<Vec<_>>()
                .join("\n"),
            ..Default::default()
        },
    });

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let preferred_row = chat_area.y.saturating_add(chat_area.height / 2);
    let start_row = (preferred_row..chat_area.y.saturating_add(chat_area.height))
        .chain(chat_area.y..preferred_row)
        .find(|row| {
            widgets::chat::selection_point_from_mouse(
                chat_area,
                &model.chat,
                &model.theme,
                model.tick_counter,
                Position::new(3, *row),
            )
            .is_some()
        })
        .expect("chat transcript should expose at least one selectable row");

    super::conversion::reset_last_copied_text();

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 3,
        row: start_row,
        modifiers: KeyModifiers::NONE,
    });
    for _ in 0..4 {
        model.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 3,
            row: start_row,
            modifiers: KeyModifiers::NONE,
        });
    }

    let anchor_point = model
        .chat_drag_anchor_point
        .expect("mouse down should capture a document anchor point");
    let current_point = model
        .chat_drag_current_point
        .expect("autoscroll should extend the current drag point");
    let expected = widgets::chat::selected_text(
        chat_area,
        &model.chat,
        &model.theme,
        model.tick_counter,
        anchor_point,
        current_point,
    )
    .expect("selection should resolve to copied text");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 3,
        row: start_row,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(
        super::conversion::last_copied_text().as_deref(),
        Some(expected.as_str())
    );
    assert_eq!(model.status_line, "Copied selection to clipboard");
}

#[test]
fn work_context_drag_selection_copies_beyond_visible_window() {
    let mut model = build_model();
    model.show_sidebar_override = Some(false);
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
            entries: vec![task::WorkContextEntry {
                path: "/tmp/demo.txt".to_string(),
                is_text: true,
                ..Default::default()
            }],
        },
    ));
    model
        .tasks
        .reduce(task::TaskAction::FilePreviewReceived(task::FilePreview {
            path: "/tmp/demo.txt".to_string(),
            content: (1..=80)
                .map(|idx| format!("line {idx}"))
                .collect::<Vec<_>>()
                .join("\n"),
            truncated: false,
            is_text: true,
        }));
    model.tasks.reduce(task::TaskAction::SelectWorkPath {
        thread_id: "thread-1".to_string(),
        path: Some("/tmp/demo.txt".to_string()),
    });
    model
        .sidebar
        .reduce(SidebarAction::SwitchTab(SidebarTab::Files));
    model.main_pane_view = MainPaneView::WorkContext;
    model.focus = FocusArea::Chat;

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let preferred_row = chat_area.y.saturating_add(chat_area.height / 2);
    let start_row = (preferred_row..chat_area.y.saturating_add(chat_area.height))
        .chain(chat_area.y..preferred_row)
        .find(|row| {
            widgets::work_context_view::selection_point_from_mouse(
                chat_area,
                &model.tasks,
                model.chat.active_thread_id(),
                model.sidebar.active_tab(),
                model.sidebar.selected_item(),
                &model.theme,
                model.task_view_scroll,
                Position::new(3, *row),
            )
            .is_some()
        })
        .expect("work-context preview should expose at least one selectable row");

    super::conversion::reset_last_copied_text();

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 3,
        row: start_row,
        modifiers: KeyModifiers::NONE,
    });
    for _ in 0..4 {
        model.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 3,
            row: start_row,
            modifiers: KeyModifiers::NONE,
        });
    }

    let anchor_point = model
        .work_context_drag_anchor_point
        .expect("mouse down should capture a preview anchor point");
    let current_point = model
        .work_context_drag_current_point
        .expect("scrolling should extend the preview selection");
    let expected = widgets::work_context_view::selected_text(
        chat_area,
        &model.tasks,
        model.chat.active_thread_id(),
        model.sidebar.active_tab(),
        model.sidebar.selected_item(),
        &model.theme,
        model.task_view_scroll,
        anchor_point,
        current_point,
    )
    .expect("selection should resolve to copied preview text");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 3,
        row: start_row,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(
        super::conversion::last_copied_text().as_deref(),
        Some(expected.as_str())
    );
}

#[test]
fn goal_view_drag_selection_copies_beyond_visible_window() {
    let mut model = build_model();
    model.show_sidebar_override = Some(false);
    model.focus = FocusArea::Chat;
    model.task_show_live_todos = false;
    model.task_show_timeline = false;
    model.task_show_files = false;
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Large Goal".to_string(),
            steps: (1..=80)
                .map(|idx| task::GoalRunStep {
                    id: format!("step-{idx}"),
                    title: format!("Step {idx}"),
                    order: idx - 1,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let chat_area = rendered_chat_area(&model);
    let start_row = chat_area.y.saturating_add(6);
    let start_col = chat_area.x.saturating_add(4);

    super::conversion::reset_last_copied_text();

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: start_col,
        row: start_row,
        modifiers: KeyModifiers::NONE,
    });
    for _ in 0..4 {
        model.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: start_col,
            row: start_row,
            modifiers: KeyModifiers::NONE,
        });
    }
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: start_col,
        row: start_row,
        modifiers: KeyModifiers::NONE,
    });

    let copied = super::conversion::last_copied_text()
        .expect("dragging across goal view content should copy selected text");
    assert!(
        copied.contains("Step"),
        "expected goal selection to include goal text, got: {copied:?}"
    );
    assert_eq!(model.status_line, "Copied selection to clipboard");
}

#[test]
fn esc_closes_work_context_even_from_input_focus() {
    let mut model = build_model();
    model.focus = FocusArea::Input;
    model.main_pane_view = MainPaneView::WorkContext;

    let handled = model.handle_key(KeyCode::Esc, KeyModifiers::NONE);

    assert!(!handled);
    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert_eq!(model.focus, FocusArea::Chat);
}

#[test]
fn status_modal_mouse_wheel_scrolls_body() {
    let mut model = build_model();
    model.status_modal_snapshot = Some(crate::client::AgentStatusSnapshotVm {
        tier: "mission_control".to_string(),
        activity: "waiting_for_operator".to_string(),
        active_thread_id: Some("thread-1".to_string()),
        active_goal_run_id: None,
        active_goal_run_title: Some("Close release gap".to_string()),
        provider_health_json: r#"{"openai":{"can_execute":true,"trip_count":0}}"#.to_string(),
        gateway_statuses_json: r#"{"slack":{"status":"connected"}}"#.to_string(),
        recent_actions_json: serde_json::to_string(
            &(0..40)
                .map(|idx| {
                    serde_json::json!({
                        "action_type": format!("tool_{idx}"),
                        "summary": format!("summary {idx}"),
                        "timestamp": 1712345678_u64 + idx,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    });
    model.status_modal_diagnostics_json = Some(
        serde_json::json!({
            "aline": {
                "available": true,
                "watcher_state": "running",
                "imported_count": 1,
                "generated_count": 1,
            }
        })
        .to_string(),
    );
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Status));

    let (_, overlay_area) = model
        .current_modal_area()
        .expect("status modal should expose an overlay area");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: overlay_area.x.saturating_add(2),
        row: overlay_area.y.saturating_add(2),
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.status_modal_scroll, 3);
}

#[test]
fn participants_modal_mouse_wheel_scrolls_body() {
    let mut model = build_model();
    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Participants Thread".to_string(),
        thread_participants: (0..20)
            .map(|idx| crate::wire::ThreadParticipantState {
                agent_id: format!("agent-{idx}"),
                agent_name: format!("Agent {idx}"),
                instruction: format!("Instruction {idx}"),
                status: "active".to_string(),
                last_contribution_at: None,
                deactivated_at: None,
                always_auto_response: false,
                created_at: idx,
                updated_at: idx,
            })
            .collect(),
        ..Default::default()
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.open_thread_participants_modal();

    let (_, overlay_area) = model
        .current_modal_area()
        .expect("participants modal should expose an overlay area");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: overlay_area.x.saturating_add(2),
        row: overlay_area.y.saturating_add(2),
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.thread_participants_modal_scroll, 3);
}

#[test]
fn statistics_modal_mouse_wheel_scrolls_body() {
    let mut model = build_model();
    model.statistics_modal_snapshot = Some(amux_protocol::AgentStatisticsSnapshot {
        window: amux_protocol::AgentStatisticsWindow::All,
        generated_at: 1,
        has_incomplete_cost_history: false,
        totals: amux_protocol::AgentStatisticsTotals {
            input_tokens: 100,
            output_tokens: 100,
            total_tokens: 200,
            cost_usd: 1.0,
            provider_count: 1,
            model_count: 80,
        },
        providers: vec![amux_protocol::ProviderStatisticsRow {
            provider: "openai".to_string(),
            input_tokens: 100,
            output_tokens: 100,
            total_tokens: 200,
            cost_usd: 1.0,
        }],
        models: (0..80)
            .map(|idx| amux_protocol::ModelStatisticsRow {
                provider: "openai".to_string(),
                model: format!("model-{idx}"),
                input_tokens: idx,
                output_tokens: idx,
                total_tokens: idx * 2,
                cost_usd: idx as f64 / 10.0,
            })
            .collect(),
        top_models_by_tokens: Vec::new(),
        top_models_by_cost: Vec::new(),
    });
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Statistics));
    model.width = 80;
    model.height = 20;

    let (_, overlay_area) = model
        .current_modal_area()
        .expect("statistics modal should expose an overlay area");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: overlay_area.x.saturating_add(2),
        row: overlay_area.y.saturating_add(4),
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.statistics_modal_scroll, 3);
}

#[test]
fn reselecting_same_sidebar_file_closes_work_context() {
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
            entries: vec![task::WorkContextEntry {
                path: "/tmp/demo.txt".to_string(),
                is_text: true,
                ..Default::default()
            }],
        },
    ));
    model.tasks.reduce(task::TaskAction::SelectWorkPath {
        thread_id: "thread-1".to_string(),
        path: Some("/tmp/demo.txt".to_string()),
    });
    model
        .sidebar
        .reduce(SidebarAction::SwitchTab(SidebarTab::Files));
    model.main_pane_view = MainPaneView::WorkContext;
    model.focus = FocusArea::Sidebar;

    model.handle_sidebar_enter();

    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert_eq!(model.focus, FocusArea::Sidebar);
}

#[test]
fn reselecting_same_sidebar_todo_closes_work_context() {
    let mut model = build_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.tasks.reduce(task::TaskAction::ThreadTodosReceived {
        thread_id: "thread-1".to_string(),
        items: vec![task::TodoItem {
            id: "todo-1".to_string(),
            content: "todo".to_string(),
            status: Some(task::TodoStatus::Pending),
            position: 0,
            step_index: None,
            created_at: 0,
            updated_at: 0,
        }],
    });
    model
        .sidebar
        .reduce(SidebarAction::SwitchTab(SidebarTab::Todos));
    model.main_pane_view = MainPaneView::WorkContext;
    model.focus = FocusArea::Sidebar;

    model.handle_sidebar_enter();

    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert_eq!(model.focus, FocusArea::Sidebar);
}

#[test]
fn attention_surface_uses_settings_tab_when_modal_open() {
    let mut model = build_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::SubAgents));

    let (surface, thread_id, goal_run_id) = model.current_attention_target();
    assert_eq!(surface, "modal:settings:subagents");
    assert_eq!(thread_id, None);
    assert_eq!(goal_run_id, None);
}

#[test]
fn attention_surface_uses_sidebar_tab_for_sidebar_focus() {
    let mut model = build_model();
    model.connected = true;
    model.auth.loaded = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread_1".to_string(),
        title: "Test".to_string(),
    });
    model.tasks.reduce(task::TaskAction::ThreadTodosReceived {
        thread_id: "thread_1".to_string(),
        items: vec![task::TodoItem {
            id: "todo_1".to_string(),
            content: "todo".to_string(),
            status: Some(task::TodoStatus::Pending),
            position: 0,
            step_index: None,
            created_at: 0,
            updated_at: 0,
        }],
    });
    model.focus = FocusArea::Sidebar;
    model
        .sidebar
        .reduce(SidebarAction::SwitchTab(SidebarTab::Todos));

    let (surface, thread_id, goal_run_id) = model.current_attention_target();
    assert_eq!(surface, "conversation:sidebar:todos");
    assert_eq!(thread_id.as_deref(), Some("thread_1"));
    assert_eq!(goal_run_id, None);
}

#[test]
fn operator_profile_onboarding_takes_precedence_over_provider_onboarding() {
    let mut model = build_model();
    model.connected = true;
    model.auth.loaded = true;
    model.auth.entries = vec![unauthenticated_entry()];
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "name".to_string(),
        field_key: "name".to_string(),
        prompt: "What should I call you?".to_string(),
        input_kind: "text".to_string(),
        optional: false,
    });

    assert!(
        model.should_show_operator_profile_onboarding(),
        "operator profile onboarding should be active"
    );
    assert!(
        !model.should_show_provider_onboarding(),
        "provider onboarding should not mask operator profile onboarding"
    );
}

#[test]
fn submit_operator_profile_answer_sends_command_and_clears_input() {
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
    model.input.set_text("Milan");

    assert!(model.submit_operator_profile_answer());
    assert_eq!(model.input.buffer(), "");
    assert!(
        model.operator_profile.question.is_none(),
        "question should clear when submission starts"
    );

    let sent = cmd_rx
        .try_recv()
        .expect("submitting answer should emit daemon command");
    match sent {
        DaemonCommand::SubmitOperatorProfileAnswer {
            session_id,
            question_id,
            answer_json,
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(question_id, "name");
            assert_eq!(answer_json, "\"Milan\"");
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn clicking_chat_file_chip_requests_file_preview() {
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

    let temp_path =
        std::env::temp_dir().join(format!("tamux-chat-preview-{}.txt", uuid::Uuid::new_v4()));
    std::fs::write(&temp_path, "preview me\n").expect("fixture file should be writable");
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Tool,
            tool_name: Some("read_file".to_string()),
            tool_arguments: Some(
                serde_json::json!({ "path": temp_path.display().to_string() }).to_string(),
            ),
            tool_status: Some("done".to_string()),
            content: "preview me".to_string(),
            ..Default::default()
        },
    });

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let chip_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::chat::hit_test(
                    chat_area,
                    &model.chat,
                    &model.theme,
                    model.tick_counter,
                    pos,
                ) == Some(chat::ChatHitTarget::ToolFilePath { message_index: 0 })
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("tool row should expose a clickable file chip");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: chip_pos.x,
        row: chip_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: chip_pos.x,
        row: chip_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    match cmd_rx.try_recv() {
        Ok(DaemonCommand::RequestFilePreview { path, max_bytes }) => {
            assert_eq!(path, temp_path.display().to_string());
            assert_eq!(max_bytes, Some(65_536));
        }
        other => panic!("expected file preview request, got {:?}", other),
    }
    assert!(matches!(model.main_pane_view, MainPaneView::FilePreview(_)));
}

#[test]
fn clicking_chat_image_attachment_requests_file_preview() {
    use base64::Engine as _;

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

    let temp_path =
        std::env::temp_dir().join(format!("tamux-chat-image-{}.png", uuid::Uuid::new_v4()));
    std::fs::write(
        &temp_path,
        base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO0pGfcAAAAASUVORK5CYII=")
            .expect("fixture PNG should decode"),
    )
    .expect("fixture PNG should write");
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content_blocks: vec![chat::AgentContentBlock::Image {
                url: Some(format!("file://{}", temp_path.display())),
                data_url: None,
                mime_type: Some("image/png".to_string()),
            }],
            ..Default::default()
        },
    });

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let image_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::chat::hit_test(
                    chat_area,
                    &model.chat,
                    &model.theme,
                    model.tick_counter,
                    pos,
                ) == Some(chat::ChatHitTarget::MessageImage { message_index: 0 })
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("assistant image attachment should be clickable");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: image_pos.x,
        row: image_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: image_pos.x,
        row: image_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    match cmd_rx.try_recv() {
        Ok(DaemonCommand::RequestFilePreview { path, max_bytes }) => {
            assert_eq!(path, temp_path.display().to_string());
            assert_eq!(max_bytes, Some(65_536));
        }
        other => panic!("expected image preview request, got {:?}", other),
    }
    assert!(matches!(model.main_pane_view, MainPaneView::FilePreview(_)));
}

fn spawned_thread_navigation_task(
    id: &str,
    title: &str,
    created_at: u64,
    thread_id: Option<&str>,
    parent_task_id: Option<&str>,
    parent_thread_id: Option<&str>,
    status: Option<task::TaskStatus>,
) -> task::AgentTask {
    task::AgentTask {
        id: id.to_string(),
        title: title.to_string(),
        created_at,
        thread_id: thread_id.map(str::to_string),
        parent_task_id: parent_task_id.map(str::to_string),
        parent_thread_id: parent_thread_id.map(str::to_string),
        status,
        ..Default::default()
    }
}

fn seed_spawned_thread_navigation_model_with_loaded_child(loaded_child: bool) -> TuiModel {
    let mut model = build_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-root".to_string(),
        title: "Root".to_string(),
    });
    if loaded_child {
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "thread-child".to_string(),
            title: "Child".to_string(),
        });
    }
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-other".to_string(),
        title: "Other".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-root".to_string()));
    model.tasks.reduce(task::TaskAction::TaskListReceived(vec![
        spawned_thread_navigation_task(
            "root-task",
            "Root worker",
            10,
            Some("thread-root"),
            None,
            None,
            Some(task::TaskStatus::InProgress),
        ),
        spawned_thread_navigation_task(
            "child-task",
            "Child worker",
            20,
            Some("thread-child"),
            Some("root-task"),
            Some("thread-root"),
            Some(task::TaskStatus::InProgress),
        ),
    ]));
    model
}

fn seed_spawned_thread_navigation_model() -> TuiModel {
    seed_spawned_thread_navigation_model_with_loaded_child(true)
}

#[test]
fn spawned_thread_navigation_enter_switches_to_child_thread_and_pushes_history() {
    let mut model = seed_spawned_thread_navigation_model();
    model.focus = FocusArea::Sidebar;
    model
        .sidebar
        .reduce(SidebarAction::SwitchTab(SidebarTab::Spawned));
    model.sidebar.navigate(1, model.sidebar_item_count());

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.chat.active_thread_id(), Some("thread-child"));
    assert_eq!(
        model.chat.thread_history_stack(),
        &["thread-root".to_string()]
    );
    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert_eq!(model.focus, FocusArea::Chat);
}

#[test]
fn spawned_thread_navigation_keyboard_tab_switch_primes_first_openable_child() {
    let mut model = seed_spawned_thread_navigation_model();
    model.focus = FocusArea::Sidebar;

    let handled = model.handle_key(KeyCode::Right, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.sidebar.active_tab(), SidebarTab::Files);

    let handled = model.handle_key(KeyCode::Right, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.sidebar.active_tab(), SidebarTab::Spawned);

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.chat.active_thread_id(), Some("thread-child"));
    assert_eq!(
        model.chat.thread_history_stack(),
        &["thread-root".to_string()]
    );
}

#[test]
fn spawned_thread_navigation_enter_opens_unloaded_child_thread() {
    let mut model = seed_spawned_thread_navigation_model_with_loaded_child(false);
    model.focus = FocusArea::Sidebar;
    model.activate_sidebar_tab(SidebarTab::Spawned);

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.chat.active_thread_id(), Some("thread-child"));
    assert_eq!(
        model.chat.thread_history_stack(),
        &["thread-root".to_string()]
    );
    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert_eq!(model.focus, FocusArea::Chat);
    assert_eq!(model.thread_loading_id.as_deref(), Some("thread-child"));
}

#[test]
fn spawned_thread_navigation_preserves_pending_child_open_across_thread_list_refresh() {
    let mut model = seed_spawned_thread_navigation_model_with_loaded_child(false);
    model.focus = FocusArea::Sidebar;
    model.activate_sidebar_tab(SidebarTab::Spawned);

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.chat.active_thread_id(), Some("thread-child"));
    assert_eq!(model.thread_loading_id.as_deref(), Some("thread-child"));

    model.handle_thread_list_event(vec![
        crate::wire::AgentThread {
            id: "thread-root".to_string(),
            title: "Root".to_string(),
            ..Default::default()
        },
        crate::wire::AgentThread {
            id: "thread-other".to_string(),
            title: "Other".to_string(),
            ..Default::default()
        },
    ]);

    assert_eq!(
        model.chat.active_thread_id(),
        Some("thread-child"),
        "thread-list refresh should not clear a child thread that is still loading"
    );
    assert_eq!(
        model.thread_loading_id.as_deref(),
        Some("thread-child"),
        "loading state should survive until the requested child thread arrives"
    );
    assert_eq!(
        model.chat.thread_history_stack(),
        &["thread-root".to_string()]
    );
}

#[test]
fn spawned_thread_navigation_mouse_click_opens_unloaded_child_thread() {
    let mut model = seed_spawned_thread_navigation_model_with_loaded_child(false);
    let sidebar_area = model
        .pane_layout()
        .sidebar
        .expect("default layout should include a sidebar");
    model.activate_sidebar_tab(SidebarTab::Spawned);

    let child_pos = (sidebar_area.y..sidebar_area.y.saturating_add(sidebar_area.height))
        .find_map(|row| {
            (sidebar_area.x..sidebar_area.x.saturating_add(sidebar_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::sidebar::hit_test(
                    sidebar_area,
                    &model.chat,
                    &model.sidebar,
                    &model.tasks,
                    model.chat.active_thread_id(),
                    pos,
                ) == Some(widgets::sidebar::SidebarHitTarget::Spawned(1))
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("spawned sidebar should expose a clickable child row");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: child_pos.x,
        row: child_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-child"));
    assert_eq!(
        model.chat.thread_history_stack(),
        &["thread-root".to_string()]
    );
    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert_eq!(model.focus, FocusArea::Chat);
    assert_eq!(model.thread_loading_id.as_deref(), Some("thread-child"));
}

#[test]
fn spawned_thread_navigation_back_action_pops_to_previous_thread() {
    let mut model = seed_spawned_thread_navigation_model();
    model.focus = FocusArea::Sidebar;
    model
        .sidebar
        .reduce(SidebarAction::SwitchTab(SidebarTab::Spawned));
    model.sidebar.navigate(1, model.sidebar_item_count());
    model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    model.focus = FocusArea::Sidebar;

    let handled = model.handle_key(KeyCode::Backspace, KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.chat.active_thread_id(), Some("thread-root"));
    assert!(
        model.chat.thread_history_stack().is_empty(),
        "back navigation should pop the spawned thread stack"
    );
}

#[test]
fn spawned_thread_navigation_back_is_disabled_when_stack_is_empty() {
    let mut model = seed_spawned_thread_navigation_model();
    model.focus = FocusArea::Sidebar;
    model
        .sidebar
        .reduce(SidebarAction::SwitchTab(SidebarTab::Spawned));

    let handled = model.handle_key(KeyCode::Backspace, KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.chat.active_thread_id(), Some("thread-root"));
    assert!(
        model.chat.thread_history_stack().is_empty(),
        "empty stack should stay unchanged"
    );
}

#[test]
fn spawned_thread_navigation_ordinary_thread_switches_do_not_mutate_stack() {
    let mut model = seed_spawned_thread_navigation_model();
    model.focus = FocusArea::Sidebar;
    model
        .sidebar
        .reduce(SidebarAction::SwitchTab(SidebarTab::Spawned));
    model.sidebar.navigate(1, model.sidebar_item_count());
    model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-other".to_string()));

    assert_eq!(model.chat.active_thread_id(), Some("thread-other"));
    assert_eq!(
        model.chat.thread_history_stack(),
        &["thread-root".to_string()],
        "direct thread selection should preserve existing spawned thread history"
    );
}

#[test]
fn clicking_repo_backed_chat_file_chip_requests_git_diff() {
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

    let repo_root = "/home/mkurman/gitlab/it/cmux-next";
    let repo_path = format!("{repo_root}/README.md");
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Tool,
            tool_name: Some("write_file".to_string()),
            tool_arguments: Some(serde_json::json!({ "path": repo_path }).to_string()),
            tool_status: Some("done".to_string()),
            content: "updated".to_string(),
            ..Default::default()
        },
    });

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let chip_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::chat::hit_test(
                    chat_area,
                    &model.chat,
                    &model.theme,
                    model.tick_counter,
                    pos,
                ) == Some(chat::ChatHitTarget::ToolFilePath { message_index: 0 })
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("tool row should expose a clickable repo-backed file chip");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: chip_pos.x,
        row: chip_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: chip_pos.x,
        row: chip_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    match cmd_rx.try_recv() {
        Ok(DaemonCommand::RequestGitDiff {
            repo_path,
            file_path,
        }) => {
            assert_eq!(repo_path, repo_root);
            assert_eq!(file_path.as_deref(), Some("README.md"));
        }
        other => panic!("expected git diff request, got {:?}", other),
    }
    assert!(matches!(model.main_pane_view, MainPaneView::FilePreview(_)));
}

#[test]
fn clicking_repo_backed_read_file_chip_requests_plain_preview() {
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

    let repo_path = "/home/mkurman/gitlab/it/cmux-next/crates/amux-daemon/src/agent/agent_loop/send_message/setup.rs";
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Tool,
            tool_name: Some("read_file".to_string()),
            tool_arguments: Some(serde_json::json!({ "path": repo_path }).to_string()),
            tool_status: Some("done".to_string()),
            content: "previewed".to_string(),
            ..Default::default()
        },
    });

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let chip_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::chat::hit_test(
                    chat_area,
                    &model.chat,
                    &model.theme,
                    model.tick_counter,
                    pos,
                ) == Some(chat::ChatHitTarget::ToolFilePath { message_index: 0 })
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("tool row should expose a clickable repo-backed read_file chip");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: chip_pos.x,
        row: chip_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: chip_pos.x,
        row: chip_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    match cmd_rx.try_recv() {
        Ok(DaemonCommand::RequestFilePreview { path, max_bytes }) => {
            assert_eq!(path, repo_path);
            assert_eq!(max_bytes, Some(65_536));
        }
        other => panic!("expected file preview request, got {:?}", other),
    }

    match &model.main_pane_view {
        MainPaneView::FilePreview(target) => {
            assert_eq!(target.path, repo_path);
            assert!(target.repo_root.is_none());
            assert!(target.repo_relative_path.is_none());
        }
        other => panic!("expected file preview pane, got {:?}", other),
    }
}

#[test]
fn clicking_read_skill_chip_requests_plain_preview() {
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

    let skills_root = "/home/mkurman/gitlab/it/cmux-next";
    let relative_path = "skills/development/superpowers/systematic-debugging/SKILL.md";
    let full_path = format!("{skills_root}/{relative_path}");
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Tool,
            tool_name: Some("read_skill".to_string()),
            tool_arguments: Some(
                serde_json::json!({ "skill": "systematic-debugging" }).to_string(),
            ),
            tool_status: Some("done".to_string()),
            content: serde_json::json!({
                "skills_root": skills_root,
                "path": relative_path,
                "content": "previewed",
                "truncated": false,
                "total_lines": 12
            })
            .to_string(),
            ..Default::default()
        },
    });

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let chip_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::chat::hit_test(
                    chat_area,
                    &model.chat,
                    &model.theme,
                    model.tick_counter,
                    pos,
                ) == Some(chat::ChatHitTarget::ToolFilePath { message_index: 0 })
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("tool row should expose a clickable read_skill chip");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: chip_pos.x,
        row: chip_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: chip_pos.x,
        row: chip_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    match cmd_rx.try_recv() {
        Ok(DaemonCommand::RequestFilePreview { path, max_bytes }) => {
            assert_eq!(path, full_path);
            assert_eq!(max_bytes, Some(65_536));
        }
        other => panic!("expected file preview request, got {:?}", other),
    }

    match &model.main_pane_view {
        MainPaneView::FilePreview(target) => {
            assert_eq!(target.path, full_path);
            assert!(target.repo_root.is_none());
            assert!(target.repo_relative_path.is_none());
        }
        other => panic!("expected file preview pane, got {:?}", other),
    }
}

#[test]
fn clicking_tool_file_path_metadata_requests_plain_preview() {
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

    let preview_path = "/home/mkurman/gitlab/it/cmux-next/.tamux/.cache/tools/thread-thread-1/bash_command-1700000123.txt";
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Tool,
            tool_name: Some("bash_command".to_string()),
            tool_status: Some("done".to_string()),
            tool_output_preview_path: Some(preview_path.to_string()),
            content: "Tool result saved to preview file\n- tool: bash_command".to_string(),
            ..Default::default()
        },
    });

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let chip_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::chat::hit_test(
                    chat_area,
                    &model.chat,
                    &model.theme,
                    model.tick_counter,
                    pos,
                ) == Some(chat::ChatHitTarget::ToolFilePath { message_index: 0 })
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("preview-backed tool row should expose a clickable file chip");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: chip_pos.x,
        row: chip_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: chip_pos.x,
        row: chip_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    match cmd_rx.try_recv() {
        Ok(DaemonCommand::RequestFilePreview { path, max_bytes }) => {
            assert_eq!(path, preview_path);
            assert_eq!(max_bytes, Some(65_536));
        }
        other => panic!("expected plain file preview request, got {:?}", other),
    }

    match &model.main_pane_view {
        MainPaneView::FilePreview(target) => {
            assert_eq!(target.path, preview_path);
            assert!(target.repo_root.is_none());
            assert!(target.repo_relative_path.is_none());
        }
        other => panic!("expected file preview pane, got {:?}", other),
    }
}

#[test]
fn closing_chat_file_preview_returns_to_conversation() {
    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.main_pane_view = MainPaneView::FilePreview(ChatFilePreviewTarget {
        path: "/tmp/demo.txt".to_string(),
        repo_root: None,
        repo_relative_path: None,
    });

    let handled = model.handle_key(KeyCode::Esc, KeyModifiers::NONE);

    assert!(!handled);
    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert_eq!(model.focus, FocusArea::Chat);
}

#[test]
fn goal_view_renders_goal_run_dossier_sections() {
    fn render_task_view(model: &mut TuiModel) -> String {
        let backend = TestBackend::new(model.width, model.height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| model.render(frame))
            .expect("task view render should succeed");

        let chat_area = rendered_chat_area(model);
        let buffer = terminal.backend().buffer();
        (chat_area.y..chat_area.y.saturating_add(chat_area.height))
            .map(|y| {
                (chat_area.x..chat_area.x.saturating_add(chat_area.width))
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);
    model.task_show_live_todos = false;
    model.task_show_timeline = false;
    model.task_show_files = false;

    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Instrument Titan".to_string(),
            goal: "Ship the first dossier-aware goal view.".to_string(),
            steps: vec![task::GoalRunStep {
                id: "step-1".to_string(),
                title: "Phone logging flow".to_string(),
                order: 0,
                ..Default::default()
            }],
            dossier: Some(task::GoalRunDossier {
                projection_state: "in_progress".to_string(),
                summary: Some("Execution is split into build and verification units.".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let plain = render_task_view(&mut model);

    assert!(plain.contains("Goal Mission Control"), "{plain}");
    assert!(plain.contains("Plan"), "{plain}");
    assert!(plain.contains("Run timeline"), "{plain}");
    assert!(plain.contains("Dossier"), "{plain}");
    assert!(plain.contains("Files"), "{plain}");
    assert!(plain.contains("Prompt"), "{plain}");
    assert!(plain.contains("Phone logging flow"), "{plain}");
    assert!(plain.contains("Execution Dossier"), "{plain}");
}

#[test]
fn goal_view_renders_goal_control_hints() {
    fn render_task_view(model: &mut TuiModel) -> String {
        let backend = TestBackend::new(model.width, model.height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| model.render(frame))
            .expect("task view render should succeed");

        let chat_area = rendered_chat_area(model);
        let buffer = terminal.backend().buffer();
        (chat_area.y..chat_area.y.saturating_add(chat_area.height))
            .map(|y| {
                (chat_area.x..chat_area.x.saturating_add(chat_area.width))
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);
    model.task_show_live_todos = false;
    model.task_show_timeline = false;
    model.task_show_files = false;

    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Paused Goal".to_string(),
            status: Some(task::GoalRunStatus::Paused),
            current_step_index: 1,
            current_step_title: Some("Implement".to_string()),
            steps: vec![
                task::GoalRunStep {
                    id: "step-1".to_string(),
                    title: "Plan".to_string(),
                    order: 0,
                    ..Default::default()
                },
                task::GoalRunStep {
                    id: "step-2".to_string(),
                    title: "Implement".to_string(),
                    order: 1,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let plain = render_task_view(&mut model);

    assert!(plain.contains("Goal Mission Control"), "{plain}");
    assert!(plain.contains("Goal"), "{plain}");
    assert!(plain.contains("Progress"), "{plain}");
    assert!(plain.contains("Active agent"), "{plain}");
    assert!(plain.contains("Needs attention"), "{plain}");
}

#[test]
fn goal_view_renders_visual_status_banner_and_control_chips() {
    fn render_task_view(model: &mut TuiModel) -> String {
        let backend = TestBackend::new(model.width, model.height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| model.render(frame))
            .expect("task view render should succeed");

        let chat_area = rendered_chat_area(model);
        let buffer = terminal.backend().buffer();
        (chat_area.y..chat_area.y.saturating_add(chat_area.height))
            .map(|y| {
                (chat_area.x..chat_area.x.saturating_add(chat_area.width))
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);
    model.task_show_live_todos = false;
    model.task_show_timeline = false;
    model.task_show_files = false;
    model.tick_counter = 12;

    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Paused Goal".to_string(),
            status: Some(task::GoalRunStatus::Paused),
            current_step_index: 1,
            current_step_title: Some("Implement".to_string()),
            steps: vec![
                task::GoalRunStep {
                    id: "step-1".to_string(),
                    title: "Plan".to_string(),
                    order: 0,
                    ..Default::default()
                },
                task::GoalRunStep {
                    id: "step-2".to_string(),
                    title: "Implement".to_string(),
                    order: 1,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let plain = render_task_view(&mut model);

    assert!(plain.contains("Goal Mission Control"), "{plain}");
    assert!(plain.contains("Plan"), "{plain}");
    assert!(plain.contains("Implement"), "{plain}");
    assert!(!plain.contains("Run Status"), "{plain}");
    assert!(!plain.contains("Controls"), "{plain}");
}

#[test]
fn goal_view_renders_live_activity_with_tools_files_and_todos() {
    fn render_task_view(model: &mut TuiModel) -> String {
        let backend = TestBackend::new(model.width, model.height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| model.render(frame))
            .expect("task view render should succeed");

        let chat_area = rendered_chat_area(model);
        let buffer = terminal.backend().buffer();
        (chat_area.y..chat_area.y.saturating_add(chat_area.height))
            .map(|y| {
                (chat_area.x..chat_area.x.saturating_add(chat_area.width))
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);
    model.task_show_live_todos = false;
    model.task_show_timeline = false;
    model.task_show_files = false;
    model.tick_counter = 21;

    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Live Goal".to_string(),
            thread_id: Some("thread-1".to_string()),
            status: Some(task::GoalRunStatus::Running),
            current_step_index: 0,
            current_step_title: Some("Patch UI".to_string()),
            steps: vec![task::GoalRunStep {
                id: "step-1".to_string(),
                title: "Patch UI".to_string(),
                order: 0,
                ..Default::default()
            }],
            events: vec![
                task::GoalRunEvent {
                    id: "event-1".to_string(),
                    phase: "tool".to_string(),
                    message: "apply_patch updated goal view".to_string(),
                    details: Some("Added status hero and activity cards".to_string()),
                    step_index: Some(0),
                    ..Default::default()
                },
                task::GoalRunEvent {
                    id: "event-2".to_string(),
                    phase: "todo".to_string(),
                    message: "goal todo updated".to_string(),
                    step_index: Some(0),
                    todo_snapshot: vec![task::TodoItem {
                        id: "todo-1".to_string(),
                        content: "Inspect failing test".to_string(),
                        status: Some(task::TodoStatus::InProgress),
                        position: 0,
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }));
    model.tasks.reduce(task::TaskAction::WorkContextReceived(
        task::ThreadWorkContext {
            thread_id: "thread-1".to_string(),
            entries: vec![task::WorkContextEntry {
                path: "crates/amux-tui/src/widgets/task_view.rs".to_string(),
                source: "apply_patch".to_string(),
                change_kind: Some("diff".to_string()),
                goal_run_id: Some("goal-1".to_string()),
                step_index: Some(0),
                is_text: true,
                ..Default::default()
            }],
        },
    ));
    model.tasks.reduce(task::TaskAction::ThreadTodosReceived {
        thread_id: "thread-1".to_string(),
        items: vec![task::TodoItem {
            id: "todo-1".to_string(),
            content: "Inspect failing test".to_string(),
            status: Some(task::TodoStatus::InProgress),
            step_index: Some(0),
            position: 0,
            ..Default::default()
        }],
    });
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });
    model.goal_workspace.set_step_expanded("step-1", true);

    let plain = render_task_view(&mut model);

    assert!(plain.contains("Run timeline"), "{plain}");
    assert!(plain.contains("apply_patch updated goal view"), "{plain}");
    assert!(plain.contains("goal todo updated"), "{plain}");
    assert!(plain.contains("Inspect failing test"), "{plain}");
    assert!(plain.contains("Selected Timeline Item"), "{plain}");
    assert!(plain.contains("[details]"), "{plain}");
}

#[test]
fn goal_view_related_tasks_use_status_checkbox_without_duplicate_status_text() {
    fn render_task_view(model: &mut TuiModel) -> String {
        let backend = TestBackend::new(model.width, model.height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| model.render(frame))
            .expect("task view render should succeed");

        let chat_area = rendered_chat_area(model);
        let buffer = terminal.backend().buffer();
        (chat_area.y..chat_area.y.saturating_add(chat_area.height))
            .map(|y| {
                (chat_area.x..chat_area.x.saturating_add(chat_area.width))
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);
    model.task_show_live_todos = false;
    model.task_show_timeline = false;
    model.task_show_files = false;

    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Goal".to_string(),
            thread_id: Some("thread-1".to_string()),
            steps: vec![task::GoalRunStep {
                id: "step-1".to_string(),
                title: "Step".to_string(),
                order: 0,
                ..Default::default()
            }],
            ..Default::default()
        }));
    model.tasks.reduce(task::TaskAction::ThreadTodosReceived {
        thread_id: "thread-1".to_string(),
        items: vec![
            task::TodoItem {
                id: "todo-1".to_string(),
                content: "Collect and index sources".to_string(),
                status: Some(task::TodoStatus::InProgress),
                step_index: Some(0),
                position: 0,
                ..Default::default()
            },
            task::TodoItem {
                id: "todo-2".to_string(),
                content: "Ground the user's background".to_string(),
                status: Some(task::TodoStatus::Completed),
                step_index: Some(0),
                position: 1,
                ..Default::default()
            },
            task::TodoItem {
                id: "todo-3".to_string(),
                content: "Review plan".to_string(),
                status: Some(task::TodoStatus::Blocked),
                step_index: Some(0),
                position: 2,
                ..Default::default()
            },
        ],
    });
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });
    model.goal_workspace.set_step_expanded("step-1", true);

    let plain = render_task_view(&mut model);

    assert!(plain.contains("[~] Collect and index sources"), "{plain}");
    assert!(plain.contains("[x] Ground the user's background"), "{plain}");
    assert!(plain.contains("[!] Review plan"), "{plain}");
    assert!(!plain.contains("Collect and index sources running"), "{plain}");
    assert!(!plain.contains("Ground the user's background done"), "{plain}");
    assert!(!plain.contains("Review plan blocked"), "{plain}");
}

#[test]
fn goal_view_paused_restart_renders_review_guidance() {
    fn render_task_view(model: &mut TuiModel) -> String {
        let backend = TestBackend::new(model.width, model.height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| model.render(frame))
            .expect("task view render should succeed");

        let chat_area = rendered_chat_area(model);
        let buffer = terminal.backend().buffer();
        (chat_area.y..chat_area.y.saturating_add(chat_area.height))
            .map(|y| {
                (chat_area.x..chat_area.x.saturating_add(chat_area.width))
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);
    model.task_show_live_todos = false;
    model.task_show_timeline = false;
    model.task_show_files = false;

    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Paused Goal".to_string(),
            status: Some(task::GoalRunStatus::Paused),
            current_step_title: Some("Implement".to_string()),
            events: vec![task::GoalRunEvent {
                id: "event-1".to_string(),
                phase: "restart".to_string(),
                message: "Daemon restarted; goal run paused for operator review.".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        }));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let plain = render_task_view(&mut model);

    assert!(plain.contains("Run timeline"), "{plain}");
    assert!(plain.contains("Daemon restarted; goal run paused"), "{plain}");
    assert!(plain.contains("operator review."), "{plain}");
}

#[test]
fn goal_view_scroll_up_moves_off_bottom_after_mouse_overscroll() {
    fn render_task_view(model: &mut TuiModel) -> Vec<String> {
        let backend = TestBackend::new(model.width, model.height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| model.render(frame))
            .expect("task view render should succeed");

        let chat_area = rendered_chat_area(model);
        let buffer = terminal.backend().buffer();
        (chat_area.y..chat_area.y.saturating_add(chat_area.height))
            .map(|y| {
                (chat_area.x..chat_area.x.saturating_add(chat_area.width))
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect()
    }

    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);
    model.task_show_live_todos = false;
    model.task_show_timeline = false;
    model.task_show_files = false;

    let steps = (1..=60)
        .map(|idx| task::GoalRunStep {
            id: format!("step-{idx}"),
            title: format!("Step {idx}"),
            instructions: format!(
                "Line {idx}A\nLine {idx}B\nLine {idx}C\nLine {idx}D\nLine {idx}E"
            ),
            order: idx,
            ..Default::default()
        })
        .collect();

    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Large Goal".to_string(),
            goal: (1..=40)
                .map(|idx| format!("Goal line {idx}"))
                .collect::<Vec<_>>()
                .join("\n"),
            steps,
            ..Default::default()
        }));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let chat_area = rendered_chat_area(&model);
    let mouse_column = chat_area.x.saturating_add(1);
    let mouse_row = chat_area.y.saturating_add(1);

    for _ in 0..400 {
        model.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: mouse_column,
            row: mouse_row,
            modifiers: KeyModifiers::NONE,
        });
    }

    let bottom_snapshot = render_task_view(&mut model);

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: mouse_column,
        row: mouse_row,
        modifiers: KeyModifiers::NONE,
    });

    let after_one_up = render_task_view(&mut model);

    assert_ne!(
        after_one_up, bottom_snapshot,
        "one upward scroll should move away from the clamped bottom view"
    );
}

#[test]
fn task_view_renders_visible_scrollbar_when_content_overflows() {
    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);

    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Large Goal".to_string(),
            goal: (1..=80)
                .map(|idx| format!("Goal line {idx}"))
                .collect::<Vec<_>>()
                .join("\n"),
            steps: (1..=40)
                .map(|idx| task::GoalRunStep {
                    id: format!("step-{idx}"),
                    title: format!("Step {idx}"),
                    instructions: format!("Line {idx}A\nLine {idx}B\nLine {idx}C"),
                    order: idx,
                    ..Default::default()
                })
                .collect(),
            events: (1..=40)
                .map(|idx| task::GoalRunEvent {
                    id: format!("event-{idx}"),
                    message: format!("Event {idx}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("task view render should succeed");

    let chat_area = rendered_chat_area(&model);
    let buffer = terminal.backend().buffer();
    let plain = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .map(|y| {
            buffer
                .cell((chat_area.x + chat_area.width - 1, y))
                .map(|cell| cell.symbol().to_string())
                .unwrap_or_default()
        })
        .collect::<String>();

    assert!(
        plain.contains("│") || plain.contains("█"),
        "expected visible scrollbar in task view, got: {plain:?}"
    );
}

#[test]
fn work_context_view_renders_visible_scrollbar_when_preview_overflows() {
    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);
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
            entries: vec![task::WorkContextEntry {
                path: "/tmp/demo.txt".to_string(),
                is_text: true,
                ..Default::default()
            }],
        },
    ));
    model
        .tasks
        .reduce(task::TaskAction::FilePreviewReceived(task::FilePreview {
            path: "/tmp/demo.txt".to_string(),
            content: (1..=120)
                .map(|idx| format!("line {idx}"))
                .collect::<Vec<_>>()
                .join("\n"),
            truncated: false,
            is_text: true,
        }));
    model.tasks.reduce(task::TaskAction::SelectWorkPath {
        thread_id: "thread-1".to_string(),
        path: Some("/tmp/demo.txt".to_string()),
    });
    model
        .sidebar
        .reduce(SidebarAction::SwitchTab(SidebarTab::Files));
    model.main_pane_view = MainPaneView::WorkContext;

    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("work-context render should succeed");

    let chat_area = rendered_chat_area(&model);
    let buffer = terminal.backend().buffer();
    let plain = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .map(|y| {
            buffer
                .cell((chat_area.x + chat_area.width - 1, y))
                .map(|cell| cell.symbol().to_string())
                .unwrap_or_default()
        })
        .collect::<String>();

    assert!(
        plain.contains("│") || plain.contains("█"),
        "expected visible scrollbar in work-context view, got: {plain:?}"
    );
}

#[test]
fn file_preview_view_renders_visible_scrollbar_when_preview_overflows() {
    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);
    model
        .tasks
        .reduce(task::TaskAction::FilePreviewReceived(task::FilePreview {
            path: "/tmp/demo.txt".to_string(),
            content: (1..=120)
                .map(|idx| format!("line {idx}"))
                .collect::<Vec<_>>()
                .join("\n"),
            truncated: false,
            is_text: true,
        }));
    model.main_pane_view = MainPaneView::FilePreview(ChatFilePreviewTarget {
        path: "/tmp/demo.txt".to_string(),
        repo_root: None,
        repo_relative_path: None,
    });

    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("file preview render should succeed");

    let chat_area = rendered_chat_area(&model);
    let buffer = terminal.backend().buffer();
    let plain = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .map(|y| {
            buffer
                .cell((chat_area.x + chat_area.width - 1, y))
                .map(|cell| cell.symbol().to_string())
                .unwrap_or_default()
        })
        .collect::<String>();

    assert!(
        plain.contains("│") || plain.contains("█"),
        "expected visible scrollbar in file preview view, got: {plain:?}"
    );
}

#[test]
fn file_preview_view_renders_image_preview_instead_of_binary_placeholder() {
    use base64::Engine as _;

    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);

    let image_path =
        std::env::temp_dir().join(format!("tamux-preview-image-{}.png", uuid::Uuid::new_v4()));
    std::fs::write(
        &image_path,
        base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO0pGfcAAAAASUVORK5CYII=")
            .expect("fixture PNG should decode"),
    )
    .expect("fixture PNG should write");

    model
        .tasks
        .reduce(task::TaskAction::FilePreviewReceived(task::FilePreview {
            path: image_path.display().to_string(),
            content: String::new(),
            truncated: false,
            is_text: false,
        }));
    model.main_pane_view = MainPaneView::FilePreview(ChatFilePreviewTarget {
        path: image_path.display().to_string(),
        repo_root: None,
        repo_relative_path: None,
    });

    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("file preview render should succeed");

    let chat_area = rendered_chat_area(&model);
    let buffer = terminal.backend().buffer();
    let plain = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .flat_map(|y| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width))
                .filter_map(move |x| buffer.cell((x, y)).map(|cell| cell.symbol().to_string()))
        })
        .collect::<String>();

    assert!(
        plain.contains("Image:"),
        "expected image preview header, got: {plain:?}"
    );
    assert!(
        !plain.contains("Binary file preview is not available."),
        "expected image preview instead of binary placeholder, got: {plain:?}"
    );
}

#[test]
fn file_preview_view_uses_available_height_for_image_preview() {
    let mut model = build_model();
    model.width = 100;
    model.height = 40;
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);

    let image_path =
        std::env::temp_dir().join(format!("tamux-preview-image-{}.png", uuid::Uuid::new_v4()));
    image::RgbaImage::from_fn(1024, 1024, |x, y| {
        image::Rgba([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8, 255])
    })
    .save(&image_path)
    .expect("fixture PNG should write");

    model
        .tasks
        .reduce(task::TaskAction::FilePreviewReceived(task::FilePreview {
            path: image_path.display().to_string(),
            content: String::new(),
            truncated: false,
            is_text: false,
        }));
    model.main_pane_view = MainPaneView::FilePreview(ChatFilePreviewTarget {
        path: image_path.display().to_string(),
        repo_root: None,
        repo_relative_path: None,
    });

    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("file preview render should succeed");
    assert!(
        crate::widgets::image_preview::process_preview_jobs_for_path_until_stable_for_tests(
            &image_path.display().to_string(),
        ),
        "expected initial file preview render to queue and complete image preview work",
    );
    terminal
        .draw(|frame| model.render(frame))
        .expect("file preview rerender should use cached image preview");
    assert!(
        crate::widgets::image_preview::process_preview_jobs_for_path_until_stable_for_tests(
            &image_path.display().to_string(),
        ),
        "expected rerendered file preview to settle any resized image preview work",
    );
    terminal
        .draw(|frame| model.render(frame))
        .expect("file preview final render should use settled image preview");

    let chat_area = rendered_chat_area(&model);
    let buffer = terminal.backend().buffer();
    let image_rows = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .filter(|&y| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).any(|x| {
                buffer
                    .cell((x, y))
                    .map(|cell| cell.symbol() == "▀")
                    .unwrap_or(false)
            })
        })
        .count();

    assert!(
        image_rows > 20,
        "expected image preview to use more than the old 20-row cap, got {image_rows} rows"
    );
}

#[test]
fn goal_composer_mission_control_preflight_renders_stable_sections() {
    let mut model = build_model();
    model.width = 100;
    model.height = 40;
    model.open_new_goal_view();
    model.goal_mission_control.prompt_text = "Ship the next release".to_string();
    model.goal_mission_control.preset_source_label = "Previous goal snapshot".to_string();
    model.goal_mission_control.save_as_default_pending = true;

    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("goal mission control preflight render should succeed");

    let buffer = terminal.backend().buffer();
    let rendered = (0..model.height)
        .map(|y| {
            (0..model.width)
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        rendered.contains("MISSION CONTROL"),
        "preflight should render the Mission Control title"
    );
    assert!(
        rendered.contains("Goal prompt"),
        "preflight should render a prompt section"
    );
    assert!(
        rendered.contains("Main model"),
        "preflight should render the selected main model section"
    );
    assert!(
        rendered.contains("Agent roster"),
        "preflight should render the goal agent roster section"
    );
    assert!(
        rendered.contains("Preset source"),
        "preflight should render the preset source label"
    );
    assert!(
        rendered.contains("Save as default"),
        "preflight should render the save-as-default toggle state"
    );
    assert!(
        rendered.contains("Ship the next release"),
        "preflight should include the current goal prompt"
    );
    assert!(
        !rendered.contains("Describe the goal in the input below"),
        "old composer helper text should no longer be rendered"
    );
}

#[test]
fn mission_control_roster_render_shows_live_now_and_pending_next_turn_labels() {
    let mut model = build_model();
    model.width = 100;
    model.height = 40;
    model.main_pane_view = MainPaneView::GoalComposer;
    model.goal_mission_control = goal_mission_control::GoalMissionControlState::from_main_assignment(
        task::GoalAgentAssignment {
            role_id: amux_protocol::AGENT_ID_SWAROG.to_string(),
            enabled: true,
            provider: "openai".to_string(),
            model: "gpt-5.4".to_string(),
            reasoning_effort: Some("medium".to_string()),
            inherit_from_main: false,
        },
        vec![
            task::GoalAgentAssignment {
                role_id: amux_protocol::AGENT_ID_SWAROG.to_string(),
                enabled: true,
                provider: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: Some("medium".to_string()),
                inherit_from_main: false,
            },
            task::GoalAgentAssignment {
                role_id: "reviewer".to_string(),
                enabled: true,
                provider: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: Some("low".to_string()),
                inherit_from_main: false,
            },
        ],
        "Goal runtime roster",
    );
    model.goal_mission_control.runtime_goal_run_id = Some("goal-1".to_string());
    model.goal_mission_control.active_runtime_assignment_index = Some(0);
    model.goal_mission_control.pending_role_assignments = Some(vec![
        task::GoalAgentAssignment {
            role_id: amux_protocol::AGENT_ID_SWAROG.to_string(),
            enabled: true,
            provider: "openai".to_string(),
            model: "gpt-5.4".to_string(),
            reasoning_effort: Some("medium".to_string()),
            inherit_from_main: false,
        },
        task::GoalAgentAssignment {
            role_id: "reviewer".to_string(),
            enabled: true,
            provider: "openai".to_string(),
            model: "gpt-5.4-mini".to_string(),
            reasoning_effort: Some("low".to_string()),
            inherit_from_main: false,
        },
    ]);
    model.goal_mission_control.pending_runtime_apply_modes = vec![
        None,
        Some(goal_mission_control::RuntimeAssignmentApplyMode::NextTurn),
    ];

    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("mission control roster render should succeed");

    let buffer = terminal.backend().buffer();
    let rendered = (0..model.height)
        .map(|y| {
            (0..model.width)
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("live now"), "{rendered}");
    assert!(rendered.contains("pending next turn"), "{rendered}");
    assert!(rendered.contains("reviewer"), "{rendered}");
    assert!(rendered.contains("gpt-5.4-mini"), "{rendered}");
}
