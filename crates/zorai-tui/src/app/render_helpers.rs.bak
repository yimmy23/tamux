use super::*;
use ratatui::style::Modifier;

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
        Line::from(Span::styled("  Goal Runner", theme.accent_primary)),
        Line::from(vec![
            Span::styled("  t                ", theme.fg_active),
            Span::styled("Toggle live todos section", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  l                ", theme.fg_active),
            Span::styled("Toggle timeline section", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  f                ", theme.fg_active),
            Span::styled("Toggle files section", theme.fg_dim),
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
            Span::styled("Open goal picker / create goal", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /goal            ", theme.fg_active),
            Span::styled("Open new goal composer", theme.fg_dim),
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
            Span::styled("  /explain         ", theme.fg_active),
            Span::styled("Explain latest goal-run decision", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /diverge         ", theme.fg_active),
            Span::styled("Prepare divergent session command", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /diverge-start   ", theme.fg_active),
            Span::styled("Start divergent session", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("  /diverge-get     ", theme.fg_active),
            Span::styled("Fetch divergent session payload", theme.fg_dim),
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

pub(super) fn render_goal_composer(frame: &mut Frame, area: Rect, theme: &ThemeTokens) {
    use ratatui::widgets::{Paragraph, Wrap};

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(area);

    let title = Paragraph::new(Line::from(Span::styled(
        "Goal Runner",
        theme.accent_primary.add_modifier(Modifier::BOLD),
    )));
    frame.render_widget(title, layout[0]);

    let content = vec![
        Line::from(Span::styled(
            "Describe the goal in the input below and press Enter.",
            theme.fg_active,
        )),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Examples", theme.fg_dim.add_modifier(Modifier::BOLD)),
            Span::raw(": "),
            Span::styled(
                "create a migration plan, implement auth, refactor a module, investigate a bug",
                theme.fg_dim,
            ),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Esc", theme.fg_active),
            Span::styled(" back to conversation  ", theme.fg_dim),
            Span::styled("Ctrl+G", theme.fg_active),
            Span::styled(" goal picker", theme.fg_dim),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(content).wrap(Wrap { trim: false }),
        layout[1],
    );
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

pub(super) fn render_openai_auth_modal(
    frame: &mut Frame,
    area: Rect,
    auth_url: Option<&str>,
    status_text: Option<&str>,
    theme: &ThemeTokens,
) {
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

    let block = Block::default()
        .title(" CHATGPT LOGIN ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_primary);

    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let mut lines = vec![
        Line::from(
            status_text
                .unwrap_or("Open this URL in your browser to complete ChatGPT authentication."),
        ),
        Line::raw(""),
        Line::from(auth_url.unwrap_or("No login URL available.")),
    ];
    if auth_url.is_some() {
        lines.push(Line::raw(""));
        lines.push(Line::from(
            "Press Enter or O to open the browser, or C to copy the link.",
        ));
    }

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, layout[0]);

    let hints = Line::from(vec![
        Span::styled("Enter/O", theme.fg_active),
        Span::styled(" open  ", theme.fg_dim),
        Span::styled("C", theme.fg_active),
        Span::styled(" copy  ", theme.fg_dim),
        Span::styled("Esc", theme.fg_active),
        Span::styled(" close", theme.fg_dim),
    ]);
    frame.render_widget(Paragraph::new(hints), layout[1]);
}

pub(super) fn chat_action_confirm_button_bounds(area: Rect) -> Option<(Rect, Rect)> {
    use ratatui::widgets::{Block, BorderType, Borders};

    if area.width < 10 || area.height < 3 {
        return None;
    }

    let inner = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .inner(area);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let button_row = layout[1];
    let confirm_width = "[Confirm]".len() as u16 + 2;
    let cancel_width = "[Cancel]".len() as u16 + 2;
    let total_width = confirm_width.saturating_add(1).saturating_add(cancel_width);
    let start_x = button_row
        .x
        .saturating_add(button_row.width.saturating_sub(total_width) / 2);
    let confirm = Rect::new(start_x, button_row.y, confirm_width, 1);
    let cancel = Rect::new(
        start_x.saturating_add(confirm_width).saturating_add(1),
        button_row.y,
        cancel_width,
        1,
    );
    Some((confirm, cancel))
}

pub(super) fn render_chat_action_confirm_modal(
    frame: &mut Frame,
    area: Rect,
    pending: Option<(&str, usize)>,
    accept_selected: bool,
    theme: &ThemeTokens,
) {
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

    let block = Block::default()
        .title(" CONFIRM ACTION ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);

    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let body = if let Some((action, message_number)) = pending {
        vec![
            Line::from(format!(
                "Proceed with {action} for message {message_number}?"
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "This action requires explicit confirmation to avoid accidental clicks.",
                theme.fg_dim,
            )),
        ]
    } else {
        vec![Line::from(Span::styled(
            "No pending message action.",
            theme.fg_dim,
        ))]
    };
    frame.render_widget(Paragraph::new(body).wrap(Wrap { trim: false }), layout[0]);

    let confirm_style = if accept_selected {
        theme.accent_primary
    } else {
        theme.fg_dim
    };
    let cancel_style = if accept_selected {
        theme.fg_dim
    } else {
        theme.accent_primary
    };
    let action_line = Line::from(vec![
        Span::styled(" [Confirm] ", confirm_style),
        Span::raw(" "),
        Span::styled(" [Cancel] ", cancel_style),
    ]);
    frame.render_widget(Paragraph::new(action_line), layout[1]);
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
