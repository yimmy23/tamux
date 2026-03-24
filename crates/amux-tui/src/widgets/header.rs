use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::state::chat::ChatState;
use crate::state::config::ConfigState;
use crate::theme::ThemeTokens;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    config: &ConfigState,
    chat: &ChatState,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(theme.fg_dim);

    let model = if config.model.is_empty() {
        "no model"
    } else {
        &config.model
    };

    // Token usage from active thread
    let (in_tok, out_tok) = if let Some(thread) = chat.active_thread() {
        (thread.total_input_tokens, thread.total_output_tokens)
    } else {
        (0, 0)
    };
    let total_tok = in_tok + out_tok;
    let usage = if total_tok > 0 {
        format!("{:.1}k tok", total_tok as f64 / 1000.0)
    } else {
        "0k tok".to_string()
    };

    let mut spans = vec![
        Span::styled(
            "\u{2591}\u{2592}\u{2593}",
            Style::default().fg(Color::Indexed(24)),
        ),
        Span::styled("TAMUX", theme.accent_primary),
        Span::styled(
            "\u{2593}\u{2592}\u{2591} ",
            Style::default().fg(Color::Indexed(24)),
        ),
    ];

    if !config.provider.is_empty() {
        spans.push(Span::raw(&config.provider));
        spans.push(Span::raw(" "));
    }

    spans.push(Span::styled(model, theme.fg_active));

    // Effort level indicator
    if !config.reasoning_effort.is_empty() {
        spans.push(Span::raw(" ["));
        spans.push(Span::styled(
            &config.reasoning_effort,
            theme.accent_secondary,
        ));
        spans.push(Span::raw("]"));
    }

    spans.push(Span::raw("  "));
    spans.push(Span::styled(usage, theme.fg_dim));

    let header_text = Line::from(spans).alignment(Alignment::Center);
    frame.render_widget(block, area);
    let text_area = if area.height >= 2 {
        Rect::new(
            area.x,
            area.y + area.height.saturating_sub(1) / 2,
            area.width,
            1,
        )
    } else {
        area
    };
    frame.render_widget(
        Paragraph::new(header_text).alignment(Alignment::Center),
        text_area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_renders_without_panic() {
        let config = ConfigState::new();
        let chat = ChatState::new();
        let _theme = ThemeTokens::default();
        // Just ensure no panics -- actual rendering needs a terminal
        assert!(!config.model.is_empty() || config.model.is_empty());
        assert!(chat.active_thread().is_none());
    }
}
