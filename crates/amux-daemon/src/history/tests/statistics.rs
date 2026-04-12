use super::*;
use amux_protocol::{AgentDbMessage, AgentDbThread, AgentStatisticsWindow};

async fn seed_statistics_messages(store: &HistoryStore, thread_id: &str, now_ms: i64) -> Result<()> {
    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("Svarog".to_string()),
            title: "Statistics".to_string(),
            created_at: now_ms - 40 * 24 * 60 * 60 * 1000,
            updated_at: now_ms,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;

    let day_ms = 24 * 60 * 60 * 1000;
    let hour_ms = 60 * 60 * 1000;
    let messages = vec![
        AgentDbMessage {
            id: "msg-old".to_string(),
            thread_id: thread_id.to_string(),
            created_at: now_ms - 35 * day_ms,
            role: "assistant".to_string(),
            content: "older costless".to_string(),
            provider: Some("openai".to_string()),
            model: Some("gpt-old".to_string()),
            input_tokens: Some(10),
            output_tokens: Some(5),
            total_tokens: Some(15),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
        AgentDbMessage {
            id: "msg-openai-a".to_string(),
            thread_id: thread_id.to_string(),
            created_at: now_ms - 2 * day_ms,
            role: "assistant".to_string(),
            content: "openai recent".to_string(),
            provider: Some("openai".to_string()),
            model: Some("gpt-5.4-mini".to_string()),
            input_tokens: Some(100),
            output_tokens: Some(50),
            total_tokens: Some(150),
            cost_usd: Some(0.30),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
        AgentDbMessage {
            id: "msg-openai-b".to_string(),
            thread_id: thread_id.to_string(),
            created_at: now_ms - 2 * day_ms + hour_ms,
            role: "assistant".to_string(),
            content: "openai followup".to_string(),
            provider: Some("openai".to_string()),
            model: Some("gpt-5.4-mini".to_string()),
            input_tokens: Some(20),
            output_tokens: Some(10),
            total_tokens: Some(30),
            cost_usd: Some(0.05),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
        AgentDbMessage {
            id: "msg-claude-a".to_string(),
            thread_id: thread_id.to_string(),
            created_at: now_ms - hour_ms,
            role: "assistant".to_string(),
            content: "anthropic today".to_string(),
            provider: Some("anthropic".to_string()),
            model: Some("claude-4".to_string()),
            input_tokens: Some(80),
            output_tokens: Some(20),
            total_tokens: Some(100),
            cost_usd: Some(0.40),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
        AgentDbMessage {
            id: "msg-claude-b".to_string(),
            thread_id: thread_id.to_string(),
            created_at: now_ms - 20 * day_ms,
            role: "assistant".to_string(),
            content: "anthropic 30d".to_string(),
            provider: Some("anthropic".to_string()),
            model: Some("claude-4".to_string()),
            input_tokens: Some(10),
            output_tokens: Some(5),
            total_tokens: Some(15),
            cost_usd: Some(0.08),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
        AgentDbMessage {
            id: "msg-gemini".to_string(),
            thread_id: thread_id.to_string(),
            created_at: now_ms - 10 * day_ms,
            role: "assistant".to_string(),
            content: "google 30d".to_string(),
            provider: Some("google".to_string()),
            model: Some("gemini-2.5".to_string()),
            input_tokens: Some(50),
            output_tokens: Some(50),
            total_tokens: Some(100),
            cost_usd: Some(0.20),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
        AgentDbMessage {
            id: "msg-o3".to_string(),
            thread_id: thread_id.to_string(),
            created_at: now_ms - 3 * day_ms,
            role: "assistant".to_string(),
            content: "o3 recent".to_string(),
            provider: Some("openai".to_string()),
            model: Some("o3-mini".to_string()),
            input_tokens: Some(40),
            output_tokens: Some(60),
            total_tokens: Some(100),
            cost_usd: Some(0.25),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
    ];

    for message in messages {
        store.add_message(&message).await?;
    }

    Ok(())
}

#[tokio::test]
async fn agent_statistics_all_time_include_provider_model_rankings_and_incomplete_cost_flag() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("now should be after epoch")
        .as_millis() as i64;
    seed_statistics_messages(&store, "thread-statistics-all", now_ms).await?;

    let snapshot = store
        .get_agent_statistics(AgentStatisticsWindow::All)
        .await?;

    assert_eq!(snapshot.window, AgentStatisticsWindow::All);
    assert!(snapshot.has_incomplete_cost_history);
    assert_eq!(snapshot.totals.input_tokens, 310);
    assert_eq!(snapshot.totals.output_tokens, 200);
    assert_eq!(snapshot.totals.total_tokens, 510);
    assert!((snapshot.totals.cost_usd - 1.28).abs() < f64::EPSILON);
    assert_eq!(snapshot.totals.provider_count, 3);
    assert_eq!(snapshot.totals.model_count, 5);

    assert_eq!(snapshot.providers.len(), 3);
    assert_eq!(snapshot.providers[0].provider, "openai");
    assert_eq!(snapshot.providers[0].total_tokens, 295);
    assert!((snapshot.providers[0].cost_usd - 0.60).abs() < f64::EPSILON);
    assert_eq!(snapshot.providers[1].provider, "anthropic");
    assert_eq!(snapshot.providers[1].total_tokens, 115);
    assert_eq!(snapshot.providers[2].provider, "google");
    assert_eq!(snapshot.providers[2].total_tokens, 100);

    assert_eq!(snapshot.models.len(), 5);
    assert_eq!(snapshot.top_models_by_tokens.len(), 5);
    assert_eq!(snapshot.top_models_by_tokens[0].provider, "openai");
    assert_eq!(snapshot.top_models_by_tokens[0].model, "gpt-5.4-mini");
    assert_eq!(snapshot.top_models_by_tokens[0].total_tokens, 180);
    assert_eq!(snapshot.top_models_by_tokens[1].provider, "anthropic");
    assert_eq!(snapshot.top_models_by_tokens[1].model, "claude-4");
    assert_eq!(snapshot.top_models_by_tokens[2].provider, "openai");
    assert_eq!(snapshot.top_models_by_tokens[2].model, "o3-mini");
    assert_eq!(snapshot.top_models_by_tokens[3].provider, "google");
    assert_eq!(snapshot.top_models_by_tokens[3].model, "gemini-2.5");
    assert_eq!(snapshot.top_models_by_tokens[4].model, "gpt-old");

    assert_eq!(snapshot.top_models_by_cost[0].provider, "anthropic");
    assert_eq!(snapshot.top_models_by_cost[0].model, "claude-4");
    assert_eq!(snapshot.top_models_by_cost[1].provider, "openai");
    assert_eq!(snapshot.top_models_by_cost[1].model, "gpt-5.4-mini");
    assert_eq!(snapshot.top_models_by_cost[2].model, "o3-mini");

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn agent_statistics_windows_apply_expected_cutoffs() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("now should be after epoch")
        .as_millis() as i64;
    seed_statistics_messages(&store, "thread-statistics-window", now_ms).await?;

    let today = store
        .get_agent_statistics(AgentStatisticsWindow::Today)
        .await?;
    assert_eq!(today.totals.total_tokens, 100);
    assert!((today.totals.cost_usd - 0.40).abs() < f64::EPSILON);
    assert!(!today.has_incomplete_cost_history);

    let seven_days = store
        .get_agent_statistics(AgentStatisticsWindow::Last7Days)
        .await?;
    assert_eq!(seven_days.totals.input_tokens, 240);
    assert_eq!(seven_days.totals.output_tokens, 140);
    assert_eq!(seven_days.totals.total_tokens, 380);
    assert!((seven_days.totals.cost_usd - 1.00).abs() < f64::EPSILON);
    assert_eq!(seven_days.totals.provider_count, 2);
    assert_eq!(seven_days.totals.model_count, 3);

    let thirty_days = store
        .get_agent_statistics(AgentStatisticsWindow::Last30Days)
        .await?;
    assert_eq!(thirty_days.totals.input_tokens, 300);
    assert_eq!(thirty_days.totals.output_tokens, 195);
    assert_eq!(thirty_days.totals.total_tokens, 495);
    assert!((thirty_days.totals.cost_usd - 1.28).abs() < f64::EPSILON);
    assert_eq!(thirty_days.totals.provider_count, 3);
    assert_eq!(thirty_days.totals.model_count, 4);
    assert!(!thirty_days.has_incomplete_cost_history);

    fs::remove_dir_all(root)?;
    Ok(())
}
