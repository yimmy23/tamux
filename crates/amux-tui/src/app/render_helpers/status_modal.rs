use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

use crate::client::AgentStatusSnapshotVm;
use crate::theme::ThemeTokens;

pub(super) fn format_status_modal_text(snapshot: &AgentStatusSnapshotVm) -> String {
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
            for action in actions.iter().take(5) {
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

    let hints = Line::from(vec![
        Span::styled("Esc", theme.fg_active),
        Span::styled(" close", theme.fg_dim),
        Span::raw("  "),
        Span::styled("j/k", theme.fg_active),
        Span::styled(" scroll", theme.fg_dim),
    ]);
    frame.render_widget(Paragraph::new(hints), layout[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_modal_formatter_includes_version_and_sections() {
        let rendered = format_status_modal_text(&AgentStatusSnapshotVm {
            tier: "mission_control".to_string(),
            activity: "waiting_for_operator".to_string(),
            active_thread_id: Some("thread-1".to_string()),
            active_goal_run_id: None,
            active_goal_run_title: Some("Close release gap".to_string()),
            provider_health_json: r#"{"openai":{"can_execute":true,"trip_count":0}}"#.to_string(),
            gateway_statuses_json: r#"{"slack":{"status":"connected"}}"#.to_string(),
            recent_actions_json: r#"[{"action_type":"tool_call","summary":"Ran status"}]"#
                .to_string(),
        });

        assert!(rendered.contains("Version:"));
        assert!(rendered.contains("Providers:"));
        assert!(rendered.contains("Recent Actions:"));
    }
}
