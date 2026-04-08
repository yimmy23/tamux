use super::*;

impl<'a> SendMessageRunner<'a> {
    pub(super) async fn maybe_rebuild_prompt_after_memory_flush(&mut self) -> Result<()> {
        if !self
            .engine
            .maybe_run_pre_compaction_memory_flush(
                &self.tid,
                self.task_id,
                &self.config,
                &self.provider_config,
                &self.system_prompt,
                self.preferred_session_id,
                self.retry_strategy,
                &mut self.last_pre_compaction_flush_signature,
            )
            .await?
        {
            return Ok(());
        }

        let memory = self.engine.current_memory_snapshot().await;
        let causal_guidance = self.engine.build_causal_guidance_summary().await;
        let sub_agents = self.engine.list_sub_agents().await;
        self.system_prompt = build_system_prompt(
            &self.config,
            &self.base_prompt,
            &memory,
            &self.memory_paths,
            &self.agent_scope_id,
            &sub_agents,
            self.operator_model_summary.as_deref(),
            self.operational_context.as_deref(),
            causal_guidance.as_deref(),
            self.learned_patterns.as_deref(),
            None,
            self.continuity_summary.as_deref(),
            self.negative_constraints_context.as_deref(),
        );
        self.system_prompt.push_str("\n\n");
        self.system_prompt.push_str(&build_runtime_identity_prompt(
            &self.runtime_agent_name,
            &self.active_provider_id,
            &self.provider_config.model,
        ));
        if let Some(recall) = self.onecontext_bootstrap.as_deref() {
            self.system_prompt.push_str("\n\n## OneContext Recall\n");
            self.system_prompt
                .push_str("Use this as historical context from prior sessions when relevant:\n");
            self.system_prompt.push_str(recall);
        }
        if let Some(skill_preflight) = self.skill_preflight.as_ref() {
            self.system_prompt.push_str("\n\n## Preloaded Skills\n");
            self.system_prompt.push_str(&skill_preflight.prompt_context);
        }
        Ok(())
    }
}
