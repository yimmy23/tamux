    use super::{
        build_list_files_script, build_write_file_command, build_write_file_script,
        command_looks_interactive, command_matches_policy_risk, command_requires_managed_state,
        daemon_tool_timeout_seconds, default_timeout_seconds_for_tool,
        execute_apply_patch,
        execute_fetch_url_with_runner, execute_gateway_message, execute_get_divergent_session,
        execute_headless_shell_command, execute_onecontext_search_with_runner, execute_read_file,
        execute_search_files_with_runner, execute_tool, execute_web_search_with_runner,
        get_available_tools, managed_alias_args, parse_capture_output, parse_tool_args,
        resolve_skill_path, run_search_files_command, should_use_linked_whatsapp_transport,
        should_use_managed_execution, validate_read_path, validate_write_path,
        wait_for_managed_command_outcome,
    };
    use crate::agent::{
        types::{AgentConfig, AgentEvent, ToolCall, ToolFunction},
        AgentEngine,
    };
    use crate::history::SkillVariantRecord;
    use crate::session_manager::SessionManager;
    use amux_protocol::{DaemonMessage, GatewaySendResult, SessionId};
    use base64::Engine;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;
    use tokio::sync::broadcast;
    use tokio::time::{timeout, Duration};
    use tokio_util::sync::CancellationToken;

    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;

    fn successful_exit_status() -> std::process::ExitStatus {
        exit_status_with_code(0)
    }

    fn exit_status_with_code(code: i32) -> std::process::ExitStatus {
        #[cfg(unix)]
        {
            std::process::ExitStatus::from_raw(code << 8)
        }

        #[cfg(windows)]
        {
            std::process::ExitStatus::from_raw(code as u32)
        }
    }

    #[test]
    fn daemon_tool_timeout_uses_300_seconds_for_onecontext_search() {
        assert_eq!(default_timeout_seconds_for_tool("onecontext_search"), 300);
        assert_eq!(
            daemon_tool_timeout_seconds("onecontext_search", &serde_json::json!({})),
            300
        );
    }

    #[test]
    fn daemon_tool_timeout_uses_300_seconds_for_fetch_url() {
        assert_eq!(default_timeout_seconds_for_tool("fetch_url"), 300);
        assert_eq!(
            daemon_tool_timeout_seconds("fetch_url", &serde_json::json!({})),
            300
        );
    }

    #[test]
    fn daemon_tool_timeout_uses_300_seconds_for_web_search() {
        assert_eq!(default_timeout_seconds_for_tool("web_search"), 300);
        assert_eq!(
            daemon_tool_timeout_seconds("web_search", &serde_json::json!({})),
            300
        );
    }

    #[test]
    fn daemon_tool_timeout_clamps_explicit_override_to_600_seconds() {
        assert_eq!(
            daemon_tool_timeout_seconds(
                "onecontext_search",
                &serde_json::json!({ "timeout_seconds": 999 })
            ),
            600
        );
    }

    #[test]
    fn onecontext_search_tool_schema_exposes_timeout_seconds() {
        let config = AgentConfig::default();
        let temp_dir = std::env::temp_dir();
        let tools = get_available_tools(&config, &temp_dir, false);
        let onecontext = tools
            .iter()
            .find(|tool| tool.function.name == "onecontext_search")
            .expect("onecontext_search tool should be available");

        let timeout_schema = onecontext
            .function
            .parameters
            .get("properties")
            .and_then(|properties| properties.get("timeout_seconds"))
            .expect("onecontext_search schema should expose timeout_seconds");

        assert_eq!(
            timeout_schema.get("type").and_then(|value| value.as_str()),
            Some("integer")
        );
        assert_eq!(
            timeout_schema
                .get("minimum")
                .and_then(|value| value.as_u64()),
            Some(0)
        );
        assert_eq!(
            timeout_schema
                .get("maximum")
                .and_then(|value| value.as_u64()),
            Some(600)
        );
        assert!(timeout_schema
            .get("description")
            .and_then(|value| value.as_str())
            .is_some_and(|value| value.contains("default: 300") && value.contains("max: 600")));
    }

    #[test]
    fn web_search_tool_schema_exposes_timeout_seconds() {
        let mut config = AgentConfig::default();
        config.tools.web_search = true;
        let temp_dir = std::env::temp_dir();
        let tools = get_available_tools(&config, &temp_dir, false);
        let web_search = tools
            .iter()
            .find(|tool| tool.function.name == "web_search")
            .expect("web_search tool should be available");

        let timeout_schema = web_search
            .function
            .parameters
            .get("properties")
            .and_then(|properties| properties.get("timeout_seconds"))
            .expect("web_search schema should expose timeout_seconds");

        assert_eq!(
            timeout_schema.get("type").and_then(|value| value.as_str()),
            Some("integer")
        );
        assert_eq!(
            timeout_schema
                .get("minimum")
                .and_then(|value| value.as_u64()),
            Some(0)
        );
        assert_eq!(
            timeout_schema
                .get("maximum")
                .and_then(|value| value.as_u64()),
            Some(600)
        );
        assert!(timeout_schema
            .get("description")
            .and_then(|value| value.as_str())
            .is_some_and(|value| value.contains("default: 300") && value.contains("max: 600")));
    }

    #[test]
    fn fetch_url_tool_schema_exposes_timeout_seconds() {
        let mut config = AgentConfig::default();
        config.tools.web_browse = true;
        let temp_dir = std::env::temp_dir();
        let tools = get_available_tools(&config, &temp_dir, false);
        let fetch_url = tools
            .iter()
            .find(|tool| tool.function.name == "fetch_url")
            .expect("fetch_url tool should be available");

        let timeout_schema = fetch_url
            .function
            .parameters
            .get("properties")
            .and_then(|properties| properties.get("timeout_seconds"))
            .expect("fetch_url schema should expose timeout_seconds");

        assert_eq!(
            timeout_schema.get("type").and_then(|value| value.as_str()),
            Some("integer")
        );
        assert_eq!(
            timeout_schema
                .get("minimum")
                .and_then(|value| value.as_u64()),
            Some(0)
        );
        assert_eq!(
            timeout_schema
                .get("maximum")
                .and_then(|value| value.as_u64()),
            Some(600)
        );
        assert!(timeout_schema
            .get("description")
            .and_then(|value| value.as_str())
            .is_some_and(|value| value.contains("default: 300") && value.contains("max: 600")));
    }

    #[test]
    fn search_files_tool_schema_exposes_timeout_seconds() {
        let config = AgentConfig::default();
        let temp_dir = std::env::temp_dir();
        let tools = get_available_tools(&config, &temp_dir, false);
        let search_files = tools
            .iter()
            .find(|tool| tool.function.name == "search_files")
            .expect("search_files tool should be available");

        let timeout_schema = search_files
            .function
            .parameters
            .get("properties")
            .and_then(|properties| properties.get("timeout_seconds"))
            .expect("search_files schema should expose timeout_seconds");

        assert_eq!(
            timeout_schema.get("type").and_then(|value| value.as_str()),
            Some("integer")
        );
        assert_eq!(
            timeout_schema
                .get("minimum")
                .and_then(|value| value.as_u64()),
            Some(0)
        );
        assert_eq!(
            timeout_schema
                .get("maximum")
                .and_then(|value| value.as_u64()),
            Some(600)
        );
        assert!(timeout_schema
            .get("description")
            .and_then(|value| value.as_str())
            .is_some_and(|value| value.contains("default: 120") && value.contains("max: 600")));
    }

    #[test]
    fn summary_alias_tool_is_exposed() {
        let config = AgentConfig::default();
        let temp_dir = std::env::temp_dir();
        let tools = get_available_tools(&config, &temp_dir, false);

        assert!(tools.iter().any(|tool| tool.function.name == "summary"));
    }

    #[test]
    fn read_file_tool_schema_exposes_offset_and_limit_defaults() {
        let config = AgentConfig::default();
        let temp_dir = std::env::temp_dir();
        let tools = get_available_tools(&config, &temp_dir, false);
        let read_file = tools
            .iter()
            .find(|tool| tool.function.name == "read_file")
            .expect("read_file tool should be available");

        let properties = read_file
            .function
            .parameters
            .get("properties")
            .expect("read_file schema should expose properties");

        let offset_schema = properties
            .get("offset")
            .expect("read_file schema should expose offset");
        assert_eq!(
            offset_schema.get("type").and_then(|value| value.as_str()),
            Some("integer")
        );
        assert!(offset_schema
            .get("description")
            .and_then(|value| value.as_str())
            .is_some_and(|value| value.contains("default: 0")));

        let limit_schema = properties
            .get("limit")
            .expect("read_file schema should expose limit");
        assert_eq!(
            limit_schema.get("type").and_then(|value| value.as_str()),
            Some("integer")
        );
        assert!(limit_schema
            .get("description")
            .and_then(|value| value.as_str())
            .is_some_and(|value| value.contains("default: 250")));
    }

    #[tokio::test]
    async fn read_file_uses_default_offset_zero_and_limit_250() {
        let root = tempdir().expect("tempdir");
        let file_path = root.path().join("sample.txt");
        let body = (0..300)
            .map(|index| format!("line-{index:03}"))
            .collect::<Vec<_>>()
            .join("\n");
        tokio::fs::write(&file_path, body)
            .await
            .expect("write sample file");

        let result = execute_read_file(&serde_json::json!({
            "path": file_path,
        }))
        .await
        .expect("read file should succeed");

        assert!(result.starts_with("line-000\nline-001"));
        assert!(result.contains("line-249"));
        assert!(!result.contains("line-250\n"));
        assert!(result.contains("truncated, showing 250 of 300 lines"));
    }

    #[tokio::test]
    async fn read_file_honors_offset_and_limit_window() {
        let root = tempdir().expect("tempdir");
        let file_path = root.path().join("sample.txt");
        let body = (0..20)
            .map(|index| format!("line-{index:03}"))
            .collect::<Vec<_>>()
            .join("\n");
        tokio::fs::write(&file_path, body)
            .await
            .expect("write sample file");

        let result = execute_read_file(&serde_json::json!({
            "path": file_path,
            "offset": 5,
            "limit": 3,
        }))
        .await
        .expect("read file should succeed");

        assert_eq!(result, "line-005\nline-006\nline-007");
    }

    #[tokio::test]
    async fn summary_alias_dispatches_to_semantic_query_summary_kind() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine =
            AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
        let (event_tx, mut event_rx) = broadcast::channel(8);
        let http_client = reqwest::Client::new();

        let alias_args = serde_json::json!({
            "path": root.path(),
            "limit": 5
        });
        let semantic_args = serde_json::json!({
            "kind": "summary",
            "path": root.path(),
            "limit": 5
        });

        let alias_result = execute_tool(
            &ToolCall {
                id: "call-summary".to_string(),
                function: ToolFunction {
                    name: "summary".to_string(),
                    arguments: alias_args.to_string(),
                },
                weles_review: None,
            },
            &engine,
            "thread-summary",
            None,
            &manager,
            None,
            &event_tx,
            root.path(),
            &http_client,
            None,
        )
        .await;

        let semantic_result = execute_tool(
            &ToolCall {
                id: "call-semantic-summary".to_string(),
                function: ToolFunction {
                    name: "semantic_query".to_string(),
                    arguments: semantic_args.to_string(),
                },
                weles_review: None,
            },
            &engine,
            "thread-semantic",
            None,
            &manager,
            None,
            &event_tx,
            root.path(),
            &http_client,
            None,
        )
        .await;

        assert!(
            !alias_result.is_error,
            "summary alias should succeed: {}",
            alias_result.content
        );
        assert!(
            !semantic_result.is_error,
            "semantic_query summary should succeed: {}",
            semantic_result.content
        );
        assert_eq!(alias_result.content, semantic_result.content);

        let workflow_notice = timeout(Duration::from_millis(250), event_rx.recv())
            .await
            .expect("summary alias should emit workflow notice")
            .expect("workflow notice should be received");
        match workflow_notice {
            AgentEvent::WorkflowNotice {
                kind,
                details: Some(details),
                ..
            } => {
                assert_eq!(kind, "semantic-query");
                let details: serde_json::Value =
                    serde_json::from_str(&details).expect("workflow notice details should be json");
                assert_eq!(
                    details.get("kind").and_then(|value| value.as_str()),
                    Some("summary")
                );
                assert_eq!(
                    details.get("limit").and_then(|value| value.as_u64()),
                    Some(5)
                );
                assert!(details
                    .get("path")
                    .and_then(|value| value.as_str())
                    .is_some_and(|value| value == root.path().to_string_lossy()));
            }
            other => panic!("expected workflow notice, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_current_datetime_dispatch_returns_local_and_utc_timestamps() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine =
            AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
        let (event_tx, _event_rx) = broadcast::channel(8);
        let http_client = reqwest::Client::new();

        let result = execute_tool(
            &ToolCall {
                id: "call-current-datetime".to_string(),
                function: ToolFunction {
                    name: "get_current_datetime".to_string(),
                    arguments: serde_json::json!({}).to_string(),
                },
                weles_review: None,
            },
            &engine,
            "thread-current-datetime",
            None,
            &manager,
            None,
            &event_tx,
            root.path(),
            &http_client,
            None,
        )
        .await;

        assert!(
            !result.is_error,
            "get_current_datetime should succeed: {}",
            result.content
        );
        assert!(result.content.contains("Current datetime:"));
        assert!(result.content.contains("Local:"));
        assert!(result.content.contains("UTC:"));
        assert!(result.content.contains("Unix timestamp (ms):"));
    }

    #[tokio::test]
    async fn onecontext_search_runtime_uses_default_timeout_on_caller_path() {
        let observed_timeout = Arc::new(Mutex::new(None));
        let observed_timeout_clone = observed_timeout.clone();

        let result = execute_onecontext_search_with_runner(
            &serde_json::json!({ "query": "timeout policy" }),
            true,
            move |request| {
                let observed_timeout = observed_timeout_clone.clone();
                async move {
                    *observed_timeout
                        .lock()
                        .expect("timeout lock should succeed") = Some(request.timeout_seconds);
                    Ok::<std::process::Output, anyhow::Error>(std::process::Output {
                        status: successful_exit_status(),
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                    })
                }
            },
        )
        .await
        .expect("onecontext search should succeed");

        assert_eq!(
            *observed_timeout
                .lock()
                .expect("timeout lock should succeed"),
            Some(300)
        );
        assert!(result.contains("No OneContext matches for \"timeout policy\""));
    }
