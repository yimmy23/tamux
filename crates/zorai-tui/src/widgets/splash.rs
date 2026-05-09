use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::theme::ThemeTokens;

pub fn render(frame: &mut Frame, area: Rect, theme: &ThemeTokens) {
    let height = area.height as usize;
    let mut lines: Vec<Line> = Vec::new();

    let pad_top = height / 3;
    for _ in 0..pad_top {
        lines.push(Line::raw(""));
    }

    let logo = Line::from(vec![
        Span::styled("\u{2591}", Style::default().fg(Color::Yellow)),
        Span::styled("\u{2592}", Style::default().fg(Color::LightYellow)),
        Span::styled("\u{2593}", Style::default().fg(Color::LightYellow)),
        Span::styled("\u{2588}", Style::default().fg(Color::Yellow)),
        Span::styled(" Z o r a i ", theme.accent_primary),
        Span::styled("\u{2588}", Style::default().fg(Color::Yellow)),
        Span::styled("\u{2593}", Style::default().fg(Color::LightYellow)),
        Span::styled("\u{2592}", Style::default().fg(Color::LightYellow)),
        Span::styled("\u{2591}", Style::default().fg(Color::Yellow)),
    ]);
    lines.push(logo);

    lines.push(Line::from(Span::styled(
        "plan \u{00b7} solve \u{00b7} ship",
        theme.fg_dim,
    )));

    lines.push(Line::raw(""));

    lines.push(Line::from(Span::styled(
        "Type a prompt to begin, or",
        theme.fg_dim,
    )));
    lines.push(Line::from(vec![
        Span::styled("Ctrl+P", theme.accent_primary),
        Span::styled(" to open command palette", theme.fg_dim),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Ctrl+T", theme.accent_primary),
        Span::styled(" to pick a thread", theme.fg_dim),
    ]));

    while lines.len() < height {
        lines.push(Line::raw(""));
    }
    lines.truncate(height);

    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::render;
    use crate::theme::ThemeTokens;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render_to_lines(width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render(frame, frame.area(), &ThemeTokens::default()))
            .expect("splash render");
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn splash_renders_brand_logo() {
        let rows = render_to_lines(60, 20);
        let body = rows.join("\n");
        assert!(
            body.contains("Z o r a i"),
            "expected brand logo in splash output, got:\n{body}"
        );
    }

    #[test]
    fn splash_renders_command_palette_hint() {
        let rows = render_to_lines(60, 20);
        let body = rows.join("\n");
        assert!(
            body.contains("Ctrl+P"),
            "expected Ctrl+P hint in splash output, got:\n{body}"
        );
    }
}
