    use super::super::*;
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

    #[test]
    fn model_picker_filters_visible_rows_by_query_and_keeps_custom_row() {
        let mut modal = ModalState::new();
        modal.reduce(crate::state::modal::ModalAction::Push(
            crate::state::modal::ModalKind::ModelPicker,
        ));
        modal.reduce(crate::state::modal::ModalAction::SetQuery("claude".into()));
        modal.set_picker_item_count(2);

        let models = vec![
            FetchedModel {
                id: "gpt-4o".into(),
                name: Some("GPT-4o".into()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
            FetchedModel {
                id: "claude-sonnet-4-6".into(),
                name: Some("Claude Sonnet 4.6".into()),
                context_window: Some(200_000),
                pricing: None,
                metadata: None,
            },
        ];

        let theme = ThemeTokens::default();
        let backend = TestBackend::new(72, 8);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| {
                render_with_models(frame, Rect::new(0, 0, 72, 8), &modal, &models, "", &theme);
            })
            .expect("render should succeed");

        let buffer = terminal.backend().buffer();
        let screen = (0..8)
            .map(|y| {
                (0..72)
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(screen.contains("Claude Sonnet 4.6"));
        assert!(screen.contains("Custom model..."));
        assert!(!screen.contains("GPT-4o"));
    }
