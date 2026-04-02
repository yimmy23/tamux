#[tokio::test]
async fn provider_auth_states_respect_codex_helper_state() {
    let _lock = crate::agent::provider_auth_store::provider_auth_test_env_lock();
    let _guard = EnvGuard::new(&[
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "TAMUX_CODEX_CLI_AUTH_PATH",
    ]);
    let root = tempdir().unwrap();
    set_test_auth_env(root.path(), &root.path().join("missing-codex-auth.json"));
    reset_openai_codex_auth_runtime_for_tests();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.auth_source = AuthSource::ChatgptSubscription;
    config.providers.insert(
        "openai".to_string(),
        ProviderConfig {
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-5.4".to_string(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source: AuthSource::ChatgptSubscription,
            api_transport: ApiTransport::Responses,
            reasoning_effort: String::new(),
            context_window_tokens: 0,
            response_schema: None,
        },
    );
    let engine: std::sync::Arc<AgentEngine> = AgentEngine::new_test(manager, config, root.path()).await;

    begin_openai_codex_auth_login().expect("login should start");
    let pending = engine
        .get_provider_auth_states()
        .await
        .into_iter()
        .find(|state| state.provider_id == "openai")
        .expect("openai state should exist");
    assert!(!pending.authenticated);

    complete_openai_codex_auth_with_code_for_tests(
        "good-code",
        &TestExchange {
            result: Ok(stored_auth_fixture()),
        },
    );
    let completed = engine
        .get_provider_auth_states()
        .await
        .into_iter()
        .find(|state| state.provider_id == "openai")
        .expect("openai state should exist");
    assert!(completed.authenticated);

    logout_openai_codex_auth().expect("logout should succeed");
    let logged_out = engine
        .get_provider_auth_states()
        .await
        .into_iter()
        .find(|state| state.provider_id == "openai")
        .expect("openai state should exist");
    assert!(!logged_out.authenticated);
}

#[tokio::test]
async fn provider_auth_states_use_codex_cli_auth_when_storage_is_empty() {
    let _lock = crate::agent::provider_auth_store::provider_auth_test_env_lock();
    let _guard = EnvGuard::new(&[
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "TAMUX_CODEX_CLI_AUTH_PATH",
    ]);
    let root = tempdir().unwrap();
    let codex_auth_path = prepare_openai_auth_test(root.path(), "codex-auth.json");
    write_codex_cli_auth_fixture(&codex_auth_path);

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.auth_source = AuthSource::ChatgptSubscription;
    config.providers.insert(
        "openai".to_string(),
        ProviderConfig {
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-5.4".to_string(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source: AuthSource::ChatgptSubscription,
            api_transport: ApiTransport::Responses,
            reasoning_effort: String::new(),
            context_window_tokens: 0,
            response_schema: None,
        },
    );
    let engine: std::sync::Arc<AgentEngine> = AgentEngine::new_test(manager, config, root.path()).await;

    assert!(read_stored_openai_codex_auth().is_none());
    let openai = engine
        .get_provider_auth_states()
        .await
        .into_iter()
        .find(|state| state.provider_id == "openai")
        .expect("openai state should exist");

    assert!(openai.authenticated);
    assert!(read_stored_openai_codex_auth().is_none());
}
