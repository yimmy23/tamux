use super::*;

#[test]
fn write_file_rejects_paths_with_trailing_whitespace() {
    let error = validate_write_path("/tmp/Dockerfile ")
        .expect_err("write_file should reject trailing whitespace");
    assert!(error.to_string().contains("leading/trailing whitespace"));
}

#[test]
fn write_file_rejects_paths_with_control_characters() {
    let error = validate_write_path("/tmp/dock\nerfile")
        .expect_err("write_file should reject control characters");
    assert!(error.to_string().contains("control characters"));
}

#[test]
fn write_file_command_encodes_path_and_content() {
    let command = build_write_file_command("/tmp/Dockerfile", "FROM scratch\n");
    assert!(command.contains("python3 -c"));
    assert!(command.contains("base64.b64decode"));
    assert!(!command.contains("/tmp/Dockerfile"));
    assert!(!command.contains("FROM scratch"));
}

#[test]
fn write_file_script_keeps_python_block_indentation() {
    let script = build_write_file_script("cGF0aA==", "Y29udGVudA==");
    assert!(script.contains("\nif actual != expected:\n    raise SystemExit("));
}

#[test]
fn list_files_rejects_paths_with_control_characters() {
    let error =
        validate_read_path("/tmp/ba\td").expect_err("list_files should reject control characters");
    assert!(error.to_string().contains("control characters"));
}

#[test]
fn parse_capture_output_decodes_payload_and_status() {
    let token = "tok123";
    let payload = "file\t12\tDockerfile\n";
    let encoded = base64::engine::general_purpose::STANDARD.encode(payload.as_bytes());
    let output = format!(
        "prefix\n__AMUX_CAPTURE_BEGIN_{token}__\n{encoded}\n__AMUX_CAPTURE_END_{token}__:0\nsuffix"
    );

    let parsed =
        parse_capture_output(output.as_bytes(), token).expect("capture output should parse");
    assert_eq!(parsed.0, 0);
    assert_eq!(parsed.1, payload);
}

#[test]
fn list_files_script_keeps_python_try_indentation() {
    let script = build_list_files_script("L3RtcA==", "tok123");
    assert!(script.contains("\ntry:\n    rows = []\n    for entry in sorted("));
    assert!(script.contains("\nexcept Exception as exc:\n    payload = f'Error: {exc}'"));
}

#[test]
fn linked_whatsapp_transport_is_used_when_native_client_exists() {
    assert!(should_use_linked_whatsapp_transport(
        "starting", true, false
    ));
}

#[test]
fn linked_whatsapp_transport_is_used_when_sidecar_exists() {
    assert!(should_use_linked_whatsapp_transport(
        "disconnected",
        false,
        true
    ));
}

#[test]
fn linked_whatsapp_transport_requires_connected_state_or_transport() {
    assert!(!should_use_linked_whatsapp_transport(
        "disconnected",
        false,
        false
    ));
}

#[test]
fn create_file_multipart_args_parse_filename_cwd_and_content() {
    let args = parse_tool_args(
            "create_file",
            "Content-Type: multipart/form-data; boundary=BOUNDARY\n\n--BOUNDARY\nContent-Disposition: form-data; name=\"filename\"\n\nnotes.md\n--BOUNDARY\nContent-Disposition: form-data; name=\"cwd\"\n\n/tmp/work\n--BOUNDARY\nContent-Disposition: form-data; name=\"file\"; filename=\"notes.md\"\nContent-Type: text/plain\n\nhello world\n--BOUNDARY--\n",
        )
        .expect("multipart payload should parse");

    assert_eq!(
        args.get("filename").and_then(|value| value.as_str()),
        Some("notes.md")
    );
    assert_eq!(
        args.get("cwd").and_then(|value| value.as_str()),
        Some("/tmp/work")
    );
    assert_eq!(
        args.get("content").and_then(|value| value.as_str()),
        Some("hello world")
    );
}

#[test]
fn write_file_multipart_args_parse_path_and_content() {
    let args = parse_tool_args(
        "write_file",
        "Content-Type: multipart/form-data; boundary=BOUNDARY\n\n--BOUNDARY\nContent-Disposition: form-data; name=\"path\"\n\n/tmp/work/notes.md\n--BOUNDARY\nContent-Disposition: form-data; name=\"file\"; filename=\"notes.md\"\nContent-Type: text/plain\n\nhello world\n--BOUNDARY--\n",
    )
    .expect("multipart payload should parse");

    assert_eq!(
        args.get("path").and_then(|value| value.as_str()),
        Some("/tmp/work/notes.md")
    );
    assert_eq!(
        args.get("content").and_then(|value| value.as_str()),
        Some("hello world")
    );
}

#[test]
fn managed_alias_leaves_security_level_for_runtime_defaults() {
    let args = serde_json::json!({
        "command": "echo hello"
    });
    let mapped = managed_alias_args(&args, "test rationale");
    assert!(
        mapped.get("security_level").is_none(),
        "alias expansion should not hardcode security defaults"
    );
}

#[test]
fn managed_alias_preserves_wait_controls() {
    let args = serde_json::json!({
        "command": "echo hello",
        "wait_for_completion": false,
        "timeout_seconds": 42
    });
    let mapped = managed_alias_args(&args, "test rationale");
    assert_eq!(
        mapped
            .get("wait_for_completion")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        mapped
            .get("timeout_seconds")
            .and_then(|value| value.as_u64()),
        Some(42)
    );
}

#[test]
fn managed_execution_prefers_terminal_for_explicit_session_or_interactive_commands() {
    assert!(should_use_managed_execution(&serde_json::json!({
        "command": "ls -la",
        "session": "abc"
    })));
    assert!(should_use_managed_execution(&serde_json::json!({
        "command": "vim Cargo.toml"
    })));
    assert!(command_looks_interactive("top"));
}

#[test]
fn managed_execution_uses_headless_for_simple_blocking_commands() {
    assert!(!should_use_managed_execution(&serde_json::json!({
        "command": "ls -la"
    })));
    assert!(!should_use_managed_execution(&serde_json::json!({
        "command": "cargo test -p tamux-tui",
        "cwd": "/tmp/work"
    })));
}

#[test]
fn managed_execution_detects_shell_state_changes() {
    assert!(command_requires_managed_state("cd /tmp"));
    assert!(command_requires_managed_state("export FOO=bar"));
    assert!(should_use_managed_execution(&serde_json::json!({
        "command": "cd /workspace && ls"
    })));
    assert!(!command_requires_managed_state("grep foo Cargo.toml"));
    assert!(!command_requires_managed_state("ls -la"));
}

#[test]
fn managed_execution_routes_policy_risky_commands_to_managed_path() {
    assert!(command_matches_policy_risk(
        "rm -rf /home/mkurman/to_remove"
    ));
    assert!(should_use_managed_execution(&serde_json::json!({
        "command": "rm -rf /home/mkurman/to_remove"
    })));
    assert!(!command_matches_policy_risk("echo hello"));
}
