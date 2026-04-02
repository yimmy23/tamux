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
fn apply_patch_harness_input_updates_adds_and_deletes_files() {
    let root = tempdir().expect("tempdir");
    let existing = root.path().join("existing.txt");
    let added = root.path().join("added.txt");
    let removed = root.path().join("removed.txt");
    std::fs::write(&existing, "alpha\nbeta\nomega\n").expect("write existing file");
    std::fs::write(&removed, "remove me\n").expect("write removed file");

    let patch = format!(
        "*** Begin Patch\n*** Update File: {}\n@@\n alpha\n-beta\n+gamma\n omega\n*** Add File: {}\n+hello\n+world\n*** Delete File: {}\n*** End Patch\n",
        existing.display(),
        added.display(),
        removed.display(),
    );

    let result = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(execute_apply_patch(&serde_json::json!({
            "input": patch,
            "explanation": "test harness parity"
        })))
        .expect("apply_patch should succeed");

    assert!(result.contains("Updated file"));
    assert!(result.contains("Added file"));
    assert!(result.contains("Deleted file"));
    assert_eq!(
        std::fs::read_to_string(&existing).expect("read existing file"),
        "alpha\ngamma\nomega\n"
    );
    assert_eq!(
        std::fs::read_to_string(&added).expect("read added file"),
        "hello\nworld\n"
    );
    assert!(!removed.exists(), "delete action should remove the file");
}

#[test]
fn apply_patch_is_classified_like_other_file_mutations() {
    let classification = crate::agent::weles_governance::classify_tool_call(
        "apply_patch",
        &serde_json::json!({
            "input": "*** Begin Patch\n*** Update File: /tmp/.env\n@@\n-OLD=1\n+OLD=2\n*** End Patch\n"
        }),
    );

    assert_eq!(
        classification.class,
        crate::agent::weles_governance::WelesGovernanceClass::GuardIfSuspicious
    );
    assert!(classification
        .reasons
        .iter()
        .any(|reason| reason.contains("sensitive")));
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
    assert!(!command_requires_managed_state("grep foo Cargo.toml"));
    assert!(!command_requires_managed_state("ls -la"));
}

#[test]
fn managed_execution_keeps_self_contained_shell_setup_commands_headless() {
    assert!(
        !command_requires_managed_state("cd /workspace && ls -la"),
        "one-shot directory changes should not require managed session state"
    );
    assert!(
        !command_requires_managed_state("source ~/.cargo/env && cargo --version"),
        "one-shot sourced environments should stay headless"
    );
    assert!(!should_use_managed_execution(&serde_json::json!({
        "command": "cd /workspace && ls -la"
    })));
    assert!(!should_use_managed_execution(&serde_json::json!({
        "command": "source ~/.cargo/env && cargo --version"
    })));
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
