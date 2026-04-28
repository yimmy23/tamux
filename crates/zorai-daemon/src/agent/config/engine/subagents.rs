use super::super::*;

impl AgentEngine {
    pub async fn set_sub_agent(&self, def: SubAgentDefinition) -> Result<()> {
        if is_attempted_weles_builtin_edit(&def) || is_weles_builtin_update_shape(&def) {
            {
                let mut config = self.config.write().await;
                apply_weles_allowed_overrides(&mut config, &def)?;
            }
            self.persist_config().await;
            return Ok(());
        }

        if is_reserved_builtin_sub_agent_id(&def.id) {
            return Err(builtin_collision_error("id", &def.id));
        }
        if is_reserved_builtin_sub_agent_name(&def.name) {
            return Err(builtin_collision_error("name", &def.name));
        }

        let mut config = self.config.write().await;
        if let Some(existing) = config.sub_agents.iter_mut().find(|s| s.id == def.id) {
            *existing = def;
        } else {
            config.sub_agents.push(def);
        }
        drop(config);
        self.persist_config().await;
        Ok(())
    }

    /// Remove a sub-agent definition by id.
    pub async fn remove_sub_agent(&self, id: &str) -> Result<bool> {
        if is_reserved_builtin_sub_agent_id(id) {
            return Err(protected_mutation_error("cannot remove daemon-owned WELES"));
        }
        let mut config = self.config.write().await;
        let before = config.sub_agents.len();
        config.sub_agents.retain(|s| s.id != id);
        let removed = config.sub_agents.len() < before;
        drop(config);
        if removed {
            self.persist_config().await;
        }
        Ok(removed)
    }

    /// List all sub-agent definitions.
    pub async fn list_sub_agents(&self) -> Vec<SubAgentDefinition> {
        self.effective_sub_agents().await
    }

    /// Get the concierge configuration.
    pub async fn get_concierge_config(&self) -> ConciergeConfig {
        self.config.read().await.concierge.clone()
    }

    /// Update the concierge configuration and persist.
    pub async fn set_concierge_config(&self, concierge: ConciergeConfig) {
        self.config.write().await.concierge = concierge;
        self.persist_config().await;
    }
}
