use super::*;

#[tokio::test]
async fn reconnect_replay_runs_once_per_outage_cycle() {
    let root = make_test_root("replay-runs-once");
    let manager = SessionManager::new_test(&root).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), &root).await;

    let mut gw =
        super::gateway::GatewayState::new(make_replay_gateway_config(), reqwest::Client::new());
    gw.telegram_replay_cursor = Some(100);
    gw.replay_cycle_active.insert("telegram".to_string());

    let result = super::gateway::ReplayFetchResult::Replay(vec![make_telegram_replay_envelope(
        "101", "777", "hello", "alice",
    )]);
    let mut seen_ids: Vec<String> = Vec::new();

    let (messages, completed) =
        super::process_replay_result(&engine.history, "telegram", result, &mut gw, &mut seen_ids)
            .await;

    if completed {
        gw.replay_cycle_active.remove("telegram");
    }

    assert!(completed);
    assert_eq!(messages.len(), 1);
    assert!(!gw.replay_cycle_active.contains("telegram"));

    let row = engine
        .history
        .load_gateway_replay_cursor("telegram", "global")
        .await
        .unwrap();
    assert_eq!(row.map(|r| r.cursor_value).as_deref(), Some("101"));

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn first_connect_without_cursor_still_skips_backlog() {
    let root = make_test_root("replay-skip-backlog");
    let manager = SessionManager::new_test(&root).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), &root).await;

    let mut gw =
        super::gateway::GatewayState::new(make_replay_gateway_config(), reqwest::Client::new());
    assert!(gw.telegram_replay_cursor.is_none());

    let result = super::gateway::ReplayFetchResult::InitializeBoundary {
        channel_id: "global".to_string(),
        cursor_value: "500".to_string(),
        cursor_type: "update_id",
    };
    let mut seen_ids: Vec<String> = Vec::new();

    let (messages, completed) =
        super::process_replay_result(&engine.history, "telegram", result, &mut gw, &mut seen_ids)
            .await;

    assert!(completed);
    assert!(messages.is_empty());

    let row = engine
        .history
        .load_gateway_replay_cursor("telegram", "global")
        .await
        .unwrap();
    assert_eq!(row.map(|r| r.cursor_value).as_deref(), Some("500"));
    assert_eq!(gw.telegram_replay_cursor, Some(500));

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn discord_initialize_boundary_seeds_live_poll_cursor() {
    let root = make_test_root("discord-replay-init-seeds-live-cursor");
    let manager = SessionManager::new_test(&root).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), &root).await;

    let mut gw =
        super::gateway::GatewayState::new(make_replay_gateway_config(), reqwest::Client::new());
    assert!(gw.discord_replay_cursors.is_empty());

    let result = super::gateway::ReplayFetchResult::InitializeBoundary {
        channel_id: "D456".to_string(),
        cursor_value: "998877665544".to_string(),
        cursor_type: "message_id",
    };
    let mut seen_ids: Vec<String> = Vec::new();

    let (messages, completed) =
        super::process_replay_result(&engine.history, "discord", result, &mut gw, &mut seen_ids)
            .await;

    assert!(completed);
    assert!(messages.is_empty());
    assert_eq!(
        gw.discord_replay_cursors.get("D456").map(String::as_str),
        Some("998877665544")
    );

    let row = engine
        .history
        .load_gateway_replay_cursor("discord", "D456")
        .await
        .unwrap();
    assert_eq!(row.map(|r| r.cursor_value).as_deref(), Some("998877665544"));

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn classified_duplicate_or_filtered_message_advances_cursor() {
    let root = make_test_root("replay-cursor-advance");
    let manager = SessionManager::new_test(&root).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), &root).await;

    let mut gw =
        super::gateway::GatewayState::new(make_replay_gateway_config(), reqwest::Client::new());
    let mut seen_ids = vec!["tg:1001".to_string(), "tg:1002".to_string()];
    assert!(gw.telegram_replay_cursor.is_none());

    let result = super::gateway::ReplayFetchResult::Replay(vec![
        make_telegram_replay_envelope_with_id("102", "tg:1001", "777", "dup text", "alice"),
        make_telegram_replay_envelope_with_id("103", "tg:1002", "777", "dup text 2", "alice"),
    ]);

    let (messages, completed) =
        super::process_replay_result(&engine.history, "telegram", result, &mut gw, &mut seen_ids)
            .await;

    assert!(completed);
    assert_eq!(messages.len(), 0);

    let row = engine
        .history
        .load_gateway_replay_cursor("telegram", "global")
        .await
        .unwrap();
    assert_eq!(row.map(|r| r.cursor_value).as_deref(), Some("103"));
    assert_eq!(gw.telegram_replay_cursor, Some(103));

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn replay_accepted_messages_do_not_preseed_shared_seen_ids() {
    let root = make_test_root("replay-no-preseed-shared-seen-ids");
    let manager = SessionManager::new_test(&root).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), &root).await;

    assert!(engine.gateway_seen_ids.lock().await.is_empty());

    let mut gw =
        super::gateway::GatewayState::new(make_replay_gateway_config(), reqwest::Client::new());

    let result = super::gateway::ReplayFetchResult::Replay(vec![
        make_telegram_replay_envelope("201", "777", "first replayed msg", "alice"),
        make_telegram_replay_envelope("202", "777", "second replayed msg", "bob"),
    ]);

    let messages = engine
        .apply_replay_results(vec![("telegram".to_string(), vec![result], true)], &mut gw)
        .await;

    assert_eq!(messages.len(), 2);
    let seen = engine.gateway_seen_ids.lock().await;
    assert!(seen.is_empty());

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn unclassified_failure_does_not_advance_cursor() {
    let root = make_test_root("replay-no-advance-on-failure");
    let manager = SessionManager::new_test(&root).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), &root).await;

    engine
        .history
        .save_gateway_replay_cursor("telegram", "global", "100", "update_id")
        .await
        .unwrap();

    let mut gw =
        super::gateway::GatewayState::new(make_replay_gateway_config(), reqwest::Client::new());
    let mut seen_ids: Vec<String> = Vec::new();

    let result = super::gateway::ReplayFetchResult::Replay(vec![
        make_telegram_replay_envelope("101", "777", "good message", "alice"),
        make_malformed_telegram_replay_envelope(),
    ]);

    let (messages, completed) =
        super::process_replay_result(&engine.history, "telegram", result, &mut gw, &mut seen_ids)
            .await;

    assert!(!completed);
    let row = engine
        .history
        .load_gateway_replay_cursor("telegram", "global")
        .await
        .unwrap();
    assert_eq!(row.map(|r| r.cursor_value).as_deref(), Some("101"));
    assert_eq!(messages.len(), 1);

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn partial_multichannel_fetch_preserves_earlier_results_and_keeps_platform_active() {
    let root = make_test_root("replay-partial-multichannel");
    let manager = SessionManager::new_test(&root).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), &root).await;

    let mut gw =
        super::gateway::GatewayState::new(make_replay_gateway_config(), reqwest::Client::new());
    gw.replay_cycle_active.insert("slack".to_string());

    let ch1_result =
        super::gateway::ReplayFetchResult::Replay(vec![make_telegram_replay_envelope(
            "301",
            "C1",
            "msg from ch1",
            "alice",
        )]);

    let messages = engine
        .apply_replay_results(
            vec![("slack".to_string(), vec![ch1_result], false)],
            &mut gw,
        )
        .await;

    assert_eq!(messages.len(), 1);
    assert!(gw.replay_cycle_active.contains("slack"));

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn complete_multichannel_fetch_removes_platform_from_active() {
    let root = make_test_root("replay-complete-multichannel");
    let manager = SessionManager::new_test(&root).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), &root).await;

    let mut gw =
        super::gateway::GatewayState::new(make_replay_gateway_config(), reqwest::Client::new());
    gw.replay_cycle_active.insert("slack".to_string());

    let ch1_result =
        super::gateway::ReplayFetchResult::Replay(vec![make_telegram_replay_envelope(
            "401",
            "C1",
            "msg from ch1",
            "alice",
        )]);
    let ch2_result =
        super::gateway::ReplayFetchResult::Replay(vec![make_telegram_replay_envelope(
            "402",
            "C2",
            "msg from ch2",
            "bob",
        )]);

    let messages = engine
        .apply_replay_results(
            vec![("slack".to_string(), vec![ch1_result, ch2_result], true)],
            &mut gw,
        )
        .await;

    assert_eq!(messages.len(), 2);
    assert!(!gw.replay_cycle_active.contains("slack"));

    fs::remove_dir_all(&root).ok();
}
