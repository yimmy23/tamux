use super::*;

impl TuiModel {
    pub(crate) fn gateway_default_agent_options(&self) -> Vec<(String, String)> {
        let mut options = Vec::new();
        for choice in crate::state::subagents::BUILTIN_PERSONA_ROLE_CHOICES {
            options.push((choice.id.to_string(), choice.label.to_string()));
        }
        for entry in self.subagents.entries.iter().filter(|entry| entry.enabled) {
            if options
                .iter()
                .any(|(id, _)| id.eq_ignore_ascii_case(&entry.id))
            {
                continue;
            }
            let label = if entry.name.trim().is_empty() {
                entry.id.clone()
            } else {
                entry.name.clone()
            };
            options.push((entry.id.clone(), label));
        }
        options
    }

    pub(crate) fn open_gateway_default_agent_picker(&mut self) {
        let options = self.gateway_default_agent_options();
        let current = self.config.gateway_default_agent.trim().to_string();
        let selected = options
            .iter()
            .position(|(id, _)| id.eq_ignore_ascii_case(&current))
            .unwrap_or(0);
        self.modal.reduce(modal::ModalAction::Push(
            modal::ModalKind::GatewayDefaultAgentPicker,
        ));
        self.modal.set_picker_item_count(options.len().max(1));
        self.modal.set_picker_cursor(selected);
    }

    pub(crate) fn gateway_default_agent_picker_scroll(&self) -> usize {
        let total_lines = self.gateway_default_agent_options().len().saturating_add(2);
        self.cursor_follow_scroll(modal::ModalKind::GatewayDefaultAgentPicker, 2, total_lines)
    }

    pub(crate) fn gateway_default_agent_picker_body(&self) -> String {
        let options = self.gateway_default_agent_options();
        let cursor = self.modal.picker_cursor();
        let current = self.config.gateway_default_agent.trim().to_string();
        let mut body = String::from("Select the default gateway agent:\n\n");
        for (index, (id, label)) in options.iter().enumerate() {
            let marker = if index == cursor { ">" } else { " " };
            let active = if id.eq_ignore_ascii_case(&current) {
                "\u{25cf}"
            } else {
                " "
            };
            body.push_str(&format!("{marker} {active} {label} ({id})\n"));
        }
        body.push_str("\n\u{2191}\u{2193} nav  Enter select  Esc cancel");
        body
    }

    pub(crate) fn submit_gateway_default_agent_picker(&mut self) {
        let options = self.gateway_default_agent_options();
        let cursor = self.modal.picker_cursor();
        let Some((agent_id, label)) = options.get(cursor).cloned() else {
            self.status_line = "No agent selected".to_string();
            return;
        };
        self.config.gateway_default_agent = agent_id;
        self.sync_config_to_daemon();
        self.status_line = format!("Gateway default agent: {label}");
        self.close_top_modal();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_model() -> TuiModel {
        let (_event_tx, event_rx) = std::sync::mpsc::channel();
        let (daemon_tx, _daemon_rx) = tokio::sync::mpsc::unbounded_channel();
        TuiModel::new(event_rx, daemon_tx)
    }

    #[test]
    fn gateway_default_agent_picker_lists_agents_and_writes_selection() {
        let mut model = test_model();
        model.subagents.entries.push(crate::state::SubAgentEntry {
            claude_permission_mode: None,
            id: "reviewer".to_string(),
            name: "Reviewer".to_string(),
            provider: "openai".to_string(),
            model: "gpt-5.4-mini".to_string(),
            role: Some("specialist".to_string()),
            enabled: true,
            builtin: false,
            immutable_identity: false,
            disable_allowed: true,
            delete_allowed: true,
            protected_reason: None,
            reasoning_effort: None,
            api_transport: None,
            openrouter_provider_order: String::new(),
            openrouter_provider_ignore: String::new(),
            openrouter_allow_fallbacks: true,
            huggingface_provider: String::new(),
            raw_json: None,
        });

        let options = model.gateway_default_agent_options();
        let has = |target: &str| options.iter().any(|(id, _)| id == target);
        assert!(has(zorai_protocol::AGENT_ID_SWAROG));
        assert!(has(zorai_protocol::AGENT_ID_RAROG));
        assert!(has("radogost"));
        assert!(has("reviewer"));

        model.open_gateway_default_agent_picker();
        assert_eq!(
            model.modal.top(),
            Some(modal::ModalKind::GatewayDefaultAgentPicker)
        );

        let reviewer_idx = options
            .iter()
            .position(|(id, _)| id == "reviewer")
            .expect("reviewer present");
        model.modal.set_picker_cursor(reviewer_idx);
        model.submit_gateway_default_agent_picker();

        assert_eq!(model.config.gateway_default_agent, "reviewer");
        assert_ne!(
            model.modal.top(),
            Some(modal::ModalKind::GatewayDefaultAgentPicker)
        );
    }
}
