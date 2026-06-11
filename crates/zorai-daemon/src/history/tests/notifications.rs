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
async fn list_notifications_filters_inactive_entries_before_limit() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let archived = sample_notification("archived-newest", 300, Some(310), None);
    let deleted = sample_notification("deleted-newer", 200, None, Some(210));
    let active = sample_notification("active-older", 100, None, None);

    store.upsert_notification(&archived).await?;
    store.upsert_notification(&deleted).await?;
    store.upsert_notification(&active).await?;

    let active_only = store.list_notifications(false, Some(1)).await?;

    assert_eq!(active_only, vec![active]);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_notifications_by_source_filters_source_before_limit() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let mut other = sample_notification("other-newest", 300, None, None);
    other.source = "anticipatory".to_string();
    let plugin = sample_notification("plugin-older", 100, None, None);

    store.upsert_notification(&other).await?;
    store.upsert_notification(&plugin).await?;

    let plugin_only = store
        .list_notifications_by_source("plugin_auth", true, Some(1))
        .await?;

    assert_eq!(plugin_only, vec![plugin]);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn archive_notifications_by_source_except_ids_updates_matching_active_rows_in_sql(
) -> Result<()> {
    let (store, root) = make_test_store().await?;
    let stale = sample_notification("stale-plugin", 100, None, None);
    let kept = sample_notification("kept-plugin", 110, None, None);
    let deleted = sample_notification("deleted-plugin", 120, None, Some(125));
    let mut other_source = sample_notification("other-source", 130, None, None);
    other_source.source = "anticipatory".to_string();

    store.upsert_notification(&stale).await?;
    store.upsert_notification(&kept).await?;
    store.upsert_notification(&deleted).await?;
    store.upsert_notification(&other_source).await?;

    let archived = store
        .archive_notifications_by_source_except_ids(
            "plugin_auth",
            &["kept-plugin".to_string()],
            500,
        )
        .await?;

    assert_eq!(archived, 1);
    let loaded = store
        .list_notifications_by_source("plugin_auth", true, Some(10))
        .await?;
    let by_id = loaded
        .into_iter()
        .map(|notification| (notification.id.clone(), notification))
        .collect::<std::collections::HashMap<_, _>>();

    assert_eq!(by_id["stale-plugin"].archived_at, Some(500));
    assert_eq!(by_id["stale-plugin"].updated_at, 500);
    assert_eq!(by_id["kept-plugin"].archived_at, None);
    assert_eq!(by_id["deleted-plugin"].deleted_at, Some(125));
    assert_eq!(by_id["deleted-plugin"].archived_at, None);

    let other_loaded = store
        .list_notifications_by_source("anticipatory", true, Some(10))
        .await?;
    assert_eq!(other_loaded[0].archived_at, None);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn mark_all_notifications_read_updates_every_unread_active_row_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let unread = sample_notification("unread", 100, None, None);
    let mut already_read = sample_notification("already-read", 110, None, None);
    already_read.read_at = Some(115);
    let archived = sample_notification("archived", 120, Some(125), None);
    let deleted = sample_notification("deleted", 130, None, Some(135));

    store.upsert_notification(&unread).await?;
    store.upsert_notification(&already_read).await?;
    store.upsert_notification(&archived).await?;
    store.upsert_notification(&deleted).await?;

    let updated = store.mark_all_notifications_read(500).await?;

    assert_eq!(updated, 1);
    let loaded = store.list_notifications(true, Some(10)).await?;
    let by_id = loaded
        .into_iter()
        .map(|notification| (notification.id.clone(), notification))
        .collect::<std::collections::HashMap<_, _>>();

    assert_eq!(by_id["unread"].read_at, Some(500));
    assert_eq!(by_id["unread"].updated_at, 500);
    assert_eq!(by_id["already-read"].read_at, Some(115));
    assert_eq!(by_id["already-read"].updated_at, 110);
    assert_eq!(by_id["archived"].read_at, None);
    assert_eq!(by_id["deleted"].read_at, None);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn archive_read_notifications_archives_read_rows_and_keeps_unread_visible() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let mut read = sample_notification("read", 100, None, None);
    read.read_at = Some(105);
    let unread = sample_notification("unread", 110, None, None);
    let mut already_archived = sample_notification("already-archived", 120, Some(125), None);
    already_archived.read_at = Some(122);

    store.upsert_notification(&read).await?;
    store.upsert_notification(&unread).await?;
    store.upsert_notification(&already_archived).await?;

    let updated = store.archive_read_notifications(600).await?;

    assert_eq!(updated, 1);
    let loaded = store.list_notifications(true, Some(10)).await?;
    let by_id = loaded
        .into_iter()
        .map(|notification| (notification.id.clone(), notification))
        .collect::<std::collections::HashMap<_, _>>();

    assert_eq!(by_id["read"].archived_at, Some(600));
    assert_eq!(by_id["read"].updated_at, 600);
    assert_eq!(by_id["unread"].archived_at, None);
    assert_eq!(by_id["already-archived"].archived_at, Some(125));

    let active = store.list_notifications(false, Some(10)).await?;
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, "unread");

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
