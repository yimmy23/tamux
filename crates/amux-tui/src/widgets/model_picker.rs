use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};

use crate::state::config::ConfigState;
use crate::state::modal::ModalState;
use crate::theme::ThemeTokens;

pub fn merge_models_for_selection(
    models: &[crate::state::config::FetchedModel],
    current_model: &str,
    custom_model_name: Option<&str>,
) -> Vec<crate::state::config::FetchedModel> {
    let mut models = models.to_vec();
    let current_model = current_model.trim();
    let custom_model_name = custom_model_name.unwrap_or("").trim();
    if !current_model.is_empty() && !models.iter().any(|model| model.id == current_model) {
        models.insert(
            0,
            crate::state::config::FetchedModel {
                id: current_model.to_string(),
                name: Some(if custom_model_name.is_empty() {
                    current_model.to_string()
                } else {
                    custom_model_name.to_string()
                }),
                context_window: None,
                pricing: None,
                metadata: None,
            },
        );
    }
    models
}

pub fn available_models_for(
    config: &ConfigState,
    current_model: &str,
    custom_model_name: Option<&str>,
) -> Vec<crate::state::config::FetchedModel> {
    merge_models_for_selection(config.fetched_models(), current_model, custom_model_name)
}

#[allow(dead_code)]
pub fn available_models(config: &ConfigState) -> Vec<crate::state::config::FetchedModel> {
    available_models_for(config, config.model(), Some(&config.custom_model_name))
}

pub fn render_for(
    frame: &mut Frame,
    area: Rect,
    modal: &ModalState,
    config: &ConfigState,
    current_model: &str,
    custom_model_name: Option<&str>,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" MODEL ")
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

    let models = available_models_for(config, current_model, custom_model_name);
    let cursor = modal.picker_cursor();
    let active_model = current_model.trim();

    let list_h = chunks[0].height as usize;
    let custom_row = "Custom model...";
    let total_rows = models.len() + 1;
    let window_start = cursor
        .saturating_sub(list_h.saturating_sub(1))
        .min(total_rows.saturating_sub(list_h));
    let window_end = (window_start + list_h).min(total_rows);

    let list_items: Vec<ListItem> = (window_start..window_end)
        .map(|absolute_index| {
            if absolute_index == models.len() {
                let is_selected = absolute_index == cursor;
                return if is_selected {
                    ListItem::new(Line::from(vec![Span::raw(" > "), Span::raw(custom_row)]))
                        .style(Style::default().bg(Color::Indexed(178)).fg(Color::Black))
                } else {
                    ListItem::new(Line::from(vec![
                        Span::raw("   "),
                        Span::styled(custom_row, theme.accent_secondary),
                    ]))
                };
            }

            let model = &models[absolute_index];
            let i = absolute_index;
            let display_name = model.name.as_deref().unwrap_or(&model.id);
            let is_selected = i == cursor;
            let is_active = model.id == active_model || display_name == active_model;

            let ctx_str = model
                .context_window
                .map(|c| format!(" ({}k ctx)", c / 1000))
                .unwrap_or_default();

            if is_selected {
                ListItem::new(Line::from(vec![
                    Span::raw(" > "),
                    Span::raw(display_name.to_string()),
                    Span::raw(ctx_str),
                ]))
                .style(Style::default().bg(Color::Indexed(178)).fg(Color::Black))
            } else if is_active && !active_model.is_empty() {
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("\u{2022} {}", display_name), theme.accent_secondary),
                    Span::styled(ctx_str, theme.fg_dim),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    Span::raw("   "),
                    Span::styled(display_name.to_string(), theme.fg_active),
                    Span::styled(ctx_str, theme.fg_dim),
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
        Span::styled(" sel/custom  ", theme.fg_dim),
        Span::styled("Esc", theme.fg_active),
        Span::styled(" close", theme.fg_dim),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[1]);
}

#[allow(dead_code)]
pub fn render(
    frame: &mut Frame,
    area: Rect,
    modal: &ModalState,
    config: &ConfigState,
    theme: &ThemeTokens,
) {
    render_for(
        frame,
        area,
        modal,
        config,
        config.model(),
        Some(&config.custom_model_name),
        theme,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::config::{ConfigAction, ConfigState, FetchedModel};

    #[test]
    fn model_picker_handles_empty_state() {
        let _modal = ModalState::new();
        let config = ConfigState::new();
        let _theme = ThemeTokens::default();
        assert!(config.fetched_models().is_empty());
    }

    #[test]
    fn model_picker_handles_fetched_state() {
        let mut config = ConfigState::new();
        config.reduce(ConfigAction::ModelsFetched(vec![FetchedModel {
            id: "gpt-4o".into(),
            name: Some("GPT-4o".into()),
            context_window: Some(128_000),
            pricing: None,
            metadata: None,
        }]));
        assert_eq!(config.fetched_models().len(), 1);
    }
}
