use super::*;
use amux_protocol::{InboxNotification, InboxNotificationAction};

fn sample_notification(
    id: &str,
    updated_at: i64,
    archived_at: Option<i64>,
    deleted_at: Option<i64>,
) -> InboxNotification {
    InboxNotification {
        id: id.to_string(),
        source: "plugin_auth".to_string(),
        kind: "plugin_needs_reconnect".to_string(),
        title: format!("{id} title"),
        body: format!("{id} body"),
        subtitle: Some("plugin reminder".to_string()),
        severity: "warning".to_string(),
        created_at: updated_at - 10,
        updated_at,
        read_at: None,
        archived_at,
        deleted_at,
        actions: vec![InboxNotificationAction {
            id: "open_plugin_settings".to_string(),
            label: "Open plugin settings".to_string(),
            action_type: "open_plugin_settings".to_string(),
            target: Some("gmail".to_string()),
            payload_json: None,
        }],
        metadata_json: None,
    }
}

#[tokio::test]
async fn notifications_roundtrip_through_agent_events_storage() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let notification = sample_notification("notif_1", 100, None, None);

    store.upsert_notification(&notification).await?;

    let loaded = store.list_notifications(false, Some(10)).await?;

    assert_eq!(loaded, vec![notification]);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_notifications_filters_archived_and_deleted_entries() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let active = sample_notification("active", 300, None, None);
    let archived = sample_notification("archived", 200, Some(210), None);
    let deleted = sample_notification("deleted", 100, None, Some(110));

    store.upsert_notification(&active).await?;
    store.upsert_notification(&archived).await?;
    store.upsert_notification(&deleted).await?;

    let active_only = store.list_notifications(false, Some(10)).await?;
    let all = store.list_notifications(true, Some(10)).await?;

    assert_eq!(active_only, vec![active.clone()]);
    assert_eq!(all, vec![active, archived, deleted]);

    fs::remove_dir_all(root)?;
    Ok(())
}
