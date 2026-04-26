use super::*;

impl TuiModel {
    pub(in crate::app) fn execute_slash_command_line(&mut self, prompt: &str) -> bool {
        let trimmed = prompt.trim_start_matches('/');
        let cmd = trimmed.split_whitespace().next().unwrap_or("");
        let args = trimmed[cmd.len()..].trim();
        if cmd == "apikey" && !args.is_empty() {
            self.config.api_key = args.to_string();
            self.status_line = format!("API key set ({}...)", &args[..args.len().min(8)]);
            if let Ok(value_json) =
                serde_json::to_string(&serde_json::Value::String(args.to_string()))
            {
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: "/api_key".to_string(),
                    value_json: value_json.clone(),
                });
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: format!("/providers/{}/api_key", self.config.provider),
                    value_json: value_json.clone(),
                });
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: format!("/{}/api_key", self.config.provider),
                    value_json,
                });
            }
            return true;
        }
        if cmd == "attach" && !args.is_empty() {
            self.attach_file(args);
            return true;
        }
        if cmd == "image" {
            self.submit_image_prompt(args.to_string());
            return true;
        }
        if cmd == "prompt" {
            let requested_agent = if args.trim().is_empty() {
                None
            } else {
                Some(args.trim().to_string())
            };
            self.request_prompt_inspection(requested_agent);
            return true;
        }
        if cmd == "explain" {
            self.execute_command("explain");
            return true;
        }
        if cmd == "diverge" {
            self.execute_command("diverge");
            return true;
        }
        if cmd == "diverge-start" {
            let mut parts = args.splitn(2, char::is_whitespace);
            let thread_id = parts.next().unwrap_or("").trim();
            let problem_statement = parts.next().unwrap_or("").trim();
            if thread_id.is_empty() || problem_statement.is_empty() {
                self.status_line = "Usage: /diverge-start <thread_id> <problem>".to_string();
            } else {
                self.send_daemon_command(DaemonCommand::StartDivergentSession {
                    problem_statement: problem_statement.to_string(),
                    thread_id: thread_id.to_string(),
                    goal_run_id: None,
                });
                self.status_line = "Starting divergent session...".to_string();
            }
            return true;
        }
        if cmd == "diverge-get" {
            let session_id = args.trim();
            if session_id.is_empty() {
                self.status_line = "Usage: /diverge-get <session_id>".to_string();
            } else {
                self.send_daemon_command(DaemonCommand::GetDivergentSession {
                    session_id: session_id.to_string(),
                });
                self.status_line = "Fetching divergent session...".to_string();
            }
            return true;
        }
        if cmd == "workspace" {
            self.handle_workspace_command(args);
            return true;
        }
        if cmd == "new-workspace" {
            self.create_workspace_task_from_args(args);
            return true;
        }
        if cmd == "workspace-run" {
            self.run_workspace_task_from_args(args);
            return true;
        }
        if cmd == "workspace-pause" {
            self.pause_workspace_task_from_args(args);
            return true;
        }
        if cmd == "workspace-stop" {
            self.stop_workspace_task_from_args(args);
            return true;
        }
        if cmd == "workspace-delete" {
            self.delete_workspace_task_from_args(args);
            return true;
        }
        if cmd == "workspace-move" {
            self.move_workspace_task_from_args(args);
            return true;
        }
        if cmd == "workspace-assign" {
            self.assign_workspace_task_from_args(args);
            return true;
        }
        if cmd == "workspace-update" {
            self.update_workspace_task_from_args(args);
            return true;
        }
        if cmd == "workspace-reviewer" {
            self.set_workspace_reviewer_from_args(args);
            return true;
        }
        if cmd == "workspace-review" {
            self.review_workspace_task_from_args(args);
            return true;
        }
        if cmd == "new" {
            let target_agent_id = if args.trim().is_empty() {
                Some(amux_protocol::AGENT_ID_SWAROG.to_string())
            } else {
                self.resolve_target_agent_id(args.trim())
            };
            match target_agent_id {
                Some(target_agent_id) => {
                    self.start_new_thread_view_for_agent(Some(target_agent_id.as_str()));
                    self.status_line = format!(
                        "New conversation for {}",
                        self.participant_display_name(&target_agent_id)
                    );
                }
                None => {
                    self.status_line = format!("Unknown agent: {}", args.trim());
                }
            }
            return true;
        }
        if cmd == "thread" && !args.trim().is_empty() {
            let Some(tab) = widgets::thread_picker::resolve_thread_picker_tab(
                args.trim(),
                &self.chat,
                &self.subagents,
            ) else {
                self.status_line = format!("Unknown agent: {}", args.trim());
                return true;
            };
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
            self.modal.set_thread_picker_tab(tab);
            self.sync_thread_picker_item_count();
            return true;
        }
        if self.is_builtin_command(cmd) {
            self.execute_command(cmd);
            return true;
        }
        false
    }

    pub(super) fn handle_enter_key(&mut self, modifiers: KeyModifiers) -> bool {
        let shift = modifiers.contains(KeyModifiers::SHIFT);
        let alt = modifiers.contains(KeyModifiers::ALT);
        let ctrl_enter = modifiers.contains(KeyModifiers::CONTROL);
        if shift || alt || ctrl_enter {
            if self.focus != FocusArea::Input {
                self.focus = FocusArea::Input;
                self.input.set_mode(input::InputMode::Insert);
            }
            self.input.reduce(input::InputAction::InsertNewline);
            return false;
        }
        if self.focus == FocusArea::Chat
            && matches!(self.main_pane_view, MainPaneView::Collaboration)
            && self.collaboration.focus() == CollaborationPaneFocus::Detail
        {
            self.submit_selected_collaboration_vote();
            return false;
        }
        if self.focus == FocusArea::Chat
            && matches!(
                self.main_pane_view,
                MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
            )
        {
            if self.goal_workspace.focused_pane()
                == crate::state::goal_workspace::GoalWorkspacePane::Plan
                && self.activate_goal_workspace_plan_target()
            {
                return false;
            }
            if self.goal_workspace.focused_pane()
                == crate::state::goal_workspace::GoalWorkspacePane::CommandBar
            {
                self.activate_goal_workspace_command_bar();
                return false;
            }
            if self.goal_workspace.focused_pane()
                == crate::state::goal_workspace::GoalWorkspacePane::Timeline
                && self.activate_goal_workspace_timeline_target()
            {
                return false;
            }
            if self.goal_workspace.focused_pane()
                == crate::state::goal_workspace::GoalWorkspacePane::Details
                && self.activate_goal_workspace_detail_target()
            {
                return false;
            }
        }
        if self.focus == FocusArea::Chat {
            if let Some(sel) = self.chat.selected_message() {
                let is_tool = self
                    .chat
                    .active_thread()
                    .and_then(|thread| thread.messages.get(sel))
                    .map(|msg| msg.role == chat::MessageRole::Tool)
                    .unwrap_or(false);
                if is_tool {
                    self.chat.toggle_tool_expansion(sel);
                }
                let has_reasoning = self
                    .chat
                    .active_thread()
                    .and_then(|thread| thread.messages.get(sel))
                    .map(|msg| {
                        (msg.role == chat::MessageRole::Assistant && msg.reasoning.is_some())
                            || widgets::message::is_meta_cognition_message(msg)
                    })
                    .unwrap_or(false);
                if has_reasoning {
                    self.chat.toggle_reasoning(sel);
                }
                return false;
            }
        }
        if self.focus == FocusArea::Sidebar {
            self.handle_sidebar_enter();
            return false;
        }
        if self.focus != FocusArea::Input {
            self.focus = FocusArea::Input;
            self.input.set_mode(input::InputMode::Insert);
            return false;
        }
        self.input.reduce(input::InputAction::Submit);
        if let Some(prompt) = self.input.take_submitted() {
            if self.should_show_operator_profile_onboarding() {
                if !prompt.starts_with('/') {
                    self.input.set_text(&prompt);
                    if self.submit_operator_profile_answer() {
                        return false;
                    }
                }
                self.input.set_text(&prompt);
                self.show_input_notice(
                    "Onboarding active: answer in input (Ctrl+S skip, Ctrl+D defer)",
                    InputNoticeKind::Warning,
                    90,
                    true,
                );
                return false;
            }
            if matches!(self.main_pane_view, MainPaneView::GoalComposer) {
                self.start_goal_run_from_mission_control();
                return false;
            }
            if prompt.starts_with('/') {
                if !self.execute_slash_command_line(&prompt) {
                    self.submit_prompt(prompt);
                }
            } else {
                self.submit_prompt(prompt);
            }
        }
        false
    }
}
