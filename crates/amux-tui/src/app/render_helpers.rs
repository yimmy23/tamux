use super::*;

pub(super) fn render_effort_picker(
    frame: &mut Frame,
    area: Rect,
    modal: &modal::ModalState,
    config: &config::ConfigState,
    theme: &ThemeTokens,
) {
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};

    let block = Block::default()
        .title(" EFFORT ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let efforts = [
        ("", "Off"),
        ("low", "Low"),
        ("medium", "Medium"),
        ("high", "High"),
        ("xhigh", "Extra High"),
    ];

    let cursor = modal.picker_cursor();
    let current = config.reasoning_effort();
    let items: Vec<ListItem> = efforts
        .iter()
        .enumerate()
        .map(|(i, (value, label))| {
            let is_current = *value == current;
            let marker = if is_current { "\u{25cf} " } else { "  " };
            let is_selected = i == cursor;

            if is_selected {
                ListItem::new(Line::from(vec![
                    Span::raw("> "),
                    Span::raw(marker),
                    Span::raw(*label),
                ]))
                .style(Style::default().bg(Color::Indexed(178)).fg(Color::Black))
            } else {
                let style = if is_current {
                    theme.accent_primary
                } else {
                    theme.fg_dim
                };
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::raw(marker),
                    Span::styled(*label, style),
                ]))
            }
        })
        .collect();

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    frame.render_widget(List::new(items), inner_chunks[0]);

    let hints = Line::from(vec![
        Span::styled("↑↓", theme.fg_active),
        Span::styled(" nav  ", theme.fg_dim),
        Span::styled("Enter", theme.fg_active),
        Span::styled(" sel  ", theme.fg_dim),
        Span::styled("Esc", theme.fg_active),
        Span::styled(" close", theme.fg_dim),
    ]);
    frame.render_widget(Paragraph::new(hints), inner_chunks[1]);
}

pub(super) fn render_help_modal(frame: &mut Frame, area: Rect, theme: &ThemeTokens) {
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

    let block = Block::default()
        .title(" KEYBOARD SHORTCUTS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);

    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::raw(""),
        Line::from(Span::styled("  Navigation", theme.accent_primary)),
        Line::from(vec![
            Span::styled("  Tab / Shift+Tab  ", theme.fg_active),
            Span::styled("Cycle focus: Chat → Sidebar → Input", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+P           ", theme.fg_active),
            Span::styled("Open command palette", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+T           ", theme.fg_active),
            Span::styled("Open thread picker", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+B           ", theme.fg_active),
            Span::styled("Toggle sidebar", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+G           ", theme.fg_active),
            Span::styled("Toggle Goal Runner view", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /                ", theme.fg_active),
            Span::styled("Open command palette (from any focus)", theme.fg_dim),
        ]),
        Line::raw(""),
        Line::from(Span::styled("  Chat (when focused)", theme.accent_primary)),
        Line::from(vec![
            Span::styled("  ↑ / ↓            ", theme.fg_active),
            Span::styled("Select message", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  PgUp / PgDn      ", theme.fg_active),
            Span::styled("Scroll chat", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+D / Ctrl+U  ", theme.fg_active),
            Span::styled("Half-page scroll", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Home / End       ", theme.fg_active),
            Span::styled("Scroll to top / bottom", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  r                ", theme.fg_active),
            Span::styled("Toggle reasoning on selected message", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  e / Enter        ", theme.fg_active),
            Span::styled("Toggle tool call expansion", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  c                ", theme.fg_active),
            Span::styled("Copy selected message to clipboard", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Esc              ", theme.fg_active),
            Span::styled("Clear selection", theme.fg_dim),
        ]),
        Line::raw(""),
        Line::from(Span::styled("  Input", theme.accent_primary)),
        Line::from(vec![
            Span::styled("  Enter            ", theme.fg_active),
            Span::styled("Send message", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+Enter      ", theme.fg_active),
            Span::styled("Insert newline", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  ← → ↑ ↓         ", theme.fg_active),
            Span::styled("Move cursor in textarea", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+Backspace   ", theme.fg_active),
            Span::styled("Delete word backwards", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+W           ", theme.fg_active),
            Span::styled("Delete word backwards", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+U           ", theme.fg_active),
            Span::styled("Clear input line", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+Z           ", theme.fg_active),
            Span::styled("Undo", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+Y           ", theme.fg_active),
            Span::styled("Redo", theme.fg_dim),
        ]),
        Line::raw(""),
        Line::from(Span::styled("  Streaming", theme.accent_primary)),
        Line::from(vec![
            Span::styled("  Esc              ", theme.fg_active),
            Span::styled("Show stop prompt (first press)", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  Esc Esc          ", theme.fg_active),
            Span::styled("Force stop stream (double press within 2s)", theme.fg_dim),
        ]),
        Line::raw(""),
        Line::from(Span::styled("  Error", theme.accent_primary)),
        Line::from(vec![
            Span::styled("  Ctrl+E           ", theme.fg_active),
            Span::styled("Open last error viewer", theme.fg_dim),
        ]),
        Line::raw(""),
        Line::from(Span::styled("  Commands (/)", theme.accent_primary)),
        Line::from(vec![
            Span::styled("  /settings        ", theme.fg_active),
            Span::styled("Open settings panel", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /provider        ", theme.fg_active),
            Span::styled("Switch LLM provider", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /model           ", theme.fg_active),
            Span::styled("Switch model", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /effort          ", theme.fg_active),
            Span::styled("Set reasoning effort", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /thread          ", theme.fg_active),
            Span::styled("Pick conversation thread", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /new             ", theme.fg_active),
            Span::styled("New conversation", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /goals           ", theme.fg_active),
            Span::styled("Open Goal Runner view", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /attach <path>   ", theme.fg_active),
            Span::styled("Attach file to message", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /view            ", theme.fg_active),
            Span::styled("Cycle transcript mode", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /help            ", theme.fg_active),
            Span::styled("This help screen", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /quit            ", theme.fg_active),
            Span::styled("Exit TUI", theme.fg_dim),
        ]),
        Line::raw(""),
        Line::from(Span::styled("  Press Esc to close", theme.fg_dim)),
    ];

    let paragraph = Paragraph::new(lines)
        .scroll((0, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

pub(super) fn render_error_modal(
    frame: &mut Frame,
    area: Rect,
    last_error: Option<&str>,
    theme: &ThemeTokens,
) {
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

    let block = Block::default()
        .title(" LAST ERROR ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_danger);

    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let content = last_error.unwrap_or("No error details available.");
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let paragraph = Paragraph::new(content).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, layout[0]);

    let hints = Line::from(vec![
        Span::styled("Esc", theme.fg_active),
        Span::styled(" close  ", theme.fg_dim),
        Span::styled("Ctrl+E", theme.fg_active),
        Span::styled(" toggle", theme.fg_dim),
    ]);
    frame.render_widget(Paragraph::new(hints), layout[1]);
}

pub(super) fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
