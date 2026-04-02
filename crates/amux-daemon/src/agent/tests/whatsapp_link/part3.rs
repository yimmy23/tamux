use super::*;

#[cfg(unix)]
#[tokio::test]
async fn attach_sidecar_process_discards_incoming_child_during_stop_window() {
    let runtime = WhatsAppLinkRuntime::new();
    {
        let mut inner = runtime.inner.lock().await;
        inner.stopping = true;
    }

    let child = Command::new("sh")
        .arg("-c")
        .arg("sleep 10")
        .spawn()
        .expect("sleep process should spawn");
    let child_pid = child.id().expect("sleep process pid should be available");

    runtime
        .attach_sidecar_process(child)
        .await
        .expect("attach should discard process while stop is active");

    {
        let inner = runtime.inner.lock().await;
        assert!(
            inner.process.is_none(),
            "process handle should not be retained while stop is active"
        );
    }

    let proc_path = std::path::PathBuf::from(format!("/proc/{child_pid}"));
    for _ in 0..10 {
        if !proc_path.exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(
        !proc_path.exists(),
        "discarded child process should be terminated"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn stop_kill_failure_emits_error_without_disconnected_and_preserves_process() {
    let runtime = WhatsAppLinkRuntime::new();
    runtime.start().await.expect("start should succeed");
    runtime
        .broadcast_linked(Some("+123456789".to_string()))
        .await;
    let mut rx = runtime.subscribe().await;
    let _ = timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("initial status snapshot should arrive")
        .expect("broadcast should be open");

    let child = Command::new("sh")
        .arg("-c")
        .arg("sleep 10")
        .spawn()
        .expect("sleep process should spawn");
    let expected_pid = child.id().expect("sleep process pid should be available");
    {
        let mut inner = runtime.inner.lock().await;
        inner.process = Some(child);
        inner.forced_stop_kill_error = Some("forced kill failure".to_string());
    }

    let err = runtime
        .stop(Some("operator_cancelled".to_string()))
        .await
        .expect_err("stop should fail when sidecar kill fails");
    assert!(
        err.to_string().contains("forced kill failure"),
        "unexpected stop error: {err}"
    );

    let error_event = timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("error event should arrive")
        .expect("broadcast should be open");
    match error_event {
        WhatsAppLinkEvent::Error {
            message,
            recoverable,
        } => {
            assert_eq!(message, "forced kill failure");
            assert!(!recoverable);
        }
        other => panic!("expected error event, got {other:?}"),
    }

    let status_event = timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("status event should arrive")
        .expect("broadcast should be open");
    match status_event {
        WhatsAppLinkEvent::Status(snapshot) => {
            assert_eq!(snapshot.state, "error");
            assert_eq!(snapshot.phone.as_deref(), Some("+123456789"));
            assert_eq!(snapshot.last_error.as_deref(), Some("forced kill failure"));
        }
        other => panic!("expected status event, got {other:?}"),
    }

    let disconnected = timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(
        disconnected.is_err(),
        "disconnected event should not be emitted on kill failure"
    );

    let snapshot = runtime.status_snapshot().await;
    assert_eq!(snapshot.state, "error");
    assert_eq!(snapshot.phone.as_deref(), Some("+123456789"));
    assert_eq!(snapshot.last_error.as_deref(), Some("forced kill failure"));

    let mut retained = {
        let mut inner = runtime.inner.lock().await;
        assert!(!inner.stopping, "runtime should clear stopping flag");
        inner
            .process
            .take()
            .expect("process handle should be retained after kill failure")
    };
    assert_eq!(
        retained
            .id()
            .expect("retained process should still have pid"),
        expected_pid
    );
    retained
        .kill()
        .await
        .expect("retained process should be killable during cleanup");
}

#[test]
fn sidecar_stderr_normalization_strips_gpu_noise_only_lines_and_keeps_actionable_errors() {
    let gpu_noise_only = "[1234:ERROR:gpu_process_host.cc(991)] GPU process launch failed\n";
    assert_eq!(normalize_sidecar_stderr(gpu_noise_only), None);

    let mixed = "[1234:ERROR:gpu_process_host.cc(991)] GPU process launch failed\nERR_REQUIRE_ESM: require() of ES Module not supported\n";
    assert_eq!(
        normalize_sidecar_stderr(mixed),
        Some("ERR_REQUIRE_ESM: require() of ES Module not supported".to_string())
    );
}

#[test]
fn sidecar_stderr_normalization_drops_sensitive_session_dump_lines() {
    let sensitive = "[wa-sidecar:info] Closing session: SessionEntry {\ncurrentRatchet: {\nprivKey: <Buffer 01 02>\n}\n";
    assert_eq!(normalize_sidecar_stderr(sensitive), None);

    let mixed = "[wa-sidecar:warn] Decrypted message with closed session.\ncurrentRatchet: {\n";
    assert_eq!(
        normalize_sidecar_stderr(mixed),
        Some("[wa-sidecar:warn] Decrypted message with closed session.".to_string())
    );

    let noisy = "registrationId: 769524623,\nbaseKey: <Buffer 05 aa>,\n}\n";
    assert_eq!(normalize_sidecar_stderr(noisy), None);
}

#[test]
fn sidecar_launcher_enforces_node_mode_and_esm_safe_bridge_startup_behavior() {
    let spec =
        build_sidecar_launch_spec("node", Path::new("frontend/electron/whatsapp-bridge.cjs"))
            .expect("launch spec should be generated");
    assert_eq!(spec.program, "node");
    assert_eq!(spec.args, vec!["frontend/electron/whatsapp-bridge.cjs"]);
    assert_eq!(spec.env.get("ELECTRON_RUN_AS_NODE"), Some(&"1".to_string()));
}

#[test]
fn sidecar_launcher_rejects_non_node_compatible_programs() {
    let err =
        build_sidecar_launch_spec("python", Path::new("frontend/electron/whatsapp-bridge.cjs"))
            .expect_err("non-node-compatible launchers must be rejected");
    assert!(
        err.to_string().contains("node-compatible"),
        "unexpected error: {err}"
    );
}

#[test]
fn sidecar_launcher_rejects_non_cjs_entrypoints() {
    let err = build_sidecar_launch_spec("node", Path::new("frontend/electron/whatsapp-bridge.mjs"))
        .expect_err("non-cjs bridge paths must be rejected");
    assert!(err.to_string().contains(".cjs"), "unexpected error: {err}");
}
