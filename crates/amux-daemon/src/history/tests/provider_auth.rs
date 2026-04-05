use super::*;

#[tokio::test]
async fn provider_auth_state_round_trips_by_provider_and_mode() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;

    assert!(
        store
            .load_provider_auth_state("openai", "chatgpt_subscription")
            .await?
            .is_none()
    );

    let state = serde_json::json!({
        "access_token": "token",
        "refresh_token": "refresh",
        "account_id": "acct_123",
        "expires_at": 12345,
    });
    store
        .save_provider_auth_state("openai", "chatgpt_subscription", &state)
        .await?;

    let row = store
        .load_provider_auth_state("openai", "chatgpt_subscription")
        .await?
        .expect("provider auth state should exist");
    assert_eq!(row.provider_id, "openai");
    assert_eq!(row.auth_mode, "chatgpt_subscription");
    assert_eq!(row.state_json, state);

    assert!(
        store
            .load_provider_auth_state("openai", "github_copilot")
            .await?
            .is_none()
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn provider_auth_state_delete_removes_only_target_mode() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;

    store
        .save_provider_auth_state(
            "openai",
            "chatgpt_subscription",
            &serde_json::json!({ "access_token": "token-a" }),
        )
        .await?;
    store
        .save_provider_auth_state(
            "openai",
            "api_key",
            &serde_json::json!({ "access_token": "token-b" }),
        )
        .await?;

    store
        .delete_provider_auth_state("openai", "chatgpt_subscription")
        .await?;

    assert!(
        store
            .load_provider_auth_state("openai", "chatgpt_subscription")
            .await?
            .is_none()
    );
    assert!(
        store
            .load_provider_auth_state("openai", "api_key")
            .await?
            .is_some()
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn replay_cursor_round_trips_by_platform_and_channel() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;

    // Initially none
    let none = store
        .load_gateway_replay_cursor("whatsapp", "chat@server")
        .await?;
    assert!(none.is_none());

    store
        .save_gateway_replay_cursor("whatsapp", "chat@server", "msg-1000", "message_id")
        .await?;

    let row = store
        .load_gateway_replay_cursor("whatsapp", "chat@server")
        .await?
        .expect("cursor should exist");
    assert_eq!(row.platform, "whatsapp");
    assert_eq!(row.channel_id, "chat@server");
    assert_eq!(row.cursor_value, "msg-1000");
    assert_eq!(row.cursor_type, "message_id");

    // different channel should be none
    assert!(
        store
            .load_gateway_replay_cursor("whatsapp", "other")
            .await?
            .is_none()
    );

    let rows = store.load_gateway_replay_cursors("whatsapp").await?;
    assert_eq!(rows.len(), 1);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn replay_cursor_upsert_replaces_existing() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;

    store
        .save_gateway_replay_cursor("telegram", "chan1", "v1", "message_id")
        .await?;
    let row = store
        .load_gateway_replay_cursor("telegram", "chan1")
        .await?
        .expect("cursor should exist");
    assert_eq!(row.cursor_value, "v1");

    store
        .save_gateway_replay_cursor("telegram", "chan1", "v2", "message_id")
        .await?;
    let row2 = store
        .load_gateway_replay_cursor("telegram", "chan1")
        .await?
        .expect("cursor should exist");
    assert_eq!(row2.cursor_value, "v2");

    fs::remove_dir_all(root)?;
    Ok(())
}
