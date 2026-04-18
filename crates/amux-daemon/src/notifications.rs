#![allow(dead_code)]

use amux_protocol::{AgentEventRow, InboxNotification, InboxNotificationAction};
use anyhow::Result;

pub const NOTIFICATION_CATEGORY: &str = "notification";
pub const NOTIFICATION_KIND: &str = "inbox_entry";

pub fn notification_event_row(notification: &InboxNotification) -> Result<AgentEventRow> {
    Ok(AgentEventRow {
        id: notification.id.clone(),
        category: NOTIFICATION_CATEGORY.to_string(),
        kind: notification.kind.clone(),
        pane_id: None,
        workspace_id: None,
        surface_id: None,
        session_id: None,
        payload_json: serde_json::to_string(notification)?,
        timestamp: notification.updated_at,
    })
}

pub fn parse_notification_row(row: &AgentEventRow) -> Option<InboxNotification> {
    if row.category != NOTIFICATION_CATEGORY {
        return None;
    }
    serde_json::from_str::<InboxNotification>(&row.payload_json).ok()
}

pub fn plugin_settings_action(plugin_name: &str) -> InboxNotificationAction {
    InboxNotificationAction {
        id: "open_plugin_settings".to_string(),
        label: "Open plugin settings".to_string(),
        action_type: "open_plugin_settings".to_string(),
        target: Some(plugin_name.to_string()),
        payload_json: None,
    }
}

pub fn open_thread_action(thread_id: &str) -> InboxNotificationAction {
    InboxNotificationAction {
        id: "open_thread".to_string(),
        label: "Open thread".to_string(),
        action_type: "open_thread".to_string(),
        target: Some(thread_id.to_string()),
        payload_json: None,
    }
}
