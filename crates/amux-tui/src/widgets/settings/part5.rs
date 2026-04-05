fn render_chat_tab<'a>(
    settings: &'a SettingsState,
    config: &'a ConfigState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Chat", theme.fg_active)));
    lines.push(Line::from(Span::styled(
        "  Configure streaming and memory",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    // Fields 0–2: toggles
    let toggles: [(usize, bool, &str); 3] = [
        (0, config.enable_streaming, "Streaming"),
        (1, config.enable_conversation_memory, "Conversation Memory"),
        (2, config.enable_honcho_memory, "Honcho Memory"),
    ];
    for (idx, enabled, name) in &toggles {
        let is_selected = settings.field_cursor() == *idx;
        let check = if *enabled { "[x]" } else { "[ ]" };
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let check_style = if *enabled {
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
            Span::styled(*name, label_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Space: toggle]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // Fields 3–5: text / password fields
    let text_fields: [(usize, &str, &str, &str, bool); 3] = [
        (
            3,
            "Honcho API Key:  ",
            config.honcho_api_key.as_str(),
            "honcho_api_key",
            true,
        ),
        (
            4,
            "Honcho Base URL: ",
            config.honcho_base_url.as_str(),
            "honcho_base_url",
            false,
        ),
        (
            5,
            "Honcho Workspace:",
            config.honcho_workspace_id.as_str(),
            "honcho_workspace_id",
            false,
        ),
    ];
    for (idx, label, value, field_name, password) in &text_fields {
        render_gateway_text_field(
            settings, theme, &mut lines, *idx, label, value, field_name, *password,
        );
    }

    let capability_toggles: [(usize, bool, &str); 10] = [
        (6, config.anticipatory_enabled, "Anticipatory Support"),
        (7, config.anticipatory_morning_brief, "Morning Brief"),
        (
            8,
            config.anticipatory_predictive_hydration,
            "Predictive Hydration",
        ),
        (9, config.anticipatory_stuck_detection, "Stuck Detection"),
        (10, config.operator_model_enabled, "Operator Model"),
        (
            11,
            config.operator_model_allow_message_statistics,
            "Message Statistics",
        ),
        (
            12,
            config.operator_model_allow_approval_learning,
            "Approval Learning",
        ),
        (
            13,
            config.operator_model_allow_attention_tracking,
            "Attention Tracking",
        ),
        (
            14,
            config.operator_model_allow_implicit_feedback,
            "Implicit Feedback",
        ),
        (15, config.collaboration_enabled, "Collaboration"),
    ];
    for (idx, enabled, name) in &capability_toggles {
        let is_selected = settings.field_cursor() == *idx;
        let check = if *enabled { "[x]" } else { "[ ]" };
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let check_style = if *enabled {
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
            Span::styled(*name, label_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Space: toggle]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    let field_16_selected = settings.field_cursor() == 16;
    lines.push(Line::from(vec![
        Span::styled(
            if field_16_selected { "> " } else { "  " },
            if field_16_selected {
                theme.accent_primary
            } else {
                theme.fg_dim
            },
        ),
        Span::styled(format!("{:<16} ", "Compliance:"), theme.fg_dim),
        Span::styled(
            config.compliance_mode.as_str(),
            if field_16_selected {
                theme.accent_primary
            } else {
                theme.fg_active
            },
        ),
        if field_16_selected {
            Span::styled("  [Enter: cycle]", theme.fg_dim)
        } else {
            Span::raw("")
        },
    ]));

    for (idx, label, value, field_name) in [(
        17usize,
        "Retention Days: ",
        config.compliance_retention_days.to_string(),
        "compliance_retention_days",
    )] {
        render_gateway_text_field(
            settings, theme, &mut lines, idx, label, &value, field_name, false,
        );
    }

    for (idx, enabled, name) in [
        (
            18usize,
            config.compliance_sign_all_events,
            "Sign All Events",
        ),
        (19usize, config.tool_synthesis_enabled, "Tool Synthesis"),
        (
            20usize,
            config.tool_synthesis_require_activation,
            "Require Activation",
        ),
    ] {
        let is_selected = settings.field_cursor() == idx;
        let check = if enabled { "[x]" } else { "[ ]" };
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
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
            Span::styled(check, check_style),
            Span::raw(" "),
            Span::styled(name, label_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Space: toggle]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    for (idx, label, value, field_name) in [(
        21usize,
        "Tool Limit:     ",
        config.tool_synthesis_max_generated_tools.to_string(),
        "tool_synthesis_max_generated_tools",
    )] {
        render_gateway_text_field(
            settings, theme, &mut lines, idx, label, &value, field_name, false,
        );
    }

    for (idx, label) in [
        (22usize, "Inspect Operator Model"),
        (23usize, "Reset Operator Model"),
        (24usize, "Inspect Collaboration"),
        (25usize, "Inspect Generated Tools"),
    ] {
        let is_selected = settings.field_cursor() == idx;
        lines.push(Line::from(vec![
            Span::styled(
                if is_selected { "> " } else { "  " },
                if is_selected {
                    theme.accent_primary
                } else {
                    theme.fg_dim
                },
            ),
            Span::styled(
                format!("{label:<22}"),
                if is_selected {
                    theme.accent_primary
                } else {
                    theme.fg_active
                },
            ),
            if is_selected {
                Span::styled("  [Enter: run]", theme.fg_dim)
            } else {
                Span::raw("")
            },
        ]));
    }

    lines
}

