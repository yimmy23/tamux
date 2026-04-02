async fn execute_setup_web_browsing(
    args: &serde_json::Value,
    agent: &super::engine::AgentEngine,
) -> Result<String> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("detect");

    match action {
        "detect" => {
            let mut report = Vec::new();
            if let Some(b) = detect_lightpanda() {
                report.push(format!("lightpanda: FOUND at {}", b.bin));
            } else {
                report.push("lightpanda: not found".to_string());
            }
            if let Some(b) = detect_chrome() {
                report.push(format!("chrome/chromium: FOUND at {}", b.bin));
            } else {
                report.push("chrome/chromium: not found".to_string());
            }
            // Check npm availability for install
            let npm_available = which::which("npm").is_ok();
            report.push(format!(
                "npm: {}",
                if npm_available {
                    "available (can install Lightpanda)"
                } else {
                    "not found (cannot auto-install Lightpanda)"
                }
            ));
            // Current config
            let config = agent.config.read().await;
            let current = config
                .extra
                .get("browse_provider")
                .and_then(|v| v.as_str())
                .unwrap_or("auto");
            report.push(format!("current browse_provider: {}", current));
            drop(config);

            Ok(report.join("\n"))
        }
        "install" => {
            // Install Lightpanda via npm
            if detect_lightpanda().is_some() {
                return Ok("Lightpanda is already installed.".to_string());
            }
            if !which::which("npm").is_ok() {
                anyhow::bail!(
                    "npm is not available on PATH. Install Node.js/npm first, \
                     or install Lightpanda manually."
                );
            }

            let output = tokio::process::Command::new("npm")
                .args(["install", "-g", "@nicholasgasior/lightpanda"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true)
                .output()
                .await?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if !output.status.success() {
                return Ok(format!(
                    "npm install failed (exit {}):\n{}\n{}",
                    output.status,
                    stdout.chars().take(500).collect::<String>(),
                    stderr.chars().take(500).collect::<String>(),
                ));
            }

            // Verify
            let installed = detect_lightpanda().is_some();
            Ok(format!(
                "npm install completed.\nLightpanda available: {}{}",
                installed,
                if !stdout.is_empty() {
                    format!("\n{}", stdout.chars().take(300).collect::<String>())
                } else {
                    String::new()
                }
            ))
        }
        "configure" => {
            let provider = args
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or("auto");

            // Validate the provider value
            if !matches!(provider, "auto" | "lightpanda" | "chrome" | "none") {
                anyhow::bail!(
                    "Invalid browse_provider: '{}'. Must be auto, lightpanda, chrome, or none.",
                    provider
                );
            }

            // Write to config
            {
                let mut config = agent.config.write().await;
                config.extra.insert(
                    "browse_provider".to_string(),
                    serde_json::Value::String(provider.to_string()),
                );
            }

            // Verify the chosen provider works
            let works = match provider {
                "lightpanda" => detect_lightpanda().is_some(),
                "chrome" => detect_chrome().is_some(),
                "auto" => detect_lightpanda().or_else(detect_chrome).is_some(),
                _ => true, // "none" always works
            };

            Ok(format!(
                "browse_provider set to '{}'.\nBrowser available: {}{}",
                provider,
                works,
                if !works && provider != "none" {
                    "\nWarning: chosen browser not found on PATH. fetch_url will fall back to raw HTTP."
                } else {
                    ""
                }
            ))
        }
        _ => anyhow::bail!(
            "Unknown action '{}'. Use detect, install, or configure.",
            action
        ),
    }
}

// ---------------------------------------------------------------------------
// Terminal/session tools — daemon owns sessions directly
// ---------------------------------------------------------------------------

