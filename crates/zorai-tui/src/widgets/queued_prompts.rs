use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::app::{QueuedPrompt, QueuedPromptAction};
use crate::theme::ThemeTokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuedPromptsHitTarget {
    Row(usize),
    Action {
        message_index: usize,
        action: QueuedPromptAction,
    },
}

#[derive(Debug, Clone, Copy)]
struct RowLayout {
    index: usize,
    y: u16,
    actions: [(QueuedPromptAction, Rect); 4],
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    prompts: &[QueuedPrompt],
    selected_index: usize,
    selected_action: QueuedPromptAction,
    current_tick: u64,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" QUEUED MESSAGES ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 || inner.width < 32 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let summary = if prompts.is_empty() {
        "No queued messages".to_string()
    } else {
        format!("{} queued  •  opens after tools finish", prompts.len())
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(summary, theme.fg_dim))),
        chunks[0],
    );

    for row in visible_rows(chunks[1], prompts, selected_index, current_tick) {
        let is_selected = row.index == selected_index;
        let prompt = &prompts[row.index];
        let prefix = if is_selected { "> " } else { "  " };
        let preview = truncate_preview(
            &prompt.display_text(),
            row.actions[0].1.x.saturating_sub(chunks[1].x + 4) as usize,
        );
        let style = if is_selected {
            theme.fg_active
        } else {
            theme.fg_dim
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(prefix, theme.accent_primary),
                Span::styled(format!("{}. ", row.index + 1), style),
                Span::styled(preview, style),
            ])),
            Rect::new(chunks[1].x, row.y, chunks[1].width, 1),
        );

        for (action, rect) in row.actions {
            let label = action_label(prompt, action, current_tick);
            let chip = format!("[{label}]");
            let chip_style = if is_selected && action == selected_action {
                Style::default()
                    .bg(Color::Indexed(178))
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else if action == QueuedPromptAction::Delete {
                theme.accent_danger
            } else if action == QueuedPromptAction::Copy && prompt.is_copied(current_tick) {
                theme.accent_success
            } else {
                theme.accent_primary
            };
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(chip, chip_style))),
                rect,
            );
        }
    }

    let hints = Line::from(vec![
        Span::styled("↑↓", theme.fg_active),
        Span::styled(" message  ", theme.fg_dim),
        Span::styled("←→", theme.fg_active),
        Span::styled(" action  ", theme.fg_dim),
        Span::styled("E", theme.fg_active),
        Span::styled(" expand  ", theme.fg_dim),
        Span::styled("Enter", theme.fg_active),
        Span::styled(" run  ", theme.fg_dim),
        Span::styled("Esc", theme.fg_active),
        Span::styled(" close", theme.fg_dim),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[2]);
}

pub fn hit_test(
    area: Rect,
    prompts: &[QueuedPrompt],
    selected_index: usize,
    current_tick: u64,
    position: Position,
) -> Option<QueuedPromptsHitTarget> {
    let inner = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .inner(area);
    if inner.height < 3 || inner.width < 32 {
        return None;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    for row in visible_rows(chunks[1], prompts, selected_index, current_tick) {
        if position.y != row.y {
            continue;
        }
        for (action, rect) in row.actions {
            if rect.width > 0
                && position.x >= rect.x
                && position.x < rect.x.saturating_add(rect.width)
            {
                return Some(QueuedPromptsHitTarget::Action {
                    message_index: row.index,
                    action,
                });
            }
        }
        if position.x >= chunks[1].x && position.x < chunks[1].x.saturating_add(chunks[1].width) {
            return Some(QueuedPromptsHitTarget::Row(row.index));
        }
    }

    None
}

fn visible_rows(
    area: Rect,
    prompts: &[QueuedPrompt],
    selected_index: usize,
    current_tick: u64,
) -> Vec<RowLayout> {
    let item_count = prompts.len();
    let list_height = area.height as usize;
    let (visible_start, visible_len) =
        crate::widgets::thread_picker::visible_window(selected_index, item_count, list_height);

    (0..visible_len)
        .map(|offset| {
            let index = visible_start + offset;
            RowLayout {
                index,
                y: area.y + offset as u16,
                actions: action_rects(area, prompts, index, current_tick, area.y + offset as u16),
            }
        })
        .collect()
}

fn action_rects(
    area: Rect,
    prompts: &[QueuedPrompt],
    index: usize,
    current_tick: u64,
    y: u16,
) -> [(QueuedPromptAction, Rect); 4] {
    let mut actions = [
        (QueuedPromptAction::Delete, Rect::default()),
        (QueuedPromptAction::Copy, Rect::default()),
        (QueuedPromptAction::SendNow, Rect::default()),
        (QueuedPromptAction::Expand, Rect::default()),
    ];
    let prompt = &prompts[index];
    let mut x = area.x.saturating_add(area.width);
    for entry in &mut actions {
        let label = format!("[{}]", action_label(prompt, entry.0, current_tick));
        let width = label.chars().count() as u16;
        x = x.saturating_sub(width);
        entry.1 = Rect::new(x, y, width, 1);
        x = x.saturating_sub(1);
    }

    [
        (QueuedPromptAction::Expand, actions[3].1),
        (QueuedPromptAction::SendNow, actions[2].1),
        (QueuedPromptAction::Copy, actions[1].1),
        (QueuedPromptAction::Delete, actions[0].1),
    ]
}

fn action_label(
    prompt: &QueuedPrompt,
    action: QueuedPromptAction,
    current_tick: u64,
) -> &'static str {
    match action {
        QueuedPromptAction::Expand => "expand",
        QueuedPromptAction::SendNow if prompt.force_send => "send now!",
        QueuedPromptAction::SendNow => "send now",
        QueuedPromptAction::Copy if prompt.is_copied(current_tick) => "copied",
        QueuedPromptAction::Copy => "copy",
        QueuedPromptAction::Delete => "delete",
    }
}

fn truncate_preview(text: &str, max_chars: usize) -> String {
    let single_line = text.lines().next().unwrap_or("").trim();
    if single_line.chars().count() <= max_chars {
        return single_line.to_string();
    }
    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }
    format!(
        "{}...",
        single_line
            .chars()
            .take(max_chars.saturating_sub(3))
            .collect::<String>()
    )
}
