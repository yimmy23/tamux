fn render_websearch_tab<'a>(
    settings: &'a SettingsState,
    config: &'a ConfigState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Web Search", theme.fg_active)));
    lines.push(Line::from(Span::styled(
        "  Configure web search tool and providers",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    // Field 0: web_search_enabled (checkbox — mirrors tool_web_search)
    {
        let is_selected = settings.field_cursor() == 0;
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let check = if config.tool_web_search { "[x]" } else { "[ ]" };
        let check_style = if config.tool_web_search {
            theme.accent_success
        } else {
            theme.fg_dim
        };
        let label_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };
        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(check, check_style),
            Span::raw(" "),
            Span::styled("Enable Web Search", label_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Space: toggle]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // Field 1: search_provider (cycle on Enter)
    {
        let is_selected = settings.field_cursor() == 1;
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let provider_val = if config.search_provider.is_empty() || config.search_provider == "none"
        {
            "none".to_string()
        } else {
            config.search_provider.clone()
        };
        let value_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };
        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(format!("{:<16} ", "Provider:"), theme.fg_dim),
            Span::styled(provider_val, value_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Enter: cycle]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // Fields 2–4: API keys (masked, inline edit)
    let api_key_fields: [(usize, &str, &str, &str); 3] = [
        (
            2,
            "Firecrawl Key:  ",
            config.firecrawl_api_key.as_str(),
            "firecrawl_api_key",
        ),
        (
            3,
            "Exa Key:        ",
            config.exa_api_key.as_str(),
            "exa_api_key",
        ),
        (
            4,
            "Tavily Key:     ",
            config.tavily_api_key.as_str(),
            "tavily_api_key",
        ),
    ];

    for (idx, label, value, field_name) in &api_key_fields {
        let is_selected = settings.field_cursor() == *idx;
        let is_editing = settings.is_editing() && settings.editing_field() == Some(field_name);
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };

        let display_value: String = if is_editing {
            format!("{}\u{2588}", settings.edit_buffer())
        } else if value.is_empty() {
            "(not set)".to_string()
        } else {
            mask_api_key(value)
        };

        let value_style = if is_editing {
            theme.fg_active
        } else if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };

        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(format!("{:<16} ", label), theme.fg_dim),
            Span::styled(display_value, value_style),
        ];
        if is_selected && !is_editing {
            spans.push(Span::styled("  [Enter: edit]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // Field 5: search_max_results (numeric inline edit)
    {
        let is_selected = settings.field_cursor() == 5;
        let is_editing =
            settings.is_editing() && settings.editing_field() == Some("search_max_results");
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };

        let display_value: String = if is_editing {
            format!("{}\u{2588}", settings.edit_buffer())
        } else {
            config.search_max_results.to_string()
        };

        let value_style = if is_editing {
            theme.fg_active
        } else if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };

        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(format!("{:<16} ", "Max Results:"), theme.fg_dim),
            Span::styled(display_value, value_style),
        ];
        if is_selected && !is_editing {
            spans.push(Span::styled("  [Enter: edit]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // Field 6: search_timeout_secs (numeric inline edit)
    {
        let is_selected = settings.field_cursor() == 6;
        let is_editing =
            settings.is_editing() && settings.editing_field() == Some("search_timeout");
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };

        let display_value: String = if is_editing {
            format!("{}\u{2588}", settings.edit_buffer())
        } else {
            format!("{}s", config.search_timeout_secs)
        };

        let value_style = if is_editing {
            theme.fg_active
        } else if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };

        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(format!("{:<16} ", "Timeout:"), theme.fg_dim),
            Span::styled(display_value, value_style),
        ];
        if is_selected && !is_editing {
            spans.push(Span::styled("  [Enter: edit]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("  Web Browsing", theme.fg_active)));

    // Field 7: browse_provider (cycle on Enter)
    {
        let is_selected = settings.field_cursor() == 7;
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let provider_val = if config.browse_provider.is_empty() {
            "auto".to_string()
        } else {
            config.browse_provider.clone()
        };
        let value_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };
        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(format!("{:<16} ", "Browser:"), theme.fg_dim),
            Span::styled(provider_val, value_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Enter: cycle]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    lines
}

