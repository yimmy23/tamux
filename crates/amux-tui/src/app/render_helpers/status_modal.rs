use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

use crate::client::AgentStatusSnapshotVm;
use crate::theme::ThemeTokens;

fn footer_hints(show_scroll_hint: bool, theme: &ThemeTokens) -> Line<'static> {
    let mut hints = vec![
        Span::styled("Esc", theme.fg_active),
        Span::styled(" close", theme.fg_dim),
    ];
    if show_scroll_hint {
        hints.push(Span::raw("  "));
        hints.push(Span::styled("j/k", theme.fg_active));
        hints.push(Span::styled(" scroll", theme.fg_dim));
    }
    Line::from(hints)
}

pub(super) fn format_status_modal_text(
    snapshot: &AgentStatusSnapshotVm,
    diagnostics_json: Option<&str>,
) -> String {
    let mut rendered = String::from("Agent Status\n============\n");
    rendered.push_str(&format!("Version:  {}\n", env!("CARGO_PKG_VERSION")));
    rendered.push_str(&format!("Tier:     {}\n", snapshot.tier.replace('_', " ")));
    rendered.push_str(&format!(
        "Activity: {}\n",
        snapshot.activity.replace('_', " ")
    ));

    if let Some(title) = &snapshot.active_goal_run_title {
        rendered.push_str(&format!("Goal:     {title}\n"));
    }
    if let Some(thread) = &snapshot.active_thread_id {
        rendered.push_str(&format!("Thread:   {thread}\n"));
    }

    if let Ok(providers) = serde_json::from_str::<serde_json::Value>(&snapshot.provider_health_json)
    {
        if let Some(obj) = providers.as_object() {
            if !obj.is_empty() {
                rendered.push_str("\nProviders:\n");
                for (name, info) in obj {
                    let can_execute = info
                        .get("can_execute")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(true);
                    let trips = info
                        .get("trip_count")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(0);
                    let health = if can_execute { "healthy" } else { "tripped" };
                    if trips > 0 {
                        rendered.push_str(&format!("  {name} - {health} (trips: {trips})\n"));
                    } else {
                        rendered.push_str(&format!("  {name} - {health}\n"));
                    }
                }
            }
        }
    }

    if let Ok(gateways) = serde_json::from_str::<serde_json::Value>(&snapshot.gateway_statuses_json)
    {
        if let Some(obj) = gateways.as_object() {
            if !obj.is_empty() {
                rendered.push_str("\nGateways:\n");
                for (platform, info) in obj {
                    let gateway_status = info
                        .get("status")
                        .and_then(|value| value.as_str())
                        .unwrap_or("unknown");
                    rendered.push_str(&format!("  {platform} - {gateway_status}\n"));
                }
            }
        }
    }

    if let Ok(actions) =
        serde_json::from_str::<Vec<serde_json::Value>>(&snapshot.recent_actions_json)
    {
        if !actions.is_empty() {
            rendered.push_str("\nRecent Actions:\n");
            for action in &actions {
                let action_type = action
                    .get("action_type")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let summary = action
                    .get("summary")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                rendered.push_str(&format!("  [{action_type}] {summary}\n"));
            }
        }
    }

    if let Some(diagnostics_json) = diagnostics_json {
        if let Ok(diagnostics) = serde_json::from_str::<serde_json::Value>(diagnostics_json) {
            if let Some(aline) = diagnostics.get("aline").and_then(|value| value.as_object()) {
                rendered.push_str("\nAline:\n");
                let available = aline
                    .get("available")
                    .and_then(|value| value.as_bool())
                    .map(|value| if value { "yes" } else { "no" })
                    .unwrap_or("unknown");
                let watcher_state = aline
                    .get("watcher_state")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown");
                let watcher_started = aline
                    .get("watcher_started")
                    .and_then(|value| value.as_bool())
                    .map(|value| if value { "yes" } else { "no" })
                    .unwrap_or("no");
                let discovered_count = aline
                    .get("discovered_count")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0);
                let selected_count = aline
                    .get("selected_count")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0);
                let imported_count = aline
                    .get("imported_count")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0);
                let generated_count = aline
                    .get("generated_count")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0);
                let short_circuit_reason = aline
                    .get("short_circuit_reason")
                    .and_then(|value| value.as_str())
                    .map(|value| value.replace('_', " "));
                let failure_stage = aline.get("failure_stage").and_then(|value| value.as_str());
                let failure_message = aline
                    .get("failure_message")
                    .and_then(|value| value.as_str());

                rendered.push_str(&format!("  Available: {available}\n"));
                rendered.push_str(&format!("  Watcher:   {watcher_state}\n"));
                rendered.push_str(&format!("  Started:   {watcher_started}\n"));
                rendered.push_str(&format!(
                    "  Sessions:  discovered {discovered_count}, selected {selected_count}, imported {imported_count}, generated {generated_count}\n"
                ));
                if let Some(reason) = short_circuit_reason {
                    rendered.push_str(&format!("  Result:    {reason}\n"));
                }
                if let Some(stage) = failure_stage {
                    if let Some(message) = failure_message {
                        rendered.push_str(&format!("  Failure:   {stage} - {message}\n"));
                    } else {
                        rendered.push_str(&format!("  Failure:   {stage}\n"));
                    }
                }
            }
            if let Some(skill_mesh) = diagnostics
                .get("skill_mesh")
                .and_then(|value| value.as_object())
            {
                rendered.push_str("\nSkill Mesh:\n");
                let backend = skill_mesh
                    .get("backend")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown");
                let state = skill_mesh
                    .get("state")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown");
                rendered.push_str(&format!("  Backend:   {backend}\n"));
                rendered.push_str(&format!("  State:     {state}\n"));
                if let Some(active_gate) = skill_mesh
                    .get("active_gate")
                    .and_then(|value| value.as_object())
                {
                    if let Some(skill) = active_gate
                        .get("recommended_skill")
                        .and_then(|value| value.as_str())
                    {
                        rendered.push_str(&format!("  Gate:      {skill}\n"));
                    }
                    if let Some(action) = active_gate
                        .get("recommended_action")
                        .and_then(|value| value.as_str())
                    {
                        rendered.push_str(&format!("  Action:    {action}\n"));
                    }
                    let approval = active_gate
                        .get("requires_approval")
                        .and_then(|value| value.as_bool())
                        .map(|value| if value { "yes" } else { "no" })
                        .unwrap_or("no");
                    rendered.push_str(&format!("  Approval:  {approval}\n"));
                    if let Some(rationale) = active_gate
                        .get("rationale")
                        .and_then(|value| value.as_array())
                        .and_then(|items| items.first())
                        .and_then(|value| value.as_str())
                    {
                        rendered.push_str(&format!("  Why:       {rationale}\n"));
                    }
                    if let Some(family) = active_gate
                        .get("capability_family")
                        .and_then(|value| value.as_array())
                    {
                        let joined = family
                            .iter()
                            .filter_map(|value| value.as_str())
                            .collect::<Vec<_>>()
                            .join(" / ");
                        if !joined.is_empty() {
                            rendered.push_str(&format!("  Family:    {joined}\n"));
                        }
                    }
                }
            }
        }
    }

    rendered.trim_end().to_string()
}

pub(super) fn format_prompt_modal_text(prompt: &crate::client::AgentPromptInspectionVm) -> String {
    let mut rendered = String::from("Agent Prompt\n============\n");
    rendered.push_str(&format!(
        "Agent:    {} ({})\n",
        prompt.agent_name, prompt.agent_id
    ));
    rendered.push_str(&format!("Provider: {}\n", prompt.provider_id));
    rendered.push_str(&format!("Model:    {}\n", prompt.model));

    for section in &prompt.sections {
        rendered.push_str("\n");
        rendered.push_str(&format!("[{}]\n", section.title));
        rendered.push_str(section.content.trim());
        rendered.push('\n');
    }

    rendered.push_str("\nFinal Prompt\n------------\n");
    rendered.push_str(prompt.final_prompt.trim());
    rendered.trim_end().to_string()
}

pub(super) fn render_status_modal(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    body: &str,
    scroll: usize,
    show_scroll_hint: bool,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);

    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(body)
            .wrap(Wrap { trim: false })
            .scroll((scroll.min(u16::MAX as usize) as u16, 0)),
        layout[0],
    );

    let hints = footer_hints(show_scroll_hint, theme);
    frame.render_widget(Paragraph::new(hints), layout[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_modal_formatter_includes_version_and_sections() {
        let rendered = format_status_modal_text(
            &AgentStatusSnapshotVm {
                tier: "mission_control".to_string(),
                activity: "waiting_for_operator".to_string(),
                active_thread_id: Some("thread-1".to_string()),
                active_goal_run_id: None,
                active_goal_run_title: Some("Close release gap".to_string()),
                provider_health_json: r#"{"openai":{"can_execute":true,"trip_count":0}}"#
                    .to_string(),
                gateway_statuses_json: r#"{"slack":{"status":"connected"}}"#.to_string(),
                recent_actions_json: r#"[{"action_type":"tool_call","summary":"Ran status"}]"#
                    .to_string(),
            },
            Some(
                r#"{"aline":{"available":true,"watcher_state":"running"},"skill_mesh":{"backend":"mesh","state":"fresh","active_gate":{"recommended_skill":"systematic-debugging","recommended_action":"request_approval systematic-debugging","requires_approval":true,"skill_read_completed":true,"rationale":["matched debug panic"],"capability_family":["development","debugging"]}}}"#,
            ),
        );

        assert!(rendered.contains("Version:"));
        assert!(rendered.contains("Providers:"));
        assert!(rendered.contains("Recent Actions:"));
        assert!(rendered.contains("Aline:"));
        assert!(rendered.contains("Watcher:"));
        assert!(rendered.contains("running"));
        assert!(rendered.contains("Skill Mesh:"));
        assert!(rendered.contains("Backend:   mesh"));
        assert!(rendered.contains("State:     fresh"));
        assert!(rendered.contains("Gate:      systematic-debugging"));
        assert!(rendered.contains("Action:    request_approval systematic-debugging"));
        assert!(rendered.contains("Approval:  yes"));
        assert!(rendered.contains("Why:       matched debug panic"));
        assert!(rendered.contains("Family:    development / debugging"));
    }

    #[test]
    fn footer_hints_hide_scroll_copy_when_scrolling_is_disabled() {
        let theme = ThemeTokens::default();
        let hints = footer_hints(false, &theme).to_string();
        assert!(!hints.contains("scroll"));
    }

    #[test]
    fn footer_hints_show_scroll_copy_when_scrolling_is_enabled() {
        let theme = ThemeTokens::default();
        let hints = footer_hints(true, &theme).to_string();
        assert!(hints.contains("j/k"));
        assert!(hints.contains("scroll"));
    }
}
