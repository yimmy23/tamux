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
            let provider = args
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or("lightpanda");

            if matches!(provider, "chrome" | "chromium") {
                anyhow::bail!(
                    "Chrome/Chromium auto-install is not supported by setup_web_browsing. \
                     Install a headless Chrome/Chromium binary manually and ensure it is on PATH."
                );
            }
            if !matches!(provider, "lightpanda" | "auto") {
                anyhow::bail!(
                    "Invalid install provider: '{}'. Supported install providers are lightpanda, auto, chrome, or chromium.",
                    provider
                );
            }

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
                anyhow::bail!(
                    "npm install failed (exit {}):\n{}\n{}",
                    output.status,
                    stdout.chars().take(500).collect::<String>(),
                    stderr.chars().take(500).collect::<String>(),
                );
            }

            // Verify
            let installed = detect_lightpanda().is_some();
            if !installed {
                anyhow::bail!(
                    "npm install completed but Lightpanda is still unavailable on PATH.\n{}",
                    stdout.chars().take(300).collect::<String>()
                );
            }
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

            // Verify the chosen provider works
            let works = match provider {
                "lightpanda" => detect_lightpanda().is_some(),
                "chrome" => detect_chrome().is_some(),
                "auto" => detect_lightpanda().or_else(detect_chrome).is_some(),
                _ => true, // "none" always works
            };

            if !works && provider == "chrome" {
                anyhow::bail!(
                    "browse_provider '{}' could not be configured because Chrome/Chromium was not found on PATH.",
                    provider
                );
            }

            // Write to config after validation so a missing browser does not persist a broken choice.
            {
                let mut config = agent.config.write().await;
                config.extra.insert(
                    "browse_provider".to_string(),
                    serde_json::Value::String(provider.to_string()),
                );
            }

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
