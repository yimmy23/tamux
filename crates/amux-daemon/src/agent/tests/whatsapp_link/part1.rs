use super::*;

#[tokio::test]
async fn start_to_qr_ready_emits_qr_event() {
    let runtime = WhatsAppLinkRuntime::new();
    let mut rx = runtime.subscribe().await;
    runtime.start().await.expect("start should succeed");
    runtime.broadcast_qr("QR-1".to_string(), Some(111)).await;
    assert_eq!(recv_until_qr(&mut rx).await.as_deref(), Some("QR-1"));
}

#[tokio::test]
async fn qr_refresh_replaces_stale_qr_without_duplicate_payload() {
    let runtime = WhatsAppLinkRuntime::new();
    let mut rx = runtime.subscribe().await;
    runtime.start().await.expect("start should succeed");
    runtime.broadcast_qr("QR-1".to_string(), Some(111)).await;
    runtime.broadcast_qr("QR-1".to_string(), Some(111)).await;
    runtime.broadcast_qr("QR-2".to_string(), Some(222)).await;

    let mut payloads = Vec::new();
    for _ in 0..10 {
        if let Ok(Ok(WhatsAppLinkEvent::Qr { ascii_qr, .. })) =
            timeout(Duration::from_millis(150), rx.recv()).await
        {
            payloads.push(ascii_qr);
        }
    }
    assert_eq!(payloads, vec!["QR-1".to_string(), "QR-2".to_string()]);
}

#[tokio::test]
async fn raw_pairing_payload_is_rendered_before_broadcast() {
    let runtime = WhatsAppLinkRuntime::new();
    let mut rx = runtime.subscribe().await;
    runtime.start().await.expect("start should succeed");

    let raw_payload = "ref,noise,identity,adv".to_string();
    runtime.broadcast_qr(raw_payload.clone(), Some(111)).await;

    let rendered = recv_until_qr(&mut rx)
        .await
        .expect("qr event should arrive");
    assert_ne!(rendered, raw_payload);
    assert!(
        rendered.contains('\n'),
        "expected a multiline QR block, got {rendered:?}"
    );
}

#[tokio::test]
async fn remembers_recent_outbound_message_ids() {
    let runtime = WhatsAppLinkRuntime::new();
    assert!(!runtime.is_recent_outbound_message_id("msg-1").await);
    runtime.remember_outbound_message_id("msg-1").await;
    assert!(runtime.is_recent_outbound_message_id("msg-1").await);
    assert!(!runtime.is_recent_outbound_message_id("msg-2").await);
}

#[tokio::test]
async fn reset_clears_runtime_state() {
    let runtime = WhatsAppLinkRuntime::new();
    runtime.start().await.expect("start should succeed");
    runtime.broadcast_qr("QR-RESET".to_string(), Some(111)).await;
    runtime
        .broadcast_linked(Some("+15551234567".to_string()))
        .await;
    runtime.reset().await.expect("reset should succeed");

    let snapshot = runtime.status_snapshot().await;
    assert_eq!(snapshot.state, "disconnected");
    assert!(snapshot.phone.is_none());
    assert!(snapshot.last_error.is_none());

    let (_, mut rx) = runtime.subscribe_with_id().await;
    assert!(
        recv_until_qr(&mut rx).await.is_none(),
        "reset should clear replayable QR state"
    );
}

#[tokio::test]
async fn persisted_provider_state_round_trips_through_history_helpers() {
    let root = tempdir().expect("tempdir");
    let history = HistoryStore::new_test_store(root.path())
        .await
        .expect("history store");
    let state = transport::PersistedState {
        linked_phone: Some("+15557654321".to_string()),
        auth_json: Some("{\"session\":true}".to_string()),
        metadata_json: Some("{\"jid\":\"123\"}".to_string()),
        last_reset_at: Some(12),
        last_linked_at: Some(34),
        updated_at: 56,
    };

    save_persisted_provider_state(&history, WHATSAPP_LINK_PROVIDER_ID, state.clone())
        .await
        .expect("save state");
    let loaded = load_persisted_provider_state(&history, WHATSAPP_LINK_PROVIDER_ID)
        .await
        .expect("load state");
    assert_eq!(loaded, Some(state));

    clear_persisted_provider_state(&history, WHATSAPP_LINK_PROVIDER_ID)
        .await
        .expect("clear state");
    assert!(
        load_persisted_provider_state(&history, WHATSAPP_LINK_PROVIDER_ID)
            .await
            .expect("load cleared state")
            .is_none()
    );
}

#[test]
fn normalize_identifier_strips_device_and_plus_prefix() {
    assert_eq!(normalize_jid_user("13383252336718:6@lid"), "13383252336718");
    assert_eq!(
        normalize_identifier("+48663977535:6@s.whatsapp.net"),
        "48663977535"
    );
    assert_eq!(normalize_identifier("+48663977535"), "48663977535");
}

#[test]
fn resolve_send_target_candidates_prefers_self_exact_jids_and_cross_namespace_fallbacks() {
    let own_identifiers = collect_normalized_identifiers(&[
        "48663977535:6@s.whatsapp.net",
        "13383252336718:6@lid",
        "+48663977535",
    ]);
    let own_exact_jids = collect_exact_jid_candidates(&[
        "13383252336718:6@lid",
        "48663977535:6@s.whatsapp.net",
    ]);

    assert_eq!(
        resolve_send_target_candidates(
            "13383252336718@lid",
            &own_identifiers,
            Some("+48663977535"),
            &own_exact_jids,
        ),
        vec![
            "13383252336718:6@lid".to_string(),
            "48663977535:6@s.whatsapp.net".to_string(),
            "13383252336718@lid".to_string(),
            "48663977535@s.whatsapp.net".to_string(),
            "13383252336718@s.whatsapp.net".to_string(),
        ]
    );

    assert_eq!(
        resolve_send_target_candidates("48663977535@s.whatsapp.net", &HashSet::new(), None, &[],),
        vec![
            "48663977535@s.whatsapp.net".to_string(),
            "48663977535@lid".to_string(),
        ]
    );
}

#[test]
fn resolve_native_send_plan_keeps_working_delivery_targets_for_self_chat() {
    let (targets, prefix_self_chat) = resolve_native_send_plan(
        "13383252336718@lid",
        "48663977535@s.whatsapp.net",
        "13383252336718@lid",
    );

    assert!(prefix_self_chat);
    assert_eq!(
        targets,
        vec![
            "13383252336718@lid".to_string(),
            "13383252336718@s.whatsapp.net".to_string(),
        ]
    );
}

#[test]
fn merge_persisted_state_update_preserves_existing_auth_and_metadata() {
    let merged = merge_persisted_state_update(
        Some(transport::PersistedState {
            linked_phone: Some("+15550000010".to_string()),
            auth_json: Some("{\"existing\":true}".to_string()),
            metadata_json: Some("{\"device\":\"a\"}".to_string()),
            last_reset_at: Some(7),
            last_linked_at: Some(8),
            updated_at: 9,
        }),
        transport::SessionUpdate {
            linked_phone: Some("+15550000011".to_string()),
            auth_json: None,
            metadata_json: Some("{\"device\":\"b\"}".to_string()),
            linked_at: Some(12),
            updated_at: 13,
        },
    );

    assert_eq!(merged.linked_phone.as_deref(), Some("+15550000011"));
    assert_eq!(merged.auth_json.as_deref(), Some("{\"existing\":true}"));
    assert_eq!(merged.metadata_json.as_deref(), Some("{\"device\":\"b\"}"));
    assert_eq!(merged.last_reset_at, Some(7));
    assert_eq!(merged.last_linked_at, Some(12));
    assert_eq!(merged.updated_at, 13);
}

#[tokio::test]
async fn persist_transport_session_update_merges_and_saves_state() {
    let root = tempdir().expect("tempdir");
    let history = HistoryStore::new_test_store(root.path())
        .await
        .expect("history store");

    save_persisted_provider_state(
        &history,
        WHATSAPP_LINK_PROVIDER_ID,
        transport::PersistedState {
            linked_phone: Some("+15550000020".to_string()),
            auth_json: Some("{\"existing\":true}".to_string()),
            metadata_json: Some("{\"device\":\"a\"}".to_string()),
            last_reset_at: Some(1),
            last_linked_at: Some(2),
            updated_at: 3,
        },
    )
    .await
    .expect("seed state");

    let merged = persist_transport_session_update(
        &history,
        WHATSAPP_LINK_PROVIDER_ID,
        transport::SessionUpdate {
            linked_phone: Some("+15550000021".to_string()),
            auth_json: None,
            metadata_json: Some("{\"device\":\"b\"}".to_string()),
            linked_at: Some(22),
            updated_at: 23,
        },
    )
    .await
    .expect("persist merged update");

    assert_eq!(merged.linked_phone.as_deref(), Some("+15550000021"));
    assert_eq!(merged.auth_json.as_deref(), Some("{\"existing\":true}"));
    assert_eq!(merged.metadata_json.as_deref(), Some("{\"device\":\"b\"}"));
    assert_eq!(merged.last_reset_at, Some(1));
    assert_eq!(merged.last_linked_at, Some(22));
    assert_eq!(merged.updated_at, 23);
    assert_eq!(
        load_persisted_provider_state(&history, WHATSAPP_LINK_PROVIDER_ID)
            .await
            .expect("load merged state"),
        Some(merged)
    );
}
