use super::*;
use amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT;

#[tokio::test]
async fn copilot_auth_states_include_provider_row_when_unconfigured() {
    let _lock = crate::agent::provider_auth_store::provider_auth_test_env_lock();
    let _guard = EnvGuard::new(&[
        "TAMUX_GITHUB_COPILOT_DISABLE_GH_CLI",
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "COPILOT_GITHUB_TOKEN",
        "GITHUB_TOKEN",
        "GH_TOKEN",
    ]);
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    std::env::set_var("TAMUX_GITHUB_COPILOT_DISABLE_GH_CLI", "1");
    std::env::set_var(
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        root.path().join("provider-auth.db"),
    );
    std::env::remove_var("COPILOT_GITHUB_TOKEN");
    std::env::remove_var("GITHUB_TOKEN");
    std::env::remove_var("GH_TOKEN");

    let states = engine.get_provider_auth_states().await;
    let copilot = states
        .into_iter()
        .find(|state| state.provider_id == PROVIDER_ID_GITHUB_COPILOT)
        .expect("github copilot provider row should be present");

    assert!(!copilot.authenticated);
}
