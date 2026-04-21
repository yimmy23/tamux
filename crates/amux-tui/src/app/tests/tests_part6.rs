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

fn goal_sidebar_model() -> TuiModel {
    let mut model = build_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Goal Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-2".to_string(),
        title: "Child Task Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-exec".to_string(),
        title: "Execution Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.tasks.reduce(task::TaskAction::TaskListReceived(vec![
        task::AgentTask {
            id: "task-1".to_string(),
            title: "Child Task One".to_string(),
            thread_id: Some("thread-1".to_string()),
            goal_run_id: Some("goal-1".to_string()),
            created_at: 10,
            ..Default::default()
        },
        task::AgentTask {
            id: "task-2".to_string(),
            title: "Child Task Two".to_string(),
            thread_id: Some("thread-2".to_string()),
            goal_run_id: Some("goal-1".to_string()),
            created_at: 20,
            ..Default::default()
        },
    ]));
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Goal Title".to_string(),
            thread_id: Some("thread-1".to_string()),
            root_thread_id: Some("thread-1".to_string()),
            active_thread_id: Some("thread-exec".to_string()),
            execution_thread_ids: vec!["thread-exec".to_string()],
            goal: "goal definition body".to_string(),
            status: Some(task::GoalRunStatus::Running),
            current_step_title: Some("Implement".to_string()),
            runtime_assignment_list: vec![task::GoalAgentAssignment {
                role_id: "implementer".to_string(),
                provider: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: Some("high".to_string()),
                enabled: true,
                inherit_from_main: false,
            }],
            planner_owner_profile: Some(task::GoalRuntimeOwnerProfile {
                agent_label: "Planner".to_string(),
                provider: "openai".to_string(),
                model: "gpt-5.4-mini".to_string(),
                reasoning_effort: Some("medium".to_string()),
            }),
            current_step_owner_profile: Some(task::GoalRuntimeOwnerProfile {
                agent_label: "Executor".to_string(),
                provider: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: Some("high".to_string()),
            }),
            child_task_ids: vec!["task-1".to_string(), "task-2".to_string()],
            last_error: Some("upstream returned an empty error body".to_string()),
            steps: vec![
                task::GoalRunStep {
                    id: "step-1".to_string(),
                    title: "Plan".to_string(),
                    order: 0,
                    instructions: "Ground the user's background before planning".to_string(),
                    summary: Some("Gather current experience and constraints first.".to_string()),
                    ..Default::default()
                },
                task::GoalRunStep {
                    id: "step-2".to_string(),
                    title: "Implement".to_string(),
                    order: 1,
                    instructions: "Collect and index sources into a reusable inventory".to_string(),
                    summary: Some("Build the source-backed inventory and checkpoints.".to_string()),
                    ..Default::default()
                },
                task::GoalRunStep {
                    id: "step-3".to_string(),
                    title: "Verify".to_string(),
                    order: 2,
                    instructions: "Check proof coverage and package the final entry plan".to_string(),
                    ..Default::default()
                },
            ],
            dossier: Some(task::GoalRunDossier {
                projection_state: "in_progress".to_string(),
                summary: Some(
                    "Execution is split into source collection and onboarding package delivery."
                        .to_string(),
                ),
                latest_resume_decision: Some(task::GoalResumeDecisionRecord {
                    action: "advance".to_string(),
                    reason_code: "step_completed".to_string(),
                    reason: Some("goal step completed successfully".to_string()),
                    details: vec!["advance via step_completed".to_string()],
                    projection_state: "in_progress".to_string(),
                    ..Default::default()
                }),
                units: vec![task::GoalDeliveryUnitRecord {
                    id: "unit-1".to_string(),
                    title: "Collect and index ingenix.ai sources".to_string(),
                    status: "in_progress".to_string(),
                    execution_binding: "builtin:swarog".to_string(),
                    verification_binding: "builtin:swarog".to_string(),
                    summary: Some("Build a source-backed inventory in markdown and csv.".to_string()),
                    proof_checks: vec![task::GoalProofCheckRecord {
                        id: "pc-1".to_string(),
                        title: "Verify sources cover company info".to_string(),
                        state: "pending".to_string(),
                        summary: Some("Need official pages plus supporting evidence.".to_string()),
                        ..Default::default()
                    }],
                    report: Some(task::GoalRunReportRecord {
                        summary: "Source inventory is in progress".to_string(),
                        state: "in_progress".to_string(),
                        notes: vec!["Capture official pages first.".to_string()],
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                report: Some(task::GoalRunReportRecord {
                    summary: "Overall goal remains in progress".to_string(),
                    state: "in_progress".to_string(),
                    notes: vec!["Approval still pending for final package.".to_string()],
                    ..Default::default()
                }),
                ..Default::default()
            }),
            events: vec![task::GoalRunEvent {
                id: "event-1".to_string(),
                phase: "execution".to_string(),
                message: "queued child task for goal step".to_string(),
                details: Some("Collect and index ingenix.ai sources -> task_123".to_string()),
                step_index: Some(1),
                todo_snapshot: vec![task::TodoItem {
                    id: "todo-impl-1".to_string(),
                    content: "Verify sources cover company info".to_string(),
                    status: Some(task::TodoStatus::Pending),
                    step_index: Some(1),
                    position: 0,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }));
    model
        .tasks
        .reduce(task::TaskAction::GoalRunCheckpointsReceived {
            goal_run_id: "goal-1".to_string(),
            checkpoints: vec![
                task::GoalRunCheckpointSummary {
                    id: "checkpoint-1".to_string(),
                    checkpoint_type: "plan".to_string(),
                    step_index: Some(1),
                    context_summary_preview: Some("Checkpoint for Implement".to_string()),
                    ..Default::default()
                },
                task::GoalRunCheckpointSummary {
                    id: "checkpoint-2".to_string(),
                    checkpoint_type: "note".to_string(),
                    context_summary_preview: Some("Loose checkpoint".to_string()),
                    ..Default::default()
                },
            ],
        });
    model.tasks.reduce(task::TaskAction::ThreadTodosReceived {
        thread_id: "thread-1".to_string(),
        items: vec![
            task::TodoItem {
                id: "todo-1".to_string(),
                content: "Draft outline".to_string(),
                status: Some(task::TodoStatus::InProgress),
                step_index: Some(0),
                position: 0,
                ..Default::default()
            },
            task::TodoItem {
                id: "todo-2".to_string(),
                content: "Verify sources".to_string(),
                status: Some(task::TodoStatus::Pending),
                step_index: Some(0),
                position: 1,
                ..Default::default()
            },
            task::TodoItem {
                id: "todo-3".to_string(),
                content: "Run checks".to_string(),
                status: Some(task::TodoStatus::Pending),
                step_index: Some(2),
                position: 0,
                ..Default::default()
            },
        ],
    });
    model.tasks.reduce(task::TaskAction::WorkContextReceived(
        task::ThreadWorkContext {
            thread_id: "thread-1".to_string(),
            entries: vec![
                task::WorkContextEntry {
                    path: "/tmp/plan.md".to_string(),
                    goal_run_id: Some("goal-1".to_string()),
                    is_text: true,
                    ..Default::default()
                },
                task::WorkContextEntry {
                    path: "/tmp/report.md".to_string(),
                    goal_run_id: Some("goal-1".to_string()),
                    is_text: true,
                    ..Default::default()
                },
            ],
        },
    ));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: Some("step-1".to_string()),
    });
    model.focus = FocusArea::Sidebar;
    model
}

fn mission_control_thread_router_model(
    active_thread_id: Option<&str>,
    root_thread_id: Option<&str>,
) -> TuiModel {
    let mut model = build_model();
    let thread_ids = [active_thread_id, root_thread_id];
    for thread_id in thread_ids.into_iter().flatten() {
        model
            .chat
            .reduce(chat::ChatAction::ThreadCreated {
                thread_id: thread_id.to_string(),
                title: format!("Thread {thread_id}"),
            });
        model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
            chat::AgentThread {
                id: thread_id.to_string(),
                title: format!("Thread {thread_id}"),
                messages: vec![chat::AgentMessage {
                    id: Some(format!("message-{thread_id}")),
                    role: chat::MessageRole::Assistant,
                    content: format!("Conversation for {thread_id}"),
                    ..Default::default()
                }],
                loaded_message_end: 1,
                total_message_count: 1,
                ..Default::default()
            },
        ));
    }

    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
        id: "goal-1".to_string(),
        title: "Goal Title".to_string(),
        thread_id: root_thread_id.map(str::to_string),
        root_thread_id: root_thread_id.map(str::to_string),
        active_thread_id: active_thread_id.map(str::to_string),
        goal: "goal definition body".to_string(),
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
        step_id: Some("step-2".to_string()),
    });
    model.focus = FocusArea::Chat;
    model.open_new_goal_view();
    model.status_line.clear();
    model
}

fn render_chat_plain(model: &mut TuiModel) -> String {
    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("render should succeed");

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

fn goal_workspace_click_targets(chat_area: Rect) -> (Position, Position, Position) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(1)])
        .split(chat_area);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(32),
            Constraint::Min(24),
        ])
        .split(layout[1]);
    let plan = Block::default().borders(Borders::ALL).inner(columns[0]);
    let timeline = Block::default().borders(Borders::ALL).inner(columns[1]);
    let details = Block::default().borders(Borders::ALL).inner(columns[2]);

    (
        Position::new(plan.x.saturating_add(1), plan.y),
        Position::new(timeline.x.saturating_add(1), timeline.y),
        Position::new(details.x.saturating_add(1), details.y),
    )
}

fn find_goal_workspace_hit_position(
    model: &TuiModel,
    expected: widgets::goal_workspace::GoalWorkspaceHitTarget,
) -> Position {
    let chat_area = rendered_chat_area(model);
    (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                let hit = match &model.main_pane_view {
                    MainPaneView::Task(SidebarItemTarget::GoalRun { goal_run_id, .. }) => {
                        widgets::goal_workspace::hit_test(
                            chat_area,
                            &model.tasks,
                            goal_run_id,
                            &model.goal_workspace,
                            pos,
                        )
                    }
                    _ => None,
                };
                (hit == Some(expected.clone())).then_some(pos)
            })
        })
        .expect("expected goal workspace hit target should be clickable")
}

#[test]
fn goal_sidebar_defaults_to_steps_on_model_init() {
    let model = build_model();
    assert_eq!(model.goal_sidebar.active_tab(), GoalSidebarTab::Steps);
}

#[test]
fn goal_sidebar_tab_cycling_stays_within_goal_tabs() {
    let mut state = GoalSidebarState::new();
    assert_eq!(state.active_tab(), GoalSidebarTab::Steps);

    state.cycle_tab_left();
    assert_eq!(state.active_tab(), GoalSidebarTab::Steps);

    state.cycle_tab_right();
    assert_eq!(state.active_tab(), GoalSidebarTab::Checkpoints);

    state.cycle_tab_right();
    assert_eq!(state.active_tab(), GoalSidebarTab::Tasks);

    state.cycle_tab_right();
    assert_eq!(state.active_tab(), GoalSidebarTab::Files);

    state.cycle_tab_right();
    assert_eq!(state.active_tab(), GoalSidebarTab::Files);

    state.cycle_tab_left();
    assert_eq!(state.active_tab(), GoalSidebarTab::Tasks);
}

#[test]
fn goal_sidebar_row_selection_clamps_per_active_tab() {
    let mut state = GoalSidebarState::new();

    state.select_row(4, 3);
    assert_eq!(state.selected_row(), 2);

    state.cycle_tab_right();
    state.select_row(1, 0);
    assert_eq!(state.selected_row(), 0);

    state.cycle_tab_right();
    state.select_row(3, 2);
    assert_eq!(state.selected_row(), 1);

    state.cycle_tab_right();
    state.select_row(7, 1);
    assert_eq!(state.selected_row(), 0);
}

#[test]
fn goal_run_render_uses_full_workspace_without_legacy_goal_sidebar_tabs() {
    let mut model = goal_sidebar_model();

    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("goal run render should succeed");

    let buffer = terminal.backend().buffer();
    let chat_area = rendered_chat_area(&model);

    let chat_plain = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .map(|y| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width))
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        chat_plain.contains("Plan")
            && chat_plain.contains("Run timeline")
            && chat_plain.contains("Goal")
            && chat_plain.contains("Prompt"),
        "expected full goal workspace content in main pane, got: {chat_plain}"
    );
    assert!(
        !chat_plain.contains("Checkpoints")
            && !chat_plain.contains("Tasks"),
        "expected legacy goal sidebar labels to be absent, got: {chat_plain}"
    );
    assert!(
        model.pane_layout().sidebar.is_none(),
        "goal workspace should take the full content width"
    );
}

#[test]
fn goal_workspace_keyboard_navigation_uses_plan_state() {
    let mut model = goal_sidebar_model();
    model.focus = FocusArea::Chat;

    let handled = model.handle_key(KeyCode::Right, KeyModifiers::NONE);
    assert!(!handled);
    assert!(model.goal_workspace.is_step_expanded("step-1"));

    let handled = model.handle_key(KeyCode::Down, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.goal_workspace.selected_plan_row(), 3);
    assert_eq!(
        model.goal_workspace.selected_plan_item(),
        Some(&goal_workspace::GoalPlanSelection::Todo {
            step_id: "step-1".to_string(),
            todo_id: "todo-1".to_string(),
        })
    );

    let handled = model.handle_key(KeyCode::Left, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.goal_workspace.selected_plan_row(), 2);
    assert_eq!(
        model.goal_workspace.selected_plan_item(),
        Some(&goal_workspace::GoalPlanSelection::Step {
            step_id: "step-1".to_string(),
        })
    );
}

#[test]
fn goal_workspace_tab_cycles_between_internal_panes_before_input() {
    let mut model = goal_sidebar_model();
    model.focus = FocusArea::Chat;

    assert_eq!(
        model.goal_workspace.focused_pane(),
        goal_workspace::GoalWorkspacePane::Plan
    );

    let handled = model.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Chat);
    assert_eq!(
        model.goal_workspace.focused_pane(),
        goal_workspace::GoalWorkspacePane::Timeline
    );

    let handled = model.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Chat);
    assert_eq!(
        model.goal_workspace.focused_pane(),
        goal_workspace::GoalWorkspacePane::Details
    );

    let handled = model.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Chat);
    assert_eq!(
        model.goal_workspace.focused_pane(),
        goal_workspace::GoalWorkspacePane::CommandBar
    );

    let handled = model.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Input);

    let handled = model.handle_key(KeyCode::BackTab, KeyModifiers::SHIFT);
    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Chat);
    assert_eq!(
        model.goal_workspace.focused_pane(),
        goal_workspace::GoalWorkspacePane::CommandBar
    );
}

#[test]
fn selected_goal_step_workspace_click_syncs_main_goal_detail_selection() {
    let mut model = goal_sidebar_model();
    let click = find_goal_workspace_hit_position(
        &model,
        widgets::goal_workspace::GoalWorkspaceHitTarget::PlanStep("step-2".to_string()),
    );
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.focus, FocusArea::Chat);
    assert_eq!(model.goal_workspace.selected_plan_row(), 3);
    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::GoalRun {
            goal_run_id,
            step_id: Some(step_id),
        }) if goal_run_id == "goal-1" && step_id == "step-2"
    ));
}

#[test]
fn goal_workspace_mouse_clicks_focus_timeline_and_details_panes() {
    let mut model = goal_sidebar_model();
    model.focus = FocusArea::Input;
    if let Some(run) = model.tasks.goal_run_by_id_mut("goal-1") {
        run.events = vec![
            task::GoalRunEvent {
                id: "event-1".to_string(),
                message: "first".to_string(),
                timestamp: 10,
                ..Default::default()
            },
            task::GoalRunEvent {
                id: "event-2".to_string(),
                message: "second".to_string(),
                timestamp: 20,
                ..Default::default()
            },
        ];
    }

    let chat_area = rendered_chat_area(&model);
    let (_, timeline, details) = goal_workspace_click_targets(chat_area);

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: timeline.x,
        row: timeline.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: timeline.x,
        row: timeline.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.focus, FocusArea::Chat);
    assert_eq!(
        model.goal_workspace.focused_pane(),
        goal_workspace::GoalWorkspacePane::Timeline
    );

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: details.x,
        row: details.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: details.x,
        row: details.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.focus, FocusArea::Chat);
    assert_eq!(
        model.goal_workspace.focused_pane(),
        goal_workspace::GoalWorkspacePane::Details
    );
}

#[test]
fn goal_workspace_mode_tabs_are_clickable_and_keyboard_focusable() {
    let mut model = goal_sidebar_model();
    model.focus = FocusArea::Chat;
    model.goal_workspace
        .set_focused_pane(goal_workspace::GoalWorkspacePane::CommandBar);

    let handled = model.handle_key(KeyCode::Right, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(
        model.goal_workspace.mode(),
        goal_workspace::GoalWorkspaceMode::Progress
    );

    let handled = model.handle_key(KeyCode::Char('4'), KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(
        model.goal_workspace.mode(),
        goal_workspace::GoalWorkspaceMode::Threads
    );

    let handled = model.handle_key(KeyCode::Char('5'), KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(
        model.goal_workspace.mode(),
        goal_workspace::GoalWorkspaceMode::NeedsAttention
    );

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(
        model.goal_workspace.focused_pane(),
        goal_workspace::GoalWorkspacePane::Plan
    );

    let mut model = goal_sidebar_model();
    let click = find_goal_workspace_hit_position(
        &model,
        widgets::goal_workspace::GoalWorkspaceHitTarget::ModeTab(
            goal_workspace::GoalWorkspaceMode::ActiveAgent,
        ),
    );
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.focus, FocusArea::Chat);
    assert_eq!(
        model.goal_workspace.focused_pane(),
        goal_workspace::GoalWorkspacePane::CommandBar
    );
    assert_eq!(
        model.goal_workspace.mode(),
        goal_workspace::GoalWorkspaceMode::ActiveAgent
    );
}

#[test]
fn goal_workspace_goal_mode_file_click_opens_work_context_preview() {
    let mut model = goal_sidebar_model();
    let click = find_goal_workspace_hit_position(
        &model,
        widgets::goal_workspace::GoalWorkspaceHitTarget::DetailFile("/tmp/plan.md".to_string()),
    );

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });

    assert!(matches!(model.main_pane_view, MainPaneView::WorkContext));
    assert_eq!(model.tasks.selected_work_path("thread-1"), Some("/tmp/plan.md"));
    assert_eq!(model.status_line, "/tmp/plan.md");
}

#[test]
fn goal_workspace_escape_from_goal_file_preview_restores_goal_run() {
    let mut model = goal_sidebar_model();
    let click = find_goal_workspace_hit_position(
        &model,
        widgets::goal_workspace::GoalWorkspaceHitTarget::DetailFile("/tmp/plan.md".to_string()),
    );

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });

    let handled = model.handle_key(KeyCode::Esc, KeyModifiers::NONE);

    assert!(!handled);
    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::GoalRun { ref goal_run_id, .. })
            if goal_run_id == "goal-1"
    ));
}

#[test]
fn goal_workspace_goal_mode_restores_old_goal_sections() {
    let mut model = goal_sidebar_model();

    let plain = render_chat_plain(&mut model);

    assert!(plain.contains("Step Actions"), "{plain}");
    assert!(!plain.contains("Controls"), "{plain}");
    assert!(plain.contains("Related Tasks"), "{plain}");
    assert!(plain.contains("Execution Dossier"), "{plain}");
    assert!(plain.contains("Goal Prompt"), "{plain}");
    assert!(plain.contains("Main agent"), "{plain}");
    assert!(plain.contains("Ground the user's background"), "{plain}");
}

#[test]
fn goal_workspace_plan_prompt_toggle_is_clickable_and_keyboard_expandable() {
    let mut model = goal_sidebar_model();

    let click = find_goal_workspace_hit_position(
        &model,
        widgets::goal_workspace::GoalWorkspaceHitTarget::PlanPromptToggle,
    );
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });

    assert!(model.goal_workspace.prompt_expanded());
    assert!(render_chat_plain(&mut model).contains("goal definition body"));

    let mut model = goal_sidebar_model();
    model.focus = FocusArea::Chat;
    model.goal_workspace.set_selected_plan_row(0);
    model.goal_workspace
        .set_selected_plan_item(Some(goal_workspace::GoalPlanSelection::PromptToggle));

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(!handled);
    assert!(model.goal_workspace.prompt_expanded());
}

#[test]
fn goal_workspace_plan_main_thread_row_opens_thread_with_return_to_goal_path() {
    let mut model = goal_sidebar_model();
    model.focus = FocusArea::Chat;
    model.goal_workspace.set_selected_plan_row(1);
    model.goal_workspace.set_selected_plan_item(Some(
        goal_workspace::GoalPlanSelection::MainThread {
            thread_id: "thread-exec".to_string(),
        },
    ));

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(!handled);
    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert_eq!(model.chat.active_thread_id(), Some("thread-exec"));
    assert!(model.mission_control_return_to_goal_target().is_some());
}

#[test]
fn goal_workspace_step_footer_actions_are_clickable() {
    let mut model = goal_sidebar_model();
    let click = find_goal_workspace_hit_position(
        &model,
        widgets::goal_workspace::GoalWorkspaceHitTarget::FooterAction(
            widgets::goal_workspace::GoalWorkspaceAction::OpenActions,
        ),
    );

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.modal.top(), Some(modal::ModalKind::GoalStepActionPicker));
}

#[test]
fn goal_workspace_progress_mode_restores_checkpoint_and_dossier_views() {
    let mut model = goal_sidebar_model();
    model.goal_workspace
        .set_mode(goal_workspace::GoalWorkspaceMode::Progress);

    let plain = render_chat_plain(&mut model);

    assert!(plain.contains("Execution Dossier"), "{plain}");
    assert!(plain.contains("Checkpoints"), "{plain}");
    assert!(plain.contains("Checkpoint for Implement"), "{plain}");
}

#[test]
fn goal_workspace_active_agent_mode_restores_assignments_and_threads() {
    let mut model = goal_sidebar_model();
    model.goal_workspace
        .set_mode(goal_workspace::GoalWorkspaceMode::ActiveAgent);

    let plain = render_chat_plain(&mut model);

    assert!(plain.contains("Executor"), "{plain}");
    assert!(plain.contains("Planner"), "{plain}");
    assert!(plain.contains("implementer"), "{plain}");
    assert!(plain.contains("thread-exec"), "{plain}");
}

#[test]
fn goal_workspace_threads_mode_lists_clickable_threads() {
    let mut model = goal_sidebar_model();
    model.goal_workspace
        .set_mode(goal_workspace::GoalWorkspaceMode::Threads);

    let plain = render_chat_plain(&mut model);

    assert!(plain.contains("Threads"), "{plain}");
    assert!(plain.contains("Executor"), "{plain}");
    assert!(plain.contains("thread-exec"), "{plain}");
    assert!(plain.contains("Planner"), "{plain}");
    assert!(plain.contains("thread-2"), "{plain}");
}

#[test]
fn goal_workspace_threads_mode_enter_opens_selected_thread() {
    let mut model = goal_sidebar_model();
    model.focus = FocusArea::Chat;
    model.goal_workspace
        .set_mode(goal_workspace::GoalWorkspaceMode::Threads);
    model.goal_workspace
        .set_focused_pane(goal_workspace::GoalWorkspacePane::Timeline);
    model.goal_workspace.set_selected_timeline_row(0);

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!handled);
    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert_eq!(model.chat.active_thread_id(), Some("thread-exec"));
}

#[test]
fn goal_workspace_needs_attention_mode_restores_non_empty_attention_surface() {
    let mut model = goal_sidebar_model();
    model.goal_workspace
        .set_mode(goal_workspace::GoalWorkspaceMode::NeedsAttention);

    let plain = render_chat_plain(&mut model);

    assert!(plain.contains("Approvals"), "{plain}");
    assert!(plain.contains("Last error"), "{plain}");
    assert!(plain.contains("upstream returned an empty error"), "{plain}");
}

#[test]
fn goal_sidebar_task_open_renders_back_to_goal_and_escape_returns_to_goal() {
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

    let mut model = goal_sidebar_model();
    model.activate_goal_sidebar_tab(GoalSidebarTab::Tasks);
    model.select_goal_sidebar_row(0);

    assert!(model.handle_goal_sidebar_enter());
    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::Task { ref task_id }) if task_id == "task-1"
    ));

    let plain = render_task_view(&mut model);
    assert!(plain.contains("Back to goal"), "{plain}");

    let handled = model.handle_key(KeyCode::Esc, KeyModifiers::NONE);
    assert!(!handled);
    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::GoalRun { ref goal_run_id, .. }) if goal_run_id == "goal-1"
    ));
}

#[test]
fn goal_sidebar_task_back_to_goal_row_click_returns_to_goal() {
    let mut model = goal_sidebar_model();
    model.activate_goal_sidebar_tab(GoalSidebarTab::Tasks);
    model.select_goal_sidebar_row(0);

    assert!(model.handle_goal_sidebar_enter());
    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::Task { ref task_id }) if task_id == "task-1"
    ));

    let chat_area = rendered_chat_area(&model);
    let back_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height)).find_map(|row| {
        (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
            let pos = Position::new(column, row);
            match widgets::task_view::hit_test(
                chat_area,
                &model.tasks,
                match &model.main_pane_view {
                    MainPaneView::Task(target) => target,
                    _ => return None,
                },
                &model.theme,
                model.task_view_scroll,
                model.task_show_live_todos,
                model.task_show_timeline,
                model.task_show_files,
                pos,
            ) {
                Some(widgets::task_view::TaskViewHitTarget::BackToGoal) => Some(pos),
                _ => None,
            }
        })
    })
    .expect("task view should expose a clickable back-to-goal row");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: back_pos.x,
        row: back_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: back_pos.x,
        row: back_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::GoalRun { ref goal_run_id, .. }) if goal_run_id == "goal-1"
    ));
}

#[test]
fn mission_control_thread_router_open_active_thread_prefers_active_thread_id() {
    let mut model =
        mission_control_thread_router_model(Some("thread-active"), Some("thread-root"));

    let handled = model.handle_key(KeyCode::Char('o'), KeyModifiers::CONTROL);

    assert!(!handled);
    assert_eq!(model.chat.active_thread_id(), Some("thread-active"));
    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert!(render_chat_plain(&mut model).contains("Return to goal"));
}

#[test]
fn mission_control_thread_router_fallback_to_root_thread_sets_status() {
    let mut model = mission_control_thread_router_model(None, Some("thread-root"));

    let handled = model.handle_key(KeyCode::Char('o'), KeyModifiers::CONTROL);

    assert!(!handled);
    assert_eq!(model.chat.active_thread_id(), Some("thread-root"));
    assert_eq!(
        model.status_line,
        "Opened root goal thread as fallback because no active goal thread was available"
    );
}

#[test]
fn mission_control_thread_router_ignores_open_thread_shortcut_when_unavailable() {
    let mut model = mission_control_thread_router_model(None, None);

    let handled = model.handle_key(KeyCode::Char('o'), KeyModifiers::CONTROL);

    assert!(!handled);
    assert!(matches!(model.main_pane_view, MainPaneView::GoalComposer));
    assert_eq!(model.chat.active_thread_id(), None);
    assert_eq!(model.status_line, "");
}

#[test]
fn threads_return_to_goal_exposes_return_to_goal_affordance_when_opened_from_mission_control() {
    let mut model =
        mission_control_thread_router_model(Some("thread-active"), Some("thread-root"));

    let handled = model.handle_key(KeyCode::Char('o'), KeyModifiers::CONTROL);

    assert!(!handled);
    let plain = render_chat_plain(&mut model);
    assert!(plain.contains("Return to goal"), "{plain}");
}

#[test]
fn threads_return_to_goal_banner_keeps_conversation_mouse_targets_aligned() {
    let mut model =
        mission_control_thread_router_model(Some("thread-active"), Some("thread-root"));

    let handled = model.handle_key(KeyCode::Char('o'), KeyModifiers::CONTROL);
    assert!(!handled);

    let conversation_area = model
        .conversation_content_area()
        .expect("conversation content area should be available");
    let (selection_pos, expected_point) = (conversation_area.y
        ..conversation_area.y.saturating_add(conversation_area.height))
        .find_map(|row| {
            (conversation_area.x..conversation_area.x.saturating_add(conversation_area.width))
                .find_map(|column| {
                    let pos = Position::new(column, row);
                    widgets::chat::selection_point_from_mouse(
                        conversation_area,
                        &model.chat,
                        &model.theme,
                        model.tick_counter,
                        pos,
                    )
                    .map(|point| (pos, point))
                })
        })
        .expect("conversation content should expose a selectable point");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: selection_pos.x,
        row: selection_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.chat_drag_anchor_point, Some(expected_point));
}

#[test]
fn threads_return_to_goal_keyboard_restores_goal_run_and_step_selection() {
    let mut model =
        mission_control_thread_router_model(Some("thread-active"), Some("thread-root"));

    let handled = model.handle_key(KeyCode::Char('o'), KeyModifiers::CONTROL);
    assert!(!handled);

    let handled = model.handle_key(KeyCode::Char('b'), KeyModifiers::NONE);

    assert!(!handled);
    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::GoalRun {
            ref goal_run_id,
            step_id: Some(ref step_id),
        }) if goal_run_id == "goal-1" && step_id == "step-2"
    ));
}

#[test]
fn threads_return_to_goal_mouse_restores_goal_run_and_step_selection() {
    let mut model =
        mission_control_thread_router_model(Some("thread-active"), Some("thread-root"));

    let handled = model.handle_key(KeyCode::Char('o'), KeyModifiers::CONTROL);
    assert!(!handled);

    let button = model
        .conversation_return_to_goal_button_area()
        .expect("return-to-goal button should be rendered");
    let click_column = button.x.saturating_add(1);
    let click_row = button.y;

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click_column,
        row: click_row,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: click_column,
        row: click_row,
        modifiers: KeyModifiers::NONE,
    });

    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::GoalRun {
            ref goal_run_id,
            step_id: Some(ref step_id),
        }) if goal_run_id == "goal-1" && step_id == "step-2"
    ));
}

#[test]
fn goal_run_input_routes_prompt_to_active_goal_thread() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;

    for (thread_id, title) in [
        ("thread-user", "User Thread"),
        ("thread-goal", "Goal Thread"),
    ] {
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: thread_id.to_string(),
            title: title.to_string(),
        });
        model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
            chat::AgentThread {
                id: thread_id.to_string(),
                title: title.to_string(),
                ..Default::default()
            },
        ));
    }
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));
    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
        id: "goal-1".to_string(),
        title: "Goal Title".to_string(),
        thread_id: Some("thread-root".to_string()),
        active_thread_id: Some("thread-goal".to_string()),
        goal: "Ship release".to_string(),
        current_step_title: Some("Implement".to_string()),
        steps: vec![task::GoalRunStep {
            id: "step-1".to_string(),
            title: "Implement".to_string(),
            order: 0,
            ..Default::default()
        }],
        ..Default::default()
    }));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: Some("step-1".to_string()),
    });
    model.focus = FocusArea::Input;
    model.input.set_text("follow the current step");

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!handled);
    match cmd_rx.try_recv() {
        Ok(DaemonCommand::SendMessage { thread_id, content, .. }) => {
            assert_eq!(thread_id.as_deref(), Some("thread-goal"));
            assert_eq!(content, "follow the current step");
        }
        other => panic!("expected send-message command, got {other:?}"),
    }
    assert_eq!(model.chat.active_thread_id(), Some("thread-goal"));
    assert_eq!(
        model
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.last())
            .map(|message| message.content.as_str()),
        Some("follow the current step")
    );
}

#[test]
fn goal_run_input_blocks_plain_prompt_without_goal_thread_target() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;

    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        ..Default::default()
    }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));
    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
        id: "goal-1".to_string(),
        title: "Goal Title".to_string(),
        goal: "Ship release".to_string(),
        current_step_title: Some("Implement".to_string()),
        steps: vec![task::GoalRunStep {
            id: "step-1".to_string(),
            title: "Implement".to_string(),
            order: 0,
            ..Default::default()
        }],
        ..Default::default()
    }));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: Some("step-1".to_string()),
    });
    model.focus = FocusArea::Input;
    model.input.set_text("follow the current step");

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!handled);
    assert!(cmd_rx.try_recv().is_err(), "goal pane should not send plain text without a goal thread target");
    assert_eq!(
        model.status_line,
        "Goal input accepts only slash commands until an active goal thread is available"
    );
    assert_eq!(model.input.buffer(), "follow the current step");
    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
}

#[test]
fn esc_from_goal_run_keeps_user_in_goals_view() {
    let mut model = goal_sidebar_model();
    model.focus = FocusArea::Chat;
    model.tasks.reduce(task::TaskAction::SelectWorkPath {
        thread_id: "thread-1".to_string(),
        path: None,
    });

    let handled = model.handle_key(KeyCode::Esc, KeyModifiers::NONE);

    assert!(!handled);
    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::GoalRun { ref goal_run_id, .. }) if goal_run_id == "goal-1"
    ));
    assert_eq!(model.focus, FocusArea::Chat);
}

#[test]
fn goal_workspace_mouse_wheel_scrolls_plan_rows() {
    let mut model = goal_sidebar_model();
    if let Some(run) = model.tasks.goal_run_by_id_mut("goal-1") {
        run.steps = (1..=60)
            .map(|idx| task::GoalRunStep {
                id: format!("step-{idx}"),
                title: format!("Step {idx}"),
                order: idx - 1,
                ..Default::default()
            })
            .collect();
    }
    model.focus = FocusArea::Chat;

    let chat_area = model.pane_layout().chat;

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: chat_area.x.saturating_add(2),
        row: chat_area.y.saturating_add(6),
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.focus, FocusArea::Chat);
    assert!(model.pane_layout().sidebar.is_none());
    assert_eq!(model.goal_workspace.plan_scroll(), 3);
}

#[test]
fn goal_workspace_mouse_wheel_scrolls_timeline_and_details_rows() {
    let mut model = goal_sidebar_model();
    if let Some(run) = model.tasks.goal_run_by_id_mut("goal-1") {
        run.events = (0..30)
            .map(|idx| task::GoalRunEvent {
                id: format!("event-{idx}"),
                phase: "execution".to_string(),
                message: format!("event {idx} with wrapped timeline details"),
                details: Some(format!("details line for event {idx} that should wrap in the timeline panel")),
                step_index: Some(1),
                ..Default::default()
            })
            .collect();
    }
    model.tasks.reduce(task::TaskAction::WorkContextReceived(
        task::ThreadWorkContext {
            thread_id: "thread-1".to_string(),
            entries: (0..30)
                .map(|idx| task::WorkContextEntry {
                    path: format!("/tmp/file-{idx}.md"),
                    goal_run_id: Some("goal-1".to_string()),
                    is_text: true,
                    ..Default::default()
                })
                .collect(),
        },
    ));
    model.focus = FocusArea::Chat;

    let chat_area = model.pane_layout().chat;
    let (_, timeline, details) = goal_workspace_click_targets(chat_area);

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: timeline.x,
        row: timeline.y.saturating_add(2),
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(model.goal_workspace.timeline_scroll(), 3);

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: details.x,
        row: details.y.saturating_add(2),
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(model.goal_workspace.detail_scroll(), 3);
}

#[test]
fn goal_workspace_keyboard_navigation_auto_scrolls_timeline_and_details() {
    let mut model = goal_sidebar_model();
    if let Some(run) = model.tasks.goal_run_by_id_mut("goal-1") {
        run.events = (0..40)
            .map(|idx| task::GoalRunEvent {
                id: format!("event-{idx}"),
                phase: "execution".to_string(),
                message: format!("event {idx}"),
                details: Some(format!("details {idx}")),
                step_index: Some(1),
                ..Default::default()
            })
            .collect();
    }
    model.tasks.reduce(task::TaskAction::WorkContextReceived(
        task::ThreadWorkContext {
            thread_id: "thread-1".to_string(),
            entries: (0..40)
                .map(|idx| task::WorkContextEntry {
                    path: format!("/tmp/detail-{idx}.md"),
                    goal_run_id: Some("goal-1".to_string()),
                    is_text: true,
                    ..Default::default()
                })
                .collect(),
        },
    ));
    model.focus = FocusArea::Chat;
    model.goal_workspace
        .set_focused_pane(goal_workspace::GoalWorkspacePane::Timeline);

    for _ in 0..20 {
        let handled = model.handle_key(KeyCode::Down, KeyModifiers::NONE);
        assert!(!handled);
    }

    assert!(model.goal_workspace.selected_timeline_row() >= 20);
    assert!(model.goal_workspace.timeline_scroll() > 0);

    model.goal_workspace
        .set_focused_pane(goal_workspace::GoalWorkspacePane::Details);
    for _ in 0..20 {
        let handled = model.handle_key(KeyCode::Down, KeyModifiers::NONE);
        assert!(!handled);
    }

    assert!(model.goal_workspace.selected_detail_row() >= 20);
    assert!(model.goal_workspace.detail_scroll() > 0);
}

#[test]
fn goal_sidebar_blocks_hidden_pinned_shortcuts() {
    let mut model = goal_sidebar_model();
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Goal Thread".to_string(),
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
        .sidebar
        .reduce(SidebarAction::SwitchTab(SidebarTab::Pinned));

    let handled = model.handle_key(KeyCode::Char('k'), KeyModifiers::CONTROL);
    assert!(!handled);

    let handled = model.handle_key(KeyCode::Char('u'), KeyModifiers::NONE);
    assert!(!handled);

    assert!(model.chat.active_thread_has_pinned_messages());
    assert_eq!(model.focus, FocusArea::Input);
    assert_eq!(model.goal_sidebar.active_tab(), GoalSidebarTab::Steps);
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
    assert!(
        !model.chat.active_thread_has_pinned_messages(),
        "selected pin should disappear immediately from the sidebar state"
    );
    assert_eq!(model.sidebar.active_tab(), SidebarTab::Todos);
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
fn chat_unpin_updates_pinned_sidebar_without_waiting_for_thread_refresh() {
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

    model.unpin_message_for_compaction(0);

    assert!(
        !model.chat.active_thread_has_pinned_messages(),
        "chat-side unpin should clear the pinned sidebar immediately"
    );
    assert_eq!(model.sidebar.active_tab(), SidebarTab::Todos);
    let command = cmd_rx
        .try_recv()
        .expect("chat-side unpin should still notify the daemon");
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
