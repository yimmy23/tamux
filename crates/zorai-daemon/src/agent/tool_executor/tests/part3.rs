    #[tokio::test]
    async fn fetch_url_runtime_uses_default_timeout_on_caller_path() {
        let observed_timeout = Arc::new(Mutex::new(None));
        let observed_timeout_clone = observed_timeout.clone();

        let result = execute_fetch_url_with_runner(
            &serde_json::json!({ "url": "https://example.com" }),
            true,
            move |_url, timeout_seconds| {
                let observed_timeout = observed_timeout_clone.clone();
                async move {
                    *observed_timeout
                        .lock()
                        .expect("timeout lock should succeed") = Some(timeout_seconds);
                    Ok::<String, anyhow::Error>("<html><body>hello</body></html>".to_string())
                }
            },
            |_url, _timeout_seconds| async move {
                Ok::<String, anyhow::Error>("<html><body>http</body></html>".to_string())
            },
        )
        .await
        .expect("fetch_url should succeed");

        assert_eq!(
            *observed_timeout
                .lock()
                .expect("timeout lock should succeed"),
            Some(300)
        );
        assert_eq!(result, "hello");
    }

    #[tokio::test]
    async fn fetch_url_runtime_clamps_timeout_override_on_caller_path() {
        let observed_browser_timeout = Arc::new(Mutex::new(None));
        let browser_timeout_clone = observed_browser_timeout.clone();
        let observed_http_timeout = Arc::new(Mutex::new(None));
        let http_timeout_clone = observed_http_timeout.clone();

        execute_fetch_url_with_runner(
            &serde_json::json!({ "url": "https://example.com", "timeout_seconds": 999 }),
            true,
            move |_url, timeout_seconds| {
                let observed_browser_timeout = browser_timeout_clone.clone();
                async move {
                    *observed_browser_timeout
                        .lock()
                        .expect("timeout lock should succeed") = Some(timeout_seconds);
                    Err::<String, anyhow::Error>(anyhow::anyhow!("browser unavailable"))
                }
            },
            move |_url, timeout_seconds| {
                let observed_http_timeout = http_timeout_clone.clone();
                async move {
                    *observed_http_timeout
                        .lock()
                        .expect("timeout lock should succeed") = Some(timeout_seconds);
                    Ok::<String, anyhow::Error>("<html><body>fallback</body></html>".to_string())
                }
            },
        )
        .await
        .expect("fetch_url should succeed");

        assert_eq!(
            *observed_browser_timeout
                .lock()
                .expect("timeout lock should succeed"),
            Some(600)
        );
        assert_eq!(
            *observed_http_timeout
                .lock()
                .expect("timeout lock should succeed"),
            Some(600)
        );
    }

    #[tokio::test]
    async fn fetch_url_runtime_falls_back_to_http_after_browser_failure() {
        let browser_attempted = Arc::new(Mutex::new(false));
        let browser_attempted_clone = browser_attempted.clone();
        let http_attempted = Arc::new(Mutex::new(false));
        let http_attempted_clone = http_attempted.clone();

        let result = execute_fetch_url_with_runner(
            &serde_json::json!({ "url": "https://example.com" }),
            true,
            move |_url, _timeout_seconds| {
                let browser_attempted = browser_attempted_clone.clone();
                async move {
                    *browser_attempted.lock().expect("lock should succeed") = true;
                    Err::<String, anyhow::Error>(anyhow::anyhow!("browser failed"))
                }
            },
            move |_url, _timeout_seconds| {
                let http_attempted = http_attempted_clone.clone();
                async move {
                    *http_attempted.lock().expect("lock should succeed") = true;
                    Ok::<String, anyhow::Error>(
                        "<html><body>fallback content</body></html>".to_string(),
                    )
                }
            },
        )
        .await
        .expect("fetch_url should fall back to http");

        assert!(*browser_attempted.lock().expect("lock should succeed"));
        assert!(*http_attempted.lock().expect("lock should succeed"));
        assert_eq!(result, "fallback content");
    }

    #[tokio::test]
    async fn fetch_url_runtime_does_not_fallback_after_browser_timeout_exhausts_budget() {
        let http_attempted = Arc::new(Mutex::new(false));
        let http_attempted_clone = http_attempted.clone();
        let started = std::time::Instant::now();

        let error = execute_fetch_url_with_runner(
            &serde_json::json!({ "url": "https://example.com", "timeout_seconds": 1 }),
            true,
            |_url, timeout_seconds| async move {
                tokio::time::sleep(Duration::from_millis((timeout_seconds * 1000) + 50)).await;
                Ok::<String, anyhow::Error>("<html><body>late browser</body></html>".to_string())
            },
            move |_url, _timeout_seconds| {
                let http_attempted = http_attempted_clone.clone();
                async move {
                    *http_attempted.lock().expect("lock should succeed") = true;
                    Ok::<String, anyhow::Error>("<html><body>fallback</body></html>".to_string())
                }
            },
        )
        .await
        .expect_err("browser timeout should consume overall budget");

        assert!(error.to_string().contains("fetch_url timed out"));
        assert!(!*http_attempted.lock().expect("lock should succeed"));
        assert!(
            started.elapsed() < Duration::from_millis(1500),
            "overall timeout should not allow a fresh fallback budget"
        );
    }

    #[tokio::test]
    async fn fetch_url_runtime_uses_remaining_budget_for_http_fallback_after_browser_failure() {
        let started = std::time::Instant::now();

        let error = execute_fetch_url_with_runner(
            &serde_json::json!({ "url": "https://example.com", "timeout_seconds": 1 }),
            true,
            |_url, _timeout_seconds| async move {
                tokio::time::sleep(Duration::from_millis(700)).await;
                Err::<String, anyhow::Error>(anyhow::anyhow!("browser failed after delay"))
            },
            |_url, _timeout_seconds| async move {
                tokio::time::sleep(Duration::from_millis(500)).await;
                Ok::<String, anyhow::Error>("<html><body>late fallback</body></html>".to_string())
            },
        )
        .await
        .expect_err("http fallback should only get remaining budget");

        assert!(error.to_string().contains("fetch_url timed out"));
        assert!(
            started.elapsed() < Duration::from_millis(1300),
            "fallback should not receive a fresh full timeout budget"
        );
    }

    #[tokio::test]
    async fn fetch_url_runtime_does_not_fallback_on_browser_timeout_error() {
        let http_attempted = Arc::new(Mutex::new(false));
        let http_attempted_clone = http_attempted.clone();

        let error = execute_fetch_url_with_runner(
            &serde_json::json!({ "url": "https://example.com", "timeout_seconds": 1 }),
            true,
            |_url, _timeout_seconds| async move {
                Err::<String, anyhow::Error>(anyhow::anyhow!(
                    "headless browser fetch timed out after 1 seconds"
                ))
            },
            move |_url, _timeout_seconds| {
                let http_attempted = http_attempted_clone.clone();
                async move {
                    *http_attempted.lock().expect("lock should succeed") = true;
                    Ok::<String, anyhow::Error>("<html><body>fallback</body></html>".to_string())
                }
            },
        )
        .await
        .expect_err("browser timeout error should not fall back to http");

        assert!(error
            .to_string()
            .contains("fetch_url timed out after 1 seconds"));
        assert!(!*http_attempted.lock().expect("lock should succeed"));
    }

    #[tokio::test]
    async fn fetch_url_runtime_returns_timeout_error_when_runner_exceeds_limit() {
        let error = execute_fetch_url_with_runner(
            &serde_json::json!({ "url": "https://example.com", "timeout_seconds": 0 }),
            false,
            |_url, _timeout_seconds| async move {
                Ok::<String, anyhow::Error>("<html><body>browser</body></html>".to_string())
            },
            |_url, _timeout_seconds| async move {
                tokio::time::sleep(Duration::from_millis(10)).await;
                Ok::<String, anyhow::Error>("<html><body>late</body></html>".to_string())
            },
        )
        .await
        .expect_err("runner exceeding timeout should return timeout error");

        assert!(error.to_string().contains("fetch_url timed out"));
        assert!(error.to_string().contains("0"));
    }

    #[tokio::test]
    async fn search_files_runtime_clamps_timeout_override_on_caller_path() {
        let observed_timeout = Arc::new(Mutex::new(None));
        let observed_timeout_clone = observed_timeout.clone();

        let result = execute_search_files_with_runner(
            &serde_json::json!({ "pattern": "needle", "timeout_seconds": 999 }),
            move |request| {
                let observed_timeout = observed_timeout_clone.clone();
                async move {
                    *observed_timeout
                        .lock()
                        .expect("timeout lock should succeed") = Some(request.timeout_seconds);
                    Ok::<super::SearchFilesCommandOutput, anyhow::Error>(
                        super::SearchFilesCommandOutput {
                            status: successful_exit_status(),
                            stdout: b"file.rs:1:needle\n".to_vec(),
                            stderr: Vec::new(),
                            truncated: false,
                        },
                    )
                }
            },
        )
        .await
        .expect("search_files should succeed");

        assert_eq!(
            *observed_timeout
                .lock()
                .expect("timeout lock should succeed"),
            Some(600)
        );
        assert_eq!(result, "file.rs:1:needle");
    }

    #[tokio::test]
    async fn search_files_runtime_returns_timeout_error_when_runner_exceeds_limit() {
        let error = execute_search_files_with_runner(
            &serde_json::json!({ "pattern": "needle", "timeout_seconds": 0 }),
            |_| async move {
                tokio::time::sleep(Duration::from_millis(10)).await;
                Ok::<super::SearchFilesCommandOutput, anyhow::Error>(
                    super::SearchFilesCommandOutput {
                        status: successful_exit_status(),
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                        truncated: false,
                    },
                )
            },
        )
        .await
        .expect_err("runner exceeding timeout should return timeout error");

        assert!(error.to_string().contains("search timed out"));
        assert!(error.to_string().contains("0"));
    }

