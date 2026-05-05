use super::*;

fn stage_text_file_write<F>(
    path: &std::path::Path,
    content: &str,
    overwrite_existing: bool,
    commit: F,
) -> anyhow::Result<()>
where
    F: FnOnce(tempfile::NamedTempFile, &std::path::Path, bool) -> std::io::Result<()>,
{
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    std::fs::create_dir_all(parent)?;
    if path.exists() && !overwrite_existing {
        anyhow::bail!("file already exists: {}", path.display());
    }

    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(staged.as_file_mut(), content.as_bytes())?;
    staged.as_file_mut().sync_all()?;

    match commit(staged, path, overwrite_existing) {
        Ok(()) => Ok(()),
        Err(error) if !overwrite_existing && error.kind() == std::io::ErrorKind::AlreadyExists => {
            anyhow::bail!("file already exists: {}", path.display())
        }
        Err(error) => Err(error.into()),
    }
}

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
        "prefix\n__ZORAI_CAPTURE_BEGIN_{token}__\n{encoded}\n__ZORAI_CAPTURE_END_{token}__:0\nsuffix"
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
fn staged_text_file_write_keeps_existing_content_when_commit_fails() {
    let root = tempdir().expect("tempdir");
    let target = root.path().join("existing.txt");
    std::fs::write(&target, "alpha\n").expect("write existing file");

    let error = stage_text_file_write(
        &target,
        "beta\n",
        true,
        |staged: tempfile::NamedTempFile, _: &std::path::Path, _| {
            assert_eq!(
                std::fs::read_to_string(staged.path()).expect("read staged file"),
                "beta\n"
            );
            Err(std::io::Error::other("simulated commit failure"))
        },
    )
    .expect_err("staged write should surface persist failures");

    assert!(error.to_string().contains("simulated commit failure"));
    assert_eq!(
        std::fs::read_to_string(&target).expect("read existing file"),
        "alpha\n"
    );
}

#[test]
fn staged_text_file_write_does_not_leave_new_target_when_commit_fails() {
    let root = tempdir().expect("tempdir");
    let target = root.path().join("new.txt");

    let error = stage_text_file_write(
        &target,
        "hello\n",
        false,
        |staged: tempfile::NamedTempFile, path: &std::path::Path, _| {
            assert!(!path.exists(), "target should not exist before commit");
            assert_eq!(
                std::fs::read_to_string(staged.path()).expect("read staged file"),
                "hello\n"
            );
            Err(std::io::Error::other("simulated commit failure"))
        },
    )
    .expect_err("staged write should surface persist failures");

    assert!(error.to_string().contains("simulated commit failure"));
    assert!(
        !target.exists(),
        "failed staged commit should not create target"
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
fn apply_patch_accepts_patch_alias_for_harness_input() {
    let root = tempdir().expect("tempdir");
    let existing = root.path().join("existing.txt");
    std::fs::write(&existing, "alpha\nbeta\nomega\n").expect("write existing file");

    let patch = format!(
        "*** Begin Patch\n*** Update File: {}\n@@\n alpha\n-beta\n+gamma\n omega\n*** End Patch\n",
        existing.display(),
    );

    tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(execute_apply_patch(&serde_json::json!({
            "patch": patch,
            "explanation": "test patch alias compatibility"
        })))
        .expect("apply_patch should accept patch alias");

    assert_eq!(
        std::fs::read_to_string(&existing).expect("read existing file"),
        "alpha\ngamma\nomega\n"
    );
}

#[test]
fn apply_patch_reports_expected_change_marker_format() {
    let root = tempdir().expect("tempdir");
    let existing = root.path().join("existing.txt");
    std::fs::write(&existing, "alpha\nbeta\nomega\n").expect("write existing file");

    let patch = format!(
        "*** Begin Patch\n*** Update File: {}\n@@\n alpha\n beta\n omega\n*** End Patch\n",
        existing.display(),
    );

    let error = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(execute_apply_patch(&serde_json::json!({
            "input": patch
        })))
        .expect_err("apply_patch should reject context-only update hunks");

    assert!(error.to_string().contains("did not contain any hunks"));
}

#[test]
fn apply_patch_ignores_context_only_hunks_before_real_changes() {
    let root = tempdir().expect("tempdir");
    let existing = root.path().join("existing.txt");
    std::fs::write(&existing, "alpha\nbeta\nomega\n").expect("write existing file");

    let patch = format!(
        "*** Begin Patch\n*** Update File: {}\n@@\n alpha\n@@\n alpha\n-beta\n+gamma\n omega\n*** End Patch\n",
        existing.display(),
    );

    tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(execute_apply_patch(&serde_json::json!({
            "input": patch
        })))
        .expect("apply_patch should ignore context-only hunks when a later hunk has changes");

    assert_eq!(
        std::fs::read_to_string(&existing).expect("read existing file"),
        "alpha\ngamma\nomega\n"
    );
}

#[test]
fn apply_patch_rolls_back_prior_changes_when_a_later_action_fails() {
    let root = tempdir().expect("tempdir");
    let existing = root.path().join("existing.txt");
    let added = root.path().join("added.txt");
    let missing = root.path().join("missing.txt");
    std::fs::write(&existing, "alpha\nbeta\nomega\n").expect("write existing file");

    let patch = format!(
        "*** Begin Patch\n*** Update File: {}\n@@\n alpha\n-beta\n+gamma\n omega\n*** Add File: {}\n+hello\n*** Delete File: {}\n*** End Patch\n",
        existing.display(),
        added.display(),
        missing.display(),
    );

    let error = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(execute_apply_patch(&serde_json::json!({
            "input": patch
        })))
        .expect_err("apply_patch should fail when a later delete action targets a missing file");

    assert!(error.to_string().contains("cannot delete missing file"));
    assert_eq!(
        std::fs::read_to_string(&existing).expect("read existing file"),
        "alpha\nbeta\nomega\n",
        "existing file should be restored when patch execution fails"
    );
    assert!(
        !added.exists(),
        "added file should not remain on disk when patch execution fails"
    );
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
    assert!(command_looks_interactive("bash"));
}

#[test]
fn managed_execution_uses_headless_for_simple_blocking_commands() {
    assert!(!should_use_managed_execution(&serde_json::json!({
        "command": "ls -la"
    })));
    assert!(!should_use_managed_execution(&serde_json::json!({
        "command": "cargo test -p zorai-tui",
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
fn managed_execution_keeps_non_interactive_bash_scripts_headless() {
    assert!(
        !command_looks_interactive("bash /tmp/update.sh 2>&1"),
        "scripted bash invocations should not be treated as interactive shells"
    );
    assert!(!should_use_managed_execution(&serde_json::json!({
        "command": "bash /tmp/update.sh 2>&1"
    })));
}

#[test]
fn managed_execution_forces_tui_shell_tools_headless() {
    let args = serde_json::json!({
        "command": "vim Cargo.toml",
        "session": "abc",
        "wait_for_completion": false
    });

    assert!(
        !should_use_managed_execution_for_surface(Some(zorai_protocol::ClientSurface::Tui), &args),
        "TUI-originated shell tools must stay headless regardless of command heuristics"
    );
    assert!(
        should_use_managed_execution_for_surface(
            Some(zorai_protocol::ClientSurface::Electron),
            &args,
        ),
        "Electron-originated calls should still honor managed execution heuristics"
    );
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

#[test]
fn managed_execution_keeps_yolo_shell_commands_headless() {
    assert!(!should_use_managed_execution(&serde_json::json!({
        "command": "python3 -c \"print('hi')\"",
        "security_level": "yolo"
    })));
    assert!(!should_use_managed_execution(&serde_json::json!({
        "command": "rm -rf /tmp/zorai-yolo-test",
        "security_level": "yolo"
    })));
    assert!(should_use_managed_execution(&serde_json::json!({
        "command": "rm -rf /tmp/zorai-highest-test",
        "security_level": "highest"
    })));
}

#[test]
fn bash_command_waits_only_for_known_quick_commands() {
    assert!(bash_command_can_wait_for_completion(
        &serde_json::json!({ "command": "pwd" })
    ));
    assert!(bash_command_can_wait_for_completion(
        &serde_json::json!({ "command": "printf ready" })
    ));
    assert!(bash_command_can_wait_for_completion(
        &serde_json::json!({ "command": "ps aux" })
    ));
    assert!(bash_command_can_wait_for_completion(
        &serde_json::json!({ "command": "pgrep -f zorai-daemon" })
    ));
    assert!(bash_command_can_wait_for_completion(
        &serde_json::json!({ "command": "kill -TERM 12345" })
    ));
    assert!(bash_command_can_wait_for_completion(
        &serde_json::json!({ "command": "pkill -f stale-zorai-worker" })
    ));
    assert!(!bash_command_can_wait_for_completion(
        &serde_json::json!({ "command": "bash /tmp/update.sh" })
    ));
    assert!(!bash_command_can_wait_for_completion(
        &serde_json::json!({ "command": "python3 -c \"import time; time.sleep(3)\"" })
    ));
    assert!(!bash_command_can_wait_for_completion(&serde_json::json!({
        "command": "pwd",
        "wait_for_completion": false
    })));
}

#[test]
fn bash_command_does_not_force_background_for_quick_process_commands() {
    for command in [
        "ps",
        "ps aux",
        "pgrep -f zorai-daemon",
        "kill -TERM 12345",
        "pkill -f stale-zorai-worker",
    ] {
        assert!(
            !bash_command_should_force_background(&serde_json::json!({ "command": command })),
            "{command} should be eligible to return inline"
        );
    }
}
