impl DaemonClient {
    pub fn request_agent_status(&self) -> Result<()> {
        self.send(ClientMessage::AgentStatusQuery)
    }

    pub fn request_agent_statistics(
        &self,
        window: amux_protocol::AgentStatisticsWindow,
    ) -> Result<()> {
        self.send(ClientMessage::AgentStatisticsQuery { window })
    }

    pub fn request_prompt_inspection(&self, agent_id: Option<String>) -> Result<()> {
        self.send(ClientMessage::AgentInspectPrompt { agent_id })
    }

    pub fn request_file_preview(
        &self,
        path: impl Into<String>,
        max_bytes: Option<usize>,
    ) -> Result<()> {
        self.send(ClientMessage::GetFilePreview {
            path: path.into(),
            max_bytes,
        })
    }

    pub fn send_message(
        &self,
        thread_id: Option<String>,
        content: String,
        session_id: Option<String>,
        target_agent_id: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSendMessage {
            thread_id,
            content,
            session_id,
            context_messages_json: None,
            client_surface: Some(amux_protocol::ClientSurface::Tui),
            target_agent_id,
        })
    }

    pub fn stop_stream(&self, thread_id: String) -> Result<()> {
        self.send(ClientMessage::AgentStopStream { thread_id })
    }

    pub fn force_compact(&self, thread_id: String) -> Result<()> {
        self.send(ClientMessage::AgentForceCompact { thread_id })
    }

    pub fn send_internal_delegate(
        &self,
        thread_id: Option<String>,
        target_agent_id: String,
        content: String,
        session_id: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentInternalDelegate {
            thread_id,
            target_agent_id,
            content,
            session_id,
            client_surface: Some(amux_protocol::ClientSurface::Tui),
        })
    }

    pub fn send_thread_participant_command(
        &self,
        thread_id: String,
        target_agent_id: String,
        action: String,
        instruction: Option<String>,
        session_id: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentThreadParticipantCommand {
            thread_id,
            target_agent_id,
            action,
            instruction,
            session_id,
            client_surface: Some(amux_protocol::ClientSurface::Tui),
        })
    }

    pub fn send_participant_suggestion(
        &self,
        thread_id: String,
        suggestion_id: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSendParticipantSuggestion {
            thread_id,
            suggestion_id,
            session_id: None,
            client_surface: Some(amux_protocol::ClientSurface::Tui),
        })
    }

    pub fn dismiss_participant_suggestion(
        &self,
        thread_id: String,
        suggestion_id: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentDismissParticipantSuggestion {
            thread_id,
            suggestion_id,
            session_id: None,
            client_surface: Some(amux_protocol::ClientSurface::Tui),
        })
    }

    pub fn retry_stream_now(&self, thread_id: String) -> Result<()> {
        self.send(ClientMessage::AgentRetryStreamNow { thread_id })
    }

    pub fn delete_messages(&self, thread_id: String, message_ids: Vec<String>) -> Result<()> {
        self.send(ClientMessage::DeleteAgentMessages {
            thread_id,
            message_ids,
        })
    }

    pub fn pin_thread_message_for_compaction(
        &self,
        thread_id: String,
        message_id: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentPinThreadMessageForCompaction {
            thread_id,
            message_id,
        })
    }

    pub fn unpin_thread_message_for_compaction(
        &self,
        thread_id: String,
        message_id: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentUnpinThreadMessageForCompaction {
            thread_id,
            message_id,
        })
    }

    pub fn spawn_session(
        &self,
        shell: Option<String>,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
    ) -> Result<()> {
        self.send(ClientMessage::SpawnSession {
            shell,
            cwd,
            env: None,
            workspace_id: None,
            cols,
            rows,
        })
    }

    pub fn control_goal_run(&self, goal_run_id: String, action: String) -> Result<()> {
        self.send(ClientMessage::AgentControlGoalRun {
            goal_run_id,
            action,
            step_index: None,
        })
    }

    pub fn fetch_models(
        &self,
        provider_id: String,
        base_url: String,
        api_key: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentFetchModels {
            provider_id,
            base_url,
            api_key,
        })
    }

    pub fn set_config_item_json(&self, key_path: String, value_json: String) -> Result<()> {
        self.send(ClientMessage::AgentSetConfigItem {
            key_path,
            value_json,
        })
    }

    pub fn set_provider_model(&self, provider_id: String, model: String) -> Result<()> {
        self.send(ClientMessage::AgentSetProviderModel { provider_id, model })
    }

    pub fn set_target_agent_provider_model(
        &self,
        target_agent_id: String,
        provider_id: String,
        model: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSetTargetAgentProviderModel {
            target_agent_id,
            provider_id,
            model,
        })
    }

    pub fn get_provider_auth_states(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetProviderAuthStates)
    }

    pub fn get_openai_codex_auth_status(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetOpenAICodexAuthStatus)
    }

    pub fn login_openai_codex(&self) -> Result<()> {
        self.send(ClientMessage::AgentLoginOpenAICodex)
    }

    pub fn logout_openai_codex(&self) -> Result<()> {
        self.send(ClientMessage::AgentLogoutOpenAICodex)
    }

    pub fn validate_provider(
        &self,
        provider_id: String,
        base_url: String,
        api_key: String,
        auth_source: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentValidateProvider {
            provider_id,
            base_url,
            api_key,
            auth_source,
        })
    }

    pub fn set_sub_agent(&self, sub_agent_json: String) -> Result<()> {
        self.send(ClientMessage::AgentSetSubAgent { sub_agent_json })
    }

    pub fn remove_sub_agent(&self, sub_agent_id: String) -> Result<()> {
        self.send(ClientMessage::AgentRemoveSubAgent { sub_agent_id })
    }

    pub fn list_sub_agents(&self) -> Result<()> {
        self.send(ClientMessage::AgentListSubAgents)
    }

    pub fn get_concierge_config(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetConciergeConfig)
    }

    pub fn set_concierge_config(&self, config_json: String) -> Result<()> {
        self.send(ClientMessage::AgentSetConciergeConfig { config_json })
    }

    pub fn request_concierge_welcome(&self) -> Result<()> {
        self.send(ClientMessage::AgentRequestConciergeWelcome)
    }

    pub fn list_checkpoints(&self, goal_run_id: String) -> Result<()> {
        self.send(ClientMessage::AgentListCheckpoints { goal_run_id })
    }

    pub fn dismiss_concierge_welcome(&self) -> Result<()> {
        self.send(ClientMessage::AgentDismissConciergeWelcome)
    }

    pub fn record_attention(
        &self,
        surface: String,
        thread_id: Option<String>,
        goal_run_id: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentRecordAttention {
            surface,
            thread_id,
            goal_run_id,
        })
    }

    pub fn start_operator_profile_session(&self, kind: String) -> Result<()> {
        self.send(ClientMessage::AgentStartOperatorProfileSession { kind })
    }

    pub fn next_operator_profile_question(&self, session_id: String) -> Result<()> {
        self.send(ClientMessage::AgentNextOperatorProfileQuestion { session_id })
    }

    pub fn submit_operator_profile_answer(
        &self,
        session_id: String,
        question_id: String,
        answer_json: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSubmitOperatorProfileAnswer {
            session_id,
            question_id,
            answer_json,
        })
    }

    pub fn skip_operator_profile_question(
        &self,
        session_id: String,
        question_id: String,
        reason: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSkipOperatorProfileQuestion {
            session_id,
            question_id,
            reason,
        })
    }

    pub fn defer_operator_profile_question(
        &self,
        session_id: String,
        question_id: String,
        defer_until_unix_ms: Option<u64>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentDeferOperatorProfileQuestion {
            session_id,
            question_id,
            defer_until_unix_ms,
        })
    }

    pub fn get_operator_profile_summary(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetOperatorProfileSummary)
    }

    pub fn answer_operator_question(&self, question_id: String, answer: String) -> Result<()> {
        self.send(ClientMessage::AgentAnswerQuestion {
            question_id,
            answer,
        })
    }

    pub fn get_operator_model(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetOperatorModel)
    }

    pub fn reset_operator_model(&self) -> Result<()> {
        self.send(ClientMessage::AgentResetOperatorModel)
    }

    pub fn get_collaboration_sessions(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetCollaborationSessions {
            parent_task_id: None,
        })
    }

    pub fn vote_on_collaboration_disagreement(
        &self,
        parent_task_id: String,
        disagreement_id: String,
        task_id: String,
        position: String,
        confidence: Option<f64>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentVoteOnCollaborationDisagreement {
            parent_task_id,
            disagreement_id,
            task_id,
            position,
            confidence,
        })
    }

    pub fn get_generated_tools(&self) -> Result<()> {
        self.send(ClientMessage::AgentListGeneratedTools)
    }

    pub fn set_operator_profile_consent(&self, consent_key: String, granted: bool) -> Result<()> {
        self.send(ClientMessage::AgentSetOperatorProfileConsent {
            consent_key,
            granted,
        })
    }

    pub fn dismiss_audit_entry(&self, entry_id: String) -> Result<()> {
        self.send(ClientMessage::AuditDismiss { entry_id })
    }

    // Plugin IPC methods (Plan 16-01)
    pub fn plugin_list(&self) -> Result<()> {
        self.send(ClientMessage::PluginList {})
    }

    pub fn plugin_list_commands(&self) -> Result<()> {
        self.send(ClientMessage::PluginListCommands {})
    }

    pub fn plugin_get(&self, name: String) -> Result<()> {
        self.send(ClientMessage::PluginGet { name })
    }

    pub fn plugin_enable(&self, name: String) -> Result<()> {
        self.send(ClientMessage::PluginEnable { name })
    }

    pub fn plugin_disable(&self, name: String) -> Result<()> {
        self.send(ClientMessage::PluginDisable { name })
    }

    pub fn plugin_get_settings(&self, name: String) -> Result<()> {
        self.send(ClientMessage::PluginGetSettings { name })
    }

    pub fn plugin_update_setting(
        &self,
        plugin_name: String,
        key: String,
        value: String,
        is_secret: bool,
    ) -> Result<()> {
        self.send(ClientMessage::PluginUpdateSettings {
            plugin_name,
            key,
            value,
            is_secret,
        })
    }

    pub fn plugin_test_connection(&self, name: String) -> Result<()> {
        self.send(ClientMessage::PluginTestConnection { name })
    }

    pub fn plugin_oauth_start(&self, name: String) -> Result<()> {
        self.send(ClientMessage::PluginOAuthStart { name })
    }

    pub fn whatsapp_link_start(&self) -> Result<()> {
        self.send(ClientMessage::AgentWhatsAppLinkStart)
    }

    pub fn whatsapp_link_stop(&self) -> Result<()> {
        self.send(ClientMessage::AgentWhatsAppLinkStop)
    }

    pub fn whatsapp_link_status(&self) -> Result<()> {
        self.send(ClientMessage::AgentWhatsAppLinkStatus)
    }

    pub fn whatsapp_link_subscribe(&self) -> Result<()> {
        self.send(ClientMessage::AgentWhatsAppLinkSubscribe)
    }

    pub fn whatsapp_link_unsubscribe(&self) -> Result<()> {
        self.send(ClientMessage::AgentWhatsAppLinkUnsubscribe)
    }

    pub fn whatsapp_link_reset(&self) -> Result<()> {
        self.send(ClientMessage::AgentWhatsAppLinkReset)
    }

    pub fn list_task_approval_rules(&self) -> Result<()> {
        self.send(ClientMessage::AgentListTaskApprovalRules)
    }

    pub fn create_task_approval_rule(&self, approval_id: String) -> Result<()> {
        self.send(ClientMessage::AgentCreateTaskApprovalRule { approval_id })
    }

    pub fn revoke_task_approval_rule(&self, rule_id: String) -> Result<()> {
        self.send(ClientMessage::AgentRevokeTaskApprovalRule { rule_id })
    }

    pub fn resolve_task_approval(&self, approval_id: String, decision: String) -> Result<()> {
        let decision = match decision.as_str() {
            "allow_once" | "approve_once" => "approve-once",
            "allow_session" | "approve_session" => "approve-session",
            _ => "deny",
        };
        self.send(ClientMessage::AgentResolveTaskApproval {
            approval_id,
            decision: decision.to_string(),
        })
    }
}
