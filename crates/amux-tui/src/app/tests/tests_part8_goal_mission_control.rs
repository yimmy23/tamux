#[test]
fn goal_composer_add_agent_hotkey_creates_another_role_assignment() {
    let mut model = build_model();
    model.goal_mission_control =
        goal_mission_control::GoalMissionControlState::from_main_assignment(
            task::GoalAgentAssignment {
                role_id: amux_protocol::AGENT_ID_SWAROG.to_string(),
                enabled: true,
                provider: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: Some("medium".to_string()),
                inherit_from_main: false,
            },
            vec![task::GoalAgentAssignment {
                role_id: amux_protocol::AGENT_ID_SWAROG.to_string(),
                enabled: true,
                provider: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: Some("medium".to_string()),
                inherit_from_main: false,
            }],
            "Main agent inheritance",
        );
    model.main_pane_view = MainPaneView::GoalComposer;
    model.focus = FocusArea::Chat;

    let handled = model.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.goal_mission_control.role_assignments.len(), 2);
    assert_eq!(model.goal_mission_control.selected_runtime_assignment_index, 1);
}

#[test]
fn goal_composer_launch_sends_preflight_role_assignments() {
    let (_daemon_tx, daemon_rx) = std::sync::mpsc::channel();
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.main_pane_view = MainPaneView::GoalComposer;
    model.focus = FocusArea::Chat;
    model.goal_mission_control =
        goal_mission_control::GoalMissionControlState::from_main_assignment(
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
                    role_id: "planning".to_string(),
                    enabled: true,
                    provider: "openai".to_string(),
                    model: "gpt-5.4-mini".to_string(),
                    reasoning_effort: Some("high".to_string()),
                    inherit_from_main: false,
                },
            ],
            "Previous goal snapshot",
        );
    model.goal_mission_control.set_prompt_text("Ship the next release");

    model.start_goal_run_from_mission_control();

    match cmd_rx.try_recv() {
        Ok(DaemonCommand::DismissConciergeWelcome) => {}
        other => panic!("expected concierge dismissal before launch, got {other:?}"),
    }
    match cmd_rx.try_recv() {
        Ok(DaemonCommand::StartGoalRun {
            goal,
            launch_assignments,
            ..
        }) => {
            assert_eq!(goal, "Ship the next release");
            assert_eq!(launch_assignments.len(), 2);
            assert_eq!(launch_assignments[1].role_id, "planning");
        }
        other => panic!("expected start-goal-run command, got {other:?}"),
    }
}
