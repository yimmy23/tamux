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

    // Field 12: skill_recommendation.enabled (toggle)
    let skill_enabled = raw
        .and_then(|r| r.get("skill_recommendation"))
        .and_then(|s| s.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    render_feature_toggle_line(
        &mut lines,
        settings,
        12,
        "Local Skill Gate",
        skill_enabled,
        theme,
    );

    // Field 13: skill_recommendation.background_community_search (toggle)
    let community_enabled = raw
        .and_then(|r| r.get("skill_recommendation"))
        .and_then(|s| s.get("background_community_search"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    render_feature_toggle_line(
        &mut lines,
        settings,
        13,
        "Community Scout",
        community_enabled,
        theme,
    );

    // Field 14: skill_recommendation.community_preapprove_timeout_secs
    let timeout_val = raw
        .and_then(|r| r.get("skill_recommendation"))
        .and_then(|s| s.get("community_preapprove_timeout_secs"))
        .and_then(|v| v.as_u64())
        .map(|v| v.to_string())
        .unwrap_or_else(|| "30".to_string());
    render_feature_field_line(
        &mut lines,
        settings,
        14,
        "Scout Timeout",
        &timeout_val,
        "  [Enter: edit]",
        theme,
    );

    // Field 15: skill_recommendation.suggest_global_enable_after_approvals
    let approvals_val = raw
        .and_then(|r| r.get("skill_recommendation"))
        .and_then(|s| s.get("suggest_global_enable_after_approvals"))
        .and_then(|v| v.as_u64())
        .map(|v| v.to_string())
        .unwrap_or_else(|| "3".to_string());
    render_feature_field_line(
        &mut lines,
        settings,
        15,
        "Suggest After",
        &approvals_val,
        "  [Enter: edit]",
        theme,
    );

    // Section: Audio
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Audio", theme.fg_active)));
    lines.push(Line::from(Span::styled(
        "  Speech-to-text and text-to-speech configuration",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    // Field 16: audio_stt_enabled (toggle)
    render_feature_toggle_line(
        &mut lines,
        settings,
        16,
        "STT Enabled",
        config.audio_stt_enabled(),
        theme,
    );

    // Field 17: audio_stt_provider
    let stt_provider = config.audio_stt_provider();
    render_feature_field_line(
        &mut lines,
        settings,
        17,
        "STT Provider",
        if stt_provider.is_empty() { "openai" } else { &stt_provider },
        "  [Enter: edit]",
        theme,
    );

    // Field 18: audio_stt_model
    let stt_model = config.audio_stt_model();
    render_feature_field_line(
        &mut lines,
        settings,
        18,
        "STT Model",
        if stt_model.is_empty() { "whisper-1" } else { &stt_model },
        "  [Enter: edit]",
        theme,
    );

    // Field 19: audio_tts_enabled (toggle)
    render_feature_toggle_line(
        &mut lines,
        settings,
        19,
        "TTS Enabled",
        config.audio_tts_enabled(),
        theme,
    );

    // Field 20: audio_tts_provider
    let tts_provider = config.audio_tts_provider();
    render_feature_field_line(
        &mut lines,
        settings,
        20,
        "TTS Provider",
        if tts_provider.is_empty() { "openai" } else { &tts_provider },
        "  [Enter: edit]",
        theme,
    );

    // Field 21: audio_tts_model
    let tts_model = config.audio_tts_model();
    render_feature_field_line(
        &mut lines,
        settings,
        21,
        "TTS Model",
        if tts_model.is_empty() {
            "gpt-4o-mini-tts"
        } else {
            &tts_model
        },
        "  [Enter: edit]",
        theme,
    );

    // Field 22: audio_tts_voice
    let tts_voice = config.audio_tts_voice();
    render_feature_field_line(
        &mut lines,
        settings,
        22,
        "TTS Voice",
        if tts_voice.is_empty() { "alloy" } else { &tts_voice },
        "  [Enter: edit]",
        theme,
    );

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Images", theme.fg_active)));
    lines.push(Line::from(Span::styled(
        "  Image generation provider and model configuration",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    let image_provider = config.image_generation_provider();
    render_feature_field_line(
        &mut lines,
        settings,
        23,
        "Image Provider",
        if image_provider.is_empty() {
            "openai"
        } else {
            &image_provider
        },
        "  [Enter: edit]",
        theme,
    );

    let image_model = config.image_generation_model();
    render_feature_field_line(
        &mut lines,
        settings,
        24,
        "Image Model",
        if image_model.is_empty() {
            "gpt-image-1"
        } else {
            &image_model
        },
        "  [Enter: edit]",
        theme,
    );

    // Hotkey hint row (non-editable)
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("  Hotkeys: ", theme.fg_dim),
        Span::styled("Ctrl+L", theme.fg_active),
        Span::styled(" (record) | ", theme.fg_dim),
        Span::styled("Ctrl+P", theme.fg_active),
        Span::styled(" (speak selected/latest) | ", theme.fg_dim),
        Span::styled("Ctrl+S", theme.fg_active),
        Span::styled(" (stop playback)", theme.fg_dim),
    ]));

    lines
}
