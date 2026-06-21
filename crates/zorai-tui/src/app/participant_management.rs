use super::*;
use std::collections::HashSet;

const PARTICIPANT_EXCLUDED_BUILTIN_IDS: [&str; 3] =
    [zorai_protocol::AGENT_ID_SWAROG, zorai_protocol::AGENT_ID_RAROG, "rod"];

const PARTICIPANT_INSTRUCTION_PREVIEW_CHARS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParticipantRow {
    pub(crate) agent_id: String,
    pub(crate) agent_name: String,
    pub(crate) active: bool,
    pub(crate) instruction: String,
}

pub(crate) fn thread_participant_agent_options(
    subagents: &crate::state::SubAgentsState,
    active_ids: &HashSet<String>,
) -> Vec<(String, String)> {
    let mut options = Vec::new();
    for choice in crate::state::subagents::BUILTIN_PERSONA_ROLE_CHOICES {
        if PARTICIPANT_EXCLUDED_BUILTIN_IDS
            .iter()
            .any(|excluded| choice.id.eq_ignore_ascii_case(excluded))
        {
            continue;
        }
        if active_ids.contains(&choice.id.to_ascii_lowercase()) {
            continue;
        }
        options.push((choice.id.to_string(), choice.label.to_string()));
    }
    for entry in subagents.entries.iter().filter(|entry| entry.enabled) {
        if active_ids.contains(&entry.id.to_ascii_lowercase()) {
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

fn truncate_instruction(instruction: &str) -> String {
    let trimmed = instruction.trim();
    if trimmed.chars().count() <= PARTICIPANT_INSTRUCTION_PREVIEW_CHARS {
        return trimmed.to_string();
    }
    let head: String = trimmed
        .chars()
        .take(PARTICIPANT_INSTRUCTION_PREVIEW_CHARS.saturating_sub(1))
        .collect();
    format!("{head}…")
}

impl TuiModel {
    pub(crate) fn thread_participant_rows(&self) -> Vec<ParticipantRow> {
        let Some(thread) = self.chat.active_thread() else {
            return Vec::new();
        };
        thread
            .thread_participants
            .iter()
            .map(|participant| ParticipantRow {
                agent_id: participant.agent_id.clone(),
                agent_name: participant.agent_name.clone(),
                active: participant.status.eq_ignore_ascii_case("active"),
                instruction: participant.instruction.clone(),
            })
            .collect()
    }

    fn thread_participant_active_ids(&self) -> HashSet<String> {
        self.thread_participant_rows()
            .into_iter()
            .filter(|row| row.active)
            .map(|row| row.agent_id.to_ascii_lowercase())
            .collect()
    }

    pub(crate) fn thread_participant_agent_options(&self) -> Vec<(String, String)> {
        thread_participant_agent_options(&self.subagents, &self.thread_participant_active_ids())
    }

    pub(crate) fn thread_participants_modal_body(&self) -> String {
        let Some(thread) = self.chat.active_thread() else {
            return "No active thread selected.".to_string();
        };
        let cursor = self.modal.picker_cursor();
        let rows = self.thread_participant_rows();

        let mut body = String::new();
        body.push_str(&format!("Thread: {}\n\n", thread.title));

        let add_marker = if cursor == 0 { ">" } else { " " };
        body.push_str(&format!("{add_marker} + Add participant\n"));
        for (index, row) in rows.iter().enumerate() {
            let marker = if cursor == index + 1 { ">" } else { " " };
            let status = if row.active { "active" } else { "inactive" };
            let instruction = truncate_instruction(&row.instruction);
            let instruction = if instruction.is_empty() {
                "(no instruction)".to_string()
            } else {
                instruction
            };
            body.push_str(&format!(
                "{marker} {} ({}) [{status}] — {instruction}\n",
                row.agent_name, row.agent_id
            ));
        }
        body.push('\n');

        body.push_str("Queued Suggestions\n------------------\n");
        if thread.queued_participant_suggestions.is_empty() {
            body.push_str("  none\n");
        } else {
            for suggestion in &thread.queued_participant_suggestions {
                let mut badges = vec![suggestion.status.clone()];
                if suggestion.force_send {
                    badges.push("force_send".to_string());
                }
                body.push_str(&format!(
                    "  {} [{}]\n    {}\n",
                    suggestion.target_agent_name,
                    badges.join(", "),
                    truncate_instruction(&suggestion.instruction)
                ));
            }
        }
        body.push_str("\n↑↓ nav  Enter select  Esc close");
        body
    }

    fn cursor_follow_scroll(
        &self,
        kind: modal::ModalKind,
        header_lines: usize,
        total_lines: usize,
    ) -> usize {
        let viewport_lines = self
            .current_modal_area()
            .filter(|(modal_kind, _)| *modal_kind == kind)
            .map(|(_, area)| area.height.saturating_sub(3) as usize)
            .unwrap_or(1)
            .max(1);
        let selected_line = header_lines.saturating_add(self.modal.picker_cursor());
        selected_line
            .saturating_sub(viewport_lines.saturating_sub(1))
            .min(total_lines.saturating_sub(viewport_lines))
    }

    pub(crate) fn thread_participants_modal_cursor_scroll(&self) -> usize {
        let total_lines = self.thread_participants_modal_body().lines().count();
        self.cursor_follow_scroll(modal::ModalKind::ThreadParticipants, 2, total_lines)
    }

    pub(crate) fn thread_participant_agent_picker_scroll(&self) -> usize {
        let total_lines = self
            .thread_participant_agent_options()
            .len()
            .saturating_add(4);
        self.cursor_follow_scroll(modal::ModalKind::ThreadParticipantAgentPicker, 2, total_lines)
    }

    pub(crate) fn thread_participant_actions_scroll(&self) -> usize {
        let total_lines = self.participant_actions_labels().len().saturating_add(4);
        self.cursor_follow_scroll(modal::ModalKind::ThreadParticipantActions, 2, total_lines)
    }

    pub(crate) fn sync_thread_participants_modal_item_count(&mut self) {
        let count = self.thread_participant_rows().len() + 1;
        self.modal.set_picker_item_count(count);
    }

    pub(crate) fn handle_thread_participants_modal_enter(&mut self) {
        let cursor = self.modal.picker_cursor();
        if cursor == 0 {
            self.open_thread_participant_agent_picker();
            return;
        }
        let rows = self.thread_participant_rows();
        let Some(row) = rows.get(cursor - 1) else {
            return;
        };
        self.participant_actions_target = Some(ParticipantActionsTarget {
            agent_id: row.agent_id.clone(),
            agent_name: row.agent_name.clone(),
            active: row.active,
        });
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadParticipantActions));
        self.modal
            .set_picker_item_count(self.participant_actions_labels().len());
    }

    pub(crate) fn open_thread_participant_agent_picker(&mut self) {
        let options = self.thread_participant_agent_options();
        self.modal.reduce(modal::ModalAction::Push(
            modal::ModalKind::ThreadParticipantAgentPicker,
        ));
        self.modal.set_picker_item_count(options.len().max(1));
        if options.is_empty() {
            self.status_line = "No available agents to add as participants".to_string();
        }
    }

    pub(crate) fn thread_participant_agent_picker_body(&self) -> String {
        let options = self.thread_participant_agent_options();
        let cursor = self.modal.picker_cursor();
        let mut body = String::from("Select an agent to add as a participant:\n\n");
        if options.is_empty() {
            body.push_str("  No available agents (all are already active participants).\n");
        } else {
            for (index, (_, label)) in options.iter().enumerate() {
                let marker = if index == cursor { ">" } else { " " };
                body.push_str(&format!("{marker} {label}\n"));
            }
        }
        body.push_str("\n↑↓ nav  Enter select  Esc cancel");
        body
    }

    pub(crate) fn submit_thread_participant_agent_picker(&mut self) {
        let options = self.thread_participant_agent_options();
        let cursor = self.modal.picker_cursor();
        let Some((agent_id, _)) = options.get(cursor).cloned() else {
            self.status_line = "No agent selected".to_string();
            return;
        };
        self.close_top_modal();
        self.begin_add_thread_participant(&agent_id);
    }

    pub(crate) fn participant_actions_labels(&self) -> Vec<String> {
        let Some(target) = self.participant_actions_target.as_ref() else {
            return Vec::new();
        };
        let mut labels = vec!["Edit instruction".to_string()];
        if target.active {
            labels.push("Stop participant".to_string());
        } else {
            labels.push("Reactivate participant".to_string());
        }
        labels.push("Remove participant".to_string());
        labels
    }

    pub(crate) fn thread_participant_actions_body(&self) -> String {
        let cursor = self.modal.picker_cursor();
        let name = self
            .participant_actions_target
            .as_ref()
            .map(|target| target.agent_name.as_str())
            .unwrap_or("participant");
        let mut body = format!("Participant: {name}\n\n");
        for (index, label) in self.participant_actions_labels().iter().enumerate() {
            let marker = if index == cursor { ">" } else { " " };
            body.push_str(&format!("{marker} {label}\n"));
        }
        body.push_str("\n↑↓ nav  Enter select  Esc cancel");
        body
    }

    pub(crate) fn submit_thread_participant_actions(&mut self) {
        let cursor = self.modal.picker_cursor();
        let labels = self.participant_actions_labels();
        let Some(label) = labels.get(cursor).cloned() else {
            return;
        };
        let Some(target) = self.participant_actions_target.clone() else {
            return;
        };
        let Some(thread_id) = self.chat.active_thread_id().map(String::from) else {
            self.status_line = "Participant actions require an active thread".to_string();
            return;
        };
        self.close_top_modal();
        match label.as_str() {
            "Edit instruction" | "Reactivate participant" => {
                let prefill = self
                    .thread_participant_rows()
                    .into_iter()
                    .find(|row| row.agent_id.eq_ignore_ascii_case(&target.agent_id))
                    .map(|row| row.instruction)
                    .unwrap_or_default();
                self.open_participant_instruction_editor(
                    thread_id,
                    target.agent_id,
                    target.agent_name,
                    false,
                    prefill,
                );
            }
            "Stop participant" => {
                self.send_thread_participant_command(
                    thread_id,
                    target.agent_id,
                    "deactivate",
                    None,
                    format!("Stop request sent for {}", target.agent_name),
                );
            }
            "Remove participant" => {
                self.send_thread_participant_command(
                    thread_id,
                    target.agent_id,
                    "remove",
                    None,
                    format!("Removed {} from this thread", target.agent_name),
                );
            }
            _ => {}
        }
    }

    pub(crate) fn open_participant_instruction_editor(
        &mut self,
        thread_id: String,
        agent_id: String,
        agent_name: String,
        is_new: bool,
        prefill: String,
    ) {
        while self.modal.top().is_some() {
            self.close_top_modal();
        }
        self.pending_participant_instruction_edit = Some(PendingParticipantInstructionEdit {
            thread_id,
            agent_id,
            agent_name: agent_name.clone(),
            is_new,
        });
        self.set_input_text(&prefill);
        self.focus = FocusArea::Input;
        let verb = if is_new { "Add" } else { "Edit" };
        self.status_line = format!("{verb} instruction for {agent_name}, then press Enter");
        self.show_input_notice(
            format!(
                "Type the instruction for {agent_name} and press Enter (Esc to cancel)"
            ),
            InputNoticeKind::Success,
            240,
            true,
        );
    }

    pub(crate) fn cancel_participant_instruction_edit(&mut self) -> bool {
        if self.pending_participant_instruction_edit.take().is_some() {
            self.input.reduce(input::InputAction::Clear);
            self.status_line = "Cancelled participant instruction edit".to_string();
            true
        } else {
            false
        }
    }

    pub(crate) fn submit_participant_instruction_edit(&mut self, text: String) -> bool {
        let Some(pending) = self.pending_participant_instruction_edit.take() else {
            return false;
        };
        let instruction = text.trim().to_string();
        self.input.reduce(input::InputAction::Clear);
        if instruction.is_empty() {
            self.status_line = format!(
                "Instruction required for {} — nothing sent",
                pending.agent_name
            );
            return true;
        }
        let verb = if pending.is_new { "added" } else { "updated" };
        let status = format!("Participant {} {verb}", pending.agent_name);
        self.send_thread_participant_command(
            pending.thread_id,
            pending.agent_id,
            "upsert",
            Some(instruction),
            status,
        );
        true
    }

    pub(crate) fn send_thread_participant_command(
        &mut self,
        thread_id: String,
        agent_id: String,
        action: &str,
        instruction: Option<String>,
        status: String,
    ) {
        self.send_daemon_command(DaemonCommand::ThreadParticipantCommand {
            thread_id,
            target_agent_id: agent_id,
            action: action.to_string(),
            instruction,
            session_id: None,
        });
        self.status_line = status;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{SubAgentEntry, SubAgentsState};

    fn subagent(id: &str, name: &str, enabled: bool) -> SubAgentEntry {
        SubAgentEntry {
            claude_permission_mode: None,
            id: id.to_string(),
            name: name.to_string(),
            provider: "openai".to_string(),
            model: "gpt-5.4".to_string(),
            role: None,
            enabled,
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
        }
    }

    #[test]
    fn agent_options_exclude_main_agent_and_disabled_subagents() {
        let mut subagents = SubAgentsState::new();
        subagents.entries = vec![subagent("hf", "hf", true), subagent("off", "Off", false)];

        let options = thread_participant_agent_options(&subagents, &HashSet::new());
        let ids = options.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>();

        assert!(!ids.contains(&zorai_protocol::AGENT_ID_SWAROG));
        assert!(!ids.contains(&zorai_protocol::AGENT_ID_RAROG));
        assert!(!ids.contains(&"rod"));
        assert!(ids.contains(&"hf"));
        assert!(!ids.contains(&"off"));
        assert!(ids.contains(&"radogost"));
    }

    #[test]
    fn agent_options_exclude_already_active_participants() {
        let mut subagents = SubAgentsState::new();
        subagents.entries = vec![subagent("hf", "hf", true)];
        let active: HashSet<String> = ["hf".to_string(), "radogost".to_string()]
            .into_iter()
            .collect();

        let options = thread_participant_agent_options(&subagents, &active);
        let ids = options.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>();

        assert!(!ids.contains(&"hf"));
        assert!(!ids.contains(&"radogost"));
        assert!(ids.contains(&"perun"));
    }
}
