use super::*;

#[test]
fn zorai_device_props_uses_desktop_platform_and_package_version() {
    let (os_name, version, platform_type) = zorai_device_props();
    assert_eq!(os_name.as_deref(), Some("Zorai"));
    assert_eq!(platform_type, Some(wa::device_props::PlatformType::Desktop));
    assert!(version.and_then(|value| value.primary).is_some());
}

#[test]
fn outbound_self_chat_messages_get_zorai_prefix() {
    let expected = format!("{}Hello", zorai_self_chat_prefix());
    assert_eq!(format_outbound_whatsapp_text("Hello", true), expected);
}

#[test]
fn outbound_non_self_chat_messages_keep_original_text() {
    assert_eq!(format_outbound_whatsapp_text("Hello", false), "Hello");
}

#[test]
fn outbound_self_chat_prefix_is_idempotent() {
    let prefixed = format!("{}Hello", zorai_self_chat_prefix());
    assert_eq!(format_outbound_whatsapp_text(&prefixed, true), prefixed);
}

#[test]
fn whatsapp_replay_cursor_roundtrips() {
    let cursor = build_whatsapp_cursor(1700000000, "msg-abc-123");
    assert_eq!(cursor, "1700000000:msg-abc-123");
    assert_eq!(
        parse_whatsapp_cursor(&cursor),
        Some((1700000000, "msg-abc-123"))
    );
}

#[test]
fn whatsapp_replay_cursor_parses_ts_only_prefix() {
    assert_eq!(
        parse_whatsapp_cursor("1700000001:some:extra:colons"),
        Some((1700000001, "some:extra:colons"))
    );
}

#[test]
fn whatsapp_replay_cursor_rejects_non_numeric_legacy_format() {
    assert_eq!(parse_whatsapp_cursor("wamid-ABC123"), None);
    assert_eq!(parse_whatsapp_cursor(""), None);
}

#[test]
fn whatsapp_self_chat_messages_still_enqueue_but_prefixed_echoes_do_not() {
    let own_identifiers =
        collect_normalized_identifiers(&["48663977535@s.whatsapp.net", "48663977535@lid"]);
    let exact_self_jids =
        collect_exact_jid_candidates(&["48663977535@s.whatsapp.net", "48663977535@lid"]);

    assert!(should_enqueue_from_me_whatsapp_message(
        "hello from phone",
        "48663977535@s.whatsapp.net",
        "48663977535@lid",
        &own_identifiers,
        &exact_self_jids,
        false
    ));
    assert!(!should_enqueue_from_me_whatsapp_message(
        &format!("{}assistant reply", zorai_self_chat_prefix()),
        "48663977535@s.whatsapp.net",
        "48663977535@lid",
        &own_identifiers,
        &exact_self_jids,
        false
    ));
}

#[test]
fn whatsapp_connected_event_triggers_replay_for_chat_with_cursor() {
    let config = make_replay_gateway_config();
    let client = reqwest::Client::new();
    let mut gw = gateway::GatewayState::new(config, client);

    gw.whatsapp_replay_cursors.insert(
        "15551234567".into(),
        build_whatsapp_cursor(1700000000, "msg1"),
    );

    mark_whatsapp_replay_active_if_cursors(&mut gw);

    assert!(
        gw.replay_cycle_active.contains("whatsapp"),
        "replay_cycle_active must contain 'whatsapp' when cursors are present"
    );
}

#[test]
fn whatsapp_first_connect_without_cursor_does_not_backfill_history() {
    let config = make_replay_gateway_config();
    let client = reqwest::Client::new();
    let mut gw = gateway::GatewayState::new(config, client);

    mark_whatsapp_replay_active_if_cursors(&mut gw);

    assert!(
        !gw.replay_cycle_active.contains("whatsapp"),
        "replay_cycle_active must NOT contain 'whatsapp' on first connect (no cursors)"
    );
}

#[tokio::test]
async fn whatsapp_self_echo_replay_is_classified_but_not_reenqueued() {
    use std::fs;
    use uuid::Uuid;

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-artifacts")
        .join(format!("wa-self-echo-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create test root");

    let history = crate::history::HistoryStore::new_test_store(&root)
        .await
        .expect("history store");

    let config = make_replay_gateway_config();
    let http = reqwest::Client::new();
    let mut gw = gateway::GatewayState::new(config, http);
    let mut seen_ids: Vec<String> = Vec::new();

    let envelope = gateway::ReplayEnvelope {
        message: gateway::IncomingMessage {
            platform: "WhatsApp".into(),
            sender: "49123456789@s.whatsapp.net".into(),
            content: "echo text".into(),
            channel: "49123456789@s.whatsapp.net".into(),
            message_id: Some("wa:ECHO001".into()),
            thread_context: None,
        },
        channel_id: "49123456789".into(),
        cursor_value: build_whatsapp_cursor(1700000042, "ECHO001"),
        cursor_type: "ts_msgid",
    };

    let result = gateway::ReplayFetchResult::Replay(vec![envelope]);

    let (accepted_msgs, completed) =
        process_replay_result(&history, "whatsapp", result, &mut gw, &mut seen_ids).await;

    assert!(
        completed,
        "replay must complete for a single valid envelope"
    );
    assert_eq!(
        accepted_msgs.len(),
        1,
        "process_replay_result must accept the message (cursor must advance)"
    );
    assert_eq!(
        gw.whatsapp_replay_cursors
            .get("49123456789")
            .map(String::as_str),
        Some("1700000042:ECHO001"),
        "in-memory cursor must advance even for an outbound echo"
    );

    let should_enqueue = false;
    let enqueued: Vec<_> = accepted_msgs
        .into_iter()
        .filter(|_| should_enqueue)
        .collect();
    assert!(
        enqueued.is_empty(),
        "outbound echo must not be enqueued despite cursor advancement"
    );

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn logged_out_clears_persisted_session_and_native_store() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    persist_transport_session_update(
        &engine.history,
        WHATSAPP_LINK_PROVIDER_ID,
        transport::SessionUpdate {
            linked_phone: Some("+15550000001".to_string()),
            auth_json: Some("{\"session\":true}".to_string()),
            metadata_json: Some("{\"device\":\"native\"}".to_string()),
            linked_at: Some(1),
            updated_at: 1,
        },
    )
    .await
    .expect("persist provider state");

    let store_path = whatsapp_native_store_path(&engine.data_dir);
    tokio::fs::write(&store_path, b"sqlite-placeholder")
        .await
        .expect("write native store");

    clear_logged_out_whatsapp_session(&engine)
        .await
        .expect("clear logged out session");

    assert!(
        load_persisted_provider_state(&engine.history, WHATSAPP_LINK_PROVIDER_ID)
            .await
            .expect("load provider state")
            .is_none(),
        "logged out cleanup must remove persisted provider state"
    );
    assert!(
        !store_path.exists(),
        "logged out cleanup must remove the native sqlite store"
    );
}
