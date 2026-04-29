use super::circuit_breaker::CircuitBreakerRegistry;
use super::*;
use std::time::Duration;
use zorai_shared::providers::{PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_OPENAI};

const AGENT_HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const AGENT_HTTP_READ_TIMEOUT: Duration = Duration::from_secs(125);

pub(in crate::agent) fn build_agent_http_client(read_timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(AGENT_HTTP_CONNECT_TIMEOUT)
        .read_timeout(read_timeout)
        .build()
        .expect("agent HTTP client configuration should be valid")
}

pub(in crate::agent) fn build_fresh_agent_http_client(read_timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(AGENT_HTTP_CONNECT_TIMEOUT)
        .read_timeout(read_timeout)
        .pool_max_idle_per_host(0)
        .build()
        .expect("fresh agent HTTP client configuration should be valid")
}

pub(in crate::agent) fn default_agent_http_read_timeout() -> Duration {
    AGENT_HTTP_READ_TIMEOUT
}

pub(in crate::agent) fn file_watch_event_is_relevant(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Any | EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

pub(crate) fn aline_available() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVAILABLE.get_or_init(|| which::which("aline").is_ok())
}

pub(in crate::agent) async fn provider_is_eligible_for_alternative(
    config: &Arc<RwLock<AgentConfig>>,
    circuit_breakers: &Arc<CircuitBreakerRegistry>,
    failed_provider: &str,
    provider_id: &str,
) -> bool {
    if provider_id == failed_provider {
        return false;
    }

    let config_guard = config.read().await;
    let Ok(resolved) = resolve_candidate_provider_config(&config_guard, provider_id) else {
        return false;
    };
    drop(config_guard);

    if resolved.model.trim().is_empty() || resolved.base_url.trim().is_empty() {
        return false;
    }

    match resolved.auth_source {
        AuthSource::ApiKey => {
            if resolved.api_key.trim().is_empty() {
                return false;
            }
        }
        AuthSource::ChatgptSubscription => {
            if provider_id != PROVIDER_ID_OPENAI
                || !super::llm_client::has_openai_chatgpt_subscription_auth()
            {
                return false;
            }
        }
        AuthSource::GithubCopilot => {
            if provider_id != PROVIDER_ID_GITHUB_COPILOT
                || !super::copilot_auth::github_copilot_has_available_models(
                    &resolved.api_key,
                    resolved.auth_source,
                )
            {
                return false;
            }
        }
    }

    let breaker_arc = circuit_breakers.get(provider_id).await;
    let mut breaker = breaker_arc.lock().await;
    breaker.can_execute(now_millis())
}

async fn collect_provider_alternatives(
    config: &Arc<RwLock<AgentConfig>>,
    circuit_breakers: &Arc<CircuitBreakerRegistry>,
    failed_provider: &str,
) -> Vec<ProviderAlternativeSuggestion> {
    let provider_ids: Vec<String> = config.read().await.providers.keys().cloned().collect();
    let mut alternatives = Vec::new();

    for provider_id in provider_ids {
        if !provider_is_eligible_for_alternative(
            config,
            circuit_breakers,
            failed_provider,
            provider_id.as_str(),
        )
        .await
        {
            continue;
        }

        let config_guard = config.read().await;
        let Ok(resolved) = resolve_candidate_provider_config(&config_guard, &provider_id) else {
            continue;
        };

        alternatives.push(ProviderAlternativeSuggestion {
            provider_id,
            model: Some(resolved.model),
            reason: "configured and healthy".to_string(),
        });
    }

    alternatives
}

pub(in crate::agent) async fn collect_provider_outage_metadata(
    config: &Arc<RwLock<AgentConfig>>,
    circuit_breakers: &Arc<CircuitBreakerRegistry>,
    failed_provider: &str,
    trip_count: u32,
    reason: impl Into<String>,
) -> ProviderCircuitOpenDetails {
    let failed_model = {
        let config_guard = config.read().await;
        resolve_candidate_provider_config(&config_guard, failed_provider)
            .ok()
            .and_then(|resolved| (!resolved.model.trim().is_empty()).then_some(resolved.model))
    };

    ProviderCircuitOpenDetails {
        provider: failed_provider.to_string(),
        failed_model,
        trip_count,
        reason: reason.into(),
        suggested_alternatives: collect_provider_alternatives(
            config,
            circuit_breakers,
            failed_provider,
        )
        .await,
    }
}

pub(in crate::agent) async fn collect_provider_health_snapshot(
    config: &Arc<RwLock<AgentConfig>>,
    circuit_breakers: &Arc<CircuitBreakerRegistry>,
) -> Vec<ProviderHealthSnapshot> {
    let provider_ids: Vec<String> = config.read().await.providers.keys().cloned().collect();
    let mut snapshots = Vec::new();

    for provider_id in provider_ids {
        let breaker_arc = circuit_breakers.get(&provider_id).await;
        let mut breaker = breaker_arc.lock().await;
        let can_execute = breaker.can_execute(now_millis());
        let trip_count = breaker.trip_count();
        drop(breaker);

        if can_execute {
            snapshots.push(ProviderHealthSnapshot {
                provider_id,
                can_execute,
                trip_count,
                failed_model: None,
                reason: None,
                suggested_alternatives: Vec::new(),
            });
            continue;
        }

        let outage = collect_provider_outage_metadata(
            config,
            circuit_breakers,
            &provider_id,
            trip_count,
            "circuit breaker open",
        )
        .await;
        snapshots.push(ProviderHealthSnapshot {
            provider_id: outage.provider,
            can_execute,
            trip_count: outage.trip_count,
            failed_model: outage.failed_model,
            reason: Some(outage.reason),
            suggested_alternatives: outage.suggested_alternatives,
        });
    }

    snapshots
}

pub(in crate::agent) fn format_provider_outage_message(
    outage: &ProviderCircuitOpenDetails,
) -> Option<String> {
    if outage.suggested_alternatives.is_empty() {
        return None;
    }

    let alternatives = outage
        .suggested_alternatives
        .iter()
        .map(|alt| match &alt.model {
            Some(model) => format!("{} ({})", alt.provider_id, model),
            None => alt.provider_id.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ");

    let model = outage
        .failed_model
        .as_ref()
        .map(|m| format!(" model '{}'", m))
        .unwrap_or_default();

    Some(format!(
        "Provider '{}'{} is temporarily unavailable ({}). Alternatives: {}.",
        outage.provider, model, outage.reason, alternatives
    ))
}
