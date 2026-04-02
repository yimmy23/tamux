use super::flow::{select_provider, set_config_item};
use super::*;

const REASONING_EFFORT_ITEMS: [(&str, &str); 6] = [
    ("None", "none"),
    ("Minimal", "minimal"),
    ("Low", "low"),
    ("Medium", "medium"),
    ("High", "high"),
    ("Extra High", "xhigh"),
];

pub(super) async fn configure_advanced_agents(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
    tier: &str,
    summary: &mut SetupSummary,
) -> Result<()> {
    if !tier_shows_step(tier, "advanced_agents") {
        return Ok(());
    }

    println!();
    println!("Advanced agent setup");
    println!("Configure Rarog and WELES separately, or keep their current defaults.");

    summary.concierge =
        configure_secondary_agent_override(framed, "Rarog", "/concierge", true).await?;
    summary.weles =
        configure_secondary_agent_override(framed, "WELES", "/builtin_sub_agents/weles", false)
            .await?;

    Ok(())
}

async fn configure_secondary_agent_override(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
    label: &str,
    base_path: &str,
    none_uses_null: bool,
) -> Result<Option<String>> {
    println!();
    let customize = matches!(
        select_list(
            &format!("Configure {label} separately?"),
            &[
                ("No, keep current defaults", ""),
                ("Yes, choose provider, model, and reasoning effort", ""),
            ],
            true,
            0,
        )?,
        Some(1)
    );

    if !customize {
        set_config_item(framed, format!("{base_path}/provider"), "null").await?;
        set_config_item(framed, format!("{base_path}/model"), "null").await?;
        set_config_item(framed, format!("{base_path}/reasoning_effort"), "null").await?;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        return Ok(Some("inherit defaults".to_string()));
    }

    let provider = select_provider(framed).await?;
    set_config_item(
        framed,
        format!("{base_path}/provider"),
        format!("\"{}\"", provider.provider_id),
    )
    .await?;

    let model = configure_secondary_model(framed, label, base_path, &provider).await?;
    let effort =
        configure_secondary_reasoning_effort(framed, label, base_path, none_uses_null).await?;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    Ok(Some(format!(
        "{} / {} / {}",
        provider.provider_name,
        if model.is_empty() {
            "(default model)"
        } else {
            model.as_str()
        },
        effort.as_deref().unwrap_or("none")
    )))
}

async fn configure_secondary_model(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
    label: &str,
    base_path: &str,
    provider: &ProviderSelection,
) -> Result<String> {
    let prompt = format!(
        "Enter {label}'s model (press Enter to use {})",
        if provider.default_model.is_empty() {
            "the provider default"
        } else {
            provider.default_model.as_str()
        }
    );
    let chosen = text_input(&prompt, &provider.default_model, false)?.unwrap_or_default();
    let model = if chosen.trim().is_empty() {
        provider.default_model.clone()
    } else {
        chosen.trim().to_string()
    };

    if model.is_empty() {
        set_config_item(framed, format!("{base_path}/model"), "null").await?;
    } else {
        set_config_item(
            framed,
            format!("{base_path}/model"),
            format!("\"{}\"", model),
        )
        .await?;
    }

    Ok(model)
}

async fn configure_secondary_reasoning_effort(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
    label: &str,
    base_path: &str,
    none_uses_null: bool,
) -> Result<Option<String>> {
    let idx = select_list(
        &format!("Select {label}'s reasoning effort:"),
        &REASONING_EFFORT_ITEMS,
        true,
        0,
    )?
    .unwrap_or(0);
    let selected = REASONING_EFFORT_ITEMS[idx].1;

    if selected == "none" {
        if none_uses_null {
            set_config_item(framed, format!("{base_path}/reasoning_effort"), "null").await?;
            Ok(None)
        } else {
            set_config_item(framed, format!("{base_path}/reasoning_effort"), "\"none\"").await?;
            Ok(Some("none".to_string()))
        }
    } else {
        set_config_item(
            framed,
            format!("{base_path}/reasoning_effort"),
            format!("\"{}\"", selected),
        )
        .await?;
        Ok(Some(selected.to_string()))
    }
}
