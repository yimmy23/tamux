use super::*;

// ── Memory tombstone tests (Phase 5) ─────────────────────────────────

#[tokio::test]
async fn memory_tombstone_insert_and_list_round_trips() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .insert_memory_tombstone(
            "t-1",
            "soul",
            "old fact about rust",
            Some("rust_version"),
            Some("Rust 1.80"),
            "consolidation",
            None,
            1000,
        )
        .await?;
    store
        .insert_memory_tombstone(
            "t-2",
            "memory",
            "stale project note",
            None,
            None,
            "decay",
            Some("prov-1"),
            2000,
        )
        .await?;

    // List all
    let all = store.list_memory_tombstones(None, 10).await?;
    assert_eq!(all.len(), 2);
    // Ordered by created_at DESC
    assert_eq!(all[0].id, "t-2");
    assert_eq!(all[1].id, "t-1");

    // List by target
    let soul_only = store.list_memory_tombstones(Some("soul"), 10).await?;
    assert_eq!(soul_only.len(), 1);
    assert_eq!(soul_only[0].id, "t-1");
    assert_eq!(soul_only[0].original_content, "old fact about rust");
    assert_eq!(soul_only[0].fact_key.as_deref(), Some("rust_version"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn memory_tombstone_delete_expired() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .insert_memory_tombstone("old", "soul", "ancient", None, None, "decay", None, 100)
        .await?;
    store
        .insert_memory_tombstone("new", "soul", "recent", None, None, "decay", None, 5000)
        .await?;

    // Delete tombstones older than 4000ms as of now=6000
    let deleted = store.delete_expired_tombstones(4000, 6000).await?;
    assert_eq!(deleted, 1);

    let remaining = store.list_memory_tombstones(None, 10).await?;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, "new");

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn memory_tombstone_restore() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .insert_memory_tombstone(
            "t-restore",
            "memory",
            "fact to restore",
            None,
            None,
            "decay",
            None,
            3000,
        )
        .await?;

    let restored = store.restore_tombstone("t-restore").await?;
    assert!(restored.is_some());
    let row = restored.unwrap();
    assert_eq!(row.original_content, "fact to restore");

    // Should be deleted after restore
    let remaining = store.list_memory_tombstones(None, 10).await?;
    assert_eq!(remaining.len(), 0);

    // Restoring non-existent returns None
    let none = store.restore_tombstone("t-restore").await?;
    assert!(none.is_none());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn gateway_thread_bindings_round_trip() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .upsert_gateway_thread_binding("Slack:C123", "thread_a", 1000)
        .await?;
    store
        .upsert_gateway_thread_binding("Discord:999", "thread_b", 1100)
        .await?;
    // overwrite existing binding
    store
        .upsert_gateway_thread_binding("Slack:C123", "thread_c", 1200)
        .await?;

    let bindings = store.list_gateway_thread_bindings().await?;
    let map: std::collections::HashMap<String, String> = bindings.into_iter().collect();
    assert_eq!(map.get("Slack:C123").map(String::as_str), Some("thread_c"));
    assert_eq!(map.get("Discord:999").map(String::as_str), Some("thread_b"));

    store.delete_gateway_thread_binding("Discord:999").await?;
    let bindings = store.list_gateway_thread_bindings().await?;
    let map: std::collections::HashMap<String, String> = bindings.into_iter().collect();
    assert_eq!(map.get("Slack:C123").map(String::as_str), Some("thread_c"));
    assert!(!map.contains_key("Discord:999"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn gateway_route_modes_round_trip() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .upsert_gateway_route_mode("Slack:C123", amux_protocol::AGENT_ID_SWAROG, 1000)
        .await?;
    store
        .upsert_gateway_route_mode("Discord:999", amux_protocol::AGENT_ID_RAROG, 1100)
        .await?;
    store
        .upsert_gateway_route_mode("Slack:C123", amux_protocol::AGENT_ID_RAROG, 1200)
        .await?;

    let modes = store.list_gateway_route_modes().await?;
    let map: std::collections::HashMap<String, String> = modes.into_iter().collect();
    assert_eq!(
        map.get("Slack:C123").map(String::as_str),
        Some(amux_protocol::AGENT_ID_RAROG)
    );
    assert_eq!(
        map.get("Discord:999").map(String::as_str),
        Some(amux_protocol::AGENT_ID_RAROG)
    );

    store.delete_gateway_route_mode("Discord:999").await?;
    let modes = store.list_gateway_route_modes().await?;
    let map: std::collections::HashMap<String, String> = modes.into_iter().collect();
    assert_eq!(
        map.get("Slack:C123").map(String::as_str),
        Some(amux_protocol::AGENT_ID_RAROG)
    );
    assert!(!map.contains_key("Discord:999"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn whatsapp_provider_state_round_trip_and_delete() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let state = WhatsAppProviderStateRow {
        provider_id: "whatsapp_web".to_string(),
        linked_phone: Some("+15551234567".to_string()),
        auth_json: Some("{\"session\":true}".to_string()),
        metadata_json: Some("{\"source\":\"test\"}".to_string()),
        last_reset_at: Some(123),
        last_linked_at: Some(456),
        updated_at: 789,
    };

    store.upsert_whatsapp_provider_state(state.clone()).await?;
    let loaded = store
        .get_whatsapp_provider_state("whatsapp_web")
        .await?
        .expect("provider state should exist");
    assert_eq!(loaded.provider_id, state.provider_id);
    assert_eq!(loaded.linked_phone, state.linked_phone);
    assert_eq!(loaded.auth_json, state.auth_json);
    assert_eq!(loaded.metadata_json, state.metadata_json);
    assert_eq!(loaded.last_reset_at, state.last_reset_at);
    assert_eq!(loaded.last_linked_at, state.last_linked_at);
    assert_eq!(loaded.updated_at, state.updated_at);

    store.delete_whatsapp_provider_state("whatsapp_web").await?;
    assert!(
        store
            .get_whatsapp_provider_state("whatsapp_web")
            .await?
            .is_none()
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn operator_profile_sessions_round_trip() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .upsert_operator_profile_session(
            "sess-a",
            "onboarding",
            r#"{"session_id":"sess-a","kind":"onboarding"}"#,
            1000,
        )
        .await?;
    store
        .upsert_operator_profile_session(
            "sess-b",
            "retrospective",
            r#"{"session_id":"sess-b","kind":"retrospective"}"#,
            1100,
        )
        .await?;
    store
        .upsert_operator_profile_session(
            "sess-a",
            "onboarding",
            r#"{"session_id":"sess-a","kind":"onboarding","state":"updated"}"#,
            1200,
        )
        .await?;

    let rows = store.list_operator_profile_sessions().await?;
    let mut by_id = std::collections::HashMap::new();
    for row in rows {
        by_id.insert(row.session_id.clone(), row);
    }
    assert_eq!(
        by_id.get("sess-a").map(|row| row.kind.as_str()),
        Some("onboarding")
    );
    assert_eq!(by_id.get("sess-a").map(|row| row.updated_at), Some(1200));
    assert_eq!(
        by_id.get("sess-b").map(|row| row.kind.as_str()),
        Some("retrospective")
    );

    store.delete_operator_profile_session("sess-b").await?;
    let rows = store.list_operator_profile_sessions().await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].session_id, "sess-a");

    fs::remove_dir_all(root)?;
    Ok(())
}
