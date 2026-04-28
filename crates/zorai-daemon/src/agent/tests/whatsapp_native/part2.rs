use super::*;

#[tokio::test]
async fn whatsapp_unmatched_sender_is_filtered_from_live_enqueue() {
    let (decision, queue_len, cursor) = simulate_live_whatsapp_enqueue(
        "+1 206 555 0123",
        "49123456789@lid",
        "49123456789@s.whatsapp.net",
        std::collections::HashSet::new(),
        Vec::new(),
        false,
        false,
        "hello from contact",
    )
    .await;

    assert_eq!(decision, WhatsAppEnqueueDecision::SuppressAllowlist);
    assert_eq!(queue_len, 0, "unmatched sender must not be enqueued");
    assert_eq!(cursor.as_deref(), Some("1700000100:LIVE001"));
}

#[tokio::test]
async fn whatsapp_matched_sender_is_accepted() {
    let (decision, queue_len, cursor) = simulate_live_whatsapp_enqueue(
        "+49 123 456789",
        "49123456789@lid",
        "49123456789@s.whatsapp.net",
        std::collections::HashSet::new(),
        Vec::new(),
        false,
        false,
        "hello from contact",
    )
    .await;

    assert_eq!(decision, WhatsAppEnqueueDecision::Enqueue);
    assert_eq!(queue_len, 1, "matched sender must be enqueued");
    assert_eq!(cursor.as_deref(), Some("1700000100:LIVE001"));
}

#[tokio::test]
async fn whatsapp_empty_allowlist_suppresses_live_enqueue() {
    let (decision, queue_len, cursor) = simulate_live_whatsapp_enqueue(
        "",
        "49123456789@lid",
        "49123456789@s.whatsapp.net",
        std::collections::HashSet::new(),
        Vec::new(),
        false,
        false,
        "hello from contact",
    )
    .await;

    assert_eq!(decision, WhatsAppEnqueueDecision::SuppressAllowlist);
    assert_eq!(
        queue_len, 0,
        "empty allowlist must not enqueue inbound messages"
    );
    assert_eq!(cursor.as_deref(), Some("1700000100:LIVE001"));
}

#[tokio::test]
async fn whatsapp_self_chat_is_filtered_when_allowlist_excludes_self() {
    let own_identifiers =
        collect_normalized_identifiers(&["48663977535@s.whatsapp.net", "48663977535@lid"]);
    let exact_self_jids =
        collect_exact_jid_candidates(&["48663977535@s.whatsapp.net", "48663977535@lid"]);

    let (decision, queue_len, cursor) = simulate_live_whatsapp_enqueue(
        "+1 206 555 0123",
        "48663977535@lid",
        "48663977535@s.whatsapp.net",
        own_identifiers,
        exact_self_jids,
        true,
        false,
        "hello from phone",
    )
    .await;

    assert_eq!(decision, WhatsAppEnqueueDecision::SuppressAllowlist);
    assert_eq!(queue_len, 0, "self-chat message must respect the allowlist");
    assert_eq!(cursor.as_deref(), Some("1700000100:LIVE001"));
}

#[tokio::test]
async fn whatsapp_self_chat_enqueues_when_allowlist_includes_self() {
    let own_identifiers =
        collect_normalized_identifiers(&["48663977535@s.whatsapp.net", "48663977535@lid"]);
    let exact_self_jids =
        collect_exact_jid_candidates(&["48663977535@s.whatsapp.net", "48663977535@lid"]);

    let (decision, queue_len, cursor) = simulate_live_whatsapp_enqueue(
        "+48 663 977 535",
        "48663977535@lid",
        "48663977535@s.whatsapp.net",
        own_identifiers,
        exact_self_jids,
        true,
        false,
        "hello from phone",
    )
    .await;

    assert_eq!(decision, WhatsAppEnqueueDecision::Enqueue);
    assert_eq!(queue_len, 1, "allowlisted self-chat message must enqueue");
    assert_eq!(cursor.as_deref(), Some("1700000100:LIVE001"));
}

#[tokio::test]
async fn whatsapp_self_echo_suppression_still_wins_with_allowlist_active() {
    let own_identifiers =
        collect_normalized_identifiers(&["48663977535@s.whatsapp.net", "48663977535@lid"]);
    let exact_self_jids =
        collect_exact_jid_candidates(&["48663977535@s.whatsapp.net", "48663977535@lid"]);

    let (decision, queue_len, cursor) = simulate_live_whatsapp_enqueue(
        "+49 111 222333",
        "48663977535@lid",
        "48663977535@s.whatsapp.net",
        own_identifiers,
        exact_self_jids,
        true,
        false,
        &format!("{}assistant reply", zorai_self_chat_prefix()),
    )
    .await;

    assert_eq!(decision, WhatsAppEnqueueDecision::SuppressSelfEcho);
    assert_eq!(queue_len, 0, "self-echo must stay suppressed");
    assert_eq!(cursor.as_deref(), Some("1700000100:LIVE001"));
}

#[tokio::test]
async fn whatsapp_replay_filtering_uses_the_same_allowlist_rule() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let mut gw = GatewayState::new(make_replay_gateway_config(), reqwest::Client::new());
    let mut seen_ids = Vec::new();

    let decision = classify_whatsapp_enqueue_decision(
        "replayed hello",
        "49123456789@s.whatsapp.net",
        "49123456789@lid",
        &std::collections::HashSet::new(),
        &[],
        false,
        false,
        "+1 206 555 0123",
    );

    let result = gateway::ReplayFetchResult::Replay(vec![gateway::ReplayEnvelope {
        message: IncomingMessage {
            platform: "WhatsApp".into(),
            sender: "49123456789@lid".into(),
            content: if matches!(decision, WhatsAppEnqueueDecision::Enqueue) {
                "replayed hello".into()
            } else {
                String::new()
            },
            channel: "49123456789@s.whatsapp.net".into(),
            message_id: Some("wa:REPLAY001".into()),
            thread_context: None,
        },
        channel_id: "49123456789".into(),
        cursor_value: build_whatsapp_cursor(1700000200, "REPLAY001"),
        cursor_type: "ts_msgid",
    }]);

    let (accepted, completed) =
        process_replay_result(&engine.history, "whatsapp", result, &mut gw, &mut seen_ids).await;

    assert_eq!(decision, WhatsAppEnqueueDecision::SuppressAllowlist);
    assert!(
        completed,
        "filtered replay message must still complete replay"
    );
    assert!(
        accepted.is_empty(),
        "filtered replay message must not be routed"
    );
    assert_eq!(
        gw.whatsapp_replay_cursors
            .get("49123456789")
            .map(String::as_str),
        Some("1700000200:REPLAY001"),
        "filtered replay message must still advance the cursor"
    );
}
