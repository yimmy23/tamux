fn render_advanced_toggle<'a>(
    lines: &mut Vec<Line<'a>>,
    settings: &'a SettingsState,
    field_idx: usize,
    label: &'a str,
    enabled: bool,
    theme: &ThemeTokens,
) {
    let is_selected = settings.field_cursor() == field_idx;
    let marker = if is_selected { "> " } else { "  " };
    let marker_style = if is_selected {
        theme.accent_primary
    } else {
        theme.fg_dim
    };
    let check = if enabled { "[x]" } else { "[ ]" };
    let check_style = if enabled {
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
        Span::styled(label, label_style),
    ];
    if is_selected {
        spans.push(Span::styled("  [Space: toggle]", theme.fg_dim));
    }
    lines.push(Line::from(spans));
}

fn render_advanced_value<'a>(
    lines: &mut Vec<Line<'a>>,
    settings: &'a SettingsState,
    config: &'a ConfigState,
    field_idx: usize,
    label: &'a str,
    value: String,
    field_name: &'a str,
    hint: &'a str,
    theme: &ThemeTokens,
) {
    let is_selected = settings.field_cursor() == field_idx;
    let is_editing = settings.is_editing() && settings.editing_field() == Some(field_name);
    let marker = if is_selected { "> " } else { "  " };
    let marker_style = if is_selected {
        theme.accent_primary
    } else {
        theme.fg_dim
    };
    let display_value = if is_editing {
        format!("{}\u{2588}", settings.edit_buffer())
    } else {
        value
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
        Span::styled(format!("{:<17} ", label), theme.fg_dim),
        Span::styled(display_value, value_style),
    ];
    if is_selected && !is_editing {
        spans.push(Span::styled(hint, theme.fg_dim));
    }
    if field_name == "context_window_tokens"
        && config.provider != amux_shared::providers::PROVIDER_ID_CUSTOM
    {
        spans.push(Span::styled("  [derived]", theme.fg_dim));
    }
    lines.push(Line::from(spans));
}

fn render_advanced_tab<'a>(
    settings: &'a SettingsState,
    config: &'a ConfigState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Advanced", theme.fg_active)));
    lines.push(Line::from(Span::styled(
        "  Context compaction, safety, and retry settings",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    render_advanced_toggle(
        &mut lines,
        settings,
        0,
        "Sandbox Managed Cmds",
        config.managed_sandbox_enabled,
        theme,
    );
    render_advanced_value(
        &mut lines,
        settings,
        config,
        1,
        "Managed Security:",
        config.managed_security_level.clone(),
        "managed_security_level",
        "  [Enter/Space: cycle]",
        theme,
    );
    lines.push(Line::from(Span::styled(
        "    Strict/highest mode prompts for risky shell commands.",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    render_advanced_toggle(
        &mut lines,
        settings,
        2,
        "Auto Compact Context",
        config.auto_compact_context,
        theme,
    );
    render_advanced_value(
        &mut lines,
        settings,
        config,
        3,
        "Compaction Mode: ",
        config.compaction_strategy.replace('_', " "),
        "compaction_strategy",
        "  [Enter/Space: cycle]",
        theme,
    );
    render_advanced_value(
        &mut lines,
        settings,
        config,
        4,
        "Max Context Msgs:",
        config.max_context_messages.to_string(),
        "max_context_messages",
        "  [Enter: edit]",
        theme,
    );
    render_advanced_value(
        &mut lines,
        settings,
        config,
        5,
        "Max Tool Loops:  ",
        config.max_tool_loops.to_string(),
        "max_tool_loops",
        "  [Enter: edit]",
        theme,
    );
    render_advanced_value(
        &mut lines,
        settings,
        config,
        6,
        "Max Retries:     ",
        config.max_retries.to_string(),
        "max_retries",
        "  [Enter: edit]",
        theme,
    );
    render_advanced_value(
        &mut lines,
        settings,
        config,
        7,
        "Retry Delay (ms):",
        config.retry_delay_ms.to_string(),
        "retry_delay_ms",
        "  [Enter: edit]",
        theme,
    );
    render_advanced_value(
        &mut lines,
        settings,
        config,
        8,
        "Message Loop (ms):",
        config.message_loop_delay_ms.to_string(),
        "message_loop_delay_ms",
        "  [Enter: edit]",
        theme,
    );
    render_advanced_value(
        &mut lines,
        settings,
        config,
        9,
        "Tool Call Gap (ms):",
        config.tool_call_delay_ms.to_string(),
        "tool_call_delay_ms",
        "  [Enter: edit]",
        theme,
    );
    render_advanced_value(
        &mut lines,
        settings,
        config,
        10,
        "LLM Stream Timeout (s):",
        config.llm_stream_chunk_timeout_secs.to_string(),
        "llm_stream_chunk_timeout_secs",
        "  [Enter: edit]",
        theme,
    );
    render_advanced_toggle(
        &mut lines,
        settings,
        11,
        "Auto Retry",
        config.auto_retry,
        theme,
    );
    render_advanced_value(
        &mut lines,
        settings,
        config,
        12,
        "Context Len Tok: ",
        config.context_window_tokens.to_string(),
        "context_window_tokens",
        if config.provider == amux_shared::providers::PROVIDER_ID_CUSTOM {
            "  [Enter: edit]"
        } else {
            ""
        },
        theme,
    );
    render_advanced_value(
        &mut lines,
        settings,
        config,
        13,
        "Budget Tokens:   ",
        config.context_budget_tokens.to_string(),
        "context_budget_tokens",
        "  [Enter: edit]",
        theme,
    );
    render_advanced_value(
        &mut lines,
        settings,
        config,
        14,
        "Compact Thres %: ",
        config.compact_threshold_pct.to_string(),
        "compact_threshold_pct",
        "  [Enter: edit]",
        theme,
    );
    render_advanced_value(
        &mut lines,
        settings,
        config,
        15,
        "Keep Recent:     ",
        config.keep_recent_on_compact.to_string(),
        "keep_recent_on_compact",
        "  [Enter: edit]",
        theme,
    );
    render_advanced_value(
        &mut lines,
        settings,
        config,
        16,
        "Bash Timeout (s):",
        config.bash_timeout_secs.to_string(),
        "bash_timeout_secs",
        "  [Enter: edit]",
        theme,
    );
    render_advanced_value(
        &mut lines,
        settings,
        config,
        17,
        "WELES Reviews:  ",
        config.weles_max_concurrent_reviews.to_string(),
        "weles_max_concurrent_reviews",
        "  [Enter: edit]",
        theme,
    );

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500} Compaction Strategy Settings \u{2500}\u{2500}",
        theme.fg_dim,
    )));

    let snapshot_start = match config.compaction_strategy.as_str() {
        "weles" => {
            render_advanced_value(
                &mut lines,
                settings,
                config,
                18,
                "WELES Provider: ",
                config.compaction_weles_provider.clone(),
                "compaction_weles_provider",
                "  [Enter/Space: cycle]",
                theme,
            );
            render_advanced_value(
                &mut lines,
                settings,
                config,
                19,
                "WELES Model:    ",
                config.compaction_weles_model.clone(),
                "compaction_weles_model",
                "  [Enter: edit]",
                theme,
            );
            render_advanced_value(
                &mut lines,
                settings,
                config,
                20,
                "WELES Reasoning:",
                config.compaction_weles_reasoning_effort.clone(),
                "compaction_weles_reasoning_effort",
                "  [Enter/Space: cycle]",
                theme,
            );
            21
        }
        "custom_model" => {
            render_advanced_value(
                &mut lines,
                settings,
                config,
                18,
                "Custom Provider:",
                config.compaction_custom_provider.clone(),
                "compaction_custom_provider",
                "  [Enter/Space: cycle]",
                theme,
            );
            render_advanced_value(
                &mut lines,
                settings,
                config,
                19,
                "Custom Base URL:",
                config.compaction_custom_base_url.clone(),
                "compaction_custom_base_url",
                "  [Enter: edit]",
                theme,
            );
            render_advanced_value(
                &mut lines,
                settings,
                config,
                20,
                "Custom Auth:    ",
                config.compaction_custom_auth_source.clone(),
                "compaction_custom_auth_source",
                "  [Enter/Space: cycle]",
                theme,
            );
            render_advanced_value(
                &mut lines,
                settings,
                config,
                21,
                "Custom Model:   ",
                config.compaction_custom_model.clone(),
                "compaction_custom_model",
                "  [Enter: edit]",
                theme,
            );
            render_advanced_value(
                &mut lines,
                settings,
                config,
                22,
                "Custom Transport:",
                config.compaction_custom_api_transport.clone(),
                "compaction_custom_api_transport",
                "  [Enter/Space: cycle]",
                theme,
            );
            render_advanced_value(
                &mut lines,
                settings,
                config,
                23,
                "Custom API Key: ",
                if config.compaction_custom_api_key.is_empty() {
                    "(empty)".to_string()
                } else {
                    "********".to_string()
                },
                "compaction_custom_api_key",
                "  [Enter: edit]",
                theme,
            );
            render_advanced_value(
                &mut lines,
                settings,
                config,
                24,
                "Assistant ID:   ",
                if config.compaction_custom_assistant_id.is_empty() {
                    "(empty)".to_string()
                } else {
                    config.compaction_custom_assistant_id.clone()
                },
                "compaction_custom_assistant_id",
                "  [Enter: edit]",
                theme,
            );
            render_advanced_value(
                &mut lines,
                settings,
                config,
                25,
                "Custom Reasoning:",
                config.compaction_custom_reasoning_effort.clone(),
                "compaction_custom_reasoning_effort",
                "  [Enter/Space: cycle]",
                theme,
            );
            render_advanced_value(
                &mut lines,
                settings,
                config,
                26,
                "Custom Ctx Tok: ",
                config.compaction_custom_context_window_tokens.to_string(),
                "compaction_custom_context_window_tokens",
                "  [Enter: edit]",
                theme,
            );
            27
        }
        _ => {
            lines.push(Line::from(Span::styled(
                "  Heuristic compaction uses the built-in rule based summary path.",
                theme.fg_dim,
            )));
            18
        }
    };

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500} Snapshot Retention \u{2500}\u{2500}",
        theme.fg_dim,
    )));

    render_advanced_toggle(
        &mut lines,
        settings,
        snapshot_start,
        "Auto Cleanup",
        config.snapshot_auto_cleanup,
        theme,
    );
    render_advanced_value(
        &mut lines,
        settings,
        config,
        snapshot_start + 1,
        "Max Snapshots:   ",
        config.snapshot_max_count.to_string(),
        "snapshot_max_count",
        "  [Enter: edit]",
        theme,
    );
    render_advanced_value(
        &mut lines,
        settings,
        config,
        snapshot_start + 2,
        "Max Snapshot Size:",
        config.snapshot_max_size_mb.to_string(),
        "snapshot_max_size_mb",
        "  [Enter: edit]",
        theme,
    );

    let size_display = if config.snapshot_total_size_bytes >= 1024 * 1024 * 1024 {
        format!(
            "{:.1} GB",
            config.snapshot_total_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
        )
    } else {
        format!(
            "{:.1} MB",
            config.snapshot_total_size_bytes as f64 / (1024.0 * 1024.0)
        )
    };
    render_advanced_value(
        &mut lines,
        settings,
        config,
        snapshot_start + 3,
        "Snapshots:       ",
        format!("{} ({})", config.snapshot_count, size_display),
        "snapshot_stats",
        "",
        theme,
    );

    lines
}
