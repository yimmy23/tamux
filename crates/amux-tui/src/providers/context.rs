use super::known_models_for_provider_auth;
#[cfg(test)]
use super::PROVIDERS;

#[cfg(test)]
pub fn is_known_default_url(url: &str) -> bool {
    PROVIDERS
        .iter()
        .any(|provider| provider.default_base_url == url)
}

pub fn known_context_window_for(provider: &str, model: &str) -> Option<u32> {
    known_models_for_provider_auth(provider, "api_key")
        .into_iter()
        .find(|entry| entry.id == model)
        .and_then(|entry| entry.context_window)
}
