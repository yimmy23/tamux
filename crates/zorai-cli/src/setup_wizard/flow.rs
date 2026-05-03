use super::*;

const SETUP_PROBE_TIMEOUT_SECS: u64 = 5;

pub(super) fn parse_fetch_models_terminal_response(msg: DaemonMessage) -> Option<Result<String>> {
    match msg {
        DaemonMessage::OperationAccepted { .. } => None,
        DaemonMessage::AgentModelsResponse { models_json, .. } => Some(Ok(models_json)),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            Some(Err(anyhow::anyhow!(message)))
        }
        _ => None,
    }
}

pub(super) fn parse_set_config_item_response(msg: DaemonMessage) -> Option<Result<()>> {
    match msg {
        DaemonMessage::OperationAccepted { .. } | DaemonMessage::AgentConfigResponse { .. } => {
            Some(Ok(()))
        }
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            Some(Err(anyhow::anyhow!(message)))
        }
        _ => None,
    }
}

pub(super) fn parse_provider_login_terminal_response(
    msg: DaemonMessage,
) -> Option<Result<Vec<ProviderAuthState>>> {
    match msg {
        DaemonMessage::OperationAccepted { .. } => None,
        DaemonMessage::AgentProviderAuthStates { states_json } => Some(
            serde_json::from_str(&states_json)
                .context("Failed to parse provider auth states from daemon"),
        ),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            Some(Err(anyhow::anyhow!(message)))
        }
        _ => None,
    }
}

pub(super) fn parse_gh_cli_token_output(stdout: &[u8]) -> Option<String> {
    let token = String::from_utf8_lossy(stdout).trim().to_string();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

fn parse_price_rate(raw: Option<&str>) -> Option<f64> {
    raw?.trim().parse::<f64>().ok()
}

fn format_price_per_million(raw: Option<&str>) -> Option<String> {
    let per_token = parse_price_rate(raw)?;
    Some(format!("${:.2}/M tok", per_token * 1_000_000.0))
}

pub(super) fn format_remote_model_pricing_subtitle(model: &RemoteModelOption) -> Option<String> {
    let pricing = model.pricing.as_ref()?;
    let prompt_rate = parse_price_rate(pricing.prompt.as_deref());
    let completion_rate = parse_price_rate(pricing.completion.as_deref());

    if matches!(prompt_rate, Some(rate) if rate == 0.0)
        && matches!(completion_rate, Some(rate) if rate == 0.0)
    {
        return Some("free".to_string());
    }

    let mut parts = Vec::new();
    if let Some(formatted) = format_price_per_million(pricing.prompt.as_deref()) {
        parts.push(format!("Prompt {formatted}"));
    }
    if let Some(formatted) = format_price_per_million(pricing.completion.as_deref()) {
        parts.push(format!("completion {formatted}"));
    }

    if parts.is_empty() {
        return None;
    }

    Some(parts.join(", "))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AuthSetupResult {
    pub auth_source: String,
    pub api_key_for_requests: String,
    pub authenticated: bool,
}

pub(super) fn provider_uses_browser_auth(provider: &ProviderSelection) -> bool {
    provider.provider_id == "github-copilot" && provider.auth_source == "github_copilot"
}

pub(super) fn should_validate_provider_after_setup(
    provider: &ProviderSelection,
    auth_result: AuthSetupResult,
) -> bool {
    !is_local_provider(&provider.provider_id)
        && (auth_result.authenticated || !auth_result.api_key_for_requests.is_empty())
}

async fn recv_fetch_models_terminal_response(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
) -> Result<String> {
    loop {
        let msg = wizard_recv(framed).await?;
        if let Some(result) = parse_fetch_models_terminal_response(msg) {
            return result;
        }
    }
}

pub(super) async fn set_config_item(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
    key_path: impl Into<String>,
    value_json: impl Into<String>,
) -> Result<()> {
    wizard_send(
        framed,
        ClientMessage::AgentSetConfigItem {
            key_path: key_path.into(),
            value_json: value_json.into(),
        },
    )
    .await?;

    loop {
        let msg = wizard_recv(framed).await?;
        if let Some(result) = parse_set_config_item_response(msg) {
            return result;
        }
    }
}

async fn store_github_copilot_auth_token_on_stream(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
    access_token: &str,
    source: &str,
) -> Result<Vec<ProviderAuthState>> {
    wizard_send(
        framed,
        ClientMessage::AgentStoreGithubCopilotAuthToken {
            access_token: access_token.to_string(),
            source: source.to_string(),
        },
    )
    .await?;

    loop {
        let msg = wizard_recv(framed).await?;
        if let Some(result) = parse_provider_login_terminal_response(msg) {
            return result;
        }
    }
}

fn run_github_copilot_gh_login() -> Result<()> {
    let status = match std::process::Command::new("gh")
        .args(["auth", "login", "--web", "--scopes", "read:org,models:read"])
        .status()
    {
        Ok(status) => status,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("GitHub CLI (`gh`) is not installed or not available in this shell");
        }
        Err(error) => {
            return Err(error).context("failed to start GitHub CLI login flow");
        }
    };
    if !status.success() {
        anyhow::bail!("GitHub CLI login flow failed");
    }
    Ok(())
}

fn read_github_copilot_gh_token() -> Result<String> {
    let output = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .context(
            "failed to read token from GitHub CLI. Make sure `gh auth status` works in this shell",
        )?;
    if !output.status.success() {
        anyhow::bail!("GitHub CLI did not return an authenticated token");
    }
    parse_gh_cli_token_output(&output.stdout)
        .ok_or_else(|| anyhow::anyhow!("GitHub CLI returned an empty token"))
}

pub(super) async fn select_provider(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
) -> Result<ProviderSelection> {
    wizard_send(framed, ClientMessage::AgentGetProviderAuthStates).await?;
    let providers: Vec<ProviderAuthState> = match wizard_recv(framed).await? {
        DaemonMessage::AgentProviderAuthStates { states_json } => {
            serde_json::from_str(&states_json)
                .context("Failed to parse provider auth states from daemon")?
        }
        other => anyhow::bail!("Unexpected daemon response: {other:?}"),
    };

    if providers.is_empty() {
        anyhow::bail!("Daemon returned empty provider list. Is the daemon running correctly?");
    }

    let provider_items: Vec<(&str, &str)> = providers
        .iter()
        .map(|p| (p.provider_name.as_str(), p.provider_id.as_str()))
        .collect();
    let provider_idx = select_list("Select your LLM provider:", &provider_items, false, 0)?
        .expect("provider selection is required");
    let selected = &providers[provider_idx];

    Ok(ProviderSelection {
        provider_id: selected.provider_id.clone(),
        provider_name: selected.provider_name.clone(),
        base_url: selected.base_url.clone(),
        default_model: selected.model.clone(),
        auth_source: selected.auth_source.clone(),
    })
}

async fn configure_provider_auth(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
    provider: &ProviderSelection,
    selected_provider: &ProviderAuthState,
) -> Result<AuthSetupResult> {
    if is_local_provider(&provider.provider_id) {
        println!("Local provider -- no API key needed.");
        return Ok(AuthSetupResult {
            auth_source: provider.auth_source.clone(),
            api_key_for_requests: String::new(),
            authenticated: true,
        });
    }

    if provider_uses_browser_auth(provider) {
        let choice_idx = if selected_provider.authenticated {
            println!(
                "{} is already authenticated.",
                provider.provider_name.clone().bold()
            );
            select_list(
                "How do you want to continue?",
                &[
                    ("Keep existing browser login", ""),
                    ("Sign in again in browser", ""),
                    ("Use a token instead", ""),
                ],
                false,
                0,
            )?
            .unwrap_or(0)
        } else {
            select_list(
                "Authenticate GitHub Copilot:",
                &[
                    ("Sign in with GitHub in browser", ""),
                    ("Use a token instead", ""),
                ],
                false,
                0,
            )?
            .unwrap_or(0)
        };

        let wants_browser_auth = if selected_provider.authenticated {
            match choice_idx {
                0 => {
                    println!("Keeping existing browser login.");
                    return Ok(AuthSetupResult {
                        auth_source: "github_copilot".to_string(),
                        api_key_for_requests: String::new(),
                        authenticated: true,
                    });
                }
                1 => true,
                _ => false,
            }
        } else {
            choice_idx == 0
        };

        if wants_browser_auth {
            println!("Starting GitHub browser login...");
            match run_github_copilot_gh_login() {
                Ok(()) => match read_github_copilot_gh_token() {
                    Ok(token) => {
                        match store_github_copilot_auth_token_on_stream(framed, &token, "gh_cli")
                            .await
                        {
                            Ok(states) => {
                                let updated_state = states
                                    .into_iter()
                                    .find(|state| state.provider_id == provider.provider_id)
                                    .ok_or_else(|| {
                                        anyhow::anyhow!(
                                            "selected provider disappeared from daemon state"
                                        )
                                    })?;
                                if !updated_state.authenticated {
                                    println!(
                                        "GitHub Copilot browser login did not complete successfully."
                                    );
                                    println!("Falling back to token entry.");
                                } else {
                                    println!("GitHub Copilot browser login completed.");
                                    return Ok(AuthSetupResult {
                                        auth_source: "github_copilot".to_string(),
                                        api_key_for_requests: String::new(),
                                        authenticated: true,
                                    });
                                }
                            }
                            Err(error) => {
                                println!("Could not import GitHub Copilot browser login: {error}");
                                println!("Falling back to token entry.");
                            }
                        }
                    }
                    Err(error) => {
                        println!("Could not read GitHub CLI token: {error}");
                        println!("Falling back to token entry.");
                    }
                },
                Err(error) => {
                    println!("GitHub browser login unavailable: {error}");
                    println!("Falling back to token entry.");
                }
            }
        }
    }

    let has_existing_api_key_auth =
        selected_provider.authenticated && selected_provider.auth_source == "api_key";

    if has_existing_api_key_auth {
        println!(
            "{} is already authenticated.",
            provider.provider_name.clone().bold()
        );
        let replace_idx = select_list(
            "Replace API key?",
            &[("No, keep existing key", ""), ("Yes, enter a new key", "")],
            false,
            0,
        )?
        .unwrap_or(0);
        if replace_idx == 0 {
            println!("Keeping existing API key.");
            return Ok(AuthSetupResult {
                auth_source: "api_key".to_string(),
                api_key_for_requests: "(existing)".to_string(),
                authenticated: true,
            });
        }
    }

    let prompt = if has_existing_api_key_auth {
        format!("Enter new API key for {}", provider.provider_name)
    } else {
        format!("Enter API key for {}", provider.provider_name)
    };
    let api_key = text_input(&prompt, "", true)?.unwrap_or_default();
    if api_key.is_empty() {
        println!("No API key entered. You can set it later with `zorai setup`.");
        return Ok(AuthSetupResult {
            auth_source: "api_key".to_string(),
            api_key_for_requests: String::new(),
            authenticated: selected_provider.authenticated,
        });
    }

    let writes = if has_existing_api_key_auth {
        vec![
            (
                "provider",
                serde_json::to_string(&provider.provider_id).unwrap_or_default(),
            ),
            (
                "api_key",
                serde_json::to_string(&api_key).unwrap_or_default(),
            ),
        ]
    } else {
        vec![
            (
                "provider",
                serde_json::to_string(&provider.provider_id).unwrap_or_default(),
            ),
            (
                "api_key",
                serde_json::to_string(&api_key).unwrap_or_default(),
            ),
            (
                "base_url",
                serde_json::to_string(&provider.base_url).unwrap_or_default(),
            ),
            (
                "model",
                serde_json::to_string(&provider.default_model).unwrap_or_default(),
            ),
        ]
    };

    for (key, val) in writes {
        set_config_item(framed, format!("/{key}"), val)
            .await
            .with_context(|| format!("Failed to set {key}"))?;
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    println!(
        "{}",
        if has_existing_api_key_auth {
            "API key updated."
        } else {
            "API key saved."
        }
    );
    Ok(AuthSetupResult {
        auth_source: "api_key".to_string(),
        api_key_for_requests: api_key,
        authenticated: true,
    })
}

async fn fetch_selected_provider_state(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
    provider_id: &str,
) -> Result<ProviderAuthState> {
    wizard_send(framed, ClientMessage::AgentGetProviderAuthStates).await?;
    let providers: Vec<ProviderAuthState> = match wizard_recv(framed).await? {
        DaemonMessage::AgentProviderAuthStates { states_json } => {
            serde_json::from_str(&states_json)
                .context("Failed to parse provider auth states from daemon")?
        }
        other => anyhow::bail!("Unexpected daemon response: {other:?}"),
    };
    providers
        .into_iter()
        .find(|p| p.provider_id == provider_id)
        .ok_or_else(|| anyhow::anyhow!("selected provider disappeared from daemon state"))
}

async fn configure_model(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
    provider: &ProviderSelection,
    api_key_saved: &str,
    tier_string: &str,
    summary: &mut SetupSummary,
) -> Result<()> {
    println!();
    let existing_model = read_config_key("model").await;
    let user_wants_replace = if let Some(ref model) = existing_model {
        println!(
            "Svarog's model is already configured ({}).",
            model.clone().bold()
        );
        match select_list(
            "Replace model?",
            &[
                ("No, keep existing model", ""),
                ("Yes, choose a new model", ""),
            ],
            false,
            0,
        )? {
            Some(1) => true,
            _ => {
                println!("Keeping existing model.");
                summary.model = Some(model.clone());
                false
            }
        }
    } else {
        false
    };

    let show_model_picker =
        user_wants_replace || (existing_model.is_none() && tier_shows_step(tier_string, "model"));

    if show_model_picker {
        let svarog_model_hint = super::agents::setup_agent_model_hint("Svarog");
        println!("Fetching available models...");
        println!("{svarog_model_hint}");
        wizard_send(
            framed,
            ClientMessage::AgentFetchModels {
                provider_id: provider.provider_id.clone(),
                base_url: provider.base_url.clone(),
                api_key: api_key_saved.to_string(),
                output_modalities: None,
            },
        )
        .await
        .context("Failed to fetch models")?;

        match recv_fetch_models_terminal_response(framed).await {
            Ok(models_json) => {
                let models: Vec<RemoteModelOption> = serde_json::from_str(&models_json)
                    .or_else(|_| {
                        serde_json::from_str::<Vec<String>>(&models_json).map(|ids| {
                            ids.into_iter()
                                .map(|id| RemoteModelOption {
                                    id,
                                    name: None,
                                    context_window: None,
                                    pricing: None,
                                    metadata: None,
                                })
                                .collect()
                        })
                    })
                    .unwrap_or_default();
                if !models.is_empty() {
                    let items: Vec<RichSelectItem> = models
                        .iter()
                        .map(|model| RichSelectItem {
                            label: model.id.clone(),
                            detail: model
                                .name
                                .as_ref()
                                .filter(|name| name.as_str() != model.id)
                                .cloned(),
                            subtitle: format_remote_model_pricing_subtitle(model),
                        })
                        .collect();
                    if let Some(idx) = select_rich_list(
                        &format!("Select default Svarog's model:\n{}", svarog_model_hint),
                        &items,
                        true,
                        0,
                    )? {
                        let chosen = &models[idx].id;
                        set_config_item(framed, "/model", format!("\"{}\"", chosen))
                            .await
                            .context("Failed to set Svarog's model")?;
                        summary.model = Some(chosen.clone());
                    } else {
                        println!("Skipped -- using default Svarog's model.");
                    }
                } else {
                    let fallback = if provider.default_model.is_empty() {
                        ""
                    } else {
                        &provider.default_model
                    };
                    if let Some(m) = text_input(
                        &format!("Enter Svarog's model ({svarog_model_hint}) or Esc to skip"),
                        fallback,
                        false,
                    )? {
                        if !m.is_empty() {
                            set_config_item(framed, "/model", format!("\"{}\"", m))
                                .await
                                .context("Failed to set Svarog's model")?;
                            summary.model = Some(m);
                        } else {
                            println!("Skipped -- using default Svarog's model.");
                        }
                    } else {
                        println!("Skipped -- using default Svarog's model.");
                    }
                }
            }
            Err(error) => {
                let message = error.to_string();
                println!("Could not fetch models: {message}");
                let fallback = if provider.default_model.is_empty() {
                    ""
                } else {
                    &provider.default_model
                };
                if let Some(m) = text_input(
                    &format!("Enter Svarog's model ({svarog_model_hint}) or Esc to skip"),
                    fallback,
                    false,
                )? {
                    if !m.is_empty() {
                        set_config_item(framed, "/model", format!("\"{}\"", m))
                            .await
                            .context("Failed to set Svarog's model")?;
                        summary.model = Some(m);
                    } else {
                        println!("Skipped -- using default Svarog's model.");
                    }
                } else {
                    println!("Skipped -- using default Svarog's model.");
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    } else if existing_model.is_none() && !provider.default_model.is_empty() {
        set_config_item(framed, "/model", format!("\"{}\"", provider.default_model))
            .await
            .context("Failed to set default Svarog's model")?;
        summary.model = Some(provider.default_model.clone());
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetupProbe {
    Ready,
    NeedsSetup,
    DaemonUnavailable,
}

pub(crate) fn setup_probe_from_config_json(config_json: &str) -> SetupProbe {
    let value: serde_json::Value = match serde_json::from_str(config_json) {
        Ok(v) => v,
        Err(_) => return SetupProbe::NeedsSetup,
    };

    let has_provider =
        matches!(value.get("provider").and_then(|v| v.as_str()), Some(s) if !s.trim().is_empty());
    let has_model =
        matches!(value.get("model").and_then(|v| v.as_str()), Some(s) if !s.trim().is_empty());

    if has_provider && has_model {
        SetupProbe::Ready
    } else {
        SetupProbe::NeedsSetup
    }
}

pub(crate) async fn probe_setup_via_ipc() -> SetupProbe {
    let mut framed = match wizard_connect().await {
        Ok(f) => f,
        Err(_) => return SetupProbe::DaemonUnavailable,
    };
    if wizard_send(&mut framed, ClientMessage::AgentGetConfig)
        .await
        .is_err()
    {
        return SetupProbe::DaemonUnavailable;
    }
    match tokio::time::timeout(
        std::time::Duration::from_secs(SETUP_PROBE_TIMEOUT_SECS),
        wizard_recv(&mut framed),
    )
    .await
    {
        Ok(Ok(DaemonMessage::AgentConfigResponse { config_json })) => {
            setup_probe_from_config_json(&config_json)
        }
        _ => SetupProbe::DaemonUnavailable,
    }
}

pub async fn run_setup_wizard() -> Result<PostSetupAction> {
    ensure_daemon_running().await?;
    let mut framed = wizard_connect()
        .await
        .context("Failed to connect to daemon for setup")?;

    println!();
    println!("{}", "Svarog, Rarog, Weles -- Agents That Live".bold());
    println!("First-time setup");
    println!();

    let tier_items = [
        ("Just getting started", "newcomer"),
        ("I've used chatbots and assistants", "familiar"),
        ("I run automations and scripting", "power_user"),
        ("I build agent systems", "expert"),
    ];
    let tier_idx = select_list(
        "How familiar are you with AI agents?",
        &tier_items,
        false,
        0,
    )?
    .expect("tier selection is required");
    let tier_string = tier_items[tier_idx].1.to_string();
    wizard_send(
        &mut framed,
        ClientMessage::AgentSetTierOverride {
            tier: Some(tier_string.clone()),
        },
    )
    .await
    .context("Failed to set tier override")?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    println!();

    let provider = select_provider(&mut framed).await?;
    let selected_provider =
        fetch_selected_provider_state(&mut framed, &provider.provider_id).await?;
    println!();
    let auth_result = configure_provider_auth(&mut framed, &provider, &selected_provider).await?;
    println!();

    set_config_item(
        &mut framed,
        "/provider",
        format!("\"{}\"", provider.provider_id),
    )
    .await
    .context("Failed to set active provider")?;
    set_config_item(
        &mut framed,
        "/auth_source",
        format!("\"{}\"", auth_result.auth_source),
    )
    .await
    .context("Failed to set provider auth source")?;

    let mut summary = SetupSummary::default();
    configure_model(
        &mut framed,
        &provider,
        &auth_result.api_key_for_requests,
        &tier_string,
        &mut summary,
    )
    .await?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    configure_advanced_agents(&mut framed, &tier_string, &mut summary).await?;

    if should_validate_provider_after_setup(&provider, auth_result.clone()) {
        println!();
        println!("Testing connection to {}...", provider.provider_name);
        match validate_provider_on_stream(
            &mut framed,
            &provider.provider_id,
            &auth_result.auth_source,
        )
        .await
        {
            Ok(true) => println!("{}", "Connection successful!".with(style::Color::Green)),
            Ok(false) => {
                println!("Provider validation returned invalid. You can retry with `zorai setup`.");
            }
            Err(e) => {
                println!("Could not test connection: {e}");
                println!("You can retry later with `zorai setup`.");
            }
        }
    }

    println!();
    let security_items = [
        ("Ask for risky actions only (strict)", "highest"),
        ("Ask for risky actions only", "moderate"),
        ("Ask for destructive actions only", "lowest"),
        ("I trust it, minimize interruptions", "yolo"),
    ];
    let security_idx = select_list(
        "How cautious should zorai be?",
        &security_items,
        false,
        default_security_index(&tier_string),
    )?
    .expect("security selection is required");
    let (security_level_str, security_level_label) = security_level_from_index(security_idx);
    set_config_item(
        &mut framed,
        "/managed_execution/security_level",
        format!("\"{}\"", security_level_str),
    )
    .await
    .context("Failed to set security level")?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    configure_web_search(&mut framed, &mut summary).await?;
    configure_gateway(&mut framed, &mut summary).await?;

    if tier_shows_step(&tier_string, "data_dir") {
        println!();
        let data_dir = zorai_protocol::zorai_data_dir();
        println!("Data stored at: {}", data_dir.display());
    }

    print_summary_and_choose_action(&provider.provider_name, security_level_label, &summary).await
}
