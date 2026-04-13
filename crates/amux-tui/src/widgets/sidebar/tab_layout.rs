use super::*;

pub(super) fn tab_hit_test(tab_area: Rect, mouse_x: u16) -> Option<SidebarTab> {
    let cells = tab_cells(tab_area);
    if mouse_x >= cells[0].x && mouse_x < cells[0].x.saturating_add(cells[0].width) {
        Some(SidebarTab::Todos)
    } else if mouse_x >= cells[1].x && mouse_x < cells[1].x.saturating_add(cells[1].width) {
        Some(SidebarTab::Files)
    } else {
        None
    }
}

pub(super) fn tab_cells(tab_area: Rect) -> [Rect; 2] {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(tab_area);
    [chunks[0], chunks[1]]
}

pub(super) fn tab_label(tab: SidebarTab) -> &'static str {
    match tab {
        SidebarTab::Files => " Files ",
        SidebarTab::Todos => " Todos ",
    }
}

#[allow(dead_code)]
pub(super) fn tab_hint_line(theme: &ThemeTokens) -> Line<'static> {
    Line::from(vec![
        Span::styled("[", theme.accent_primary),
        Span::styled(" todos ", theme.fg_dim),
        Span::styled("]", theme.accent_primary),
        Span::styled("  ", theme.fg_dim),
        Span::styled("[", theme.accent_primary),
        Span::styled(" files ", theme.fg_dim),
        Span::styled("]", theme.accent_primary),
        Span::styled("  click tab", theme.fg_dim),
    ])
}
