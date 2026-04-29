use super::*;
use zorai_protocol::{InboxNotification, InboxNotificationAction};

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

#[tokio::test]
async fn upsert_notification_preserves_existing_read_and_archive_state() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let mut archived = sample_notification("notif_1", 100, None, None);
    archived.read_at = Some(110);
    archived.archived_at = Some(120);
    store.upsert_notification(&archived).await?;

    let fresh_from_daemon = sample_notification("notif_1", 130, None, None);
    store.upsert_notification(&fresh_from_daemon).await?;

    let loaded = store.list_notifications(true, Some(10)).await?;

    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].updated_at, 130);
    assert_eq!(loaded[0].read_at, Some(110));
    assert_eq!(loaded[0].archived_at, Some(120));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn upsert_agent_event_preserves_notification_lifecycle_state() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let mut archived = sample_notification("notif_1", 100, None, None);
    archived.read_at = Some(110);
    archived.archived_at = Some(120);
    store
        .upsert_agent_event(&crate::notifications::notification_event_row(&archived)?)
        .await?;

    let fresh_from_client = sample_notification("notif_1", 130, None, None);
    store
        .upsert_agent_event(&crate::notifications::notification_event_row(
            &fresh_from_client,
        )?)
        .await?;

    let loaded = store.list_notifications(true, Some(10)).await?;

    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].updated_at, 130);
    assert_eq!(loaded[0].read_at, Some(110));
    assert_eq!(loaded[0].archived_at, Some(120));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn upsert_agent_event_does_not_create_sidecar_lexical_index() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let notification = sample_notification("notif_1", 100, None, None);

    let started = std::time::Instant::now();
    store
        .upsert_agent_event(&crate::notifications::notification_event_row(
            &notification,
        )?)
        .await?;

    assert!(
        started.elapsed() < std::time::Duration::from_millis(150),
        "notification upsert took longer than expected"
    );
    assert!(
        !root.join("search-index").exists(),
        "notification upsert should not create a sidecar lexical index"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}
