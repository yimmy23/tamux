//! Agent configuration get/set.

use super::*;

impl AgentEngine {
    pub async fn get_config(&self) -> AgentConfig {
        self.config.read().await.clone()
    }

    pub async fn set_config(&self, config: AgentConfig) {
        *self.config.write().await = config;
        self.persist_config().await;
    }

    /// Build provider auth states by merging persisted config with PROVIDER_DEFINITIONS.
    pub async fn get_provider_auth_states(&self) -> Vec<ProviderAuthState> {
        use crate::agent::types::{ProviderAuthState, PROVIDER_DEFINITIONS};

        let config = self.config.read().await;
        let mut states = Vec::new();

        for def in PROVIDER_DEFINITIONS {
            let (authenticated, auth_source, model, base_url) =
                if let Some(pc) = config.providers.get(def.id) {
                    (
                        !pc.api_key.is_empty(),
                        pc.auth_source,
                        pc.model.clone(),
                        pc.base_url.clone(),
                    )
                } else if config.provider == def.id {
                    // Fall back to top-level config if this is the active provider.
                    (
                        !config.api_key.is_empty(),
                        config.auth_source,
                        config.model.clone(),
                        config.base_url.clone(),
                    )
                } else {
                    (
                        false,
                        AuthSource::default(),
                        def.default_model.to_string(),
                        def.default_base_url.to_string(),
                    )
                };

            states.push(ProviderAuthState {
                provider_id: def.id.to_string(),
                provider_name: def.name.to_string(),
                authenticated,
                auth_source,
                model,
                base_url,
            });
        }

        states
    }

    /// Upsert a sub-agent definition (matched by id).
    pub async fn set_sub_agent(&self, def: SubAgentDefinition) {
        let mut config = self.config.write().await;
        if let Some(existing) = config.sub_agents.iter_mut().find(|s| s.id == def.id) {
            *existing = def;
        } else {
            config.sub_agents.push(def);
        }
        drop(config);
        self.persist_config().await;
    }

    /// Remove a sub-agent definition by id.
    pub async fn remove_sub_agent(&self, id: &str) -> bool {
        let mut config = self.config.write().await;
        let before = config.sub_agents.len();
        config.sub_agents.retain(|s| s.id != id);
        let removed = config.sub_agents.len() < before;
        drop(config);
        if removed {
            self.persist_config().await;
        }
        removed
    }

    /// List all sub-agent definitions.
    pub async fn list_sub_agents(&self) -> Vec<SubAgentDefinition> {
        self.config.read().await.sub_agents.clone()
    }
}
