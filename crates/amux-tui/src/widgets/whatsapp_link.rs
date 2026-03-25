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
        lines.push(Line::from(Span::styled(
            format!("QR expires at: {expires_at_ms} ms"),
            theme.fg_dim,
        )));
        lines.push(Line::raw(""));
    }

    if let Some(qr) = link.ascii_qr() {
        for line in qr.lines() {
            lines.push(Line::from(Span::styled(line.to_string(), theme.fg_active)));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "Waiting for QR payload…",
            theme.fg_dim,
        )));
    }

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
