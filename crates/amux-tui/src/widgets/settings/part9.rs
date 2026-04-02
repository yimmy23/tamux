fn render_advanced_tab<'a>(
    settings: &'a SettingsState,
    config: &'a ConfigState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Advanced", theme.fg_active)));
    lines.push(Line::from(Span::styled(
        "  Context compaction and retry settings",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    // Field 0: managed_sandbox_enabled (toggle)
    {
        let is_selected = settings.field_cursor() == 0;
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let check = if config.managed_sandbox_enabled {
            "[x]"
        } else {
            "[ ]"
        };
        let check_style = if config.managed_sandbox_enabled {
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
            Span::styled("Sandbox Managed Cmds", label_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Space: toggle]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // Field 1: managed_security_level (cycle)
    {
        let is_selected = settings.field_cursor() == 1;
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let value_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };
        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled("Managed Security: ", theme.fg_dim),
            Span::styled(config.managed_security_level.clone(), value_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Enter/Space: cycle]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(Span::styled(
        "    Strict/highest mode prompts for risky shell commands (e.g. rm -rf).",
        theme.fg_dim,
    )));

    lines.push(Line::raw(""));

    // Field 2: auto_compact_context (toggle)
    {
        let is_selected = settings.field_cursor() == 2;
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let check = if config.auto_compact_context {
            "[x]"
        } else {
            "[ ]"
        };
        let check_style = if config.auto_compact_context {
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
            Span::styled("Auto Compact Context", label_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Space: toggle]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // Fields 3–11: numeric inline-edit fields
    let numeric_fields: [(usize, &str, String, &str); 10] = [
        (
            3,
            "Max Context Msgs:",
            config.max_context_messages.to_string(),
            "max_context_messages",
        ),
        (
            4,
            "Max Tool Loops:  ",
            config.max_tool_loops.to_string(),
            "max_tool_loops",
        ),
        (
            5,
            "Max Retries:     ",
            config.max_retries.to_string(),
            "max_retries",
        ),
        (
            6,
            "Retry Delay (ms):",
            config.retry_delay_ms.to_string(),
            "retry_delay_ms",
        ),
        (
            7,
            "Auto Retry:      ",
            if config.auto_retry {
                "on".to_string()
            } else {
                "off".to_string()
            },
            "auto_retry",
        ),
        (
            8,
            "Context Len Tok: ",
            config.context_window_tokens.to_string(),
            "context_window_tokens",
        ),
        (
            9,
            "Budget Tokens:   ",
            config.context_budget_tokens.to_string(),
            "context_budget_tokens",
        ),
        (
            10,
            "Compact Thres %: ",
            config.compact_threshold_pct.to_string(),
            "compact_threshold_pct",
        ),
        (
            11,
            "Keep Recent:     ",
            config.keep_recent_on_compact.to_string(),
            "keep_recent_on_compact",
        ),
        (
            12,
            "Bash Timeout (s):",
            config.bash_timeout_secs.to_string(),
            "bash_timeout_secs",
        ),
    ];
    for (idx, label, value, field_name) in &numeric_fields {
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
        } else {
            value.clone()
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
            spans.push(Span::styled("  [Enter: edit]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // ── Snapshot Retention section ───────────────────────────────────────────
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500} Snapshot Retention \u{2500}\u{2500}",
        theme.fg_dim,
    )));

    // Field 12: snapshot_auto_cleanup (toggle)
    {
        let is_selected = settings.field_cursor() == 12;
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let check = if config.snapshot_auto_cleanup {
            "[x]"
        } else {
            "[ ]"
        };
        let check_style = if config.snapshot_auto_cleanup {
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
            Span::styled("Auto Cleanup", label_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Space: toggle]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // Fields 13-14: snapshot numeric fields
    let snapshot_fields: [(usize, &str, String, &str); 2] = [
        (
            13,
            "Max Snapshots:   ",
            config.snapshot_max_count.to_string(),
            "snapshot_max_count",
        ),
        (
            14,
            "Max Snapshot Size:",
            config.snapshot_max_size_mb.to_string(),
            "snapshot_max_size_mb",
        ),
    ];
    for (idx, label, value, field_name) in &snapshot_fields {
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
        } else {
            value.clone()
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
            spans.push(Span::styled("  [Enter: edit]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // Field 15: snapshot_stats (read-only info line)
    {
        let is_selected = settings.field_cursor() == 15;
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
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
        let info = format!("{} ({})", config.snapshot_count, size_display);
        let info_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };
        let spans = vec![
            Span::styled(marker, marker_style),
            Span::styled("Snapshots:        ", theme.fg_dim),
            Span::styled(info, info_style),
        ];
        lines.push(Line::from(spans));
    }

    lines
}

