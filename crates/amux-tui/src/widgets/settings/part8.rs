fn render_features_tab<'a>(
    settings: &'a SettingsState,
    config: &'a ConfigState,
    tier: &crate::state::tier::TierState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let raw = config.agent_config_raw.as_ref();

    // Section: Tier & Security
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  Tier & Security",
        theme.fg_active,
    )));
    lines.push(Line::from(Span::styled(
        "  Feature tier and security controls",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    // Field 0: tier_override (cycle)
    let tier_val = raw
        .and_then(|r| r.get("tier"))
        .and_then(|t| t.get("user_override"))
        .and_then(|v| v.as_str())
        .unwrap_or(&tier.current_tier);
    render_feature_field_line(
        &mut lines,
        settings,
        0,
        "Tier Override",
        tier_val,
        "  [Enter/Space: cycle]",
        theme,
    );

    // Field 1: managed_security_level (cycle)
    let security_val = raw
        .and_then(|r| r.get("managed_security_level"))
        .and_then(|v| v.as_str())
        .unwrap_or("balanced");
    render_feature_field_line(
        &mut lines,
        settings,
        1,
        "Security Level",
        security_val,
        "  [Enter/Space: cycle]",
        theme,
    );

    // Section: Heartbeat
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Heartbeat", theme.fg_active)));
    lines.push(Line::raw(""));

    // Field 2: heartbeat.cron
    let cron_val = raw
        .and_then(|r| r.get("heartbeat"))
        .and_then(|h| h.get("cron"))
        .and_then(|v| v.as_str())
        .unwrap_or("*/15 * * * *");
    render_feature_field_line(
        &mut lines,
        settings,
        2,
        "Cron Schedule",
        cron_val,
        "  [Enter: edit]",
        theme,
    );

    // Field 3: heartbeat.quiet_start
    let quiet_start = raw
        .and_then(|r| r.get("heartbeat"))
        .and_then(|h| h.get("quiet_start"))
        .and_then(|v| v.as_str())
        .unwrap_or("22:00");
    render_feature_field_line(
        &mut lines,
        settings,
        3,
        "Quiet Start",
        quiet_start,
        "  [Enter: edit]",
        theme,
    );

    // Field 4: heartbeat.quiet_end
    let quiet_end = raw
        .and_then(|r| r.get("heartbeat"))
        .and_then(|h| h.get("quiet_end"))
        .and_then(|v| v.as_str())
        .unwrap_or("07:00");
    render_feature_field_line(
        &mut lines,
        settings,
        4,
        "Quiet End",
        quiet_end,
        "  [Enter: edit]",
        theme,
    );

    // Fields 5-8: heartbeat check toggles
    let check_toggles: [(usize, &str, &str); 4] = [
        (5, "check_stale_todos", "Check Stale Todos"),
        (6, "check_stuck_goals", "Check Stuck Goals"),
        (7, "check_unreplied_messages", "Check Unreplied Msgs"),
        (8, "check_repo_changes", "Check Repo Changes"),
    ];
    for (idx, key, label) in &check_toggles {
        let enabled = raw
            .and_then(|r| r.get("heartbeat"))
            .and_then(|h| h.get(*key))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        render_feature_toggle_line(&mut lines, settings, *idx, label, enabled, theme);
    }

    // Section: Memory & Learning
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  Memory & Learning",
        theme.fg_active,
    )));
    lines.push(Line::raw(""));

    // Field 9: consolidation.enabled (toggle)
    let consol_enabled = raw
        .and_then(|r| r.get("consolidation"))
        .and_then(|c| c.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    render_feature_toggle_line(
        &mut lines,
        settings,
        9,
        "Consolidation",
        consol_enabled,
        theme,
    );

    // Field 10: consolidation.decay_half_life_hours
    let decay_val = raw
        .and_then(|r| r.get("consolidation"))
        .and_then(|c| c.get("decay_half_life_hours"))
        .and_then(|v| v.as_f64())
        .map(|v| format!("{:.0}", v))
        .unwrap_or_else(|| "69".to_string());
    render_feature_field_line(
        &mut lines,
        settings,
        10,
        "Decay Half-Life",
        &decay_val,
        "  [Enter: edit]",
        theme,
    );

    // Field 11: heuristic_promotion_threshold
    let heur_val = raw
        .and_then(|r| r.get("consolidation"))
        .and_then(|c| c.get("heuristic_promotion_threshold"))
        .and_then(|v| v.as_u64())
        .map(|v| v.to_string())
        .unwrap_or_else(|| "5".to_string());
    render_feature_field_line(
        &mut lines,
        settings,
        11,
        "Heuristic Thresh",
        &heur_val,
        "  [Enter: edit]",
        theme,
    );

    // Section: Skills
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Skills", theme.fg_active)));
    lines.push(Line::raw(""));

    // Field 12: skill_discovery.enabled (toggle)
    let skill_enabled = raw
        .and_then(|r| r.get("skill_discovery"))
        .and_then(|s| s.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    render_feature_toggle_line(
        &mut lines,
        settings,
        12,
        "Auto-Discovery",
        skill_enabled,
        theme,
    );

    // Field 13: skill_discovery.promotion_threshold
    let promo_val = raw
        .and_then(|r| r.get("skill_discovery"))
        .and_then(|s| s.get("promotion_threshold"))
        .and_then(|v| v.as_u64())
        .map(|v| v.to_string())
        .unwrap_or_else(|| "3".to_string());
    render_feature_field_line(
        &mut lines,
        settings,
        13,
        "Promotion Thresh",
        &promo_val,
        "  [Enter: edit]",
        theme,
    );

    lines
}

