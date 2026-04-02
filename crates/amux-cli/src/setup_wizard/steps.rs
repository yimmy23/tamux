use super::flow::set_config_item;
use super::*;

pub(super) async fn configure_web_search(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
    summary: &mut SetupSummary,
) -> Result<()> {
    println!();
    let existing_ws = if read_config_key("firecrawl_api_key").await.is_some() {
        Some("Firecrawl")
    } else if read_config_key("exa_api_key").await.is_some() {
        Some("Exa")
    } else if read_config_key("tavily_api_key").await.is_some() {
        Some("Tavily")
    } else {
        None
    };

    let should_configure = if let Some(provider) = existing_ws {
        println!("Web search is already configured ({provider}).");
        match select_list(
            "Replace web search configuration?",
            &[("No, keep existing", ""), ("Yes, reconfigure", "")],
            true,
            0,
        )? {
            Some(1) => true,
            _ => {
                summary.web_search = Some(provider.to_string());
                false
            }
        }
    } else {
        true
    };

    if should_configure {
        let items = [
            ("Firecrawl", "firecrawl_api_key"),
            ("Exa", "exa_api_key"),
            ("Tavily", "tavily_api_key"),
            ("Skip", ""),
        ];
        match select_list(
            "Configure web search? (enables agent web browsing)",
            &items,
            true,
            0,
        )? {
            Some(idx) if idx < 3 => {
                let (provider_label, key_name) = items[idx];
                if let Some(key) = text_input(&format!("Enter {provider_label} API key"), "", true)?
                {
                    if !key.is_empty() {
                        set_config_item(framed, "/tools/web_search", "true")
                            .await
                            .context("Failed to enable web search")?;
                        set_config_item(framed, format!("/{key_name}"), format!("\"{}\"", key))
                            .await
                            .context("Failed to set web search API key")?;
                        summary.web_search = Some(provider_label.to_string());
                        println!("Web search configured with {provider_label}.");
                    } else {
                        println!("Skipped -- you can add web search later with `tamux setup`.");
                    }
                } else {
                    println!("Skipped -- you can add web search later with `tamux setup`.");
                }
            }
            _ => println!("Skipped -- you can add web search later with `tamux setup`."),
        }
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    Ok(())
}

pub(super) async fn configure_gateway(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
    summary: &mut SetupSummary,
) -> Result<()> {
    println!();
    let existing_gw = if read_config_key("gateway.slack_token").await.is_some() {
        Some("Slack")
    } else if read_config_key("gateway.discord_token").await.is_some() {
        Some("Discord")
    } else if read_config_key("gateway.telegram_token").await.is_some() {
        Some("Telegram")
    } else {
        None
    };

    let should_configure = if let Some(platform) = existing_gw {
        println!("Gateway is already configured ({platform}).");
        match select_list(
            "Replace gateway configuration?",
            &[("No, keep existing", ""), ("Yes, reconfigure", "")],
            true,
            0,
        )? {
            Some(1) => true,
            _ => {
                summary.gateway = Some(platform.to_string());
                false
            }
        }
    } else {
        true
    };

    if !should_configure {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        return Ok(());
    }

    let gateway_items = gateway_choice_items();
    match select_list(
        "Configure a chat gateway? (Slack, Discord, Telegram, WhatsApp)",
        &gateway_items,
        true,
        0,
    )? {
        Some(idx) if idx + 1 < gateway_items.len() => {
            let (platform_label, platform) = gateway_items[idx];
            if platform == "whatsapp" {
                let raw = loop {
                    println!();
                    match resolve_whatsapp_allowlist_prompt(collect_whatsapp_allowlist_input()?) {
                        WhatsAppAllowlistPromptResolution::Accept(raw) => break Some(raw),
                        WhatsAppAllowlistPromptResolution::Retry(message) => println!("{message}"),
                        WhatsAppAllowlistPromptResolution::Cancel => {
                            println!(
                                "Skipped -- you can configure WhatsApp later with `tamux setup`."
                            );
                            break None;
                        }
                    }
                };
                if let Some(raw) = raw {
                    for write in whatsapp_gateway_config_writes(&raw)? {
                        set_config_item(framed, write.key_path, write.value_json)
                            .await
                            .context("Failed to save WhatsApp gateway setup")?;
                    }
                    let linked = run_whatsapp_link_subflow(framed)
                        .await
                        .context("WhatsApp link flow failed during setup")?;
                    summary.gateway = Some(platform_label.to_string());
                    summary.whatsapp_linked = linked;
                    if linked {
                        println!("WhatsApp gateway linked.");
                    } else {
                        println!("WhatsApp gateway selected (link skipped).");
                    }
                }
            } else if let Some(token) =
                text_input(&format!("Enter {platform_label} token"), "", true)?
            {
                if !token.is_empty() {
                    set_config_item(framed, "/gateway/enabled", "true")
                        .await
                        .context("Failed to enable gateway")?;
                    set_config_item(
                        framed,
                        format!("/gateway/{}_token", platform),
                        format!("\"{}\"", token),
                    )
                    .await
                    .context("Failed to set gateway token")?;
                    summary.gateway = Some(platform_label.to_string());
                    println!("{platform_label} gateway configured.");
                } else {
                    println!("Skipped -- you can configure gateways later with `tamux setup`.");
                }
            } else {
                println!("Skipped -- you can configure gateways later with `tamux setup`.");
            }
        }
        _ => println!("Skipped -- you can configure gateways later with `tamux setup`."),
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    Ok(())
}

pub(super) async fn print_summary_and_choose_action(
    provider_name: &str,
    security_level_label: &str,
    summary: &SetupSummary,
) -> Result<PostSetupAction> {
    println!();
    println!("{}", "Setup complete!".bold());
    println!();
    println!("  Provider:  {provider_name}");
    println!("  Security:  {security_level_label}");
    if let Some(ref model) = summary.model {
        println!("  Model:     {model}");
    }
    if let Some(ref ws) = summary.web_search {
        println!("  Web Search: Enabled ({ws})");
    }
    if let Some(ref gw) = summary.gateway {
        if gw == "WhatsApp" {
            if summary.whatsapp_linked {
                println!("  Gateway:   WhatsApp linked");
            } else {
                println!("  Gateway:   WhatsApp selected (link skipped)");
            }
        } else {
            println!("  Gateway:   {gw} configured");
        }
    }
    if let Some(ref concierge) = summary.concierge {
        println!("  Rarog:    {concierge}");
    }
    if let Some(ref weles) = summary.weles {
        println!("  WELES:    {weles}");
    }
    println!();
    let launch_items = post_setup_choices();
    let launch_idx = select_list("What would you like to run now?", &launch_items, false, 0)?
        .expect("post-setup selection is required");
    Ok(post_setup_action_from_index(launch_idx))
}
