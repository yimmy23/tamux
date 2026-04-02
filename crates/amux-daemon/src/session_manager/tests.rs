use super::*;

#[cfg(unix)]
#[tokio::test]
async fn list_omits_dead_sessions_and_managed_execution_rejects_them() {
    let root = tempfile::tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let (session_id, _rx) = manager
        .spawn(Some("/bin/true".to_string()), None, None, None, 80, 24)
        .await
        .expect("spawn test session");

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert!(
        manager
            .list()
            .await
            .into_iter()
            .all(|session| session.id != session_id),
        "dead sessions should not be offered as active choices"
    );

    let error = manager
        .execute_managed_command(
            session_id,
            ManagedCommandRequest {
                command: "echo hello".to_string(),
                rationale: "test".to_string(),
                allow_network: false,
                sandbox_enabled: false,
                security_level: amux_protocol::SecurityLevel::Lowest,
                cwd: None,
                language_hint: None,
                source: amux_protocol::ManagedCommandSource::Agent,
            },
        )
        .await
        .expect_err("dead sessions must be rejected for managed execution");

    assert!(error.to_string().contains("not alive"));
}
