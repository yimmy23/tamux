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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WelesCompactionChoice {
    Yes,
    NoUseDefault,
}

pub(super) fn setup_agent_model_hint(label: &str) -> &'static str {
    match label.trim().to_ascii_lowercase().as_str() {
        "rarog" => "Rarog should stay light, cheap, and responsive.",
        "weles" => "Weles should stay strong enough for review and governance.",
        _ => "Svarog is the main working fire. Prefer your strongest model.",
    }
}

pub(super) fn setup_agent_reasoning_hint(label: &str) -> &'static str {
    match label.trim().to_ascii_lowercase().as_str() {
        "rarog" => "Non-reasoning is fine for Rarog; add more only if it clearly helps.",
        "weles" => "Weles does not need to be your top model, but avoid weak review setups.",
        _ => "Svarog handles primary execution and longer reasoning chains.",
    }
}

pub(super) fn weles_compaction_writes(
    choice: WelesCompactionChoice,
    provider_id: &str,
    model: &str,
    reasoning_effort: Option<&str>,
) -> Vec<ConfigWrite> {
    match choice {
        WelesCompactionChoice::NoUseDefault => vec![
            ConfigWrite {
                key_path: "/auto_compact_context".to_string(),
                value_json: "true".to_string(),
            },
            ConfigWrite {
                key_path: "/compaction/strategy".to_string(),
                value_json: "\"heuristic\"".to_string(),
            },
        ],
        WelesCompactionChoice::Yes => vec![
            ConfigWrite {
                key_path: "/auto_compact_context".to_string(),
                value_json: "true".to_string(),
            },
            ConfigWrite {
                key_path: "/compaction/strategy".to_string(),
                value_json: "\"weles\"".to_string(),
            },
            ConfigWrite {
                key_path: "/compaction/weles/provider".to_string(),
                value_json: serde_json::to_string(provider_id).unwrap_or_else(|_| "\"\"".into()),
            },
            ConfigWrite {
                key_path: "/compaction/weles/model".to_string(),
                value_json: serde_json::to_string(model).unwrap_or_else(|_| "\"\"".into()),
            },
            ConfigWrite {
                key_path: "/compaction/weles/reasoning_effort".to_string(),
                value_json: serde_json::to_string(reasoning_effort.unwrap_or("none"))
                    .unwrap_or_else(|_| "\"none\"".into()),
            },
        ],
    }
}

fn should_offer_weles_compaction(label: &str, model: &str) -> bool {
    label.trim().eq_ignore_ascii_case("weles") && !model.trim().is_empty()
}

fn select_weles_compaction_choice() -> Result<WelesCompactionChoice> {
    let items = [
        (
            "Yes",
            "Enable auto compaction and use this WELES provider/model.",
        ),
        (
            "No, use default",
            "Enable auto compaction with the default heuristic strategy.",
        ),
    ];
    let idx =
        select_list("Use this WELES model for auto-compaction?", &items, true, 0)?.unwrap_or(0);

    Ok(match idx {
        0 => WelesCompactionChoice::Yes,
        _ => WelesCompactionChoice::NoUseDefault,
    })
}

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
                (
                    "Yes, choose provider, model, and reasoning effort",
                    setup_agent_model_hint(label),
                ),
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
    let compaction_choice = if should_offer_weles_compaction(label, &model) {
        Some(select_weles_compaction_choice()?)
    } else {
        None
    };
    let effort =
        configure_secondary_reasoning_effort(framed, label, base_path, none_uses_null).await?;
    if let Some(choice) = compaction_choice {
        for write in
            weles_compaction_writes(choice, &provider.provider_id, &model, effort.as_deref())
        {
            set_config_item(framed, write.key_path, write.value_json).await?;
        }
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let mut summary_line = format!(
        "{} / {} / {}",
        provider.provider_name,
        if model.is_empty() {
            "(default model)"
        } else {
            model.as_str()
        },
        effort.as_deref().unwrap_or("none")
    );
    if let Some(choice) = compaction_choice {
        let suffix = match choice {
            WelesCompactionChoice::Yes => "  [auto-compaction: WELES]",
            WelesCompactionChoice::NoUseDefault => "  [auto-compaction: heuristic]",
        };
        summary_line.push_str(suffix);
    }
    Ok(Some(summary_line))
}

async fn configure_secondary_model(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
    label: &str,
    base_path: &str,
    provider: &ProviderSelection,
) -> Result<String> {
    println!("{}", setup_agent_model_hint(label));
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
        &format!(
            "Select {label}'s reasoning effort:\n{}",
            setup_agent_reasoning_hint(label)
        ),
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
