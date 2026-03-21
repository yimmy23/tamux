use ratatui::prelude::*;
use ratatui::text::Span;
use ratatui::widgets::Tabs;

use crate::state::sidebar::{SidebarItemTarget, SidebarState, SidebarTab};
use crate::state::task::TaskState;
use crate::theme::ThemeTokens;

pub enum SidebarHitTarget {
    Tab(SidebarTab),
    Item(SidebarItemTarget),
}

fn content_inner(area: Rect) -> Rect {
    area
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    sidebar: &SidebarState,
    tasks: &TaskState,
    theme: &ThemeTokens,
    _focused: bool,
) {
    // Tab titles
    let active_tab = sidebar.active_tab();
    let tab_index = match active_tab {
        crate::state::sidebar::SidebarTab::Tasks => 0,
        crate::state::sidebar::SidebarTab::Subagents => 1,
    };

    let tabs = Tabs::new(vec!["Tasks", "Subagents"])
        .select(tab_index)
        .style(theme.fg_dim)
        .highlight_style(theme.fg_active)
        .divider(Span::styled(" | ", theme.fg_dim));

    let inner = content_inner(area);

    if inner.height < 2 {
        return;
    }

    // Split inner into tabs line + body
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    frame.render_widget(tabs, chunks[0]);

    // Body -- routed to real widgets based on active tab
    match active_tab {
        crate::state::sidebar::SidebarTab::Tasks => {
            super::task_tree::render(frame, chunks[1], tasks, sidebar, theme);
        }
        crate::state::sidebar::SidebarTab::Subagents => {
            super::subagents::render(frame, chunks[1], tasks, sidebar, theme);
        }
    }
}

pub fn hit_test(
    area: Rect,
    sidebar: &SidebarState,
    tasks: &TaskState,
    mouse: Position,
) -> Option<SidebarHitTarget> {
    let inner = content_inner(area);
    if inner.height < 2
        || mouse.x < inner.x
        || mouse.x >= inner.x.saturating_add(inner.width)
        || mouse.y < inner.y
        || mouse.y >= inner.y.saturating_add(inner.height)
    {
        return None;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    if mouse.y == chunks[0].y {
        let rel_x = mouse.x.saturating_sub(chunks[0].x);
        let split = chunks[0].width / 2;
        if rel_x < split {
            return Some(SidebarHitTarget::Tab(SidebarTab::Tasks));
        }
        if rel_x < chunks[0].width {
            return Some(SidebarHitTarget::Tab(SidebarTab::Subagents));
        }
        return None;
    }

    let row = mouse.y.saturating_sub(chunks[1].y) as usize;
    let body_height = chunks[1].height as usize;
    match sidebar.active_tab() {
        SidebarTab::Tasks => {
            crate::widgets::task_tree::row_target_at(tasks, sidebar, body_height, row)
                .map(SidebarHitTarget::Item)
        }
        SidebarTab::Subagents => {
            crate::widgets::subagents::row_target_at(tasks, sidebar, body_height, row)
                .map(SidebarHitTarget::Item)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::sidebar::SidebarState;
    use crate::state::task::TaskState;

    #[test]
    fn sidebar_handles_empty_state() {
        let sidebar = SidebarState::new();
        let tasks = TaskState::new();
        let _theme = ThemeTokens::default();
        assert_eq!(
            sidebar.active_tab(),
            crate::state::sidebar::SidebarTab::Tasks
        );
        assert!(tasks.tasks().is_empty());
    }
}
