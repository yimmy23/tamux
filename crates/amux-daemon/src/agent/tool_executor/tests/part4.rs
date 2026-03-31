    #[tokio::test]
    async fn search_files_runtime_returns_no_matches_only_for_grep_exit_code_one() {
        let result = execute_search_files_with_runner(
            &serde_json::json!({ "pattern": "needle" }),
            |_| async move {
                Ok::<super::SearchFilesCommandOutput, anyhow::Error>(
                    super::SearchFilesCommandOutput {
                        status: exit_status_with_code(1),
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                        truncated: false,
                    },
                )
            },
        )
        .await
        .expect("rg exit code 1 should be treated as no matches");

        assert_eq!(result, "No matches found.");
    }

    #[tokio::test]
    async fn search_files_runtime_surfaces_real_rg_failures() {
        let error = execute_search_files_with_runner(
            &serde_json::json!({ "pattern": "[" }),
            |_| async move {
                Ok::<super::SearchFilesCommandOutput, anyhow::Error>(
                    super::SearchFilesCommandOutput {
                        status: exit_status_with_code(2),
                        stdout: Vec::new(),
                        stderr: b"grep: Invalid regular expression".to_vec(),
                        truncated: false,
                    },
                )
            },
        )
        .await
        .expect_err("rg exit code >1 should be treated as a real failure");

        assert!(error.to_string().contains("invalid regex"));
        assert!(error.to_string().contains("Invalid regular expression"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn search_files_subprocess_helper_kills_child_when_timeout_drops_future() {
        let dir = tempdir().expect("tempdir should succeed");
        let pid_path = dir.path().join("search-files-timeout.pid");
        let script = format!(
            "import os, pathlib, time; pid_path = pathlib.Path(r\"{}\"); pid_path.parent.mkdir(parents=True, exist_ok=True); pid_path.write_text(str(os.getpid())); time.sleep(30)",
            pid_path.display()
        );

        let mut command = tokio::process::Command::new("python3");
        command.arg("-c").arg(script);

        let task = tokio::spawn(run_search_files_command(command));

        let pid = timeout(Duration::from_secs(2), async {
            loop {
                if let Ok(raw) = fs::read_to_string(&pid_path) {
                    let raw = raw.trim();
                    if !raw.is_empty() {
                        break raw
                            .parse::<u32>()
                            .expect("pid file should contain a valid pid");
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("pid file should be written promptly");

        task.abort();
        let join_error = task
            .await
            .expect_err("aborted task should not complete successfully");
        assert!(
            join_error.is_cancelled(),
            "task abort should cancel the future"
        );

        let proc_path = std::path::PathBuf::from(format!("/proc/{pid}"));
        timeout(Duration::from_secs(1), async {
            loop {
                if !proc_path.exists() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("timed out subprocess should be killed when future is dropped");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn search_files_bounded_subprocess_kills_child_when_global_cap_is_hit() {
        let dir = tempdir().expect("tempdir should succeed");
        let pid_path = dir.path().join("search-files-bounded.pid");
        let script = format!(
            "import os, pathlib, sys, time; pathlib.Path(r\"{}\").write_text(str(os.getpid())); print('first:1:needle', flush=True); print('second:2:needle', flush=True); time.sleep(30)",
            pid_path.display()
        );

        let mut command = tokio::process::Command::new("python3");
        command.arg("-c").arg(script);

        let started = std::time::Instant::now();
        let output = super::run_search_files_command_bounded(command, 1)
            .await
            .expect("bounded helper should succeed");

        assert!(
            output.truncated,
            "bounded helper should mark output truncated"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "first:1:needle");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "bounded helper should terminate promptly after reaching the cap"
        );

        let pid = timeout(Duration::from_secs(1), async {
            loop {
                if let Ok(raw) = fs::read_to_string(&pid_path) {
                    let raw = raw.trim();
                    if !raw.is_empty() {
                        break raw
                            .parse::<u32>()
                            .expect("pid file should contain a valid pid");
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("pid file should be written promptly");

        let proc_path = std::path::PathBuf::from(format!("/proc/{pid}"));
        timeout(Duration::from_secs(1), async {
            loop {
                if !proc_path.exists() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("bounded helper should kill subprocess once cap is reached");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn search_files_bounded_subprocess_does_not_truncate_slow_exact_cap() {
        let script = "import sys, time; print('first:1:needle', flush=True); time.sleep(0.2)";

        let mut command = tokio::process::Command::new("python3");
        command.arg("-c").arg(script);

        let output = super::run_search_files_command_bounded(command, 1)
            .await
            .expect("bounded helper should succeed");

        assert!(
            !output.truncated,
            "exact-cap output should not be marked truncated"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "first:1:needle");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn search_files_bounded_subprocess_handles_non_utf8_output_lossily() {
        let script = "import os, sys; os.write(sys.stdout.fileno(), b'bad\\xffpath:1:needle\\n')";

        let mut command = tokio::process::Command::new("python3");
        command.arg("-c").arg(script);

        let output = super::run_search_files_command_bounded(command, 1)
            .await
            .expect("bounded helper should succeed");

        assert!(
            !output.truncated,
            "single non-utf8 line should not be truncated"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "bad\u{fffd}path:1:needle"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn search_files_bounded_subprocess_rejects_huge_single_line() {
        let oversized_line_len = 70_000usize;
        let script = format!(
            "import sys; sys.stdout.write('x' * {}); sys.stdout.flush()",
            oversized_line_len
        );

        let mut command = tokio::process::Command::new("python3");
        command.arg("-c").arg(script);

        let error = super::run_search_files_command_bounded(command, 1)
            .await
            .err()
            .expect("oversized single line should be rejected");

        assert!(error.to_string().contains("search output line exceeded"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn search_files_bounded_subprocess_limits_captured_stderr_bytes() {
        let noisy_stderr_len = 70_000usize;
        let script = format!(
            "import sys; print('ok:1:needle'); sys.stderr.write('e' * {}); sys.stderr.flush()",
            noisy_stderr_len
        );

        let mut command = tokio::process::Command::new("python3");
        command.arg("-c").arg(script);

        let output = super::run_search_files_command_bounded(command, 1)
            .await
            .expect("bounded helper should succeed");

        assert!(
            !output.truncated,
            "stderr overflow alone should not mark stdout truncated"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "ok:1:needle");
        assert!(output.stderr.len() < noisy_stderr_len);
    }

    #[tokio::test]
    async fn onecontext_search_runtime_returns_timeout_error_when_runner_exceeds_limit() {
        let error = execute_onecontext_search_with_runner(
            &serde_json::json!({ "query": "timeout policy", "timeout_seconds": 0 }),
            true,
            |_| async move {
                tokio::time::sleep(Duration::from_millis(10)).await;
                Ok::<std::process::Output, anyhow::Error>(std::process::Output {
                    status: successful_exit_status(),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            },
        )
        .await
        .expect_err("runner exceeding timeout should return timeout error");

        assert!(error.to_string().contains("onecontext search timed out"));
    }

    #[tokio::test]
    async fn onecontext_search_rejects_negative_timeout_seconds() {
        let error = execute_onecontext_search_with_runner(
            &serde_json::json!({ "query": "timeout policy", "timeout_seconds": -1 }),
            true,
            |_| async move {
                panic!("runner should not execute when timeout is invalid");
                #[allow(unreachable_code)]
                Ok::<std::process::Output, anyhow::Error>(std::process::Output {
                    status: successful_exit_status(),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            },
        )
        .await
        .expect_err("negative timeout should be rejected");

        assert!(error
            .to_string()
            .contains("'timeout_seconds' must be a non-negative integer"));
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
        let error = validate_read_path("/tmp/ba\td")
            .expect_err("list_files should reject control characters");
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
