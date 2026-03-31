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
