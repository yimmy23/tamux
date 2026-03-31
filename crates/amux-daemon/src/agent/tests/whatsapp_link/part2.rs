use super::*;

#[tokio::test]
async fn apply_transport_event_persists_session_update_and_updates_runtime() {
    let root = tempdir().expect("tempdir");
    let history = HistoryStore::new_test_store(root.path())
        .await
        .expect("history store");
    let runtime = WhatsAppLinkRuntime::new();

    apply_transport_event(
        &runtime,
        &history,
        WHATSAPP_LINK_PROVIDER_ID,
        transport::WhatsAppTransportEvent::Starting,
    )
    .await
    .expect("starting event should apply");
    apply_transport_event(
        &runtime,
        &history,
        WHATSAPP_LINK_PROVIDER_ID,
        transport::WhatsAppTransportEvent::SessionUpdated(transport::SessionUpdate {
            linked_phone: Some("+15550000040".to_string()),
            auth_json: Some("{\"session\":true}".to_string()),
            metadata_json: Some("{\"device\":\"bridge\"}".to_string()),
            linked_at: Some(41),
            updated_at: 42,
        }),
    )
    .await
    .expect("session update should persist");

    let starting_snapshot = runtime.status_snapshot().await;
    assert_eq!(starting_snapshot.state, "starting");
    assert_eq!(
        load_persisted_provider_state(&history, WHATSAPP_LINK_PROVIDER_ID)
            .await
            .expect("load persisted state")
            .as_ref()
            .and_then(|state| state.linked_phone.as_deref()),
        Some("+15550000040")
    );

    apply_transport_event(
        &runtime,
        &history,
        WHATSAPP_LINK_PROVIDER_ID,
        transport::WhatsAppTransportEvent::Linked {
            phone: Some("+15550000040".to_string()),
        },
    )
    .await
    .expect("linked event should update runtime");
    let connected_snapshot = runtime.status_snapshot().await;
    assert_eq!(connected_snapshot.state, "connected");
    assert_eq!(connected_snapshot.phone.as_deref(), Some("+15550000040"));
}

#[tokio::test]
async fn start_transport_bridge_restores_state_and_forwards_events() {
    let root = tempdir().expect("tempdir");
    let history = HistoryStore::new_test_store(root.path())
        .await
        .expect("history store");
    save_persisted_provider_state(
        &history,
        WHATSAPP_LINK_PROVIDER_ID,
        transport::PersistedState {
            linked_phone: Some("+15550000050".to_string()),
            auth_json: Some("{\"session\":\"restored\"}".to_string()),
            metadata_json: Some("{\"device\":\"restored\"}".to_string()),
            last_reset_at: None,
            last_linked_at: Some(51),
            updated_at: 52,
        },
    )
    .await
    .expect("seed persisted state");

    let runtime = Arc::new(WhatsAppLinkRuntime::new());
    let transport = Arc::new(transport::ScriptedTransport::new());
    let mut rx = runtime.subscribe().await;

    let bridge = start_transport_bridge(runtime.clone(), history.clone(), transport.clone())
        .await
        .expect("transport bridge should start");

    assert_eq!(
        transport
            .restored_state()
            .await
            .as_ref()
            .and_then(|state| state.linked_phone.as_deref()),
        Some("+15550000050")
    );

    transport.emit_qr("QR-BRIDGE", Some(60)).await;
    transport
        .emit_linked(transport::SessionUpdate {
            linked_phone: Some("+15550000051".to_string()),
            auth_json: Some("{\"session\":\"updated\"}".to_string()),
            metadata_json: Some("{\"device\":\"updated\"}".to_string()),
            linked_at: Some(61),
            updated_at: 62,
        })
        .await;

    assert_eq!(recv_until_qr(&mut rx).await.as_deref(), Some("QR-BRIDGE"));
    assert_eq!(
        recv_until_linked(&mut rx).await.flatten().as_deref(),
        Some("+15550000051")
    );
    assert_eq!(
        load_persisted_provider_state(&history, WHATSAPP_LINK_PROVIDER_ID)
            .await
            .expect("load updated persisted state")
            .as_ref()
            .and_then(|state| state.linked_phone.as_deref()),
        Some("+15550000051")
    );

    bridge.abort();
}

#[tokio::test]
async fn transport_event_bridge_surfaces_lag_as_recoverable_error() {
    let root = tempdir().expect("tempdir");
    let history = HistoryStore::new_test_store(root.path())
        .await
        .expect("history store");
    let runtime = Arc::new(WhatsAppLinkRuntime::new());
    let (tx, rx) = broadcast::channel(1);
    let mut runtime_rx = runtime.subscribe().await;
    let bridge = spawn_transport_event_bridge(
        runtime.clone(),
        history,
        WHATSAPP_LINK_PROVIDER_ID.to_string(),
        rx,
    );

    let _ = tx.send(transport::WhatsAppTransportEvent::Starting);
    let _ = tx.send(transport::WhatsAppTransportEvent::Qr {
        ascii_qr: "QR-LAGGED".to_string(),
        expires_at_ms: Some(1),
    });
    let (message, recoverable) = recv_until_error(&mut runtime_rx)
        .await
        .expect("lagged bridge should emit recoverable error");
    assert!(recoverable);
    assert!(message.contains("lagged"));

    bridge.abort();
}

#[tokio::test]
async fn connected_emits_linked_and_updates_snapshot() {
    let runtime = WhatsAppLinkRuntime::new();
    let mut rx = runtime.subscribe().await;
    runtime.start().await.expect("start should succeed");
    runtime
        .broadcast_linked(Some("+123456789".to_string()))
        .await;

    assert_eq!(
        recv_until_linked(&mut rx).await.flatten().as_deref(),
        Some("+123456789")
    );
    let snapshot = runtime.status_snapshot().await;
    assert_eq!(snapshot.state, "connected");
    assert_eq!(snapshot.phone.as_deref(), Some("+123456789"));
}

#[tokio::test]
async fn stop_emits_disconnected_and_clears_active_session() {
    let runtime = WhatsAppLinkRuntime::new();
    let mut rx = runtime.subscribe().await;
    runtime.start().await.expect("start should succeed");
    runtime
        .broadcast_linked(Some("+123456789".to_string()))
        .await;
    runtime
        .stop(Some("operator_cancelled".to_string()))
        .await
        .expect("stop should succeed");

    assert_eq!(
        recv_until_disconnected(&mut rx).await.flatten().as_deref(),
        Some("operator_cancelled")
    );
    let snapshot = runtime.status_snapshot().await;
    assert_eq!(snapshot.state, "disconnected");
    assert_eq!(snapshot.phone, None);
}

#[tokio::test]
async fn new_subscriber_gets_immediate_latest_status_snapshot() {
    let runtime = WhatsAppLinkRuntime::new();
    runtime.start().await.expect("start should succeed");
    runtime
        .broadcast_linked(Some("+123456789".to_string()))
        .await;
    let mut rx = runtime.subscribe().await;
    let event = timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("status snapshot should arrive")
        .expect("broadcast should be open");
    match event {
        WhatsAppLinkEvent::Status(snapshot) => {
            assert_eq!(snapshot.state, "connected");
            assert_eq!(snapshot.phone.as_deref(), Some("+123456789"));
        }
        other => panic!("expected status snapshot, got {other:?}"),
    }
}

#[tokio::test]
async fn subscribe_snapshot_is_not_broadcast_to_existing_subscribers() {
    let runtime = WhatsAppLinkRuntime::new();
    runtime
        .broadcast_linked(Some("+123456789".to_string()))
        .await;

    let mut existing = runtime.subscribe().await;
    let _ = timeout(Duration::from_millis(250), existing.recv())
        .await
        .expect("initial snapshot should arrive for first subscriber")
        .expect("broadcast should be open");

    let mut newcomer = runtime.subscribe().await;
    let newcomer_event = timeout(Duration::from_millis(250), newcomer.recv())
        .await
        .expect("new subscriber should get immediate snapshot")
        .expect("broadcast should be open");
    assert!(matches!(newcomer_event, WhatsAppLinkEvent::Status(_)));

    let duplicate = timeout(Duration::from_millis(75), existing.recv()).await;
    assert!(duplicate.is_err(), "existing subscriber got duplicate status");
}

#[tokio::test]
async fn new_subscriber_replays_qr_after_status_snapshot() {
    let runtime = WhatsAppLinkRuntime::new();
    runtime.start().await.expect("start should succeed");
    runtime
        .broadcast_qr("QR-REPLAY".to_string(), Some(4242))
        .await;

    let mut rx = runtime.subscribe().await;
    let first = timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("first replay event should arrive")
        .expect("broadcast should be open");
    match first {
        WhatsAppLinkEvent::Status(snapshot) => assert_eq!(snapshot.state, "qr_ready"),
        other => panic!("expected status replay, got {other:?}"),
    }

    let second = timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("second replay event should arrive")
        .expect("broadcast should be open");
    match second {
        WhatsAppLinkEvent::Qr {
            ascii_qr,
            expires_at_ms,
        } => {
            assert_eq!(ascii_qr, "QR-REPLAY");
            assert_eq!(expires_at_ms, Some(4242));
        }
        other => panic!("expected qr replay, got {other:?}"),
    }
}

#[tokio::test]
async fn concurrent_broadcasts_do_not_precede_subscribe_replay_status() {
    let runtime = std::sync::Arc::new(WhatsAppLinkRuntime::new());
    runtime.start().await.expect("start should succeed");

    let broadcaster = {
        let runtime = runtime.clone();
        tokio::spawn(async move {
            for i in 0..64 {
                runtime.broadcast_error(format!("err-{i}"), true).await;
                tokio::task::yield_now().await;
            }
        })
    };

    let mut rx = runtime.subscribe().await;
    let first = timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("first replay event should arrive")
        .expect("broadcast should be open");
    assert!(
        matches!(first, WhatsAppLinkEvent::Status(_)),
        "first event for a new subscriber must be status replay"
    );

    broadcaster.await.expect("broadcaster should join");
}

#[tokio::test]
async fn error_event_updates_snapshot_state_and_payload() {
    let runtime = WhatsAppLinkRuntime::new();
    let mut rx = runtime.subscribe().await;
    let _ = timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("initial snapshot should arrive")
        .expect("broadcast should be open");

    runtime
        .broadcast_error("socket timeout".to_string(), true)
        .await;

    let error_event = timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("error event should arrive")
        .expect("broadcast should be open");
    match error_event {
        WhatsAppLinkEvent::Error {
            message,
            recoverable,
        } => {
            assert_eq!(message, "socket timeout");
            assert!(recoverable);
        }
        other => panic!("expected error event, got {other:?}"),
    }

    let status_event = timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("status event should arrive")
        .expect("broadcast should be open");
    match status_event {
        WhatsAppLinkEvent::Status(snapshot) => {
            assert_eq!(snapshot.state, "error");
            assert_eq!(snapshot.last_error.as_deref(), Some("socket timeout"));
        }
        other => panic!("expected status event, got {other:?}"),
    }

    let snapshot = runtime.status_snapshot().await;
    assert_eq!(snapshot.state, "error");
    assert_eq!(snapshot.last_error.as_deref(), Some("socket timeout"));
}

#[tokio::test]
async fn recoverable_error_while_connected_keeps_connected_state() {
    let runtime = WhatsAppLinkRuntime::new();
    let mut rx = runtime.subscribe().await;
    let _ = timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("initial snapshot should arrive")
        .expect("broadcast should be open");

    runtime
        .broadcast_linked(Some("+123456789".to_string()))
        .await;
    let _ = timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("linked event should arrive")
        .expect("broadcast should be open");
    let _ = timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("status event should arrive")
        .expect("broadcast should be open");

    runtime
        .broadcast_error("transient decrypt warning".to_string(), true)
        .await;

    let error_event = timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("error event should arrive")
        .expect("broadcast should be open");
    assert!(matches!(
        error_event,
        WhatsAppLinkEvent::Error {
            recoverable: true,
            ..
        }
    ));

    let status_event = timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("status event should arrive")
        .expect("broadcast should be open");
    match status_event {
        WhatsAppLinkEvent::Status(snapshot) => {
            assert_eq!(snapshot.state, "connected");
            assert_eq!(snapshot.last_error.as_deref(), Some("transient decrypt warning"));
        }
        other => panic!("expected status event, got {other:?}"),
    }
}
