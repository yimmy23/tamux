use super::flow::{
    fetch_setup_model_options, select_provider, set_config_item, setup_model_select_items,
};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupCompactionChoice {
    Heuristic,
    Weles,
    CustomLlm,
}

#[derive(Debug, Clone, Default)]
struct SecondaryAgentSetup {
    summary: Option<String>,
    provider_id: Option<String>,
    provider_name: Option<String>,
    model: Option<String>,
    reasoning_effort: Option<String>,
}

pub(super) fn setup_agent_model_hint(label: &str) -> &'static str {
    match label.trim().to_ascii_lowercase().as_str() {
        "rarog" => "Rarog should stay light, cheap, and responsive.",
        "weles" => "Weles should stay strong enough for review and governance.",
        "custom compaction" | "custom llm" => {
            "Choose a model with a context window large enough to summarize old thread context."
        }
        _ => "Svarog is the main working fire. Prefer your strongest model.",
    }
}

pub(super) fn setup_agent_reasoning_hint(label: &str) -> &'static str {
    match label.trim().to_ascii_lowercase().as_str() {
        "rarog" => "Non-reasoning is fine for Rarog; add more only if it clearly helps.",
        "weles" => "Weles does not need to be your top model, but avoid weak review setups.",
        "custom compaction" | "custom llm" => {
            "Use enough reasoning to preserve decisions and constraints during summarization."
        }
        _ => "Svarog handles primary execution and longer reasoning chains.",
    }
}

pub(super) fn compaction_choice_items() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "Heuristic",
            "Fast rule-based compaction without an LLM call.",
        ),
        ("WELES", "Use WELES or its configured fallback model."),
        (
            "Custom LLM",
            "Choose a dedicated provider, model, and reasoning effort.",
        ),
    ]
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

pub(super) fn custom_llm_compaction_writes(
    provider_id: &str,
    base_url: &str,
    model: &str,
    api_key: Option<&str>,
    auth_source: &str,
    reasoning_effort: Option<&str>,
) -> Vec<ConfigWrite> {
    vec![
        ConfigWrite {
            key_path: "/auto_compact_context".to_string(),
            value_json: "true".to_string(),
        },
        ConfigWrite {
            key_path: "/compaction/strategy".to_string(),
            value_json: "\"custom_model\"".to_string(),
        },
        ConfigWrite {
            key_path: "/compaction/custom_model/provider".to_string(),
            value_json: serde_json::to_string(provider_id).unwrap_or_else(|_| "\"\"".into()),
        },
        ConfigWrite {
            key_path: "/compaction/custom_model/base_url".to_string(),
            value_json: serde_json::to_string(base_url).unwrap_or_else(|_| "\"\"".into()),
        },
        ConfigWrite {
            key_path: "/compaction/custom_model/model".to_string(),
            value_json: serde_json::to_string(model).unwrap_or_else(|_| "\"\"".into()),
        },
        ConfigWrite {
            key_path: "/compaction/custom_model/api_key".to_string(),
            value_json: serde_json::to_string(api_key.unwrap_or(""))
                .unwrap_or_else(|_| "\"\"".into()),
        },
        ConfigWrite {
            key_path: "/compaction/custom_model/auth_source".to_string(),
            value_json: serde_json::to_string(auth_source).unwrap_or_else(|_| "\"api_key\"".into()),
        },
        ConfigWrite {
            key_path: "/compaction/custom_model/reasoning_effort".to_string(),
            value_json: serde_json::to_string(reasoning_effort.unwrap_or("none"))
                .unwrap_or_else(|_| "\"none\"".into()),
        },
    ]
}

fn select_compaction_choice() -> Result<SetupCompactionChoice> {
    let idx = select_list(
        "Select auto-compaction strategy:",
        compaction_choice_items(),
        true,
        0,
    )?
    .unwrap_or(0);
    Ok(match idx {
        1 => SetupCompactionChoice::Weles,
        2 => SetupCompactionChoice::CustomLlm,
        _ => SetupCompactionChoice::Heuristic,
    })
}

pub(super) async fn configure_advanced_agents(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
    tier: &str,
    summary: &mut SetupSummary,
) -> Result<()> {
    if !tier_shows_step(tier, "advanced_agents") {
        return Ok(());
    }

    println!();
    println!("Advanced agent setup");
    println!("Configure Rarog and WELES separately, or keep their current defaults.");

    summary.concierge = configure_secondary_agent_override(framed, "Rarog", "/concierge", true)
        .await?
        .summary;
    let weles =
        configure_secondary_agent_override(framed, "WELES", "/builtin_sub_agents/weles", false)
            .await?;
    summary.weles = weles.summary.clone();
    summary.compaction = configure_compaction(framed, &weles).await?;

    Ok(())
}

async fn configure_secondary_agent_override(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
    label: &str,
    base_path: &str,
    none_uses_null: bool,
) -> Result<SecondaryAgentSetup> {
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
        return Ok(SecondaryAgentSetup {
            summary: Some("inherit defaults".to_string()),
            ..SecondaryAgentSetup::default()
        });
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
    let summary_line = format!(
        "{} / {} / {}",
        provider.provider_name,
        if model.is_empty() {
            "(default model)"
        } else {
            model.as_str()
        },
        effort.as_deref().unwrap_or("none")
    );
    Ok(SecondaryAgentSetup {
        summary: Some(summary_line),
        provider_id: Some(provider.provider_id),
        provider_name: Some(provider.provider_name),
        model: Some(model),
        reasoning_effort: effort,
    })
}

async fn configure_compaction(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
    weles: &SecondaryAgentSetup,
) -> Result<Option<String>> {
    println!();
    let choice = select_compaction_choice()?;
    match choice {
        SetupCompactionChoice::Heuristic => {
            for write in weles_compaction_writes(WelesCompactionChoice::NoUseDefault, "", "", None)
            {
                set_config_item(framed, write.key_path, write.value_json).await?;
            }
            Ok(Some("heuristic".to_string()))
        }
        SetupCompactionChoice::Weles => {
            let provider_id = weles.provider_id.as_deref().unwrap_or("");
            let model = weles.model.as_deref().unwrap_or("");
            let effort = weles.reasoning_effort.as_deref();
            for write in
                weles_compaction_writes(WelesCompactionChoice::Yes, provider_id, model, effort)
            {
                set_config_item(framed, write.key_path, write.value_json).await?;
            }
            let provider_label = weles.provider_name.as_deref().unwrap_or("WELES fallback");
            let model_label = if model.trim().is_empty() {
                "configured fallback"
            } else {
                model
            };
            Ok(Some(format!("{provider_label} / {model_label} / WELES")))
        }
        SetupCompactionChoice::CustomLlm => configure_custom_llm_compaction(framed).await,
    }
}

async fn configure_custom_llm_compaction(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
) -> Result<Option<String>> {
    println!("Custom LLM compaction");
    let provider = select_provider(framed).await?;
    let model = select_model_for_provider(framed, "custom compaction", &provider).await?;
    let api_key = if is_local_provider(&provider.provider_id) {
        String::new()
    } else {
        text_input(
            "Enter API key for custom compaction provider (press Enter to use existing main-provider or custom-auth credentials if available)",
            "",
            true,
        )?
        .unwrap_or_default()
    };
    let effort = select_reasoning_effort("custom compaction")?;
    for write in custom_llm_compaction_writes(
        &provider.provider_id,
        &provider.base_url,
        &model,
        Some(api_key.trim()),
        &provider.auth_source,
        effort.as_deref(),
    ) {
        set_config_item(framed, write.key_path, write.value_json).await?;
    }
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
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
    label: &str,
    base_path: &str,
    provider: &ProviderSelection,
) -> Result<String> {
    let model = select_model_for_provider(framed, label, provider).await?;
    set_secondary_model(framed, base_path, &model).await?;
    Ok(model)
}

async fn select_model_for_provider(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
    label: &str,
    provider: &ProviderSelection,
) -> Result<String> {
    let hint = setup_agent_model_hint(label);
    println!("Fetching available models...");
    println!("{hint}");

    match fetch_setup_model_options(framed, provider, "").await {
        Ok(models) if !models.is_empty() => {
            let default_index = models
                .iter()
                .position(|model| model.id == provider.default_model)
                .unwrap_or(0);
            let items = setup_model_select_items(&models);

            if let Some(idx) = select_rich_list(
                &format!("Select {label}'s model:\n{hint}"),
                &items,
                true,
                default_index,
            )? {
                let model = models[idx].id.clone();
                return Ok(model);
            }

            println!("Skipped -- using {label}'s provider default model.");
            return Ok(provider.default_model.clone());
        }
        Ok(_) => {
            println!("No models were returned for {}.", provider.provider_name);
        }
        Err(error) => {
            println!("Could not fetch models: {error}");
        }
    }

    select_model_text_fallback(label, provider)
}

fn select_model_text_fallback(label: &str, provider: &ProviderSelection) -> Result<String> {
    let hint = setup_agent_model_hint(label);
    let prompt = format!(
        "Enter {label}'s model ({hint}) or press Enter to use {}",
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

    Ok(model)
}

async fn set_secondary_model(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
    base_path: &str,
    model: &str,
) -> Result<()> {
    if model.is_empty() {
        set_config_item(framed, format!("{base_path}/model"), "null").await
    } else {
        set_config_item(
            framed,
            format!("{base_path}/model"),
            format!("\"{}\"", model),
        )
        .await
    }
}

async fn configure_secondary_reasoning_effort(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
    label: &str,
    base_path: &str,
    none_uses_null: bool,
) -> Result<Option<String>> {
    let effort = select_reasoning_effort(label)?;
    write_reasoning_effort(framed, base_path, effort.as_deref(), none_uses_null).await?;
    Ok(effort)
}

fn select_reasoning_effort(label: &str) -> Result<Option<String>> {
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
        Ok(None)
    } else {
        Ok(Some(selected.to_string()))
    }
}

async fn write_reasoning_effort(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
    base_path: &str,
    effort: Option<&str>,
    none_uses_null: bool,
) -> Result<()> {
    match effort {
        Some(value) => {
            set_config_item(
                framed,
                format!("{base_path}/reasoning_effort"),
                format!("\"{}\"", value),
            )
            .await
        }
        None if none_uses_null => {
            set_config_item(framed, format!("{base_path}/reasoning_effort"), "null").await
        }
        None => set_config_item(framed, format!("{base_path}/reasoning_effort"), "\"none\"").await,
    }
}
