impl TuiModel {
    fn sidebar_visible(&self) -> bool {
        if !matches!(
            self.main_pane_view,
            MainPaneView::Conversation | MainPaneView::WorkContext
        ) {
            return false;
        }
        let Some(thread_id) = self.chat.active_thread_id() else {
            return false;
        };
        !self.tasks.todos_for_thread(thread_id).is_empty()
            || self
                .tasks
                .work_context_for_thread(thread_id)
                .is_some_and(|context| !context.entries.is_empty())
            || self.chat.active_thread_has_pinned_messages()
    }

    fn current_attention_target(&self) -> (String, Option<String>, Option<String>) {
        if let Some(modal) = self.modal.top() {
            let surface = match modal {
                modal::ModalKind::Settings => {
                    format!(
                        "modal:settings:{}",
                        settings_tab_label(self.settings.active_tab())
                    )
                }
                modal::ModalKind::ApprovalOverlay => "modal:approval".to_string(),
                modal::ModalKind::OperatorQuestionOverlay => {
                    "modal:operator_question".to_string()
                }
                modal::ModalKind::ApprovalCenter => "modal:approval_center".to_string(),
                modal::ModalKind::ChatActionConfirm => "modal:chat_action_confirm".to_string(),
                modal::ModalKind::PinnedBudgetExceeded => {
                    "modal:pinned_budget_exceeded".to_string()
                }
                modal::ModalKind::CommandPalette => "modal:command_palette".to_string(),
                modal::ModalKind::ThreadParticipants => "modal:thread_participants".to_string(),
                modal::ModalKind::ThreadPicker => "modal:thread_picker".to_string(),
                modal::ModalKind::GoalPicker => "modal:goal_picker".to_string(),
                modal::ModalKind::QueuedPrompts => "modal:queued_prompts".to_string(),
                modal::ModalKind::ProviderPicker => "modal:provider_picker".to_string(),
                modal::ModalKind::ModelPicker => "modal:model_picker".to_string(),
                modal::ModalKind::OpenAIAuth => "modal:openai_auth".to_string(),
                modal::ModalKind::ErrorViewer => "modal:error_viewer".to_string(),
                modal::ModalKind::EffortPicker => "modal:effort_picker".to_string(),
                modal::ModalKind::Notifications => "modal:notifications".to_string(),
                modal::ModalKind::ToolsPicker => "modal:tools_picker".to_string(),
                modal::ModalKind::ViewPicker => "modal:view_picker".to_string(),
                modal::ModalKind::Status => "modal:status".to_string(),
                modal::ModalKind::Statistics => "modal:statistics".to_string(),
                modal::ModalKind::PromptViewer => "modal:prompt".to_string(),
                modal::ModalKind::Help => "modal:help".to_string(),
                modal::ModalKind::WhatsAppLink => "modal:whatsapp_link".to_string(),
            };
            return (
                surface,
                self.chat.active_thread_id().map(str::to_string),
                None,
            );
        }

        match &self.main_pane_view {
            MainPaneView::Conversation => match self.focus {
                FocusArea::Chat => (
                    "conversation:chat".to_string(),
                    self.chat.active_thread_id().map(str::to_string),
                    None,
                ),
                FocusArea::Input => {
                    if self.should_show_provider_onboarding() {
                        ("conversation:onboarding".to_string(), None, None)
                    } else {
                        (
                            "conversation:input".to_string(),
                            self.chat.active_thread_id().map(str::to_string),
                            None,
                        )
                    }
                }
                FocusArea::Sidebar => (
                    format!(
                        "conversation:sidebar:{}",
                        sidebar_tab_label(self.sidebar.active_tab())
                    ),
                    self.chat.active_thread_id().map(str::to_string),
                    None,
                ),
            },
            MainPaneView::Collaboration => (
                "collaboration:workspace".to_string(),
                self.chat.active_thread_id().map(str::to_string),
                None,
            ),
            MainPaneView::Task(target) => (
                "task:detail".to_string(),
                self.target_thread_id(target),
                target_goal_run_id(self, target),
            ),
            MainPaneView::WorkContext => (
                "task:work_context".to_string(),
                self.chat.active_thread_id().map(str::to_string),
                None,
            ),
            MainPaneView::FilePreview(target) => (
                "task:file_preview".to_string(),
                self.chat.active_thread_id().map(str::to_string),
                target.repo_relative_path.clone(),
            ),
            MainPaneView::GoalComposer => (
                "task:goal_composer".to_string(),
                self.chat.active_thread_id().map(str::to_string),
                None,
            ),
        }
    }

    fn publish_attention_surface_if_changed(&mut self) {
        if !self.connected {
            return;
        }
        let (surface, thread_id, goal_run_id) = self.current_attention_target();
        let signature = format!(
            "{}|{}|{}",
            surface,
            thread_id.as_deref().unwrap_or_default(),
            goal_run_id.as_deref().unwrap_or_default()
        );
        if self.last_attention_surface.as_deref() == Some(signature.as_str()) {
            return;
        }
        self.last_attention_surface = Some(signature);
        self.send_daemon_command(DaemonCommand::RecordAttention {
            surface,
            thread_id,
            goal_run_id,
        });
    }
}
