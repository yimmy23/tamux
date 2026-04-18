use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};

use crate::providers::{ProviderDef, PROVIDERS};
use crate::state::auth::AuthState;
use crate::state::config::ConfigState;
use crate::state::modal::ModalState;
use crate::theme::ThemeTokens;
use amux_shared::providers::PROVIDER_ID_CUSTOM;

pub fn available_provider_defs(auth: &AuthState) -> Vec<&'static ProviderDef> {
    PROVIDERS
        .iter()
        .filter(|provider| {
            provider.id == PROVIDER_ID_CUSTOM
                || auth
                    .entries
                    .iter()
                    .any(|entry| entry.authenticated && entry.provider_id == provider.id)
        })
        .collect()
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    modal: &ModalState,
    config: &ConfigState,
    auth: &AuthState,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" PROVIDER ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 {
        return;
    }

    // Split: list (flex) + hints (1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let cursor = modal.picker_cursor();
    let active_provider = config.provider();
    let providers = available_provider_defs(auth);

    if providers.is_empty() {
        frame.render_widget(
            Paragraph::new("No authenticated providers. Configure one in Auth.")
                .style(theme.fg_dim),
            chunks[0],
        );

        let hints = Line::from(vec![
            Span::raw(" "),
            Span::styled("Esc", theme.fg_active),
            Span::styled(" close", theme.fg_dim),
        ]);
        frame.render_widget(Paragraph::new(hints), chunks[1]);
        return;
    }

    let list_items: Vec<ListItem> = providers
        .iter()
        .enumerate()
        .map(|(i, provider_def)| {
            let is_selected = i == cursor;
            let is_active = provider_def.id == active_provider
                || provider_def.name.eq_ignore_ascii_case(active_provider);

            if is_selected {
                ListItem::new(Line::from(vec![
                    Span::raw(" > "),
                    Span::raw(provider_def.name),
                ]))
                .style(Style::default().bg(Color::Indexed(178)).fg(Color::Black))
            } else if is_active && !active_provider.is_empty() {
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("\u{2022} {}", provider_def.name),
                        theme.accent_secondary,
                    ),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    Span::raw("   "),
                    Span::styled(provider_def.name, theme.fg_active),
                ]))
            }
        })
        .collect();

    let list = List::new(list_items);
    frame.render_widget(list, chunks[0]);

    // Hints
    let hints = Line::from(vec![
        Span::raw(" "),
        Span::styled("↑↓", theme.fg_active),
        Span::styled(" nav  ", theme.fg_dim),
        Span::styled("Enter", theme.fg_active),
        Span::styled(" sel  ", theme.fg_dim),
        Span::styled("Esc", theme.fg_active),
        Span::styled(" close", theme.fg_dim),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::auth::{AuthState, ProviderAuthEntry};
    use amux_shared::providers::{PROVIDER_ID_AZURE_OPENAI, PROVIDER_ID_GROQ, PROVIDER_ID_OPENAI};

    #[test]
    fn available_provider_defs_filters_to_authenticated_entries_plus_custom() {
        let mut auth = AuthState::new();
        auth.entries = vec![
            ProviderAuthEntry {
                provider_id: PROVIDER_ID_OPENAI.to_string(),
                provider_name: "OpenAI".to_string(),
                authenticated: true,
                auth_source: "api_key".to_string(),
                model: "gpt-5.4".to_string(),
            },
            ProviderAuthEntry {
                provider_id: PROVIDER_ID_GROQ.to_string(),
                provider_name: "Groq".to_string(),
                authenticated: false,
                auth_source: "api_key".to_string(),
                model: "llama".to_string(),
            },
            ProviderAuthEntry {
                provider_id: PROVIDER_ID_AZURE_OPENAI.to_string(),
                provider_name: "Azure OpenAI".to_string(),
                authenticated: false,
                auth_source: "api_key".to_string(),
                model: String::new(),
            },
        ];

        let defs = available_provider_defs(&auth);
        assert!(defs
            .iter()
            .any(|provider| provider.id == PROVIDER_ID_OPENAI));
        assert!(defs
            .iter()
            .any(|provider| provider.id == PROVIDER_ID_CUSTOM));
        assert!(!defs
            .iter()
            .any(|provider| provider.id == PROVIDER_ID_AZURE_OPENAI));
        assert!(!defs.iter().any(|provider| provider.id == PROVIDER_ID_GROQ));
    }

    #[test]
    fn provider_picker_handles_empty_state() {
        let modal = ModalState::new();
        let config = ConfigState::new();
        let _theme = ThemeTokens::default();
        assert_eq!(modal.picker_cursor(), 0);
        assert_eq!(config.provider(), PROVIDER_ID_OPENAI);
    }
}
