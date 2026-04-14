use super::*;

#[test]
fn parse_cli_help_extracts_long_flags() {
    let help = "\
Usage: demo [OPTIONS]\n\
\n\
Options:\n\
  -n, --namespace <NAMESPACE>  Namespace to inspect\n\
      --all                    Include everything\n";
    let params = parse_cli_help_parameters(help);
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].name, "namespace");
    assert_eq!(params[0].param_type, "string");
    assert_eq!(params[1].name, "all");
    assert_eq!(params[1].param_type, "boolean");
}

#[test]
fn detect_cli_wrapper_synthesis_proposal_maps_safe_unknown_tool_name() {
    let proposal = super::detect_cli_wrapper_synthesis_proposal("cargo_check")
        .expect("safe cargo subcommand should produce a CLI wrapper proposal");

    assert_eq!(proposal.tool_name, "cargo_check");
    assert_eq!(proposal.target, "cargo check");
}

#[test]
fn detect_cli_wrapper_synthesis_proposal_from_command_maps_safe_readonly_shell_command() {
    let proposal = super::detect_cli_wrapper_synthesis_proposal_from_command("git status --short")
        .expect("safe readonly shell command should produce a CLI wrapper proposal");

    assert_eq!(proposal.tool_name, "git_status");
    assert_eq!(proposal.target, "git status");
}

#[test]
fn detect_cli_wrapper_synthesis_proposal_rejects_mutating_tokens() {
    assert!(super::detect_cli_wrapper_synthesis_proposal("cargo_install").is_none());
}

#[test]
fn detect_cli_wrapper_synthesis_proposal_from_command_rejects_complex_or_mutating_shell() {
    assert!(
        super::detect_cli_wrapper_synthesis_proposal_from_command("cargo install ripgrep")
            .is_none()
    );
    assert!(
        super::detect_cli_wrapper_synthesis_proposal_from_command("git status | cat").is_none()
    );
}

#[test]
fn equivalent_generated_cli_tool_matches_target_and_ignores_archived_records() -> Result<()> {
    let agent_data_dir = std::env::temp_dir().join(format!(
        "amux-generated-tools-existing-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&agent_data_dir)?;

    let proposal = super::detect_cli_wrapper_synthesis_proposal_from_command("git status --short")
        .expect("proposal should parse");

    save_generated_tool(
        &agent_data_dir,
        &GeneratedToolRecord {
            id: "git_status_existing".to_string(),
            name: "git_status_existing".to_string(),
            description: "existing tool".to_string(),
            kind: GeneratedToolKind::Cli,
            parameters: Vec::new(),
            status: "active".to_string(),
            created_at: 1,
            updated_at: 1,
            last_used_at: None,
            calls_total: 0,
            calls_success: 0,
            calls_failure: 0,
            calls_timeout: 0,
            sessions_used: 0,
            last_session_key: None,
            promoted_skill_path: None,
            cli: Some(GeneratedCliSpec {
                invocation: vec!["git".to_string(), "status".to_string()],
                help_source: "git status".to_string(),
            }),
            openapi: None,
        },
    )?;

    assert!(super::has_equivalent_generated_cli_tool(
        &agent_data_dir,
        &proposal
    )?);
    let existing = super::find_equivalent_generated_cli_tool(&agent_data_dir, &proposal)?
        .expect("existing equivalent generated tool metadata");
    assert_eq!(
        existing.get("status").and_then(|value| value.as_str()),
        Some("active")
    );

    save_generated_tool(
        &agent_data_dir,
        &GeneratedToolRecord {
            id: "git_status_archived".to_string(),
            name: "git_status_archived".to_string(),
            description: "archived tool".to_string(),
            kind: GeneratedToolKind::Cli,
            parameters: Vec::new(),
            status: "archived".to_string(),
            created_at: 2,
            updated_at: 2,
            last_used_at: None,
            calls_total: 0,
            calls_success: 0,
            calls_failure: 0,
            calls_timeout: 0,
            sessions_used: 0,
            last_session_key: None,
            promoted_skill_path: None,
            cli: Some(GeneratedCliSpec {
                invocation: vec!["git".to_string(), "status".to_string()],
                help_source: "git status".to_string(),
            }),
            openapi: None,
        },
    )?;

    let _ = std::fs::remove_dir_all(&agent_data_dir);
    Ok(())
}

#[test]
fn prune_generated_tools_keeps_active_and_promoted_records() -> Result<()> {
    let agent_data_dir = std::env::temp_dir().join(format!(
        "amux-generated-tools-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&agent_data_dir)?;
    for (id, status, updated_at) in [
        ("tool-new-old", "new", 10),
        ("tool-active", "active", 11),
        ("tool-promoted", "promoted", 12),
        ("tool-new-fresh", "new", 13),
    ] {
        save_generated_tool(
            &agent_data_dir,
            &GeneratedToolRecord {
                id: id.to_string(),
                name: id.to_string(),
                description: id.to_string(),
                kind: GeneratedToolKind::Cli,
                parameters: Vec::new(),
                status: status.to_string(),
                created_at: updated_at,
                updated_at,
                last_used_at: None,
                calls_total: 0,
                calls_success: 0,
                calls_failure: 0,
                calls_timeout: 0,
                sessions_used: 0,
                last_session_key: None,
                promoted_skill_path: None,
                cli: Some(GeneratedCliSpec {
                    invocation: vec!["echo".to_string()],
                    help_source: "help".to_string(),
                }),
                openapi: None,
            },
        )?;
    }

    prune_generated_tools(&agent_data_dir, 3)?;
    let remaining = load_generated_tools(&agent_data_dir)?
        .into_iter()
        .map(|record| record.id)
        .collect::<Vec<_>>();
    assert!(remaining.iter().any(|id| id == "tool-active"));
    assert!(remaining.iter().any(|id| id == "tool-promoted"));
    assert!(!remaining.iter().any(|id| id == "tool-new-old"));
    let _ = std::fs::remove_dir_all(&agent_data_dir);
    Ok(())
}

#[tokio::test]
async fn cli_generated_tool_execution_is_blocked_when_filesystem_disabled() {
    let record = GeneratedToolRecord {
        id: "tool-echo".to_string(),
        name: "tool-echo".to_string(),
        description: "Echo test tool".to_string(),
        kind: GeneratedToolKind::Cli,
        parameters: Vec::new(),
        status: "active".to_string(),
        created_at: 1,
        updated_at: 1,
        last_used_at: None,
        calls_total: 0,
        calls_success: 0,
        calls_failure: 0,
        calls_timeout: 0,
        sessions_used: 0,
        last_session_key: None,
        promoted_skill_path: None,
        cli: Some(GeneratedCliSpec {
            invocation: vec!["ls".to_string()],
            help_source: "help".to_string(),
        }),
        openapi: None,
    };

    let mut sandbox = ToolSynthesisSandboxConfig::default();
    sandbox.allow_filesystem = false;

    let error = run_cli_generated_tool(&record, &serde_json::json!({}), &sandbox)
        .await
        .expect_err("CLI generated tools should be blocked when filesystem access is disabled");

    assert!(
        error.to_string().contains("filesystem access"),
        "expected filesystem guard error, got: {error}"
    );
}

#[test]
fn promote_generated_tool_requires_reviewed_status() -> Result<()> {
    let test_root = std::env::temp_dir().join(format!(
        "amux-generated-tools-promote-test-{}",
        uuid::Uuid::new_v4()
    ));
    let agent_data_dir = test_root.join("agent");
    std::fs::create_dir_all(&agent_data_dir)?;

    save_generated_tool(
        &agent_data_dir,
        &GeneratedToolRecord {
            id: "tool-new".to_string(),
            name: "tool-new".to_string(),
            description: "Fresh generated tool".to_string(),
            kind: GeneratedToolKind::Cli,
            parameters: Vec::new(),
            status: "new".to_string(),
            created_at: 1,
            updated_at: 1,
            last_used_at: None,
            calls_total: 0,
            calls_success: 0,
            calls_failure: 0,
            calls_timeout: 0,
            sessions_used: 0,
            last_session_key: None,
            promoted_skill_path: None,
            cli: Some(GeneratedCliSpec {
                invocation: vec!["echo".to_string()],
                help_source: "help".to_string(),
            }),
            openapi: None,
        },
    )?;

    let error = promote_generated_tool(&agent_data_dir, "tool-new")
        .expect_err("new generated tools should not be promotable before review");

    assert!(
        error.to_string().contains("not ready for promotion"),
        "expected promotion gate error, got: {error}"
    );

    assert!(
        generated_tools_dir(&agent_data_dir)
            .join("tool-new")
            .join("tool.json")
            .exists(),
        "rejected promotion should not remove the generated tool record"
    );
    assert!(
        !super::skills_dir(&agent_data_dir)
            .join("generated")
            .join("use-tool-new.md")
            .exists(),
        "rejected promotion should not create a promoted skill artifact"
    );

    let _ = std::fs::remove_dir_all(&agent_data_dir);
    let _ = std::fs::remove_dir_all(&test_root);
    Ok(())
}

#[test]
fn retire_generated_tool_marks_tool_archived_and_removes_promoted_skill_artifact() -> Result<()> {
    let test_root = std::env::temp_dir().join(format!(
        "amux-generated-tools-retire-test-{}",
        uuid::Uuid::new_v4()
    ));
    let agent_data_dir = test_root.join("agent");
    std::fs::create_dir_all(&agent_data_dir)?;

    let promoted_skill_path = super::skills_dir(&agent_data_dir)
        .join("generated")
        .join("use-tool-retire.md");
    std::fs::create_dir_all(
        promoted_skill_path
            .parent()
            .expect("generated skill parent"),
    )?;
    std::fs::write(&promoted_skill_path, "# generated tool skill\n")?;

    save_generated_tool(
        &agent_data_dir,
        &GeneratedToolRecord {
            id: "tool-retire".to_string(),
            name: "tool-retire".to_string(),
            description: "Retirable generated tool".to_string(),
            kind: GeneratedToolKind::Cli,
            parameters: Vec::new(),
            status: "promoted".to_string(),
            created_at: 1,
            updated_at: 1,
            last_used_at: None,
            calls_total: 0,
            calls_success: 0,
            calls_failure: 0,
            calls_timeout: 0,
            sessions_used: 0,
            last_session_key: None,
            promoted_skill_path: Some(promoted_skill_path.display().to_string()),
            cli: Some(GeneratedCliSpec {
                invocation: vec!["echo".to_string()],
                help_source: "help".to_string(),
            }),
            openapi: None,
        },
    )?;

    let retired_json = retire_generated_tool(&agent_data_dir, "tool-retire")?;
    let retired: serde_json::Value = serde_json::from_str(&retired_json)?;

    assert_eq!(retired["status"], "archived");
    assert!(retired["promoted_skill_path"].is_null());
    assert!(
        !promoted_skill_path.exists(),
        "retiring a promoted generated tool should remove its promoted skill artifact"
    );

    let saved = load_generated_tool(&agent_data_dir, "tool-retire")?
        .expect("retired tool should remain in registry");
    assert_eq!(saved.status, "archived");
    assert!(saved.promoted_skill_path.is_none());

    let _ = std::fs::remove_dir_all(&test_root);
    Ok(())
}
