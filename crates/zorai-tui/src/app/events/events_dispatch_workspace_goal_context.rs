use super::*;

impl TuiModel {
    pub(in crate::app) fn handle_workspace_goal_context_client_event(
        &mut self,
        event: ClientEvent,
    ) -> Option<ClientEvent> {
        match event {
            ClientEvent::TaskList(tasks) => {
                self.handle_task_list_event(tasks);
                None
            }
            ClientEvent::TaskUpdate(task) => {
                self.handle_task_update_event(task);
                None
            }
            ClientEvent::WorkspaceSettings(settings) => {
                self.workspace.set_settings(settings);
                None
            }
            ClientEvent::WorkspaceSettingsList(settings) => {
                self.workspace.set_settings_list(settings);
                if self.modal.top() == Some(modal::ModalKind::WorkspacePicker) {
                    self.sync_workspace_picker_item_count();
                }
                self.status_line = "Workspaces loaded".to_string();
                None
            }
            ClientEvent::WorkspaceTaskList {
                workspace_id,
                tasks,
            } => {
                self.workspace.set_tasks(workspace_id, tasks);
                self.status_line = "Workspace refreshed".to_string();
                None
            }
            ClientEvent::WorkspaceTaskUpdated(task) => {
                let active_runtime_thread_id = workspace_task_active_thread_id(&task);
                let visible_workspace_updated =
                    matches!(self.main_pane_view, MainPaneView::Workspace)
                        && self.workspace.workspace_id() == task.workspace_id.as_str();
                let should_refresh_active_runtime = active_runtime_thread_id
                    .as_deref()
                    .is_some_and(|thread_id| {
                        !self.missing_runtime_thread_ids.contains(thread_id)
                            && !self.empty_hydrated_runtime_thread_ids.contains(thread_id)
                            && self.chat.active_thread_id() == Some(thread_id)
                            && self
                                .chat
                                .active_thread()
                                .is_some_and(|thread| thread.messages.is_empty())
                    });
                self.workspace.upsert_task(task);
                if let Some(thread_id) =
                    active_runtime_thread_id.filter(|_| should_refresh_active_runtime)
                {
                    self.request_latest_thread_page(thread_id, true);
                }
                if visible_workspace_updated {
                    self.refresh_workspace_board();
                }
                self.status_line = "Workspace task updated".to_string();
                None
            }
            ClientEvent::WorkspaceTaskDeleted {
                task_id,
                deleted_at,
            } => {
                self.workspace.mark_deleted(&task_id, deleted_at);
                self.status_line = "Workspace task deleted".to_string();
                None
            }
            ClientEvent::WorkspaceNotices { notices, .. } => {
                self.workspace.set_notices(notices);
                None
            }
            ClientEvent::WorkspaceNoticeUpdated(notice) => {
                self.workspace.upsert_notice(notice);
                None
            }
            ClientEvent::GoalRunList(runs) => {
                self.handle_goal_run_list_event(runs);
                None
            }
            ClientEvent::GoalRunStarted(run) => {
                self.handle_goal_run_started_event(run);
                None
            }
            ClientEvent::GoalRunDetail(Some(run)) => {
                if self.is_placeholder_goal_run_detail(&run) {
                    self.clear_goal_hydration_refresh(&run.id);
                } else {
                    self.clear_goal_hydration_refresh(&run.id);
                    self.handle_goal_run_detail_event(run);
                }
                None
            }
            ClientEvent::GoalRunDetail(None) => None,
            ClientEvent::GoalRunUpdate(run) => {
                self.handle_goal_run_update_event(run);
                None
            }
            ClientEvent::GoalRunControlled { goal_run_id, ok } => {
                if ok {
                    self.request_authoritative_goal_run_refresh(goal_run_id);
                    self.status_line = "Goal run updated".to_string();
                } else {
                    self.status_line = "Goal run update failed".to_string();
                }
                None
            }
            ClientEvent::GoalRunDeleted {
                goal_run_id,
                deleted,
            } => {
                if deleted {
                    let cleared_approval_id = self
                        .tasks
                        .goal_run_by_id(&goal_run_id)
                        .and_then(|run| run.awaiting_approval_id.clone());
                    let viewing_deleted_goal = if let MainPaneView::Task(target) =
                        &self.main_pane_view
                    {
                        target_goal_run_id(self, target).as_deref() == Some(goal_run_id.as_str())
                    } else {
                        false
                    };
                    let deleted_goal_run_id = goal_run_id.clone();
                    self.tasks
                        .reduce(task::TaskAction::GoalRunDeleted { goal_run_id });
                    self.clear_goal_hydration_refresh(&deleted_goal_run_id);
                    if let Some(approval_id) = cleared_approval_id {
                        self.approval
                            .reduce(crate::state::ApprovalAction::ClearResolved(approval_id));
                    }
                    if self.modal.top() == Some(modal::ModalKind::GoalPicker) {
                        self.sync_goal_picker_item_count();
                    }
                    if viewing_deleted_goal {
                        self.main_pane_view = MainPaneView::Conversation;
                    }
                    self.status_line = "Goal run deleted".to_string();
                } else {
                    self.status_line = "Goal run delete failed".to_string();
                }
                None
            }
            ClientEvent::GoalRunCheckpoints {
                goal_run_id,
                checkpoints,
            } => {
                self.handle_goal_run_checkpoints_event(goal_run_id, checkpoints);
                None
            }
            ClientEvent::GoalHydrationScheduleFailed { goal_run_id } => {
                self.clear_goal_hydration_refresh(&goal_run_id);
                None
            }
            ClientEvent::ThreadTodos {
                thread_id,
                goal_run_id,
                step_index,
                items,
            } => {
                self.handle_thread_todos_event(thread_id, goal_run_id, step_index, items);
                None
            }
            ClientEvent::WorkContext(context) => {
                self.handle_work_context_event(context);
                None
            }
            ClientEvent::GitDiff {
                repo_path,
                file_path,
                diff,
            } => {
                self.handle_git_diff_event(repo_path, file_path, diff);
                None
            }
            ClientEvent::FilePreview {
                path,
                content,
                truncated,
                is_text,
            } => {
                self.handle_file_preview_event(path, content, truncated, is_text);
                None
            }
            ClientEvent::AgentConfig(cfg) => {
                self.handle_agent_config_event(cfg);
                None
            }
            ClientEvent::AgentConfigRaw(raw) => {
                self.handle_agent_config_raw_event(raw);
                None
            }
            ClientEvent::ExternalRuntimeMigrationResult(raw) => {
                self.handle_external_runtime_migration_result(raw);
                None
            }
            other => Some(other),
        }
    }
}
