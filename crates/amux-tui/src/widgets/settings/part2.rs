fn advanced_single_line_edit_layout(config: &ConfigState, field: &str) -> Option<(usize, usize)> {
    let row = match config.compaction_strategy.as_str() {
        "weles" => match field {
            "max_context_messages" => 10,
            "max_tool_loops" => 11,
            "max_retries" => 12,
            "retry_delay_ms" => 13,
            "message_loop_delay_ms" => 14,
            "tool_call_delay_ms" => 15,
            "llm_stream_chunk_timeout_secs" => 16,
            "context_budget_tokens" => 19,
            "compact_threshold_pct" => 20,
            "keep_recent_on_compact" => 21,
            "bash_timeout_secs" => 22,
            "weles_max_concurrent_reviews" => 23,
            "compaction_weles_model" => 27,
            "snapshot_max_count" => 32,
            "snapshot_max_size_mb" => 33,
            _ => return None,
        },
        "custom_model" => match field {
            "max_context_messages" => 10,
            "max_tool_loops" => 11,
            "max_retries" => 12,
            "retry_delay_ms" => 13,
            "message_loop_delay_ms" => 14,
            "tool_call_delay_ms" => 15,
            "llm_stream_chunk_timeout_secs" => 16,
            "context_budget_tokens" => 19,
            "compact_threshold_pct" => 20,
            "keep_recent_on_compact" => 21,
            "bash_timeout_secs" => 22,
            "weles_max_concurrent_reviews" => 23,
            "compaction_custom_base_url" => 27,
            "compaction_custom_model" => 29,
            "compaction_custom_api_key" => 31,
            "compaction_custom_assistant_id" => 32,
            "compaction_custom_context_window_tokens" => 34,
            "snapshot_max_count" => 38,
            "snapshot_max_size_mb" => 39,
            _ => return None,
        },
        _ => match field {
            "max_context_messages" => 10,
            "max_tool_loops" => 11,
            "max_retries" => 12,
            "retry_delay_ms" => 13,
            "message_loop_delay_ms" => 14,
            "tool_call_delay_ms" => 15,
            "llm_stream_chunk_timeout_secs" => 16,
            "context_budget_tokens" => 19,
            "compact_threshold_pct" => 20,
            "keep_recent_on_compact" => 21,
            "bash_timeout_secs" => 22,
            "weles_max_concurrent_reviews" => 23,
            "snapshot_max_count" => 30,
            "snapshot_max_size_mb" => 31,
            _ => return None,
        },
    };

    Some((row, 20))
}

fn single_line_edit_layout(
    settings: &SettingsState,
    config: &ConfigState,
    field: &str,
) -> Option<(usize, usize)> {
    match settings.active_tab() {
        SettingsTab::Provider => match field {
            "base_url" => Some((5, 19)),
            "custom_model_entry" => Some((7, 19)),
            "assistant_id" => Some((9, 19)),
            "context_window_tokens" => Some((11, 19)),
            _ => None,
        },
        SettingsTab::WebSearch => match field {
            "firecrawl_api_key" => Some((6, 19)),
            "exa_api_key" => Some((7, 19)),
            "tavily_api_key" => Some((8, 19)),
            "search_max_results" => Some((9, 19)),
            "search_timeout" => Some((10, 19)),
            _ => None,
        },
        SettingsTab::Chat => match field {
            "honcho_api_key" => Some((7, 19)),
            "honcho_base_url" => Some((8, 19)),
            "honcho_workspace_id" => Some((9, 19)),
            _ => None,
        },
        SettingsTab::Gateway => match field {
            "gateway_prefix" => Some((5, 19)),
            "slack_token" => Some((8, 19)),
            "slack_channel_filter" => Some((9, 19)),
            "telegram_token" => Some((12, 19)),
            "telegram_allowed_chats" => Some((13, 19)),
            "discord_token" => Some((16, 19)),
            "discord_channel_filter" => Some((17, 19)),
            "discord_allowed_users" => Some((18, 19)),
            "whatsapp_allowed_contacts" => Some((21, 19)),
            "whatsapp_token" => Some((22, 19)),
            "whatsapp_phone_id" => Some((23, 19)),
            "whatsapp_link_device" => Some((24, 19)),
            "whatsapp_relink_device" => Some((25, 19)),
            _ => None,
        },
        SettingsTab::Auth => None,
        SettingsTab::Agent => match field {
            "base_url" => Some((5, 19)),
            "custom_model_entry" => Some((7, 19)),
            "assistant_id" => Some((9, 19)),
            "context_window_tokens" => Some((11, 19)),
            _ => None,
        },
        SettingsTab::SubAgents => None,
        SettingsTab::Concierge => None,
        SettingsTab::Advanced => advanced_single_line_edit_layout(config, field),
        SettingsTab::Tools => None,
        SettingsTab::Features => match field {
            "feat_heartbeat_cron" => Some((8, 20)),
            "feat_heartbeat_quiet_start" => Some((9, 20)),
            "feat_heartbeat_quiet_end" => Some((10, 20)),
            "feat_decay_half_life_hours" => Some((19, 20)),
            "feat_heuristic_promotion_threshold" => Some((20, 20)),
            "feat_skill_promotion_threshold" => Some((25, 20)),
            _ => None,
        },
        SettingsTab::Plugins => None,
    }
}

fn textarea_edit_layout(settings: &SettingsState, field: &str) -> Option<(usize, usize)> {
    match settings.active_tab() {
        SettingsTab::Agent if field == "system_prompt" => Some((17, 4)),
        SettingsTab::Gateway if field == "whatsapp_allowed_contacts" => Some((23, 4)),
        _ => None,
    }
}

fn advanced_settings_row_hit(config: &ConfigState, row: usize) -> Option<(usize, Option<usize>)> {
    match config.compaction_strategy.as_str() {
        "weles" => match row {
            4 => Some((0, None)),
            5 => Some((1, None)),
            8 => Some((2, None)),
            9 => Some((3, None)),
            10 => Some((4, None)),
            11 => Some((5, None)),
            12 => Some((6, None)),
            13 => Some((7, None)),
            14 => Some((8, None)),
            15 => Some((9, None)),
            16 => Some((10, None)),
            17 => Some((11, None)),
            18 => Some((12, None)),
            19 => Some((13, None)),
            20 => Some((14, None)),
            23 => Some((15, None)),
            24 => Some((16, None)),
            25 => Some((17, None)),
            26 => Some((18, None)),
            29 => Some((19, None)),
            30 => Some((20, None)),
            31 => Some((21, None)),
            32 => Some((22, None)),
            33 => Some((23, None)),
            _ => None,
        },
        "custom_model" => match row {
            4 => Some((0, None)),
            5 => Some((1, None)),
            8 => Some((2, None)),
            9 => Some((3, None)),
            10 => Some((4, None)),
            11 => Some((5, None)),
            12 => Some((6, None)),
            13 => Some((7, None)),
            14 => Some((8, None)),
            15 => Some((9, None)),
            16 => Some((10, None)),
            17 => Some((11, None)),
            18 => Some((12, None)),
            19 => Some((13, None)),
            20 => Some((14, None)),
            23 => Some((15, None)),
            24 => Some((16, None)),
            25 => Some((17, None)),
            26 => Some((18, None)),
            27 => Some((19, None)),
            28 => Some((20, None)),
            29 => Some((21, None)),
            30 => Some((22, None)),
            31 => Some((23, None)),
            32 => Some((24, None)),
            35 => Some((25, None)),
            36 => Some((26, None)),
            37 => Some((27, None)),
            38 => Some((28, None)),
            _ => None,
        },
        _ => match row {
            4 => Some((0, None)),
            5 => Some((1, None)),
            8 => Some((2, None)),
            9 => Some((3, None)),
            10 => Some((4, None)),
            11 => Some((5, None)),
            12 => Some((6, None)),
            13 => Some((7, None)),
            14 => Some((8, None)),
            15 => Some((9, None)),
            16 => Some((10, None)),
            17 => Some((11, None)),
            18 => Some((12, None)),
            19 => Some((13, None)),
            20 => Some((14, None)),
            26 => Some((15, None)),
            27 => Some((16, None)),
            28 => Some((17, None)),
            29 => Some((18, None)),
            30 => Some((19, None)),
            31 => Some((20, None)),
            _ => None,
        },
    }
}

fn settings_row_hit(
    settings: &SettingsState,
    config: &ConfigState,
    subagents: &SubAgentsState,
    row: usize,
) -> Option<(usize, Option<usize>)> {
    match settings.active_tab() {
        SettingsTab::Provider => row
            .checked_sub(4)
            .filter(|idx| *idx < 8)
            .map(|idx| (idx, None)),
        SettingsTab::Tools => row
            .checked_sub(4)
            .filter(|idx| *idx < 7)
            .map(|idx| (idx, None)),
        SettingsTab::WebSearch => row
            .checked_sub(4)
            .filter(|idx| *idx < 7)
            .map(|idx| (idx, None)),
        SettingsTab::Chat => row
            .checked_sub(4)
            .filter(|idx| *idx < 6)
            .map(|idx| (idx, None)),
        SettingsTab::Advanced => advanced_settings_row_hit(config, row),
        SettingsTab::Gateway => match row {
            4 => Some((0, None)),
            5 => Some((1, None)),
            8 => Some((2, None)),
            9 => Some((3, None)),
            12 => Some((4, None)),
            13 => Some((5, None)),
            16 => Some((6, None)),
            17 => Some((7, None)),
            18 => Some((8, None)),
            21 => Some((9, None)),
            r if settings.is_editing()
                && settings.is_textarea()
                && settings.editing_field() == Some("whatsapp_allowed_contacts") =>
            {
                let textarea_lines = settings.edit_buffer().lines().count().max(1);
                match r {
                    22..=23 => Some((9, None)),
                    line if line >= 24 && line < 24 + textarea_lines => Some((9, None)),
                    line if line == 24 + textarea_lines => Some((10, None)),
                    line if line == 25 + textarea_lines => Some((11, None)),
                    line if line == 26 + textarea_lines => Some((12, None)),
                    line if line == 27 + textarea_lines => Some((13, None)),
                    _ => None,
                }
            }
            22 => Some((10, None)),
            23 => Some((11, None)),
            24 => Some((12, None)),
            25 => Some((13, None)),
            _ => None,
        },
        SettingsTab::Auth => row
            .checked_sub(4)
            .filter(|idx| *idx < 3)
            .map(|idx| (idx, None)),
        SettingsTab::Agent => {
            if settings.is_editing()
                && settings.is_textarea()
                && settings.editing_field() == Some("system_prompt")
            {
                let prompt_lines = settings.edit_buffer().lines().count().max(1);
                match row {
                    4..=11 => Some((row - 4, None)),
                    r if (17..=20 + prompt_lines).contains(&r) => Some((8, None)),
                    r if r == 21 + prompt_lines => Some((9, None)),
                    _ => None,
                }
            } else {
                match row {
                    4..=11 => Some((row - 4, None)),
                    17 => Some((8, None)),
                    18 => Some((9, None)),
                    _ => None,
                }
            }
        }
        SettingsTab::SubAgents => {
            let list_len = subagents.entries.len();
            if list_len > 0 && (4..4 + list_len).contains(&row) {
                Some((0, Some(row - 4)))
            } else {
                match row {
                    r if r == 5 + list_len => Some((1, None)),
                    r if r == 6 + list_len => Some((2, None)),
                    r if r == 7 + list_len => Some((3, None)),
                    r if r == 8 + list_len => Some((4, None)),
                    _ => None,
                }
            }
        }
        SettingsTab::Concierge => row
            .checked_sub(4)
            .filter(|idx| *idx < 4)
            .map(|idx| (idx, None)),
        SettingsTab::Features => match row {
            // Tier & Security section: rows 4-5 => fields 0-1
            4 => Some((0, None)),
            5 => Some((1, None)),
            // Heartbeat section: rows 8-14 => fields 2-8
            8 => Some((2, None)),
            9 => Some((3, None)),
            10 => Some((4, None)),
            11 => Some((5, None)),
            12 => Some((6, None)),
            13 => Some((7, None)),
            14 => Some((8, None)),
            // Memory & Learning section: rows 17-20 => fields 9-11
            17 => Some((9, None)),
            18 => Some((10, None)),
            19 => Some((11, None)),
            // Skills section: rows 23-24 => fields 12-13
            23 => Some((12, None)),
            24 => Some((13, None)),
            _ => None,
        },
        SettingsTab::Plugins => None, // Plugin tab uses external navigation via PluginSettingsState
    }
}

fn auth_row_action_offsets(
    content_area: Rect,
    entry: &crate::state::auth::ProviderAuthEntry,
) -> (u16, u16, u16) {
    let primary_label = auth_primary_label(entry);
    let test_label = auth_secondary_label(entry);
    let actions_width =
        primary_label.chars().count() as u16 + 1 + test_label.chars().count() as u16;
    let primary_start = content_area
        .x
        .saturating_add(content_area.width.saturating_sub(actions_width));
    let primary_end = primary_start.saturating_add(primary_label.chars().count() as u16);
    let test_start = primary_end.saturating_add(1);
    (primary_start, primary_end, test_start)
}

fn auth_primary_label(entry: &crate::state::auth::ProviderAuthEntry) -> &'static str {
    match (
        entry.provider_id.as_str(),
        entry.authenticated,
        entry.auth_source.as_str(),
    ) {
        ("github-copilot", false, "github_copilot") => "[Token]",
        (_, true, _) => "[Logout]",
        _ => "[API Key]",
    }
}

fn auth_secondary_label(entry: &crate::state::auth::ProviderAuthEntry) -> &'static str {
    match (
        entry.provider_id.as_str(),
        entry.authenticated,
        entry.auth_source.as_str(),
    ) {
        ("openai", false, _) => "[ChatGPT]",
        ("github-copilot", false, "github_copilot") => "[Browser]",
        _ => "[Test]",
    }
}

fn auth_hit_test(
    content_area: Rect,
    auth: &crate::state::auth::AuthState,
    mouse: Position,
) -> Option<SettingsHitTarget> {
    let row = mouse.y.saturating_sub(content_area.y) as usize;
    let entry_index = row.checked_sub(4)?;
    let entry = auth.entries.get(entry_index)?;
    let (primary_start, primary_end, test_start) = auth_row_action_offsets(content_area, entry);
    if mouse.x >= primary_start && mouse.x < primary_end {
        Some(SettingsHitTarget::AuthAction {
            index: entry_index,
            action: AuthTabAction::Primary,
        })
    } else if mouse.x >= test_start {
        Some(SettingsHitTarget::AuthAction {
            index: entry_index,
            action: AuthTabAction::Test,
        })
    } else {
        Some(SettingsHitTarget::AuthProviderItem(entry_index))
    }
}

fn subagent_row_action_offsets(
    content_area: Rect,
    entry: &crate::state::subagents::SubAgentEntry,
) -> (u16, u16, u16, u16, u16) {
    let edit_label = "[Edit]";
    let delete_label = if entry.delete_allowed {
        "[Delete]"
    } else {
        "[Protected]"
    };
    let toggle_label = if entry.enabled {
        if entry.disable_allowed {
            "[Disable]"
        } else {
            "[Locked]"
        }
    } else {
        "[Enable]"
    };
    let actions_width = edit_label.chars().count() as u16
        + 1
        + delete_label.chars().count() as u16
        + 1
        + toggle_label.chars().count() as u16;
    let edit_start = content_area
        .x
        .saturating_add(content_area.width.saturating_sub(actions_width));
    let delete_start = edit_start.saturating_add(edit_label.chars().count() as u16 + 1);
    let toggle_start = delete_start.saturating_add(delete_label.chars().count() as u16 + 1);
    (
        edit_start,
        delete_start,
        toggle_start,
        delete_start.saturating_sub(1),
        toggle_start.saturating_add(toggle_label.chars().count() as u16),
    )
}

fn subagents_hit_test(
    content_area: Rect,
    subagents: &SubAgentsState,
    mouse: Position,
) -> Option<SettingsHitTarget> {
    let row = mouse.y.saturating_sub(content_area.y) as usize;
    let list_len = subagents.entries.len();
    if list_len > 0 && (4..4 + list_len).contains(&row) {
        let index = row - 4;
        if let Some(entry) = subagents.entries.get(index) {
            let (edit_start, delete_start, toggle_start, _, toggle_end) =
                subagent_row_action_offsets(content_area, entry);
            if mouse.x >= edit_start && mouse.x < delete_start.saturating_sub(1) {
                return Some(SettingsHitTarget::SubAgentRowAction {
                    index,
                    action: SubAgentTabAction::Edit,
                });
            }
            if mouse.x >= delete_start && mouse.x < toggle_start.saturating_sub(1) {
                return Some(SettingsHitTarget::SubAgentRowAction {
                    index,
                    action: SubAgentTabAction::Delete,
                });
            }
            if mouse.x >= toggle_start && mouse.x < toggle_end {
                return Some(SettingsHitTarget::SubAgentRowAction {
                    index,
                    action: SubAgentTabAction::Toggle,
                });
            }
        }
        return Some(SettingsHitTarget::SubAgentListItem(index));
    }
    match row {
        r if r == 5 + list_len => Some(SettingsHitTarget::SubAgentAction(SubAgentTabAction::Add)),
        _ => None,
    }
}

fn render_tab_content<'a>(
    content_width: u16,
    settings: &'a SettingsState,
    config: &'a ConfigState,
    modal: &'a ModalState,
    auth: &'a crate::state::auth::AuthState,
    subagents: &'a SubAgentsState,
    concierge: &'a ConciergeState,
    tier: &crate::state::tier::TierState,
    plugin_settings: &PluginSettingsState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    match settings.active_tab() {
        SettingsTab::Provider => render_provider_tab(settings, config, theme),
        SettingsTab::Tools => render_tools_tab(settings, config, theme),
        SettingsTab::WebSearch => render_websearch_tab(settings, config, theme),
        SettingsTab::Chat => render_chat_tab(settings, config, theme),
        SettingsTab::Gateway => render_gateway_tab(settings, config, modal, theme),
        SettingsTab::Auth => render_auth_tab(content_width, auth, config, theme),
        SettingsTab::Agent => render_agent_tab(settings, config, theme),
        SettingsTab::SubAgents => render_subagents_tab(content_width, settings, subagents, theme),
        SettingsTab::Concierge => render_concierge_tab(settings, concierge, theme),
        SettingsTab::Features => render_features_tab(settings, config, tier, theme),
        SettingsTab::Advanced => render_advanced_tab(settings, config, theme),
        SettingsTab::Plugins => render_plugins_tab(settings, plugin_settings, content_width, theme),
    }
}
