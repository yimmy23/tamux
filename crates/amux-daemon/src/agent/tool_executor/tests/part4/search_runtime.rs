use super::*;

#[test]
fn safe_snippet_preview_truncates_multibyte_text_on_char_boundaries() {
    let text = format!("{}’suffix", "a".repeat(299));

    let snippet = super::safe_snippet_preview(&text, 300);

    assert_eq!(snippet.chars().count(), 303);
    assert!(snippet.ends_with("’..."));
}

#[tokio::test]
async fn search_files_runtime_returns_no_matches_only_for_grep_exit_code_one() {
    let result = execute_search_files_with_runner(
        &serde_json::json!({ "pattern": "needle" }),
        |_| async move {
            Ok::<super::SearchFilesCommandOutput, anyhow::Error>(super::SearchFilesCommandOutput {
                status: exit_status_with_code(1),
                stdout: Vec::new(),
                stderr: Vec::new(),
                truncated: false,
            })
        },
    )
    .await
    .expect("rg exit code 1 should be treated as no matches");

    assert_eq!(result, "No matches found.");
}

#[tokio::test]
async fn search_files_runtime_surfaces_real_rg_failures() {
    let error =
        execute_search_files_with_runner(&serde_json::json!({ "pattern": "[" }), |_| async move {
            Ok::<super::SearchFilesCommandOutput, anyhow::Error>(super::SearchFilesCommandOutput {
                status: exit_status_with_code(2),
                stdout: Vec::new(),
                stderr: b"grep: Invalid regular expression".to_vec(),
                truncated: false,
            })
        })
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

#[tokio::test]
async fn onecontext_search_reports_exit_status_when_stderr_is_empty() {
    let error = execute_onecontext_search_with_runner(
        &serde_json::json!({ "query": "event retrieval", "scope": "event" }),
        true,
        |_| async move {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                Ok::<std::process::Output, anyhow::Error>(std::process::Output {
                    status: std::process::ExitStatus::from_raw(256),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }
            #[cfg(windows)]
            {
                use std::os::windows::process::ExitStatusExt;
                Ok::<std::process::Output, anyhow::Error>(std::process::Output {
                    status: std::process::ExitStatus::from_raw(1),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }
        },
    )
    .await
    .expect_err("nonzero exit with empty stderr should include exit context");

    let message = error.to_string();
    assert!(message.contains("onecontext search failed"));
    assert!(message.contains("event scope"));
    assert!(message.contains("exit status"));
}

#[tokio::test]
async fn onecontext_search_normalizes_simple_query_before_runner() {
    let result = execute_onecontext_search_with_runner(
        &serde_json::json!({
            "query": "Collaboration Architecture Ideation Elm-style\nbroadcast contribution daemon-first",
            "scope": "event",
            "no_regex": true
        }),
        true,
        |request| async move {
            assert_eq!(
                request.bounded_query,
                "Collaboration.*Architecture.*Ideation.*Elm.*style.*broadcast.*contribution.*daemon.*first"
            );
            Ok::<std::process::Output, anyhow::Error>(std::process::Output {
                status: successful_exit_status(),
                stdout: b"No events found.\n".to_vec(),
                stderr: Vec::new(),
            })
        },
    )
    .await
    .expect("sanitized simple query should be passed to the runner");

    assert!(result.contains("OneContext results"));
}

#[test]
fn prepare_onecontext_search_query_preserves_explicit_regex_queries() {
    let prepared =
        prepare_onecontext_search_query("JustifySkip|explicit_rationale_required", false, 300)
            .expect("explicit regex query should be preserved");

    assert_eq!(prepared, "JustifySkip|explicit_rationale_required");
}
