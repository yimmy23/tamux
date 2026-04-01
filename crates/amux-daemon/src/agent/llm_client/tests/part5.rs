use crate::agent::openai_codex_auth::{
    complete_openai_codex_auth_flow_with_result_for_tests,
    complete_openai_codex_auth_with_code_for_tests, logout_openai_codex_auth,
    current_pending_openai_codex_flow_id_for_tests, mark_openai_codex_auth_timeout_for_tests,
    openai_codex_auth_status, tombstone_present_for_tests, OpenAICodexExchange,
};
use crate::agent::types::{AgentConfig, ApiTransport, ProviderConfig};
use crate::agent::AgentEngine;
use crate::session_manager::SessionManager;

#[test]
fn pending_login_reuses_flow() {
    let _lock = provider_auth_store::provider_auth_test_env_lock();
    let temp_dir = tempdir().expect("tempdir should succeed");
    let _env_guard = EnvGuard::new(&["TAMUX_PROVIDER_AUTH_DB_PATH", "TAMUX_CODEX_CLI_AUTH_PATH"]);
    std::env::set_var(
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        temp_dir.path().join("provider-auth.db"),
    );
    std::env::set_var(
        "TAMUX_CODEX_CLI_AUTH_PATH",
        temp_dir.path().join("missing-codex-auth.json"),
    );
    clear_openai_codex_auth_test_state();

    let first = begin_openai_codex_auth_login().expect("first login should succeed");
    let second = begin_openai_codex_auth_login().expect("second login should succeed");

    assert_eq!(first.status.as_deref(), Some("pending"));
    assert_eq!(second.status.as_deref(), Some("pending"));
    assert_eq!(first.auth_url, second.auth_url);
}

struct TestExchange {
    result: std::result::Result<StoredOpenAICodexAuth, String>,
}

impl OpenAICodexExchange for TestExchange {
    fn exchange_authorization_code(
        &self,
        _code: &str,
        _verifier: &str,
    ) -> Result<StoredOpenAICodexAuth> {
        match &self.result {
            Ok(auth) => Ok(auth.clone()),
            Err(message) => Err(anyhow::anyhow!(message.clone())),
        }
    }
}

fn stored_auth_fixture() -> StoredOpenAICodexAuth {
    StoredOpenAICodexAuth {
        provider: Some("openai-codex".to_string()),
        auth_mode: Some("chatgpt_subscription".to_string()),
        access_token: "header.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiYWNjdC0xIn0sImV4cCI6NDEwMjQ0NDgwMH0.signature".to_string(),
        refresh_token: "refresh-token".to_string(),
        account_id: Some("acct-1".to_string()),
        expires_at: Some(4_102_444_800_000),
        source: Some("tamux".to_string()),
        updated_at: Some(4_102_444_800_000),
        created_at: Some(4_102_444_800_000),
    }
}

#[test]
fn login_timeout_sets_error_state() {
    let _lock = provider_auth_store::provider_auth_test_env_lock();
    let temp_dir = tempdir().expect("tempdir should succeed");
    let _env_guard = EnvGuard::new(&[
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "TAMUX_CODEX_CLI_AUTH_PATH",
    ]);
    std::env::set_var(
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        temp_dir.path().join("provider-auth.db"),
    );
    std::env::set_var(
        "TAMUX_CODEX_CLI_AUTH_PATH",
        temp_dir.path().join("missing-codex-auth.json"),
    );
    clear_openai_codex_auth_test_state();
    begin_openai_codex_auth_login().expect("login should start");

    let status = mark_openai_codex_auth_timeout_for_tests();

    assert_eq!(status.status.as_deref(), Some("error"));
    assert_eq!(status.available, false);
    assert!(status
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("timed out"));
}

#[test]
fn exchange_failure_sets_error_state() {
    let _lock = provider_auth_store::provider_auth_test_env_lock();
    let temp_dir = tempdir().expect("tempdir should succeed");
    let _env_guard = EnvGuard::new(&[
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "TAMUX_CODEX_CLI_AUTH_PATH",
    ]);
    std::env::set_var(
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        temp_dir.path().join("provider-auth.db"),
    );
    std::env::set_var(
        "TAMUX_CODEX_CLI_AUTH_PATH",
        temp_dir.path().join("missing-codex-auth.json"),
    );
    clear_openai_codex_auth_test_state();
    begin_openai_codex_auth_login().expect("login should start");

    let status = complete_openai_codex_auth_with_code_for_tests(
        "bad-code",
        &TestExchange {
            result: Err("exchange failed".to_string()),
        },
    );

    assert_eq!(status.status.as_deref(), Some("error"));
    assert_eq!(status.available, false);
    assert!(status
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("exchange failed"));
}

#[test]
fn successful_login_persists_auth_and_reports_completed() {
    let _lock = provider_auth_store::provider_auth_test_env_lock();
    let temp_dir = tempdir().expect("tempdir should succeed");
    let _env_guard = EnvGuard::new(&[
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "TAMUX_CODEX_CLI_AUTH_PATH",
    ]);
    std::env::set_var(
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        temp_dir.path().join("provider-auth.db"),
    );
    std::env::set_var(
        "TAMUX_CODEX_CLI_AUTH_PATH",
        temp_dir.path().join("missing-codex-auth.json"),
    );
    clear_openai_codex_auth_test_state();
    begin_openai_codex_auth_login().expect("login should start");

    let completed = complete_openai_codex_auth_with_code_for_tests(
        "good-code",
        &TestExchange {
            result: Ok(stored_auth_fixture()),
        },
    );

    assert_eq!(completed.status.as_deref(), Some("completed"));
    assert!(completed.available);
    assert!(completed.auth_url.is_none());

    let status = openai_codex_auth_status(false);
    assert!(status.available);
    assert_eq!(status.status.as_deref(), Some("completed"));
    assert_eq!(status.account_id.as_deref(), Some("acct-1"));
}

#[test]
fn status_during_pending_returns_same_auth_url() {
    let _lock = provider_auth_store::provider_auth_test_env_lock();
    let temp_dir = tempdir().expect("tempdir should succeed");
    let _env_guard = EnvGuard::new(&[
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "TAMUX_CODEX_CLI_AUTH_PATH",
    ]);
    std::env::set_var(
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        temp_dir.path().join("provider-auth.db"),
    );
    std::env::set_var(
        "TAMUX_CODEX_CLI_AUTH_PATH",
        temp_dir.path().join("missing-codex-auth.json"),
    );
    clear_openai_codex_auth_test_state();

    let login = begin_openai_codex_auth_login().expect("login should start");
    let status = openai_codex_auth_status(false);

    assert_eq!(status.status.as_deref(), Some("pending"));
    assert_eq!(status.auth_url, login.auth_url);
    assert_eq!(status.available, false);
    assert!(status.account_id.is_none());
    assert!(status.error.is_none());
}

#[test]
fn logout_during_pending_cancels_flow() {
    let _lock = provider_auth_store::provider_auth_test_env_lock();
    let temp_dir = tempdir().expect("tempdir should succeed");
    let _env_guard = EnvGuard::new(&[
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "TAMUX_CODEX_CLI_AUTH_PATH",
    ]);
    std::env::set_var(
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        temp_dir.path().join("provider-auth.db"),
    );
    std::env::set_var(
        "TAMUX_CODEX_CLI_AUTH_PATH",
        temp_dir.path().join("missing-codex-auth.json"),
    );
    clear_openai_codex_auth_test_state();
    begin_openai_codex_auth_login().expect("login should start");

    logout_openai_codex_auth().expect("logout should succeed");
    let status = openai_codex_auth_status(false);

    assert_eq!(status.status, None);
    assert!(!status.available);
    assert!(status.auth_url.is_none());
}

#[test]
fn logout_tombstone_blocks_codex_import() {
    let _lock = provider_auth_store::provider_auth_test_env_lock();
    let temp_dir = tempdir().expect("tempdir should succeed");
    let _env_guard = EnvGuard::new(&[
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "TAMUX_CODEX_CLI_AUTH_PATH",
    ]);
    std::env::set_var(
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        temp_dir.path().join("provider-auth.db"),
    );
    let codex_auth_path = temp_dir.path().join("codex-auth.json");
    std::env::set_var("TAMUX_CODEX_CLI_AUTH_PATH", &codex_auth_path);
    clear_openai_codex_auth_test_state();
    std::fs::write(
        &codex_auth_path,
        r#"{"tokens":{"access_token":"header.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiYWNjdC0xIn0sImV4cCI6NDEwMjQ0NDgwMH0.signature","refresh_token":"refresh-token"}}"#,
    )
    .expect("write codex auth fixture");

    logout_openai_codex_auth().expect("logout should succeed");
    assert!(tombstone_present_for_tests());
    assert!(import_codex_cli_auth_if_present().expect("import should not error").is_none());

    reset_openai_codex_auth_runtime_for_tests();
    assert!(tombstone_present_for_tests());
    assert!(import_codex_cli_auth_if_present().expect("import should not error").is_none());
}

#[test]
fn explicit_login_clears_tombstone() {
    let _lock = provider_auth_store::provider_auth_test_env_lock();
    let temp_dir = tempdir().expect("tempdir should succeed");
    let _env_guard = EnvGuard::new(&[
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "TAMUX_CODEX_CLI_AUTH_PATH",
    ]);
    std::env::set_var(
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        temp_dir.path().join("provider-auth.db"),
    );
    std::env::set_var(
        "TAMUX_CODEX_CLI_AUTH_PATH",
        temp_dir.path().join("missing-codex-auth.json"),
    );
    clear_openai_codex_auth_test_state();
    logout_openai_codex_auth().expect("logout should succeed");
    assert!(tombstone_present_for_tests());

    let login = begin_openai_codex_auth_login().expect("login should start");

    assert_eq!(login.status.as_deref(), Some("pending"));
    assert!(!tombstone_present_for_tests());
}

#[test]
fn login_after_error_starts_fresh_flow() {
    let _lock = provider_auth_store::provider_auth_test_env_lock();
    let temp_dir = tempdir().expect("tempdir should succeed");
    let _env_guard = EnvGuard::new(&[
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "TAMUX_CODEX_CLI_AUTH_PATH",
    ]);
    std::env::set_var(
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        temp_dir.path().join("provider-auth.db"),
    );
    std::env::set_var(
        "TAMUX_CODEX_CLI_AUTH_PATH",
        temp_dir.path().join("missing-codex-auth.json"),
    );
    clear_openai_codex_auth_test_state();
    begin_openai_codex_auth_login().expect("login should start");
    mark_openai_codex_auth_timeout_for_tests();

    let fresh = begin_openai_codex_auth_login().expect("fresh login should start");

    assert_eq!(fresh.status.as_deref(), Some("pending"));
    assert!(fresh.error.is_none());
    assert!(fresh.auth_url.is_some());
}

#[test]
fn stale_flow_completion_returns_current_pending_status() {
    let _lock = provider_auth_store::provider_auth_test_env_lock();
    let temp_dir = tempdir().expect("tempdir should succeed");
    let _env_guard = EnvGuard::new(&[
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "TAMUX_CODEX_CLI_AUTH_PATH",
    ]);
    std::env::set_var(
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        temp_dir.path().join("provider-auth.db"),
    );
    std::env::set_var(
        "TAMUX_CODEX_CLI_AUTH_PATH",
        temp_dir.path().join("missing-codex-auth.json"),
    );
    clear_openai_codex_auth_test_state();

    begin_openai_codex_auth_login().expect("first login should start");
    let stale_flow_id = current_pending_openai_codex_flow_id_for_tests()
        .expect("pending flow id should be present");
    logout_openai_codex_auth().expect("logout should succeed");
    let current = begin_openai_codex_auth_login().expect("second login should start");

    let status = complete_openai_codex_auth_flow_with_result_for_tests(
        &stale_flow_id,
        Ok(stored_auth_fixture()),
    );

    assert_eq!(status.status.as_deref(), Some("pending"));
    assert_eq!(status.auth_url, current.auth_url);
    assert!(!status.available);
}

#[test]
fn successful_login_reports_error_when_persistence_fails() {
    let _lock = provider_auth_store::provider_auth_test_env_lock();
    let temp_dir = tempdir().expect("tempdir should succeed");
    let _env_guard = EnvGuard::new(&[
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "TAMUX_CODEX_CLI_AUTH_PATH",
    ]);
    std::env::set_var(
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        temp_dir.path().join("provider-auth.db"),
    );
    std::env::set_var(
        "TAMUX_CODEX_CLI_AUTH_PATH",
        temp_dir.path().join("missing-codex-auth.json"),
    );
    clear_openai_codex_auth_test_state();
    begin_openai_codex_auth_login().expect("login should start");
    std::env::set_var("TAMUX_PROVIDER_AUTH_DB_PATH", temp_dir.path());

    let status = complete_openai_codex_auth_with_code_for_tests(
        "good-code",
        &TestExchange {
            result: Ok(stored_auth_fixture()),
        },
    );

    assert_eq!(status.status.as_deref(), Some("error"));
    assert!(!status.available);
    assert!(status
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("persist"));
}

#[test]
fn status_refresh_reports_import_persistence_failure() {
    let _lock = provider_auth_store::provider_auth_test_env_lock();
    let temp_dir = tempdir().expect("tempdir should succeed");
    let _env_guard = EnvGuard::new(&[
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "TAMUX_CODEX_CLI_AUTH_PATH",
    ]);
    let codex_auth_path = temp_dir.path().join("codex-auth.json");
    let invalid_parent = temp_dir.path().join("not-a-dir");
    std::fs::write(&invalid_parent, "blocking parent").expect("write blocking file");
    std::env::set_var(
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        invalid_parent.join("provider-auth.db"),
    );
    std::env::set_var("TAMUX_CODEX_CLI_AUTH_PATH", &codex_auth_path);
    clear_openai_codex_auth_test_state();
    std::fs::write(
        &codex_auth_path,
        r#"{"tokens":{"access_token":"header.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiYWNjdC0xIn0sImV4cCI6NDEwMjQ0NDgwMH0.signature","refresh_token":"refresh-token"}}"#,
    )
    .expect("write codex auth fixture");

    let status = openai_codex_auth_status(true);

    assert_eq!(status.status.as_deref(), Some("error"));
    assert!(!status.available);
    assert!(status
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("persist"));
}

#[tokio::test]
async fn provider_auth_states_respect_codex_helper_state() {
    let _lock = crate::agent::provider_auth_store::provider_auth_test_env_lock();
    let _guard = EnvGuard::new(&[
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "TAMUX_CODEX_CLI_AUTH_PATH",
    ]);
    let root = tempdir().unwrap();
    std::env::set_var(
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        root.path().join("provider-auth.db"),
    );
    std::env::set_var(
        "TAMUX_CODEX_CLI_AUTH_PATH",
        root.path().join("missing-codex-auth.json"),
    );
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
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

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

#[test]
fn codex_status_payload_omits_secrets() {
    let _lock = provider_auth_store::provider_auth_test_env_lock();
    let temp_dir = tempdir().expect("tempdir should succeed");
    let _env_guard = EnvGuard::new(&[
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "TAMUX_CODEX_CLI_AUTH_PATH",
    ]);
    std::env::set_var(
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        temp_dir.path().join("provider-auth.db"),
    );
    std::env::set_var(
        "TAMUX_CODEX_CLI_AUTH_PATH",
        temp_dir.path().join("missing-codex-auth.json"),
    );
    reset_openai_codex_auth_runtime_for_tests();
    begin_openai_codex_auth_login().expect("login should start");
    let pending_json = serde_json::to_value(begin_openai_codex_auth_login().expect("pending login")).expect("serialize pending");
    assert!(pending_json.get("accessToken").is_none());
    assert!(pending_json.get("refreshToken").is_none());

    complete_openai_codex_auth_with_code_for_tests(
        "good-code",
        &TestExchange {
            result: Ok(stored_auth_fixture()),
        },
    );
    let status_json = serde_json::to_value(openai_codex_auth_status(false)).expect("serialize status");
    assert!(status_json.get("accessToken").is_none());
    assert!(status_json.get("refreshToken").is_none());
}
