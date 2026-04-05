#[cfg(test)]
use super::*;
use amux_shared::providers::{
    PROVIDER_ID_CHATGPT_SUBSCRIPTION, PROVIDER_ID_CUSTOM, PROVIDER_ID_GROQ,
    PROVIDER_ID_OPENAI,
};
use tempfile::TempDir;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

async fn make_test_engine(config: AgentConfig) -> (Arc<AgentEngine>, TempDir) {
    let temp_dir = TempDir::new().expect("temp dir");
    let session_manager = SessionManager::new_test(temp_dir.path()).await;
    let history = HistoryStore::new_test_store(temp_dir.path())
        .await
        .expect("history store");
    let data_dir = temp_dir.path().join("agent");
    std::fs::create_dir_all(&data_dir).expect("create agent data dir");
    let engine = AgentEngine::new_with_storage_and_http_client(
        session_manager,
        config,
        history,
        data_dir,
        build_agent_http_client(Duration::from_millis(75)),
    );
    (engine, temp_dir)
}

fn provider_config(
    base_url: &str,
    model: &str,
    api_key: &str,
    auth_source: AuthSource,
) -> ProviderConfig {
    ProviderConfig {
        base_url: base_url.to_string(),
        model: model.to_string(),
        api_key: api_key.to_string(),
        assistant_id: String::new(),
        auth_source,
        api_transport: ApiTransport::ChatCompletions,
        reasoning_effort: String::new(),
        context_window_tokens: 0,
        response_schema: None,
        stop_sequences: None,
        temperature: None,
        top_p: None,
        top_k: None,
        metadata: None,
        service_tier: None,
        container: None,
        inference_geo: None,
        cache_control: None,
        max_tokens: None,
        anthropic_tool_choice: None,
        output_effort: None,
    }
}

fn write_openai_subscription_auth() {
    let auth = serde_json::json!({
        "provider": "openai-codex",
        "auth_mode": "chatgpt_subscription",
        "access_token": "header.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiacctMSJ9LCJleHAiOjQxMDI0NDQ4MDB9.signature",
        "refresh_token": "refresh-token",
        "account_id": "acct-1",
        "expires_at": 4_102_444_800_000i64,
        "source": "test",
        "updated_at": 4_102_444_800_000i64,
        "created_at": 4_102_444_800_000i64
    });
    super::provider_auth_store::save_provider_auth_state(
        PROVIDER_ID_OPENAI,
        PROVIDER_ID_CHATGPT_SUBSCRIPTION,
        &auth,
    )
    .expect("write auth fixture");
}

#[tokio::test]
async fn provider_alternative_excludes_placeholder_provider_row() {
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.providers.insert(
        PROVIDER_ID_CUSTOM.to_string(),
        provider_config("", "", "", AuthSource::ApiKey),
    );
    let (engine, _temp_dir) = make_test_engine(config).await;

    let suggestion = engine.suggest_alternative_provider(PROVIDER_ID_OPENAI).await;

    assert!(
        suggestion.is_none(),
        "placeholder provider rows must not be suggested"
    );
}

#[tokio::test]
async fn provider_alternative_excludes_failed_provider_itself() {
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.providers.insert(
        PROVIDER_ID_OPENAI.to_string(),
        provider_config(
            "https://api.openai.com/v1",
            "gpt-4o",
            "valid-key",
            AuthSource::ApiKey,
        ),
    );
    let (engine, _temp_dir) = make_test_engine(config).await;

    let suggestion = engine.suggest_alternative_provider(PROVIDER_ID_OPENAI).await;

    assert!(
        suggestion.is_none(),
        "the failed provider itself must not be suggested as an alternative"
    );
}

#[tokio::test]
async fn provider_alternative_excludes_open_breaker_provider() {
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.providers.insert(
        PROVIDER_ID_CUSTOM.to_string(),
        provider_config(
            "https://example.invalid/v1",
            "model-a",
            "valid-key",
            AuthSource::ApiKey,
        ),
    );
    let (engine, _temp_dir) = make_test_engine(config).await;
    {
        let breaker = engine.circuit_breakers.get(PROVIDER_ID_CUSTOM).await;
        let mut breaker = breaker.lock().await;
        let now = now_millis();
        for offset in 0..5 {
            breaker.record_failure(now + offset);
        }
    }

    let suggestion = engine.suggest_alternative_provider(PROVIDER_ID_OPENAI).await;

    assert!(
        suggestion.is_none(),
        "providers with open circuit breakers must not be suggested"
    );
}

#[tokio::test]
async fn provider_alternative_includes_configured_healthy_provider() {
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.providers.insert(
        PROVIDER_ID_CUSTOM.to_string(),
        provider_config(
            "https://example.invalid/v1",
            "model-a",
            "valid-key",
            AuthSource::ApiKey,
        ),
    );
    let (engine, _temp_dir) = make_test_engine(config).await;

    let suggestion = engine.suggest_alternative_provider(PROVIDER_ID_OPENAI).await;

    let suggestion = suggestion.expect("healthy provider should be suggested");
    assert!(
        suggestion.contains(PROVIDER_ID_CUSTOM),
        "expected healthy configured provider to be suggested, got: {suggestion}"
    );
}

#[tokio::test]
async fn provider_alternative_excludes_openai_subscription_without_auth() {
    let _env_guard = crate::agent::provider_auth_store::provider_auth_test_env_lock();
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("provider-auth.db");
    std::env::set_var("TAMUX_PROVIDER_AUTH_DB_PATH", &db_path);
    std::env::set_var(
        "TAMUX_CODEX_CLI_AUTH_PATH",
        temp_dir.path().join("missing-codex-auth.json"),
    );

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_GROQ.to_string();
    config.providers.insert(
        PROVIDER_ID_OPENAI.to_string(),
        provider_config(
            "https://api.openai.com/v1",
            "gpt-5.4",
            "",
            AuthSource::ChatgptSubscription,
        ),
    );
    let (engine, _temp_dir) = make_test_engine(config).await;

    let suggestion = engine.suggest_alternative_provider(PROVIDER_ID_GROQ).await;

    std::env::remove_var("TAMUX_PROVIDER_AUTH_DB_PATH");
    std::env::remove_var("TAMUX_CODEX_CLI_AUTH_PATH");
    assert!(
        suggestion.is_none(),
        "OpenAI subscription auth must be present before suggesting it as an alternative"
    );
}

#[tokio::test]
async fn provider_alternative_uses_candidate_default_model_for_empty_named_model() {
    let _env_guard = crate::agent::provider_auth_store::provider_auth_test_env_lock();
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("provider-auth.db");
    std::env::set_var("TAMUX_PROVIDER_AUTH_DB_PATH", &db_path);
    write_openai_subscription_auth();
    std::env::set_var(
        "TAMUX_CODEX_CLI_AUTH_PATH",
        temp_dir.path().join("missing-codex-auth.json"),
    );

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4".to_string();
    config.providers.insert(
        PROVIDER_ID_GROQ.to_string(),
        provider_config("", "", "groq-key", AuthSource::ApiKey),
    );
    let (engine, _temp_dir) = make_test_engine(config).await;

    let resolved = {
        let config = engine.config.read().await;
        resolve_candidate_provider_config(&config, PROVIDER_ID_GROQ)
            .expect("candidate provider should resolve with its default model")
    };
    let suggestion = engine.suggest_alternative_provider(PROVIDER_ID_OPENAI).await;

    std::env::remove_var("TAMUX_PROVIDER_AUTH_DB_PATH");
    std::env::remove_var("TAMUX_CODEX_CLI_AUTH_PATH");
    assert_eq!(resolved.model, "llama-3.3-70b-versatile");
    assert!(
        suggestion
            .as_deref()
            .unwrap_or_default()
            .contains(PROVIDER_ID_GROQ),
        "expected groq to remain eligible using its own default model"
    );
}

async fn spawn_hung_http_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind hung http server");
    let addr = listener.local_addr().expect("hung server local addr");
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buffer = [0u8; 1024];
                let _ = socket.read(&mut buffer).await;
                tokio::time::sleep(Duration::from_secs(15)).await;
            });
        }
    });
    format!("http://{addr}/v1")
}

#[tokio::test]
async fn send_message_times_out_hung_provider_request() {
    let server_url = spawn_hung_http_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let session_manager = SessionManager::new_test(temp_dir.path()).await;
    let history = HistoryStore::new_test_store(temp_dir.path())
        .await
        .expect("history store");
    let data_dir = temp_dir.path().join("agent");
    std::fs::create_dir_all(&data_dir).expect("create agent data dir");

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = server_url;
    config.model = "gpt-4o-mini".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.max_retries = 0;
    config.auto_retry = false;

    let engine = AgentEngine::new_with_storage_and_http_client(
        session_manager,
        config,
        history,
        data_dir,
        build_agent_http_client(Duration::from_millis(75)),
    );

    let result = tokio::time::timeout(
        Duration::from_secs(4),
        engine.send_message_inner(
            None,
            "What model are you?",
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        ),
    )
    .await
    .expect("hung provider request should time out at the HTTP layer, not the test harness");

    let error = match result {
        Ok(_) => panic!("hung provider should surface as an error"),
        Err(error) => error,
    };
    let error_text = error.to_string().to_lowercase();
    assert!(
        error_text.contains("timed out"),
        "expected timeout error, got: {error}"
    );
}
