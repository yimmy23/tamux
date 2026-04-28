use super::*;

fn unique_test_db_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("zorai-auth-{name}-{}.sqlite", Uuid::new_v4()))
}

#[cfg(unix)]
#[test]
fn github_copilot_flow_imports_existing_gh_token() {
    use std::os::unix::fs::PermissionsExt;

    let _lock = auth_test_env_lock().lock().expect("lock auth env");
    let db_path = unique_test_db_path("gh-import");
    let script_path = std::env::temp_dir().join(format!("zorai-gh-{}", Uuid::new_v4()));
    let old_db_path = std::env::var(PROVIDER_AUTH_DB_PATH_ENV).ok();
    let old_gh_cli_path = std::env::var(GITHUB_CLI_PATH_ENV).ok();
    std::fs::write(
        &script_path,
        "#!/bin/sh\nif [ \"$1\" = \"auth\" ] && [ \"$2\" = \"token\" ]; then\n  printf 'ghu_test_token\\n'\n  exit 0\nfi\nexit 1\n",
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script_path, perms).unwrap();

    std::env::set_var(PROVIDER_AUTH_DB_PATH_ENV, &db_path);
    std::env::set_var(GITHUB_CLI_PATH_ENV, &script_path);

    let result = begin_github_copilot_auth_flow().unwrap();
    assert!(matches!(
        result,
        GithubCopilotAuthFlowResult::ImportedFromGhCli
    ));
    let stored = read_stored_github_copilot_auth().expect("stored copilot auth");
    assert_eq!(stored.access_token, "ghu_test_token");
    assert_eq!(stored.source, "gh_cli_import");

    if let Some(value) = old_db_path {
        std::env::set_var(PROVIDER_AUTH_DB_PATH_ENV, value);
    } else {
        std::env::remove_var(PROVIDER_AUTH_DB_PATH_ENV);
    }
    if let Some(value) = old_gh_cli_path {
        std::env::set_var(GITHUB_CLI_PATH_ENV, value);
    } else {
        std::env::remove_var(GITHUB_CLI_PATH_ENV);
    }
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&script_path);
}
