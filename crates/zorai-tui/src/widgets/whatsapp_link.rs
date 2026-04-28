use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

use crate::state::modal::ModalState;
use crate::theme::ThemeTokens;

pub fn render(frame: &mut Frame, area: Rect, modal: &ModalState, theme: &ThemeTokens) {
    let block = Block::default()
        .title(" WhatsApp Link ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.accent_primary);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let link = modal.whatsapp_link();
    let mut lines: Vec<Line<'_>> = vec![
        Line::from(Span::styled(
            link.status_text().to_string(),
            theme.fg_active,
        )),
        Line::raw(""),
    ];

    if let Some(phone) = link.phone() {
        lines.push(Line::from(Span::styled(
            format!("Phone: {phone}"),
            theme.accent_success,
        )));
        lines.push(Line::raw(""));
    }

    if let Some(err) = link.last_error() {
        lines.push(Line::from(Span::styled(
            format!("Last error: {err}"),
            theme.accent_danger,
        )));
        lines.push(Line::raw(""));
    }

    if let Some(expires_at_ms) = link.expires_at_ms() {
        let _ = expires_at_ms;
        lines.push(Line::from(Span::styled(
            "QR code is time-limited and will refresh automatically.",
            theme.fg_dim,
        )));
        lines.push(Line::raw(""));
    }

    if let Some(qr) = link.ascii_qr() {
        for line in qr.lines() {
            lines.push(Line::from(Span::styled(line.to_string(), theme.fg_active)));
        }
    } else {
        let waiting_copy = if link.last_error().is_some() {
            "QR unavailable due to error — relink required"
        } else {
            "Waiting for QR payload…"
        };
        lines.push(Line::from(Span::styled(waiting_copy, theme.fg_dim)));
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "Only allowed WhatsApp numbers will be forwarded and receive replies.",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("Esc", theme.fg_active),
        Span::styled(" close + stop linking", theme.fg_dim),
    ]));
    lines.push(Line::from(vec![
        Span::styled("c", theme.fg_active),
        Span::styled(" cancel + stop linking", theme.fg_dim),
    ]));

    frame.render_widget(
        Paragraph::new(lines)
            .style(theme.fg_active)
            .wrap(Wrap { trim: false }),
        inner,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn whatsapp_link_modal_uses_human_qr_expiry_copy() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        let mut modal = ModalState::new();
        modal.set_whatsapp_link_qr("██ QR".to_string(), Some(42));

        terminal
            .draw(|frame| render(frame, frame.area(), &modal, &ThemeTokens::default()))
            .expect("render should not panic");

        let buffer = terminal.backend().buffer();
        let text = (0..24)
            .map(|y| {
                (0..80)
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("QR code is time-limited and will refresh automatically."));
        assert!(!text.contains("QR expires at:"));
        assert!(!text.contains(" ms"));
    }
}
