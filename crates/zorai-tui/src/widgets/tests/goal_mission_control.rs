use super::*;
use crate::state::goal_mission_control::GoalMissionControlState;
use crate::state::task::GoalAgentAssignment;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn sample_state() -> GoalMissionControlState {
    GoalMissionControlState::from_main_assignment(
        GoalAgentAssignment {
            role_id: zorai_protocol::AGENT_ID_SWAROG.to_string(),
            enabled: true,
            provider: "openai".to_string(),
            model: "gpt-5.4".to_string(),
            reasoning_effort: Some("medium".to_string()),
            inherit_from_main: false,
        },
        vec![GoalAgentAssignment {
            role_id: zorai_protocol::AGENT_ID_SWAROG.to_string(),
            enabled: true,
            provider: "openai".to_string(),
            model: "gpt-5.4".to_string(),
            reasoning_effort: Some("medium".to_string()),
            inherit_from_main: false,
        }],
        "Previous goal snapshot",
    )
}

fn render_plain_text(can_open_active_thread: bool) -> String {
    let state = sample_state();
    let area = Rect::new(0, 0, 90, 28);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("terminal should initialize");

    terminal
        .draw(|frame| {
            render_preflight(
                frame,
                area,
                &state,
                can_open_active_thread,
                &ThemeTokens::default(),
            );
        })
        .expect("mission control widget render should succeed");

    let buffer = terminal.backend().buffer();
    (area.y..area.y.saturating_add(area.height))
        .map(|y| {
            (area.x..area.x.saturating_add(area.width))
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_return_banner_plain_text(area: Rect) -> String {
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("terminal should initialize");

    terminal
        .draw(|frame| {
            render_return_to_goal_banner(frame, area, &ThemeTokens::default());
        })
        .expect("return-to-goal banner render should succeed");

    let buffer = terminal.backend().buffer();
    (area.y..area.y.saturating_add(area.height))
        .map(|y| {
            (area.x..area.x.saturating_add(area.width))
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn mission_control_thread_router_widget_renders_open_active_thread_control() {
    let plain = render_plain_text(true);

    assert!(plain.contains("Thread Router"), "{plain}");
    assert!(plain.contains("Open active thread"), "{plain}");
    assert!(plain.contains("Ctrl+O"), "{plain}");
}

#[test]
fn mission_control_thread_router_widget_renders_unavailable_status_when_disabled() {
    let plain = render_plain_text(false);

    assert!(plain.contains("Thread routing is unavailable"), "{plain}");
}

#[test]
fn mission_control_thread_router_widget_hit_test_tracks_open_active_thread_button() {
    let area = Rect::new(0, 0, 90, 28);
    let router_area = thread_router_area(area).expect("thread router area should resolve");
    let button =
        open_active_thread_button_area(router_area).expect("open-active-thread button expected");

    let hit = hit_test(
        area,
        Position::new(button.x.saturating_add(1), button.y),
        true,
    );

    assert_eq!(hit, Some(GoalMissionControlHitTarget::OpenActiveThread));
}

#[test]
fn mission_control_thread_router_widget_hit_test_ignores_disabled_open_thread_control() {
    let area = Rect::new(0, 0, 90, 28);
    let router_area = thread_router_area(area).expect("thread router area should resolve");
    let button =
        open_active_thread_button_area(router_area).expect("open-active-thread button expected");

    let hit = hit_test(
        area,
        Position::new(button.x.saturating_add(1), button.y),
        false,
    );

    assert_eq!(hit, None);
}

#[test]
fn mission_control_return_banner_renders_return_to_goal_affordance() {
    let plain = render_return_banner_plain_text(Rect::new(0, 0, 72, 3));

    assert!(plain.contains("Return to goal"), "{plain}");
    assert!(plain.contains("source goal run"), "{plain}");
}
