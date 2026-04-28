use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};
use serde_json::Value;

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
    let models = available_models_for(config, current_model, custom_model_name);
    render_with_models(frame, area, modal, &models, current_model, theme);
}

pub fn render_with_models(
    frame: &mut Frame,
    area: Rect,
    modal: &ModalState,
    models: &[crate::state::config::FetchedModel],
    current_model: &str,
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

    if cursor == models.len() {
        let hints = custom_model_hints(theme);
        frame.render_widget(Paragraph::new(hints), chunks[1]);
    } else if let Some(model) = models.get(cursor) {
        let details = highlighted_model_details(model, theme);
        frame.render_widget(Paragraph::new(details), chunks[1]);
    } else {
        let hints = custom_model_hints(theme);
        frame.render_widget(Paragraph::new(hints), chunks[1]);
    }
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
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::Terminal;
    use serde_json::json;

    fn render_picker_screen(models: Vec<FetchedModel>, cursor: usize) -> Vec<String> {
        let mut modal = ModalState::new();
        modal.set_picker_item_count(models.len() + 1);
        modal.reduce(crate::state::modal::ModalAction::Navigate(cursor as i32));

        let theme = ThemeTokens::default();
        let backend = TestBackend::new(72, 8);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| {
                render_with_models(frame, Rect::new(0, 0, 72, 8), &modal, &models, "", &theme);
            })
            .expect("render should succeed");

        let buffer = terminal.backend().buffer();
        (0..8)
            .map(|y| {
                (0..72)
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect()
    }

    fn footer_line(screen: &str) -> String {
        screen
            .lines()
            .nth(6)
            .unwrap_or("")
            .strip_prefix('║')
            .and_then(|line| line.strip_suffix('║'))
            .unwrap_or("")
            .trim_end()
            .to_string()
    }

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

    #[test]
    fn model_picker_footer_shows_modalities_and_pricing_for_highlighted_model() {
        let screen = render_picker_screen(
            vec![FetchedModel {
                id: "gpt-4o".into(),
                name: Some("GPT-4o".into()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    prompt: Some("$0.005".into()),
                    completion: Some("$0.015".into()),
                    image: Some("$0.020".into()),
                    request: None,
                    web_search: None,
                    internal_reasoning: None,
                    input_cache_read: None,
                    input_cache_write: None,
                    audio: Some("$0.030".into()),
                }),
                metadata: Some(json!({
                    "modality": "text+audio->text+audio"
                })),
            }],
            0,
        )
        .join("\n");

        assert_eq!(
            footer_line(&screen),
            " modalities: text, audio, image  input: $0.005  output: $0.015"
        );
    }

    #[test]
    fn model_picker_footer_uses_request_pricing_when_prompt_and_completion_missing() {
        let screen = render_picker_screen(
            vec![FetchedModel {
                id: "gpt-4o-mini".into(),
                name: Some("GPT-4o Mini".into()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    prompt: None,
                    completion: None,
                    image: None,
                    request: Some("$0.123".into()),
                    web_search: None,
                    internal_reasoning: None,
                    input_cache_read: None,
                    input_cache_write: None,
                    audio: None,
                }),
                metadata: Some(json!({
                    "modality": "text"
                })),
            }],
            0,
        )
        .join("\n");

        assert_eq!(
            footer_line(&screen),
            " modalities: text  input: $0.123  output: $0.123"
        );
    }

    #[test]
    fn model_picker_keeps_key_hints_for_custom_model_row() {
        let screen = render_picker_screen(
            vec![FetchedModel {
                id: "gpt-4o".into(),
                name: Some("GPT-4o".into()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            }],
            1,
        )
        .join("\n");

        assert_eq!(footer_line(&screen), " ↑↓ nav  Enter sel/custom  Esc close");
    }
}

fn highlighted_model_details(
    model: &crate::state::config::FetchedModel,
    theme: &ThemeTokens,
) -> Line<'static> {
    let modalities = model_modalities(model);
    let (input_price, output_price) = model_prices(model);

    Line::from(vec![
        Span::raw(" "),
        Span::styled("modalities", theme.fg_dim),
        Span::raw(": "),
        Span::styled(modalities, theme.fg_active),
        Span::raw("  "),
        Span::styled("input", theme.fg_dim),
        Span::raw(": "),
        Span::styled(input_price, theme.fg_active),
        Span::raw("  "),
        Span::styled("output", theme.fg_dim),
        Span::raw(": "),
        Span::styled(output_price, theme.fg_active),
    ])
}

fn custom_model_hints(theme: &ThemeTokens) -> Line<'static> {
    Line::from(vec![
        Span::raw(" "),
        Span::styled("↑↓", theme.fg_active),
        Span::styled(" nav  ", theme.fg_dim),
        Span::styled("Enter", theme.fg_active),
        Span::styled(" sel/custom  ", theme.fg_dim),
        Span::styled("Esc", theme.fg_active),
        Span::styled(" close", theme.fg_dim),
    ])
}

fn model_prices(model: &crate::state::config::FetchedModel) -> (String, String) {
    let Some(pricing) = model.pricing.as_ref() else {
        return ("n/a".to_string(), "n/a".to_string());
    };

    let input = first_pricing_signal(&[
        pricing.prompt.as_deref(),
        pricing.completion.as_deref(),
        pricing.request.as_deref(),
        pricing.image.as_deref(),
        pricing.internal_reasoning.as_deref(),
        pricing.web_search.as_deref(),
        pricing.audio.as_deref(),
        pricing.input_cache_read.as_deref(),
        pricing.input_cache_write.as_deref(),
    ]);
    let output = first_pricing_signal(&[
        pricing.completion.as_deref(),
        pricing.prompt.as_deref(),
        pricing.request.as_deref(),
        pricing.image.as_deref(),
        pricing.internal_reasoning.as_deref(),
        pricing.web_search.as_deref(),
        pricing.audio.as_deref(),
        pricing.input_cache_read.as_deref(),
        pricing.input_cache_write.as_deref(),
    ]);

    (input, output)
}

fn model_modalities(model: &crate::state::config::FetchedModel) -> String {
    let mut modalities = Vec::new();
    let metadata = model.metadata.as_ref();

    collect_modalities(
        metadata
            .and_then(|value| value.pointer("/architecture/input_modalities"))
            .or_else(|| metadata.and_then(|value| value.pointer("/input_modalities")))
            .or_else(|| metadata.and_then(|value| value.pointer("/modalities"))),
        &mut modalities,
    );
    collect_modalities(
        metadata
            .and_then(|value| value.pointer("/architecture/output_modalities"))
            .or_else(|| metadata.and_then(|value| value.pointer("/output_modalities")))
            .or_else(|| metadata.and_then(|value| value.pointer("/modalities"))),
        &mut modalities,
    );
    collect_modalities(
        metadata
            .and_then(|value| value.pointer("/architecture/modality"))
            .or_else(|| metadata.and_then(|value| value.pointer("/modality"))),
        &mut modalities,
    );

    collect_pricing_modality(
        model
            .pricing
            .as_ref()
            .and_then(|pricing| pricing.image.as_deref()),
        "image",
        &mut modalities,
    );
    collect_pricing_modality(
        model
            .pricing
            .as_ref()
            .and_then(|pricing| pricing.audio.as_deref()),
        "audio",
        &mut modalities,
    );

    if modalities.is_empty() {
        "n/a".to_string()
    } else {
        modalities.join(", ")
    }
}

fn collect_modalities(value: Option<&Value>, modalities: &mut Vec<String>) {
    let Some(value) = value else {
        return;
    };

    match value {
        Value::Array(items) => {
            for item in items {
                if let Some(modality) = item.as_str() {
                    collect_modality_token(modality, modalities);
                }
            }
        }
        Value::String(modality) => {
            collect_modality_token(modality, modalities);
        }
        _ => {}
    }
}

fn collect_pricing_modality(value: Option<&str>, modality: &str, modalities: &mut Vec<String>) {
    if value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
        && !modalities.iter().any(|existing| existing == modality)
    {
        modalities.push(modality.to_string());
    }
}

fn collect_modality_token(value: &str, modalities: &mut Vec<String>) {
    for token in value
        .split(|ch: char| !ch.is_ascii_alphabetic())
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let token = token.to_ascii_lowercase();
        if is_known_modality(&token) && !modalities.iter().any(|existing| existing == &token) {
            modalities.push(token);
        }
    }
}

fn is_known_modality(value: &str) -> bool {
    matches!(value, "text" | "image" | "audio" | "video")
}

fn first_pricing_signal(fields: &[Option<&str>]) -> String {
    fields
        .iter()
        .flatten()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
        .unwrap_or("n/a")
        .to_string()
}
