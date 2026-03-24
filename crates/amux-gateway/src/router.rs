//! Message router: translates incoming chat platform messages into daemon
//! managed-command requests.
//!
//! The routing logic recognises these command prefixes:
//!
//! - `!<command>`    — bang prefix (e.g. `!ls -la`)
//! - `/run <command>` — explicit run prefix (e.g. `/run cargo build`)
//! - `/task <description>` or `/queue <description>` — enqueue a daemon task
//! - `/schedule <when> -- <description-or-command>` — enqueue a scheduled task
//! - `/remind <when> -- <message>` — schedule a reminder back to the same chat destination
//! - `/tasks` — list queued daemon tasks
//! - `/cancel-task <id>` — cancel a daemon task
//!
//! Messages that do not match either prefix are ignored (return `None`).

use amux_protocol::{ManagedCommandRequest, ManagedCommandSource, SecurityLevel};

#[derive(Debug, Clone)]
pub struct GatewayTaskRequest {
    pub title: String,
    pub description: String,
    pub priority: String,
    pub command: Option<String>,
    pub session_id: Option<String>,
    pub dependencies: Vec<String>,
    pub scheduled_at: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum GatewayAction {
    ManagedCommand(ManagedCommandRequest),
    EnqueueTask(GatewayTaskRequest),
    ListTasks,
    CancelTask { task_id: String },
}

// ---------------------------------------------------------------------------
// Gateway message / response types
// ---------------------------------------------------------------------------

/// An inbound message received from a chat platform.
#[derive(Debug, Clone)]
pub struct GatewayMessage {
    /// Platform name (e.g. "slack", "telegram", "discord").
    pub platform: String,
    /// Platform-specific channel identifier.
    pub channel_id: String,
    /// Platform-specific user identifier.
    pub user_id: String,
    /// Raw message text.
    pub text: String,
    /// Unix timestamp (seconds since epoch).
    pub timestamp: u64,
}

/// A response to be delivered back to a chat platform channel.
#[derive(Debug, Clone)]
pub struct GatewayResponse {
    /// Text body of the response.
    pub text: String,
    /// Target channel identifier.
    pub channel_id: String,
}

// ---------------------------------------------------------------------------
// Routing logic
// ---------------------------------------------------------------------------

/// Attempt to extract a daemon command from an incoming chat message.
///
/// Returns a gateway action when the message matches a recognised command
/// prefix, or `None` if it should be ignored.
pub fn route_message(msg: &GatewayMessage) -> Option<GatewayAction> {
    let text = msg.text.trim();

    if text.eq_ignore_ascii_case("/tasks") {
        return Some(GatewayAction::ListTasks);
    }

    if let Some(task_id) = text.strip_prefix("/cancel-task ") {
        let task_id = task_id.trim();
        if !task_id.is_empty() {
            return Some(GatewayAction::CancelTask {
                task_id: task_id.to_string(),
            });
        }
        return None;
    }

    if let Some(description) = text
        .strip_prefix("/task ")
        .or_else(|| text.strip_prefix("/queue "))
    {
        return build_task_action(msg, description.trim(), None);
    }

    if let Some(spec) = text.strip_prefix("/schedule ") {
        let (when, payload) = spec.split_once(" -- ")?;
        let scheduled_at = parse_schedule_time(when.trim()).ok()?;
        return build_task_action(msg, payload.trim(), Some(scheduled_at));
    }

    if let Some(spec) = text.strip_prefix("/remind ") {
        let (when, message) = spec.split_once(" -- ")?;
        let scheduled_at = parse_schedule_time(when.trim()).ok()?;
        let reminder = message.trim();
        if reminder.is_empty() {
            return None;
        }
        return Some(GatewayAction::EnqueueTask(build_reminder_task(
            msg,
            reminder,
            scheduled_at,
        )));
    }

    // Match `!<command>` or `/run <command>`.
    let command = if let Some(cmd) = text.strip_prefix('!') {
        cmd.trim().to_string()
    } else if let Some(cmd) = text.strip_prefix("/run ") {
        cmd.trim().to_string()
    } else {
        return None;
    };

    if command.is_empty() {
        return None;
    }

    Some(GatewayAction::ManagedCommand(ManagedCommandRequest {
        command,
        rationale: format!(
            "Gateway command from {} user {} in #{}",
            msg.platform, msg.user_id, msg.channel_id,
        ),
        allow_network: true,
        sandbox_enabled: true,
        security_level: SecurityLevel::Moderate,
        cwd: None,
        language_hint: None,
        source: ManagedCommandSource::Gateway,
    }))
}

fn build_task_action(
    msg: &GatewayMessage,
    payload: &str,
    scheduled_at: Option<u64>,
) -> Option<GatewayAction> {
    if payload.is_empty() {
        return None;
    }

    let command = if let Some(cmd) = payload.strip_prefix('!') {
        let trimmed = cmd.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    } else if let Some(cmd) = payload.strip_prefix("/run ") {
        let trimmed = cmd.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    } else {
        None
    };

    let title = if let Some(command) = command.as_deref() {
        truncate_title(&format!("Scheduled command: {command}"))
    } else {
        truncate_title(payload)
    };

    let mut description = String::new();
    if let Some(command) = command.as_deref() {
        description.push_str("Run the scheduled gateway command when it becomes eligible.");
        description.push_str(
            "\nUse execute_managed_command unless a different daemon tool is more appropriate.",
        );
        description.push_str(&format!("\nCommand: {command}"));
    } else {
        description.push_str(payload);
    }
    description.push_str("\n\nGateway origin:");
    description.push_str(&format!("\n- platform: {}", msg.platform));
    description.push_str(&format!("\n- channel_id: {}", msg.channel_id));
    description.push_str(&format!("\n- user_id: {}", msg.user_id));
    description.push_str("\nIf the task needs to send a follow-up message back to this same destination, use the matching gateway messaging tool with these identifiers.");

    Some(GatewayAction::EnqueueTask(GatewayTaskRequest {
        title,
        description,
        priority: "normal".to_string(),
        command,
        session_id: None,
        dependencies: Vec::new(),
        scheduled_at,
    }))
}

fn build_reminder_task(
    msg: &GatewayMessage,
    reminder: &str,
    scheduled_at: u64,
) -> GatewayTaskRequest {
    let mut description = format!(
        "Send the following reminder back to the same {} destination at the scheduled time:\n\n{}",
        msg.platform, reminder
    );
    match msg.platform.as_str() {
        "discord" => {
            description.push_str(&format!(
                "\n\nUse send_discord_message with channel_id='{}'.",
                msg.channel_id
            ));
        }
        "slack" => {
            description.push_str(&format!(
                "\n\nUse send_slack_message with channel='{}'.",
                msg.channel_id
            ));
        }
        "telegram" => {
            description.push_str(&format!(
                "\n\nUse send_telegram_message with chat_id='{}'.",
                msg.channel_id
            ));
        }
        _ => {
            description
                .push_str("\n\nUse the matching gateway messaging tool for this destination.");
        }
    }
    description.push_str(&format!("\nOrigin user_id: {}", msg.user_id));

    GatewayTaskRequest {
        title: truncate_title(&format!("Reminder: {reminder}")),
        description,
        priority: "normal".to_string(),
        command: None,
        session_id: None,
        dependencies: Vec::new(),
        scheduled_at: Some(scheduled_at),
    }
}

fn parse_schedule_time(value: &str) -> Result<u64, String> {
    if value.is_empty() {
        return Err("empty schedule time".to_string());
    }

    if let Ok(duration) = humantime::parse_duration(value) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis() as u64;
        return Ok(now_ms.saturating_add(duration.as_millis() as u64));
    }

    let at = humantime::parse_rfc3339_weak(value).map_err(|error| error.to_string())?;
    Ok(at
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis() as u64)
}

fn truncate_title(value: &str) -> String {
    let mut title = value.lines().next().unwrap_or(value).trim().to_string();
    if title.len() > 72 {
        title.truncate(69);
        title.push_str("...");
    }
    title
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(text: &str) -> GatewayMessage {
        GatewayMessage {
            platform: "test".into(),
            channel_id: "ch-1".into(),
            user_id: "u-1".into(),
            text: text.into(),
            timestamp: 0,
        }
    }

    #[test]
    fn bang_prefix_routes() {
        let GatewayAction::ManagedCommand(req) = route_message(&make_msg("!ls -la")).unwrap()
        else {
            panic!("expected managed command");
        };
        assert_eq!(req.command, "ls -la");
    }

    #[test]
    fn run_prefix_routes() {
        let GatewayAction::ManagedCommand(req) =
            route_message(&make_msg("/run cargo build")).unwrap()
        else {
            panic!("expected managed command");
        };
        assert_eq!(req.command, "cargo build");
    }

    #[test]
    fn plain_text_ignored() {
        assert!(route_message(&make_msg("hello world")).is_none());
    }

    #[test]
    fn empty_bang_ignored() {
        assert!(route_message(&make_msg("!")).is_none());
    }

    #[test]
    fn whitespace_trimmed() {
        let GatewayAction::ManagedCommand(req) = route_message(&make_msg("  !echo hi  ")).unwrap()
        else {
            panic!("expected managed command");
        };
        assert_eq!(req.command, "echo hi");
    }

    #[test]
    fn schedule_task_routes() {
        let GatewayAction::EnqueueTask(task) =
            route_message(&make_msg("/schedule 10m -- remind me to review the PR")).unwrap()
        else {
            panic!("expected queued task");
        };
        assert!(task.scheduled_at.is_some());
        assert!(task.command.is_none());
        assert!(task.description.contains("review the PR"));
    }

    #[test]
    fn remind_routes() {
        let GatewayAction::EnqueueTask(task) =
            route_message(&make_msg("/remind 5m -- Standup in five minutes")).unwrap()
        else {
            panic!("expected reminder task");
        };
        assert!(task.scheduled_at.is_some());
        assert!(task.description.contains("Standup in five minutes"));
    }
}
