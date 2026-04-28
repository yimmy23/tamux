use super::*;

pub(super) fn render_goal_mission_control_preflight(
    frame: &mut Frame,
    area: Rect,
    state: &crate::state::goal_mission_control::GoalMissionControlState,
    can_open_active_thread: bool,
    theme: &ThemeTokens,
) {
    widgets::goal_mission_control::render_preflight(
        frame,
        area,
        state,
        can_open_active_thread,
        theme,
    );
}
