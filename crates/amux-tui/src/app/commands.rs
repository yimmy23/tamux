use super::*;
use std::path::{Path, PathBuf};

#[path = "commands_goal_targets.rs"]
mod goal_targets;

#[derive(Debug, Clone)]
enum GoalSidebarCommandItem {
    Step { step_id: String },
    Checkpoint { step_id: Option<String> },
    Task { target: sidebar::SidebarItemTarget },
    File { thread_id: String, path: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GoalActionPickerItem {
    PauseGoal,
    ResumeGoal,
    StopGoal,
    RetryStep,
    RerunFromStep,
    CycleRuntimeAssignment,
    EditRuntimeProvider,
    EditRuntimeModel,
    EditRuntimeReasoning,
    EditRuntimeRole,
    ToggleRuntimeEnabled,
    ToggleRuntimeInherit,
    ApplyRuntimeNextTurn,
    ApplyRuntimeReassignActiveStep,
    ApplyRuntimeRestartActiveStep,
}

impl GoalActionPickerItem {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::PauseGoal => "Pause Goal",
            Self::ResumeGoal => "Resume Goal",
            Self::StopGoal => "Stop Goal",
            Self::RetryStep => "Retry Step",
            Self::RerunFromStep => "Rerun From Step",
            Self::CycleRuntimeAssignment => "Select Next Runtime Agent",
            Self::EditRuntimeProvider => "Edit Runtime Provider",
            Self::EditRuntimeModel => "Edit Runtime Model",
            Self::EditRuntimeReasoning => "Edit Runtime Reasoning",
            Self::EditRuntimeRole => "Edit Runtime Role",
            Self::ToggleRuntimeEnabled => "Toggle Runtime Enabled",
            Self::ToggleRuntimeInherit => "Toggle Runtime Inherit",
            Self::ApplyRuntimeNextTurn => "Apply Next Turn",
            Self::ApplyRuntimeReassignActiveStep => "Reassign Active Step",
            Self::ApplyRuntimeRestartActiveStep => "Restart Active Step",
        }
    }
}

impl TuiModel {
    pub(super) fn mission_control_navigation_state(&self) -> MissionControlNavigationState {
        self.mission_control_navigation.clone()
    }

    pub(super) fn update_mission_control_navigation_state(
        &mut self,
        update: impl FnOnce(&mut MissionControlNavigationState),
    ) {
        update(&mut self.mission_control_navigation);
    }

    pub(super) fn mission_control_source_goal_target(&self) -> Option<sidebar::SidebarItemTarget> {
        self.mission_control_navigation_state().source_goal_target
    }

    pub(super) fn set_mission_control_source_goal_target(
        &mut self,
        target: Option<sidebar::SidebarItemTarget>,
    ) {
        self.update_mission_control_navigation_state(|state| {
            state.source_goal_target = target;
        });
    }

    pub(super) fn mission_control_return_to_goal_target(
        &self,
    ) -> Option<sidebar::SidebarItemTarget> {
        self.mission_control_navigation_state()
            .return_to_goal_target
    }

    pub(super) fn set_mission_control_return_to_goal_target(
        &mut self,
        target: Option<sidebar::SidebarItemTarget>,
    ) {
        self.update_mission_control_navigation_state(|state| {
            state.return_to_goal_target = target;
        });
    }

    pub(super) fn mission_control_return_to_thread_id(&self) -> Option<String> {
        self.mission_control_navigation_state().return_to_thread_id
    }

    pub(super) fn set_mission_control_return_to_thread_id(&mut self, thread_id: Option<String>) {
        self.update_mission_control_navigation_state(|state| {
            state.return_to_thread_id = thread_id;
        });
    }

    pub(super) fn clear_mission_control_return_context(&mut self) {
        self.set_mission_control_return_to_goal_target(None);
        self.set_mission_control_return_to_thread_id(None);
    }

    pub(super) fn current_goal_return_target(&self) -> Option<sidebar::SidebarItemTarget> {
        self.mission_control_return_to_goal_target().or_else(|| {
            if matches!(self.main_pane_view, MainPaneView::GoalComposer) {
                self.mission_control_source_goal_target()
            } else {
                self.current_goal_target_for_mission_control()
            }
        })
    }

    pub(super) fn set_mission_control_return_targets(
        &mut self,
        goal_target: Option<sidebar::SidebarItemTarget>,
        thread_id: Option<String>,
    ) {
        self.set_mission_control_return_to_goal_target(goal_target);
        self.set_mission_control_return_to_thread_id(thread_id);
    }

    fn open_work_context_for_thread(
        &mut self,
        thread_id: String,
        path: Option<String>,
        parent_thread_id: Option<String>,
        goal_target: Option<sidebar::SidebarItemTarget>,
        status_line: String,
    ) {
        self.set_mission_control_return_targets(goal_target, parent_thread_id);
        self.main_pane_view = MainPaneView::WorkContext;
        self.task_view_scroll = 0;
        self.focus = FocusArea::Chat;
        if let Some(path) = path {
            self.tasks.reduce(task::TaskAction::SelectWorkPath {
                thread_id: thread_id.clone(),
                path: Some(path),
            });
        }
        self.request_preview_for_selected_path(&thread_id);
        self.status_line = status_line;
    }

    pub(super) fn current_goal_target_for_mission_control(
        &self,
    ) -> Option<sidebar::SidebarItemTarget> {
        match &self.main_pane_view {
            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun {
                goal_run_id,
                step_id,
            }) => Some(sidebar::SidebarItemTarget::GoalRun {
                goal_run_id: goal_run_id.clone(),
                step_id: step_id.clone(),
            }),
            MainPaneView::Task(sidebar::SidebarItemTarget::Task { task_id }) => {
                self.parent_goal_target_for_task(task_id)
            }
            _ => None,
        }
    }

    fn mission_control_goal_run(&self) -> Option<&task::GoalRun> {
        let target = self.mission_control_source_goal_target()?;
        let goal_run_id = target_goal_run_id(self, &target)?;
        self.tasks.goal_run_by_id(&goal_run_id)
    }

    fn mission_control_thread_target(&self) -> Option<(String, bool)> {
        let run = self.mission_control_goal_run()?;
        run.active_thread_id
            .clone()
            .map(|thread_id| (thread_id, false))
            .or_else(|| {
                run.root_thread_id
                    .clone()
                    .map(|thread_id| (thread_id, true))
            })
    }

    fn goal_prompt_thread_target(&self) -> Option<(sidebar::SidebarItemTarget, String)> {
        let target = self.current_goal_target_for_mission_control()?;
        let goal_run_id = target_goal_run_id(self, &target)?;
        let run = self.tasks.goal_run_by_id(&goal_run_id)?;
        let thread_id = run
            .thread_id
            .clone()
            .or_else(|| run.root_thread_id.clone())
            .or_else(|| run.active_thread_id.clone())
            .or_else(|| run.execution_thread_ids.first().cloned())
            .or_else(|| {
                self.tasks
                    .tasks()
                    .iter()
                    .find(|task| task.goal_run_id.as_deref() == Some(run.id.as_str()))
                    .and_then(|task| task.thread_id.clone())
            })?;
        Some((target, thread_id))
    }

    pub(super) fn mission_control_has_thread_target(&self) -> bool {
        self.mission_control_thread_target().is_some()
    }

    fn runtime_assignments_for_goal_run(
        &self,
        run: &task::GoalRun,
    ) -> (Vec<task::GoalAgentAssignment>, bool) {
        if !run.runtime_assignment_list.is_empty() {
            (run.runtime_assignment_list.clone(), false)
        } else if !run.launch_assignment_snapshot.is_empty() {
            (run.launch_assignment_snapshot.clone(), true)
        } else {
            let fallback_profile = self.current_conversation_agent_profile();
            (
                vec![task::GoalAgentAssignment {
                    role_id: amux_protocol::AGENT_ID_SWAROG.to_string(),
                    enabled: true,
                    provider: fallback_profile.provider,
                    model: fallback_profile.model,
                    reasoning_effort: fallback_profile.reasoning_effort,
                    inherit_from_main: false,
                }],
                true,
            )
        }
    }

    fn runtime_assignment_matches_owner(
        assignment: &task::GoalAgentAssignment,
        owner: &task::GoalRuntimeOwnerProfile,
    ) -> bool {
        assignment.provider == owner.provider
            && assignment.model == owner.model
            && assignment.reasoning_effort == owner.reasoning_effort
    }

    fn active_runtime_assignment_index_for_run(
        &self,
        run: &task::GoalRun,
        assignments: &[task::GoalAgentAssignment],
    ) -> Option<usize> {
        run.current_step_owner_profile
            .as_ref()
            .or(run.planner_owner_profile.as_ref())
            .and_then(|owner| {
                assignments.iter().position(|assignment| {
                    Self::runtime_assignment_matches_owner(assignment, owner)
                })
            })
    }

    fn sync_goal_mission_control_from_run(&mut self, run: &task::GoalRun, preserve_pending: bool) {
        let (assignments, uses_fallback) = self.runtime_assignments_for_goal_run(run);
        let active_index = self.active_runtime_assignment_index_for_run(run, &assignments);
        let preserved_pending = preserve_pending
            && self.goal_mission_control.runtime_goal_run_id.as_deref() == Some(run.id.as_str())
            && self.goal_mission_control.pending_role_assignments.is_some();
        let preserved_overlay = preserved_pending
            .then(|| self.goal_mission_control.pending_role_assignments.clone())
            .flatten();
        let preserved_modes = preserved_pending
            .then(|| {
                self.goal_mission_control
                    .pending_runtime_apply_modes
                    .clone()
            })
            .unwrap_or_default();
        let preserved_selection = preserved_pending
            .then_some(self.goal_mission_control.selected_runtime_assignment_index);

        self.goal_mission_control.configure_runtime_assignments(
            run.id.clone(),
            assignments,
            active_index,
            uses_fallback,
        );
        if let Some(overlay) = preserved_overlay {
            self.goal_mission_control.pending_role_assignments = Some(overlay);
            self.goal_mission_control.pending_runtime_apply_modes = preserved_modes;
        }
        if let Some(selection) = preserved_selection {
            self.goal_mission_control
                .set_selected_runtime_assignment_index(selection);
        }
    }

    fn sync_goal_mission_control_from_selected_goal_run(&mut self) -> bool {
        let Some(run) = self.selected_goal_run().cloned() else {
            return false;
        };
        self.set_mission_control_source_goal_target(self.current_goal_target_for_mission_control());
        self.sync_goal_mission_control_from_run(&run, true);
        true
    }

    pub(super) fn sidebar_uses_goal_sidebar(&self) -> bool {
        matches!(
            self.main_pane_view,
            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
        ) && self.sidebar_visible()
    }

    fn active_goal_sidebar_goal_run(&self) -> Option<&str> {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return None;
        };
        Some(goal_run_id.as_str())
    }

    pub(super) fn goal_sidebar_item_count(&self) -> usize {
        self.active_goal_sidebar_goal_run()
            .map(|goal_run_id| {
                self.goal_sidebar_item_count_for_tab(goal_run_id, self.goal_sidebar.active_tab())
            })
            .unwrap_or(0)
    }

    pub(super) fn goal_workspace_plan_items(
        &self,
    ) -> Vec<crate::state::goal_workspace::GoalPlanSelection> {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return Vec::new();
        };
        widgets::goal_workspace::plan_selection_rows(&self.tasks, goal_run_id, &self.goal_workspace)
            .into_iter()
            .map(|(_, selection)| selection)
            .collect()
    }

    pub(super) fn sync_goal_workspace_selection_for_active_goal_pane(&mut self) {
        let items = self.goal_workspace_plan_items();
        if items.is_empty() {
            self.goal_workspace.set_selected_plan_row(0);
            self.goal_workspace.set_selected_plan_item(None);
            self.goal_workspace.set_plan_scroll(0);
            return;
        }

        let current_step_selection = match &self.main_pane_view {
            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun {
                step_id: Some(step_id),
                ..
            }) => Some(crate::state::goal_workspace::GoalPlanSelection::Step {
                step_id: step_id.clone(),
            }),
            _ => None,
        };

        let target = self
            .goal_workspace
            .selected_plan_item()
            .cloned()
            .filter(|item| items.contains(item))
            .or_else(|| current_step_selection.filter(|item| items.contains(item)))
            .unwrap_or_else(|| items[0].clone());

        let row = items.iter().position(|item| *item == target).unwrap_or(0);
        self.goal_workspace.set_selected_plan_row(row);
        self.goal_workspace.set_selected_plan_item(Some(target));
        self.clamp_goal_workspace_plan_scroll_to_selection();
    }

    pub(super) fn select_goal_workspace_plan_item(
        &mut self,
        item: crate::state::goal_workspace::GoalPlanSelection,
    ) -> bool {
        let changed = match &item {
            crate::state::goal_workspace::GoalPlanSelection::Step { step_id }
            | crate::state::goal_workspace::GoalPlanSelection::Todo { step_id, .. } => {
                self.select_goal_step_in_active_run(step_id.clone())
            }
            crate::state::goal_workspace::GoalPlanSelection::PromptToggle
            | crate::state::goal_workspace::GoalPlanSelection::MainThread { .. } => false,
        };
        let items = self.goal_workspace_plan_items();
        let row = items
            .iter()
            .position(|candidate| candidate == &item)
            .unwrap_or(0);
        self.goal_workspace.set_selected_plan_row(row);
        self.goal_workspace.set_selected_plan_item(Some(item));
        self.clamp_goal_workspace_plan_scroll_to_selection();
        changed
    }

    pub(super) fn clamp_goal_workspace_plan_scroll_to_selection(&mut self) {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return;
        };

        let area = self.pane_layout().chat;
        let (viewport_height, viewport_width) = {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(4), Constraint::Min(1)])
                .split(area);
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(40),
                    Constraint::Percentage(32),
                    Constraint::Min(24),
                ])
                .split(layout[1]);
            let inner = Block::default().borders(Borders::ALL).inner(columns[0]);
            (inner.height as usize, inner.width as usize)
        };
        if viewport_height == 0 {
            self.goal_workspace.set_plan_scroll(0);
            return;
        }

        let max_scroll = widgets::goal_workspace::max_plan_scroll(
            area,
            &self.tasks,
            goal_run_id,
            &self.goal_workspace,
        );
        let selected_row = widgets::goal_workspace::plan_visual_row_for_selection(
            &self.tasks,
            goal_run_id,
            &self.goal_workspace,
            viewport_width,
        )
        .unwrap_or(0);
        let current_scroll = self.goal_workspace.plan_scroll().min(max_scroll);
        let next_scroll = if selected_row < current_scroll {
            selected_row
        } else if selected_row >= current_scroll.saturating_add(viewport_height) {
            selected_row
                .saturating_add(1)
                .saturating_sub(viewport_height)
        } else {
            current_scroll
        };
        self.goal_workspace
            .set_plan_scroll(next_scroll.min(max_scroll));
    }

    pub(super) fn step_goal_workspace_plan_selection(&mut self, delta: i32) -> bool {
        self.sync_goal_workspace_selection_for_active_goal_pane();
        let items = self.goal_workspace_plan_items();
        if items.is_empty() {
            return false;
        }
        let current = self.goal_workspace.selected_plan_row().min(items.len() - 1);
        let next = if delta >= 0 {
            (current + delta as usize).min(items.len() - 1)
        } else {
            current.saturating_sub((-delta) as usize)
        };
        self.select_goal_workspace_plan_item(items[next].clone())
    }

    pub(super) fn activate_goal_workspace_plan_target(&mut self) -> bool {
        let Some(selection) = self.goal_workspace.selected_plan_item().cloned() else {
            return false;
        };
        match selection {
            crate::state::goal_workspace::GoalPlanSelection::PromptToggle => {
                self.goal_workspace.toggle_prompt_expanded();
                self.sync_goal_workspace_selection_for_active_goal_pane();
                self.clamp_goal_workspace_plan_scroll_to_selection();
                true
            }
            crate::state::goal_workspace::GoalPlanSelection::MainThread { thread_id } => {
                self.open_thread_conversation(thread_id);
                true
            }
            crate::state::goal_workspace::GoalPlanSelection::Step { step_id } => {
                self.goal_workspace.toggle_step_expanded(step_id);
                self.sync_goal_workspace_selection_for_active_goal_pane();
                self.clamp_goal_workspace_plan_scroll_to_selection();
                true
            }
            crate::state::goal_workspace::GoalPlanSelection::Todo { .. } => false,
        }
    }

    pub(super) fn expand_selected_goal_workspace_step(&mut self) -> bool {
        self.sync_goal_workspace_selection_for_active_goal_pane();
        let Some(selection) = self.goal_workspace.selected_plan_item().cloned() else {
            return false;
        };
        let crate::state::goal_workspace::GoalPlanSelection::Step { step_id } = selection else {
            return false;
        };
        self.goal_workspace.set_step_expanded(step_id, true);
        self.sync_goal_workspace_selection_for_active_goal_pane();
        true
    }

    pub(super) fn collapse_goal_workspace_selection(&mut self) -> bool {
        self.sync_goal_workspace_selection_for_active_goal_pane();
        let Some(selection) = self.goal_workspace.selected_plan_item().cloned() else {
            return false;
        };
        match selection {
            crate::state::goal_workspace::GoalPlanSelection::PromptToggle
            | crate::state::goal_workspace::GoalPlanSelection::MainThread { .. } => false,
            crate::state::goal_workspace::GoalPlanSelection::Step { step_id } => {
                if self.goal_workspace.is_step_expanded(&step_id) {
                    self.goal_workspace.set_step_expanded(step_id, false);
                    self.sync_goal_workspace_selection_for_active_goal_pane();
                    true
                } else {
                    false
                }
            }
            crate::state::goal_workspace::GoalPlanSelection::Todo { step_id, .. } => self
                .select_goal_workspace_plan_item(
                    crate::state::goal_workspace::GoalPlanSelection::Step { step_id },
                ),
        }
    }

    pub(super) fn focus_next_goal_workspace_pane(&mut self) -> bool {
        if !matches!(
            self.main_pane_view,
            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
        ) || self.focus != FocusArea::Chat
        {
            return false;
        }

        match self.goal_workspace.focused_pane() {
            crate::state::goal_workspace::GoalWorkspacePane::CommandBar => false,
            crate::state::goal_workspace::GoalWorkspacePane::Plan => {
                self.goal_workspace
                    .set_focused_pane(crate::state::goal_workspace::GoalWorkspacePane::Timeline);
                true
            }
            crate::state::goal_workspace::GoalWorkspacePane::Timeline => {
                self.goal_workspace
                    .set_focused_pane(crate::state::goal_workspace::GoalWorkspacePane::Details);
                true
            }
            crate::state::goal_workspace::GoalWorkspacePane::Details => {
                self.goal_workspace
                    .set_focused_pane(crate::state::goal_workspace::GoalWorkspacePane::CommandBar);
                true
            }
        }
    }

    pub(super) fn focus_prev_goal_workspace_pane(&mut self) -> bool {
        if !matches!(
            self.main_pane_view,
            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
        ) || self.focus != FocusArea::Chat
        {
            return false;
        }

        match self.goal_workspace.focused_pane() {
            crate::state::goal_workspace::GoalWorkspacePane::Plan => {
                self.goal_workspace
                    .set_focused_pane(crate::state::goal_workspace::GoalWorkspacePane::CommandBar);
                true
            }
            crate::state::goal_workspace::GoalWorkspacePane::Timeline => {
                self.goal_workspace
                    .set_focused_pane(crate::state::goal_workspace::GoalWorkspacePane::Plan);
                true
            }
            crate::state::goal_workspace::GoalWorkspacePane::Details => {
                self.goal_workspace
                    .set_focused_pane(crate::state::goal_workspace::GoalWorkspacePane::Timeline);
                true
            }
            crate::state::goal_workspace::GoalWorkspacePane::CommandBar => {
                self.goal_workspace
                    .set_focused_pane(crate::state::goal_workspace::GoalWorkspacePane::Details);
                true
            }
        }
    }

    pub(super) fn set_goal_workspace_mode(
        &mut self,
        mode: crate::state::goal_workspace::GoalWorkspaceMode,
    ) -> bool {
        if !matches!(
            self.main_pane_view,
            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
        ) {
            return false;
        }
        let changed = self.goal_workspace.mode() != mode;
        self.goal_workspace.set_mode(mode);
        self.goal_workspace.set_selected_timeline_row(0);
        self.goal_workspace.set_selected_detail_row(0);
        self.goal_workspace.set_timeline_scroll(0);
        self.goal_workspace.set_detail_scroll(0);
        changed
    }

    pub(super) fn cycle_goal_workspace_mode(&mut self, delta: i32) -> bool {
        let modes = [
            crate::state::goal_workspace::GoalWorkspaceMode::Goal,
            crate::state::goal_workspace::GoalWorkspaceMode::Files,
            crate::state::goal_workspace::GoalWorkspaceMode::Progress,
            crate::state::goal_workspace::GoalWorkspaceMode::ActiveAgent,
            crate::state::goal_workspace::GoalWorkspaceMode::Threads,
            crate::state::goal_workspace::GoalWorkspaceMode::NeedsAttention,
        ];
        let current = modes
            .iter()
            .position(|mode| *mode == self.goal_workspace.mode())
            .unwrap_or(0);
        let next = if delta >= 0 {
            (current + delta as usize).min(modes.len() - 1)
        } else {
            current.saturating_sub((-delta) as usize)
        };
        self.set_goal_workspace_mode(modes[next])
    }

    pub(super) fn activate_goal_workspace_command_bar(&mut self) -> bool {
        if self.goal_workspace.focused_pane()
            != crate::state::goal_workspace::GoalWorkspacePane::CommandBar
        {
            return false;
        }
        self.goal_workspace
            .set_focused_pane(crate::state::goal_workspace::GoalWorkspacePane::Plan);
        true
    }

    pub(super) fn step_goal_workspace_timeline_selection(&mut self, delta: i32) -> bool {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return false;
        };
        let row_count = widgets::goal_workspace::timeline_row_count(
            &self.tasks,
            goal_run_id,
            &self.goal_workspace,
        );
        if row_count == 0 {
            self.goal_workspace.set_selected_timeline_row(0);
            return false;
        }
        let current = self
            .goal_workspace
            .selected_timeline_row()
            .min(row_count - 1);
        let next = if delta >= 0 {
            (current + delta as usize).min(row_count - 1)
        } else {
            current.saturating_sub((-delta) as usize)
        };
        let changed = next != current;
        self.goal_workspace.set_selected_timeline_row(next);
        self.clamp_goal_workspace_timeline_scroll_to_selection();
        changed
    }

    pub(super) fn selected_goal_workspace_timeline_target(
        &self,
    ) -> Option<crate::widgets::goal_workspace::GoalWorkspaceHitTarget> {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return None;
        };
        crate::widgets::goal_workspace::timeline_targets(
            &self.tasks,
            goal_run_id,
            &self.goal_workspace,
        )
        .into_iter()
        .find_map(|(index, target)| {
            (index == self.goal_workspace.selected_timeline_row()).then_some(target)
        })
    }

    pub(super) fn activate_goal_workspace_timeline_target(&mut self) -> bool {
        let Some(target) = self.selected_goal_workspace_timeline_target() else {
            return false;
        };
        match target {
            crate::widgets::goal_workspace::GoalWorkspaceHitTarget::DetailFile(path) => {
                self.open_file_preview_path(path.clone());
                self.status_line = path;
                true
            }
            crate::widgets::goal_workspace::GoalWorkspaceHitTarget::ThreadRow(thread_id) => {
                self.open_thread_conversation(thread_id);
                true
            }
            _ => false,
        }
    }

    pub(super) fn step_goal_workspace_detail_selection(&mut self, delta: i32) -> bool {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return false;
        };
        let row_count = widgets::goal_workspace::detail_target_count(
            &self.tasks,
            goal_run_id,
            &self.goal_workspace,
        );
        if row_count == 0 {
            self.goal_workspace.set_selected_detail_row(0);
            return false;
        }
        let current = self.goal_workspace.selected_detail_row().min(row_count - 1);
        let next = if delta >= 0 {
            (current + delta as usize).min(row_count - 1)
        } else {
            current.saturating_sub((-delta) as usize)
        };
        let changed = next != current;
        self.goal_workspace.set_selected_detail_row(next);
        self.clamp_goal_workspace_detail_scroll_to_selection();
        changed
    }

    pub(super) fn clamp_goal_workspace_timeline_scroll_to_selection(&mut self) {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return;
        };
        let area = self.pane_layout().chat;
        let viewport_height = widgets::goal_workspace::timeline_viewport_height(area);
        let viewport_width = {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(4),
                    Constraint::Min(1),
                    Constraint::Length(3),
                ])
                .split(area);
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(40),
                    Constraint::Percentage(32),
                    Constraint::Min(24),
                ])
                .split(layout[1]);
            Block::default()
                .borders(Borders::ALL)
                .inner(columns[1])
                .width as usize
        };
        if viewport_height == 0 {
            self.goal_workspace.set_timeline_scroll(0);
            return;
        }
        let max_scroll = widgets::goal_workspace::max_timeline_scroll(
            area,
            &self.tasks,
            goal_run_id,
            &self.goal_workspace,
        );
        let Some(selected_row) = widgets::goal_workspace::timeline_visual_row_for_selection(
            &self.tasks,
            goal_run_id,
            &self.goal_workspace,
            viewport_width,
        ) else {
            self.goal_workspace.set_timeline_scroll(0);
            return;
        };
        let current_scroll = self.goal_workspace.timeline_scroll().min(max_scroll);
        let next_scroll = if selected_row < current_scroll {
            selected_row
        } else if selected_row >= current_scroll.saturating_add(viewport_height) {
            selected_row
                .saturating_add(1)
                .saturating_sub(viewport_height)
        } else {
            current_scroll
        };
        self.goal_workspace
            .set_timeline_scroll(next_scroll.min(max_scroll));
    }

    pub(super) fn clamp_goal_workspace_detail_scroll_to_selection(&mut self) {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return;
        };
        let area = self.pane_layout().chat;
        let viewport_height = widgets::goal_workspace::detail_viewport_height(area);
        let viewport_width = {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(4),
                    Constraint::Min(1),
                    Constraint::Length(3),
                ])
                .split(area);
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(40),
                    Constraint::Percentage(32),
                    Constraint::Min(24),
                ])
                .split(layout[1]);
            Block::default()
                .borders(Borders::ALL)
                .inner(columns[2])
                .width as usize
        };
        if viewport_height == 0 {
            self.goal_workspace.set_detail_scroll(0);
            return;
        }
        let max_scroll = widgets::goal_workspace::max_detail_scroll(
            area,
            &self.tasks,
            goal_run_id,
            &self.goal_workspace,
        );
        let Some(selected_row) = widgets::goal_workspace::detail_visual_row_for_selection(
            &self.tasks,
            goal_run_id,
            &self.goal_workspace,
            viewport_width,
        ) else {
            self.goal_workspace.set_detail_scroll(0);
            return;
        };
        let current_scroll = self.goal_workspace.detail_scroll().min(max_scroll);
        let next_scroll = if selected_row < current_scroll {
            selected_row
        } else if selected_row >= current_scroll.saturating_add(viewport_height) {
            selected_row
                .saturating_add(1)
                .saturating_sub(viewport_height)
        } else {
            current_scroll
        };
        self.goal_workspace
            .set_detail_scroll(next_scroll.min(max_scroll));
    }

    pub(super) fn selected_goal_workspace_detail_target(
        &self,
    ) -> Option<crate::widgets::goal_workspace::GoalWorkspaceHitTarget> {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return None;
        };
        crate::widgets::goal_workspace::detail_targets(
            &self.tasks,
            goal_run_id,
            &self.goal_workspace,
        )
        .into_iter()
        .find_map(|(index, target)| {
            (index == self.goal_workspace.selected_detail_row()).then_some(target)
        })
    }

    pub(super) fn activate_goal_workspace_detail_target(&mut self) -> bool {
        let Some(target) = self.selected_goal_workspace_detail_target() else {
            return false;
        };
        match target {
            crate::widgets::goal_workspace::GoalWorkspaceHitTarget::DetailFile(path) => {
                let Some(run) = self.selected_goal_run() else {
                    return false;
                };
                let Some(thread_id) = run.thread_id.clone() else {
                    return false;
                };
                if self.chat.active_thread_id() != Some(thread_id.as_str()) {
                    self.chat
                        .reduce(chat::ChatAction::SelectThread(thread_id.clone()));
                }
                self.sidebar.reduce(sidebar::SidebarAction::SwitchTab(
                    sidebar::SidebarTab::Files,
                ));
                self.tasks.reduce(task::TaskAction::SelectWorkPath {
                    thread_id: thread_id.clone(),
                    path: Some(path.clone()),
                });
                if let Some(index) = self.filtered_sidebar_file_index(&path) {
                    let item_count = self.sidebar_item_count();
                    self.sidebar.select(index, item_count);
                }
                let status_line = path.clone();
                self.open_work_context_for_thread(
                    thread_id,
                    None,
                    None,
                    self.current_goal_return_target(),
                    status_line,
                );
                true
            }
            crate::widgets::goal_workspace::GoalWorkspaceHitTarget::DetailTask(task_id) => {
                self.open_sidebar_target(sidebar::SidebarItemTarget::Task { task_id });
                self.focus = FocusArea::Chat;
                true
            }
            crate::widgets::goal_workspace::GoalWorkspaceHitTarget::DetailThread(thread_id) => {
                self.open_thread_conversation(thread_id);
                true
            }
            crate::widgets::goal_workspace::GoalWorkspaceHitTarget::DetailAction(action) => {
                self.activate_goal_workspace_action(action)
            }
            crate::widgets::goal_workspace::GoalWorkspaceHitTarget::DetailTimelineDetails(_) => {
                self.status_line = "Timeline details are shown in the right pane".to_string();
                true
            }
            crate::widgets::goal_workspace::GoalWorkspaceHitTarget::FooterAction(_) => false,
            crate::widgets::goal_workspace::GoalWorkspaceHitTarget::DetailCheckpoint(_)
            | crate::widgets::goal_workspace::GoalWorkspaceHitTarget::ThreadRow(_)
            | crate::widgets::goal_workspace::GoalWorkspaceHitTarget::TimelineRow(_)
            | crate::widgets::goal_workspace::GoalWorkspaceHitTarget::PlanPromptToggle
            | crate::widgets::goal_workspace::GoalWorkspaceHitTarget::PlanMainThread(_)
            | crate::widgets::goal_workspace::GoalWorkspaceHitTarget::PlanStep(_)
            | crate::widgets::goal_workspace::GoalWorkspaceHitTarget::PlanTodo { .. }
            | crate::widgets::goal_workspace::GoalWorkspaceHitTarget::ModeTab(_) => false,
        }
    }

    pub(super) fn activate_goal_workspace_action(
        &mut self,
        action: crate::widgets::goal_workspace::GoalWorkspaceAction,
    ) -> bool {
        match action {
            crate::widgets::goal_workspace::GoalWorkspaceAction::ToggleGoalRun => {
                self.request_selected_goal_run_toggle_confirmation()
            }
            crate::widgets::goal_workspace::GoalWorkspaceAction::OpenActions => {
                self.open_goal_step_action_picker()
            }
            crate::widgets::goal_workspace::GoalWorkspaceAction::RetryStep => {
                self.request_selected_goal_step_retry_confirmation()
            }
            crate::widgets::goal_workspace::GoalWorkspaceAction::RerunFromStep => {
                self.request_selected_goal_step_rerun_confirmation()
            }
            crate::widgets::goal_workspace::GoalWorkspaceAction::RefreshGoal => {
                let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
                    &self.main_pane_view
                else {
                    return false;
                };
                self.request_authoritative_goal_run_refresh(goal_run_id.clone());
                self.status_line = "Refreshing goal metadata".to_string();
                true
            }
        }
    }

    pub(super) fn step_goal_sidebar_tab(&mut self, delta: i32) {
        if delta < 0 {
            self.goal_sidebar.cycle_tab_left();
        } else if delta > 0 {
            self.goal_sidebar.cycle_tab_right();
        }
        self.reconcile_goal_sidebar_selection_for_active_goal_pane();
    }

    pub(super) fn activate_goal_sidebar_tab(&mut self, tab: GoalSidebarTab) {
        while self.goal_sidebar.active_tab() != tab {
            match (self.goal_sidebar.active_tab(), tab) {
                (GoalSidebarTab::Steps, GoalSidebarTab::Checkpoints)
                | (GoalSidebarTab::Steps, GoalSidebarTab::Tasks)
                | (GoalSidebarTab::Steps, GoalSidebarTab::Files)
                | (GoalSidebarTab::Checkpoints, GoalSidebarTab::Tasks)
                | (GoalSidebarTab::Checkpoints, GoalSidebarTab::Files)
                | (GoalSidebarTab::Tasks, GoalSidebarTab::Files) => self.step_goal_sidebar_tab(1),
                _ => self.step_goal_sidebar_tab(-1),
            }
        }
    }

    pub(super) fn navigate_goal_sidebar(&mut self, delta: i32) {
        self.goal_sidebar
            .navigate(delta, self.goal_sidebar_item_count());
        self.sync_goal_sidebar_selection_anchor();
    }

    pub(super) fn select_goal_sidebar_row(&mut self, index: usize) {
        self.goal_sidebar
            .select_row(index, self.goal_sidebar_item_count());
        self.sync_goal_sidebar_selection_anchor();
    }

    fn active_goal_sidebar_item(&self) -> Option<GoalSidebarCommandItem> {
        let goal_run_id = self.active_goal_sidebar_goal_run()?;
        let run = self.tasks.goal_run_by_id(goal_run_id)?;
        let selected_row = self.goal_sidebar.selected_row();

        match self.goal_sidebar.active_tab() {
            GoalSidebarTab::Steps => {
                let mut steps = run.steps.clone();
                steps.sort_by_key(|step| step.order);
                let step = steps.get(selected_row)?;
                Some(GoalSidebarCommandItem::Step {
                    step_id: step.id.clone(),
                })
            }
            GoalSidebarTab::Checkpoints => {
                let checkpoint = self
                    .tasks
                    .checkpoints_for_goal_run(goal_run_id)
                    .get(selected_row)?;
                let step_id = checkpoint.step_index.and_then(|step_index| {
                    run.steps
                        .iter()
                        .find(|step| step.order as usize == step_index)
                        .map(|step| step.id.clone())
                });
                Some(GoalSidebarCommandItem::Checkpoint { step_id })
            }
            GoalSidebarTab::Tasks => {
                let tasks: Vec<_> = if !run.child_task_ids.is_empty() {
                    run.child_task_ids
                        .iter()
                        .filter_map(|task_id| self.tasks.task_by_id(task_id))
                        .collect()
                } else {
                    self.tasks
                        .tasks()
                        .iter()
                        .filter(|task| task.goal_run_id.as_deref() == Some(goal_run_id))
                        .collect()
                };
                let task = *tasks.get(selected_row)?;
                Some(GoalSidebarCommandItem::Task {
                    target: sidebar::SidebarItemTarget::Task {
                        task_id: task.id.clone(),
                    },
                })
            }
            GoalSidebarTab::Files => {
                let thread_id = run.thread_id.clone()?;
                let context = self.tasks.work_context_for_thread(&thread_id)?;
                let entry = context
                    .entries
                    .iter()
                    .filter(|entry| match entry.goal_run_id.as_deref() {
                        Some(entry_goal_run_id) => entry_goal_run_id == goal_run_id,
                        None => true,
                    })
                    .nth(selected_row)?;
                Some(GoalSidebarCommandItem::File {
                    thread_id,
                    path: entry.path.clone(),
                })
            }
        }
    }

    pub(super) fn handle_goal_sidebar_enter(&mut self) -> bool {
        let Some(item) = self.active_goal_sidebar_item() else {
            return false;
        };

        match item {
            GoalSidebarCommandItem::Step { step_id } => {
                if self.select_goal_step_in_active_run(step_id) {
                    self.focus = FocusArea::Chat;
                    return true;
                }
            }
            GoalSidebarCommandItem::Checkpoint { step_id } => {
                let Some(step_id) = step_id else {
                    self.status_line = "Checkpoint has no linked step".to_string();
                    return false;
                };
                if self.select_goal_step_in_active_run(step_id) {
                    self.focus = FocusArea::Chat;
                    return true;
                }
            }
            GoalSidebarCommandItem::Task { target } => {
                self.open_sidebar_target(target);
                self.focus = FocusArea::Chat;
                return true;
            }
            GoalSidebarCommandItem::File { thread_id, path } => {
                let status_line = path.clone();
                self.open_work_context_for_thread(
                    thread_id.clone(),
                    Some(path),
                    None,
                    self.current_goal_return_target(),
                    status_line,
                );
                return true;
            }
        }

        false
    }

    pub(super) fn resolve_target_agent_id(&self, agent_alias: &str) -> Option<String> {
        match agent_alias.trim().to_ascii_lowercase().as_str() {
            "" => None,
            "svarog" | "swarog" | "main" => Some(amux_protocol::AGENT_ID_SWAROG.to_string()),
            "rarog" | "concierge" => Some(amux_protocol::AGENT_ID_RAROG.to_string()),
            "weles" => Some("weles".to_string()),
            "swarozyc" | "radogost" | "domowoj" | "swietowit" | "perun" | "mokosh" | "dazhbog"
            | "rod" => Some(agent_alias.trim().to_ascii_lowercase()),
            _ => self.subagents.entries.iter().find_map(|entry| {
                if entry.id.eq_ignore_ascii_case(agent_alias)
                    || entry.name.eq_ignore_ascii_case(agent_alias)
                    || entry
                        .id
                        .strip_suffix("_builtin")
                        .is_some_and(|alias| alias.eq_ignore_ascii_case(agent_alias))
                {
                    Some(
                        entry
                            .id
                            .strip_suffix("_builtin")
                            .unwrap_or(entry.id.as_str())
                            .to_ascii_lowercase(),
                    )
                } else {
                    None
                }
            }),
        }
    }

    fn voice_lookup_string(raw: Option<&serde_json::Value>, path: &[&str]) -> Option<String> {
        raw.and_then(|value| {
            path.iter()
                .try_fold(value, |acc, key| acc.get(*key))
                .and_then(|value| value.as_str())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
    }

    fn voice_lookup_bool(raw: Option<&serde_json::Value>, path: &[&str]) -> Option<bool> {
        raw.and_then(|value| {
            path.iter()
                .try_fold(value, |acc, key| acc.get(*key))
                .and_then(|value| value.as_bool())
        })
    }

    fn voice_audio_string(
        raw: Option<&serde_json::Value>,
        flat_key: &str,
        nested_path: &[&str],
        fallback: &str,
    ) -> String {
        Self::voice_lookup_string(raw, nested_path)
            .or_else(|| Self::voice_lookup_string(raw, &[flat_key]))
            .or_else(|| {
                Self::voice_lookup_string(raw.and_then(|value| value.get("extra")), &[flat_key])
            })
            .unwrap_or_else(|| fallback.to_string())
    }

    fn voice_audio_bool(
        raw: Option<&serde_json::Value>,
        flat_key: &str,
        nested_path: &[&str],
        fallback: bool,
    ) -> bool {
        Self::voice_lookup_bool(raw, nested_path)
            .or_else(|| Self::voice_lookup_bool(raw, &[flat_key]))
            .or_else(|| {
                Self::voice_lookup_bool(raw.and_then(|value| value.get("extra")), &[flat_key])
            })
            .unwrap_or(fallback)
    }

    pub(super) fn toggle_voice_capture(&mut self) {
        if self.voice_recording {
            if let Some(path) = self.stop_voice_capture() {
                let raw = self.config.agent_config_raw.as_ref();
                let provider = Self::voice_audio_string(
                    raw,
                    "audio_stt_provider",
                    &["audio", "stt", "provider"],
                    amux_shared::providers::PROVIDER_ID_OPENAI,
                );
                let model = Self::voice_audio_string(
                    raw,
                    "audio_stt_model",
                    &["audio", "stt", "model"],
                    "whisper-1",
                );
                let language = Self::voice_lookup_string(raw, &["audio", "stt", "language"])
                    .or_else(|| Self::voice_lookup_string(raw, &["audio_stt_language"]))
                    .or_else(|| {
                        Self::voice_lookup_string(
                            raw.and_then(|value| value.get("extra")),
                            &["audio_stt_language"],
                        )
                    });
                let args_json = serde_json::json!({
                    "path": path,
                    "mime_type": "audio/wav",
                    "provider": provider,
                    "model": model,
                    "language": language,
                })
                .to_string();
                self.send_daemon_command(DaemonCommand::SpeechToText { args_json });
                self.status_line = "Transcribing voice capture...".to_string();
            }
            return;
        }

        let enabled = Self::voice_audio_bool(
            self.config.agent_config_raw.as_ref(),
            "audio_stt_enabled",
            &["audio", "stt", "enabled"],
            true,
        );
        if !enabled {
            self.status_line = "STT disabled in audio settings".to_string();
            return;
        }
        self.start_voice_capture();
    }

    pub(super) fn speak_latest_assistant_message(&mut self) {
        let Some(thread) = self.chat.active_thread() else {
            self.status_line = "Open a thread first".to_string();
            return;
        };

        let selected_index = self.chat.selected_message();
        let selected_content = selected_index
            .and_then(|idx| thread.messages.get(idx))
            .filter(|message| {
                message.role == chat::MessageRole::Assistant && !message.content.trim().is_empty()
            })
            .map(|message| message.content.clone());

        let content_to_speak = if let Some(content) = selected_content {
            content
        } else if selected_index.is_some() {
            self.status_line = "Selected message is not speakable assistant text".to_string();
            self.show_input_notice(
                "Select an assistant message to speak",
                InputNoticeKind::Warning,
                60,
                true,
            );
            return;
        } else {
            let Some(message) = thread.messages.iter().rev().find(|message| {
                message.role == chat::MessageRole::Assistant && !message.content.trim().is_empty()
            }) else {
                self.status_line = "No assistant message available to speak".to_string();
                return;
            };
            message.content.clone()
        };

        let enabled = Self::voice_audio_bool(
            self.config.agent_config_raw.as_ref(),
            "audio_tts_enabled",
            &["audio", "tts", "enabled"],
            true,
        );
        if !enabled {
            self.status_line = "TTS disabled in audio settings".to_string();
            return;
        }

        if let Some(mut child) = self.voice_player.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        let raw = self.config.agent_config_raw.as_ref();
        let provider = Self::voice_audio_string(
            raw,
            "audio_tts_provider",
            &["audio", "tts", "provider"],
            amux_shared::providers::PROVIDER_ID_OPENAI,
        );
        let model = Self::voice_audio_string(
            raw,
            "audio_tts_model",
            &["audio", "tts", "model"],
            "gpt-4o-mini-tts",
        );
        let voice =
            Self::voice_audio_string(raw, "audio_tts_voice", &["audio", "tts", "voice"], "alloy");
        let args_json = serde_json::json!({
            "input": content_to_speak,
            "provider": provider,
            "model": model,
            "voice": voice,
        })
        .to_string();
        self.send_daemon_command(DaemonCommand::TextToSpeech { args_json });
        self.status_line = "Synthesizing speech...".to_string();
        self.set_active_thread_activity("preparing speech");
    }

    pub(super) fn known_agent_directive_aliases(&self) -> Vec<String> {
        let mut aliases = vec![
            "main".to_string(),
            "svarog".to_string(),
            "swarog".to_string(),
            "weles".to_string(),
            "veles".to_string(),
            amux_protocol::AGENT_ID_RAROG.to_string(),
            "swarozyc".to_string(),
            "radogost".to_string(),
            "domowoj".to_string(),
            "swietowit".to_string(),
            "perun".to_string(),
            "mokosh".to_string(),
            "dazhbog".to_string(),
        ];
        for entry in &self.subagents.entries {
            aliases.push(entry.id.clone());
            aliases.push(entry.name.clone());
        }
        aliases.sort();
        aliases.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        aliases
    }

    pub(super) fn participant_display_name(&self, agent_alias: &str) -> String {
        if let Some(display_name) = builtin_participant_display_name(agent_alias) {
            return display_name;
        }
        if let Some(entry) = self.subagents.entries.iter().find(|entry| {
            entry.id.eq_ignore_ascii_case(agent_alias)
                || entry.name.eq_ignore_ascii_case(agent_alias)
        }) {
            return entry.name.clone();
        }
        agent_alias.to_string()
    }

    fn builtin_persona_configured(&self, agent_alias: &str) -> bool {
        let Some(raw) = self.config.agent_config_raw.as_ref() else {
            return false;
        };
        let key = match agent_alias.to_ascii_lowercase().as_str() {
            "swarozyc" => "swarozyc",
            "radogost" => "radogost",
            "domowoj" => "domowoj",
            "swietowit" => "swietowit",
            "perun" => "perun",
            "mokosh" => "mokosh",
            "dazhbog" => "dazhbog",
            _ => return true,
        };
        let Some(entry) = raw
            .get("builtin_sub_agents")
            .and_then(|value| value.get(key))
            .and_then(|value| value.as_object())
        else {
            return false;
        };
        let provider = entry
            .get("provider")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let model = entry
            .get("model")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        provider.is_some() && model.is_some()
    }

    fn open_builtin_persona_setup_flow(&mut self, agent_alias: &str, prompt: String) {
        let target_agent_id = agent_alias.trim().to_ascii_lowercase();
        let target_agent_name = self.participant_display_name(agent_alias);
        let config_snapshot = BuiltinPersonaSetupConfigSnapshot {
            provider: self.config.provider.clone(),
            base_url: self.config.base_url.clone(),
            model: self.config.model.clone(),
            custom_model_name: self.config.custom_model_name.clone(),
            api_key: self.config.api_key.clone(),
            assistant_id: self.config.assistant_id.clone(),
            auth_source: self.config.auth_source.clone(),
            api_transport: self.config.api_transport.clone(),
            custom_context_window_tokens: self.config.custom_context_window_tokens,
            context_window_tokens: self.config.context_window_tokens,
            fetched_models: self.config.fetched_models().to_vec(),
        };
        self.pending_builtin_persona_setup = Some(PendingBuiltinPersonaSetup {
            target_agent_id,
            target_agent_name: target_agent_name.clone(),
            prompt,
            config_snapshot,
        });
        self.settings_picker_target = Some(SettingsPickerTarget::BuiltinPersonaProvider);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
        self.modal.set_picker_item_count(
            widgets::provider_picker::available_provider_defs(&self.auth).len(),
        );
        self.status_line = format!("Configure {} provider", target_agent_name);
    }

    pub(super) fn restore_builtin_persona_setup_config_snapshot(&mut self) {
        let Some(setup) = self.pending_builtin_persona_setup.as_ref() else {
            return;
        };
        let snapshot = &setup.config_snapshot;
        self.config.provider = snapshot.provider.clone();
        self.config.base_url = snapshot.base_url.clone();
        self.config.model = snapshot.model.clone();
        self.config.custom_model_name = snapshot.custom_model_name.clone();
        self.config.api_key = snapshot.api_key.clone();
        self.config.assistant_id = snapshot.assistant_id.clone();
        self.config.auth_source = snapshot.auth_source.clone();
        self.config.api_transport = snapshot.api_transport.clone();
        self.config.custom_context_window_tokens = snapshot.custom_context_window_tokens;
        self.config.context_window_tokens = snapshot.context_window_tokens;
        self.config.reduce(config::ConfigAction::ModelsFetched(
            snapshot.fetched_models.clone(),
        ));
    }

    fn resolve_preview_path(path: &str) -> PathBuf {
        let raw = PathBuf::from(path);
        if raw.is_absolute() {
            raw
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(raw)
        }
    }

    fn find_repo_root(path: &Path) -> Option<PathBuf> {
        let mut current = path.parent().or_else(|| Some(path));
        while let Some(candidate) = current {
            if candidate.join(".git").exists() {
                return Some(candidate.to_path_buf());
            }
            current = candidate.parent();
        }
        None
    }

    pub(super) fn open_chat_tool_file_preview(&mut self, message_index: usize) {
        let Some(message) = self
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.get(message_index))
        else {
            return;
        };
        let Some(chip) = widgets::chat::tool_file_chip(message) else {
            return;
        };

        let resolved_path = Self::resolve_preview_path(&chip.path);
        let show_plain_preview = message.tool_output_preview_path.is_some()
            || matches!(
                chip.tool_name.as_str(),
                "read_file" | "read_skill" | "generate_image"
            );
        let repo_root = if show_plain_preview {
            None
        } else {
            Self::find_repo_root(&resolved_path)
        };
        let repo_relative_path = repo_root.as_ref().and_then(|root| {
            resolved_path
                .strip_prefix(root)
                .ok()
                .map(|path| path.to_string_lossy().to_string())
        });
        let target = ChatFilePreviewTarget {
            path: resolved_path.to_string_lossy().to_string(),
            repo_root: repo_root
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            repo_relative_path,
        };

        if let Some(repo_root) = target.repo_root.as_ref() {
            self.send_daemon_command(DaemonCommand::RequestGitDiff {
                repo_path: repo_root.clone(),
                file_path: target.repo_relative_path.clone(),
            });
        } else {
            self.send_daemon_command(DaemonCommand::RequestFilePreview {
                path: target.path.clone(),
                max_bytes: Some(65_536),
            });
        }

        let parent_thread_id = matches!(
            self.main_pane_view,
            MainPaneView::Conversation | MainPaneView::WorkContext
        )
        .then(|| self.chat.active_thread_id().map(str::to_string))
        .flatten();
        self.set_mission_control_return_targets(
            self.current_goal_return_target(),
            parent_thread_id,
        );
        self.main_pane_view = MainPaneView::FilePreview(target);
        self.task_view_scroll = 0;
        self.focus = FocusArea::Chat;
    }

    pub(super) fn open_chat_message_image_preview(&mut self, message_index: usize) {
        let Some(message) = self
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.get(message_index))
        else {
            return;
        };
        let Some(path) = widgets::chat::message_image_preview_path(message) else {
            return;
        };

        let target = ChatFilePreviewTarget {
            path,
            repo_root: None,
            repo_relative_path: None,
        };
        self.send_daemon_command(DaemonCommand::RequestFilePreview {
            path: target.path.clone(),
            max_bytes: Some(65_536),
        });
        let parent_thread_id = matches!(
            self.main_pane_view,
            MainPaneView::Conversation | MainPaneView::WorkContext
        )
        .then(|| self.chat.active_thread_id().map(str::to_string))
        .flatten();
        self.set_mission_control_return_targets(
            self.current_goal_return_target(),
            parent_thread_id,
        );
        self.main_pane_view = MainPaneView::FilePreview(target);
        self.task_view_scroll = 0;
        self.focus = FocusArea::Chat;
    }

    pub(super) fn open_file_preview_path(&mut self, path: String) {
        let target = ChatFilePreviewTarget {
            path,
            repo_root: None,
            repo_relative_path: None,
        };
        self.send_daemon_command(DaemonCommand::RequestFilePreview {
            path: target.path.clone(),
            max_bytes: Some(65_536),
        });
        let parent_thread_id = matches!(
            self.main_pane_view,
            MainPaneView::Conversation | MainPaneView::WorkContext
        )
        .then(|| self.chat.active_thread_id().map(str::to_string))
        .flatten();
        self.set_mission_control_return_targets(
            self.current_goal_return_target(),
            parent_thread_id,
        );
        self.main_pane_view = MainPaneView::FilePreview(target);
        self.task_view_scroll = 0;
        self.focus = FocusArea::Chat;
    }

    pub(super) fn filtered_goal_runs(&self) -> Vec<&task::GoalRun> {
        let query = self.modal.command_query().to_lowercase();
        self.tasks
            .goal_runs()
            .iter()
            .filter(|run| {
                query.is_empty()
                    || run.title.to_lowercase().contains(&query)
                    || run.goal.to_lowercase().contains(&query)
            })
            .collect()
    }

    pub(super) fn selected_thread_picker_thread(&self) -> Option<&chat::AgentThread> {
        let cursor = self.modal.picker_cursor();
        if cursor == 0 {
            return None;
        }
        widgets::thread_picker::filtered_threads(&self.chat, &self.modal, &self.subagents)
            .get(cursor - 1)
            .copied()
    }

    pub(super) fn selected_goal_picker_run(&self) -> Option<&task::GoalRun> {
        let cursor = self.modal.picker_cursor();
        if cursor == 0 {
            return None;
        }
        self.filtered_goal_runs().get(cursor - 1).copied()
    }

    pub(super) fn can_stop_selected_thread(&self) -> bool {
        self.selected_thread_picker_thread().is_some_and(|thread| {
            self.chat.active_thread_id() == Some(thread.id.as_str()) && self.assistant_busy()
        })
    }

    pub(super) fn can_resume_selected_thread(&self) -> bool {
        self.selected_thread_picker_thread().is_some_and(|thread| {
            thread
                .messages
                .iter()
                .rev()
                .find(|message| message.role == chat::MessageRole::Assistant)
                .is_some_and(|message| message.content.trim_end().ends_with("[stopped]"))
        })
    }

    pub(super) fn selected_thread_picker_confirm_action(&self) -> Option<PendingConfirmAction> {
        let thread = self.selected_thread_picker_thread()?;
        let title = widgets::thread_picker::thread_display_title(thread);
        if self.can_stop_selected_thread() {
            Some(PendingConfirmAction::StopThread {
                thread_id: thread.id.clone(),
                title,
            })
        } else if self.can_resume_selected_thread() {
            Some(PendingConfirmAction::ResumeThread {
                thread_id: thread.id.clone(),
                title,
            })
        } else {
            None
        }
    }

    pub(super) fn selected_goal_picker_toggle_action(&self) -> Option<PendingConfirmAction> {
        let run = self.selected_goal_picker_run()?;
        let title = run.title.clone();
        match run.status {
            Some(task::GoalRunStatus::Paused) => Some(PendingConfirmAction::ResumeGoalRun {
                goal_run_id: run.id.clone(),
                title,
            }),
            Some(task::GoalRunStatus::Queued)
            | Some(task::GoalRunStatus::Planning)
            | Some(task::GoalRunStatus::Running)
            | Some(task::GoalRunStatus::AwaitingApproval) => {
                Some(PendingConfirmAction::PauseGoalRun {
                    goal_run_id: run.id.clone(),
                    title,
                })
            }
            _ => None,
        }
    }

    pub(super) fn selected_goal_run(&self) -> Option<&task::GoalRun> {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return None;
        };
        self.tasks.goal_run_by_id(goal_run_id)
    }

    fn selected_goal_run_id(&self) -> Option<String> {
        self.selected_goal_run()
            .map(|run| run.id.clone())
            .or_else(|| self.goal_mission_control.runtime_goal_run_id.clone())
    }

    pub(super) fn open_mission_control_runtime_editor(&mut self) -> bool {
        if matches!(self.main_pane_view, MainPaneView::GoalComposer)
            && self.goal_mission_control.runtime_mode()
        {
            if let Some(run) = self.mission_control_goal_run().cloned() {
                let preserve_pending = self.goal_mission_control.pending_role_assignments.is_some();
                self.sync_goal_mission_control_from_run(&run, preserve_pending);
            }
        } else if !self.sync_goal_mission_control_from_selected_goal_run() {
            return false;
        }
        if let Some(target) = self.current_goal_target_for_mission_control() {
            self.set_mission_control_return_targets(Some(target), None);
        }
        self.main_pane_view = MainPaneView::GoalComposer;
        self.focus = FocusArea::Chat;
        self.task_view_scroll = 0;
        true
    }

    pub(super) fn cancel_goal_mission_control(&mut self) -> bool {
        if !matches!(self.main_pane_view, MainPaneView::GoalComposer) {
            return false;
        }

        let fallback_target =
            self.goal_mission_control
                .runtime_goal_run_id
                .as_ref()
                .map(|goal_run_id| sidebar::SidebarItemTarget::GoalRun {
                    goal_run_id: goal_run_id.clone(),
                    step_id: None,
                });
        let target = self
            .mission_control_source_goal_target()
            .or(fallback_target);

        if let Some(target) = target {
            self.open_sidebar_target(target);
            self.focus = FocusArea::Chat;
            self.status_line = "Closed Mission Control".to_string();
        } else {
            self.set_main_pane_conversation(FocusArea::Chat);
            self.status_line = "Cancelled new goal".to_string();
        }
        true
    }

    fn selected_runtime_assignment_preview(&self) -> Option<(usize, task::GoalAgentAssignment)> {
        let index = self.goal_mission_control.selected_runtime_assignment_index;
        self.goal_mission_control
            .selected_runtime_assignment()
            .cloned()
            .map(|assignment| (index, assignment))
    }

    pub(super) fn stage_mission_control_assignment_modal_edit(
        &mut self,
        field: goal_mission_control::RuntimeAssignmentEditField,
    ) -> bool {
        if matches!(self.main_pane_view, MainPaneView::GoalComposer) {
            if self
                .goal_mission_control
                .display_role_assignments()
                .is_empty()
            {
                return false;
            }
        } else if !self.open_mission_control_runtime_editor() {
            return false;
        }
        let Some((row_index, _)) = self.selected_runtime_assignment_preview() else {
            return false;
        };
        self.goal_mission_control
            .stage_runtime_edit(row_index, field);
        match field {
            goal_mission_control::RuntimeAssignmentEditField::Provider => {
                self.settings_picker_target = Some(SettingsPickerTarget::Provider);
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
                let item_count =
                    widgets::provider_picker::available_provider_defs(&self.auth).len();
                self.modal.set_picker_item_count(item_count);
            }
            goal_mission_control::RuntimeAssignmentEditField::Model => {
                if !self.open_mission_control_assignment_model_picker() {
                    self.goal_mission_control.clear_runtime_edit();
                    return false;
                }
            }
            goal_mission_control::RuntimeAssignmentEditField::ReasoningEffort => {
                self.settings_picker_target = Some(SettingsPickerTarget::SubAgentReasoningEffort);
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::EffortPicker));
                self.modal.set_picker_item_count(6);
            }
            goal_mission_control::RuntimeAssignmentEditField::Role => {
                self.settings_picker_target = Some(SettingsPickerTarget::SubAgentRole);
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::RolePicker));
                self.modal.set_picker_item_count(
                    crate::state::subagents::SUBAGENT_ROLE_PRESETS.len() + 1,
                );
            }
            goal_mission_control::RuntimeAssignmentEditField::Enabled
            | goal_mission_control::RuntimeAssignmentEditField::InheritFromMain => {}
        }
        true
    }

    pub(super) fn update_selected_runtime_assignment(
        &mut self,
        update: impl FnOnce(&mut task::GoalAgentAssignment),
    ) -> bool {
        if matches!(self.main_pane_view, MainPaneView::GoalComposer)
            && !self.goal_mission_control.runtime_mode()
        {
            let updated = self
                .goal_mission_control
                .update_selected_preflight_assignment(update);
            if updated {
                self.status_line = "Mission Control preflight roster updated".to_string();
            }
            return updated;
        }
        let Some(goal_run_id) = self.selected_goal_run_id() else {
            return false;
        };
        if !self.open_mission_control_runtime_editor() {
            return false;
        }
        let Some((row_index, mut assignment)) = self.selected_runtime_assignment_preview() else {
            return false;
        };
        update(&mut assignment);
        let apply_mode = if self
            .goal_mission_control
            .selected_assignment_matches_active_step()
        {
            self.goal_mission_control.stage_runtime_change(
                goal_run_id,
                row_index,
                assignment,
                goal_mission_control::RuntimeAssignmentApplyMode::NextTurn,
            );
            self.modal.reduce(modal::ModalAction::Push(
                modal::ModalKind::GoalStepActionPicker,
            ));
            self.modal.set_picker_item_count(3);
            self.status_line =
                "Choose how the active step should adopt the pending roster change".to_string();
            return true;
        } else {
            goal_mission_control::RuntimeAssignmentApplyMode::NextTurn
        };
        self.goal_mission_control.stage_runtime_change(
            goal_run_id,
            row_index,
            assignment,
            apply_mode,
        );
        self.goal_mission_control
            .apply_runtime_assignment_change(row_index, apply_mode);
        self.status_line = "Mission Control roster updated for the next turn".to_string();
        true
    }

    pub(super) fn cycle_selected_runtime_assignment(&mut self) -> bool {
        if !self.open_mission_control_runtime_editor() {
            return false;
        }
        if !self
            .goal_mission_control
            .cycle_selected_runtime_assignment(1)
        {
            return false;
        }
        let role_label = self
            .goal_mission_control
            .selected_runtime_row_label()
            .unwrap_or("runtime assignment");
        self.status_line = format!("Mission Control selected {role_label}");
        true
    }

    fn runtime_assignment_confirmation_items(&self) -> Vec<GoalActionPickerItem> {
        if self.goal_mission_control.pending_runtime_change.is_some() {
            vec![
                GoalActionPickerItem::ApplyRuntimeNextTurn,
                GoalActionPickerItem::ApplyRuntimeReassignActiveStep,
                GoalActionPickerItem::ApplyRuntimeRestartActiveStep,
            ]
        } else {
            Vec::new()
        }
    }

    pub(super) fn mission_control_role_picker_value(&self) -> String {
        self.goal_mission_control
            .pending_runtime_edit
            .as_ref()
            .filter(|edit| edit.field == goal_mission_control::RuntimeAssignmentEditField::Role)
            .and_then(|edit| {
                self.goal_mission_control
                    .display_role_assignments()
                    .get(edit.row_index)
            })
            .map(|assignment| assignment.role_id.clone())
            .or_else(|| {
                self.subagents
                    .editor
                    .as_ref()
                    .map(|editor| editor.role.clone())
            })
            .unwrap_or_default()
    }

    pub(super) fn mission_control_effort_picker_value(&self) -> Option<String> {
        self.goal_mission_control
            .pending_runtime_edit
            .as_ref()
            .filter(|edit| {
                edit.field == goal_mission_control::RuntimeAssignmentEditField::ReasoningEffort
            })
            .and_then(|edit| {
                self.goal_mission_control
                    .display_role_assignments()
                    .get(edit.row_index)
            })
            .and_then(|assignment| assignment.reasoning_effort.clone())
    }

    pub(super) fn runtime_model_picker_current_selection(
        &self,
    ) -> Option<(String, Option<String>)> {
        self.goal_mission_control
            .pending_runtime_edit
            .as_ref()
            .filter(|edit| edit.field == goal_mission_control::RuntimeAssignmentEditField::Model)
            .and_then(|edit| {
                self.goal_mission_control
                    .display_role_assignments()
                    .get(edit.row_index)
            })
            .map(|assignment| (assignment.model.clone(), None))
    }

    pub(super) fn open_mission_control_assignment_model_picker(&mut self) -> bool {
        let Some((_, assignment)) = self.selected_runtime_assignment_preview() else {
            return false;
        };
        let provider_id = assignment.provider.clone();
        let (base_url, api_key, auth_source) = self.provider_auth_snapshot(&provider_id);
        let models = providers::known_models_for_provider_auth(&provider_id, &auth_source);
        self.config
            .reduce(config::ConfigAction::ModelsFetched(models));
        if self.should_fetch_remote_models(&provider_id, &auth_source) {
            self.send_daemon_command(DaemonCommand::FetchModels {
                provider_id,
                base_url,
                api_key,
                output_modalities: None,
            });
        }
        self.settings_picker_target = Some(SettingsPickerTarget::Model);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
        self.sync_model_picker_item_count();
        true
    }

    pub(super) fn begin_mission_control_custom_model_edit(&mut self) {
        let Some((_, assignment)) = self.selected_runtime_assignment_preview() else {
            self.status_line = "Mission Control roster is unavailable".to_string();
            return;
        };
        if self.modal.top() != Some(modal::ModalKind::Settings) {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
        }
        self.settings
            .start_editing("mission_control_assignment_model", &assignment.model);
        self.status_line = "Enter mission control model ID".to_string();
    }

    pub(super) fn available_runtime_assignment_models(
        &self,
    ) -> Vec<crate::state::config::FetchedModel> {
        if let Some((current_model, custom_model_name)) =
            self.runtime_model_picker_current_selection()
        {
            widgets::model_picker::available_models_for(
                &self.config,
                &current_model,
                custom_model_name.as_deref(),
            )
        } else {
            Vec::new()
        }
    }

    pub(super) fn confirm_runtime_assignment_change(
        &mut self,
        apply_mode: goal_mission_control::RuntimeAssignmentApplyMode,
    ) -> bool {
        let Some(change) = self.goal_mission_control.pending_runtime_change.clone() else {
            return false;
        };
        self.goal_mission_control
            .apply_runtime_assignment_change(change.row_index, apply_mode);
        self.status_line = format!(
            "Mission Control roster updated: {}",
            apply_mode.roster_status_label()
        );
        true
    }

    pub(super) fn selected_goal_run_toggle_action(&self) -> Option<PendingConfirmAction> {
        let run = self.selected_goal_run()?;
        let title = run.title.clone();
        match run.status {
            Some(task::GoalRunStatus::Paused) => Some(PendingConfirmAction::ResumeGoalRun {
                goal_run_id: run.id.clone(),
                title,
            }),
            Some(task::GoalRunStatus::Queued)
            | Some(task::GoalRunStatus::Planning)
            | Some(task::GoalRunStatus::Running)
            | Some(task::GoalRunStatus::AwaitingApproval) => {
                Some(PendingConfirmAction::PauseGoalRun {
                    goal_run_id: run.id.clone(),
                    title,
                })
            }
            _ => None,
        }
    }

    pub(super) fn request_selected_goal_run_toggle_confirmation(&mut self) -> bool {
        let Some(action) = self.selected_goal_run_toggle_action() else {
            return false;
        };
        self.open_pending_action_confirm(action);
        true
    }

    pub(super) fn request_selected_goal_run_stop_confirmation(&mut self) -> bool {
        let Some(run) = self.selected_goal_run() else {
            return false;
        };
        if matches!(
            run.status,
            Some(task::GoalRunStatus::Completed)
                | Some(task::GoalRunStatus::Failed)
                | Some(task::GoalRunStatus::Cancelled)
        ) {
            return false;
        }
        self.open_pending_action_confirm(PendingConfirmAction::StopGoalRun {
            goal_run_id: run.id.clone(),
            title: run.title.clone(),
        });
        true
    }

    pub(super) fn request_preview_for_selected_path(&mut self, thread_id: &str) {
        let Some(context) = self.tasks.work_context_for_thread(thread_id) else {
            return;
        };
        let Some(selected_path) = self.tasks.selected_work_path(thread_id) else {
            return;
        };
        let Some(entry) = context
            .entries
            .iter()
            .find(|entry| entry.path == selected_path)
        else {
            return;
        };
        if let Some(repo_root) = entry.repo_root.as_deref() {
            self.send_daemon_command(DaemonCommand::RequestGitDiff {
                repo_path: repo_root.to_string(),
                file_path: Some(entry.path.clone()),
            });
        } else {
            self.send_daemon_command(DaemonCommand::RequestFilePreview {
                path: entry.path.clone(),
                max_bytes: Some(65_536),
            });
        }
    }

    pub(super) fn ensure_task_view_preview(&mut self) {
        let MainPaneView::Task(target) = &self.main_pane_view else {
            return;
        };
        let Some(thread_id) = self.target_thread_id(target) else {
            return;
        };
        if self.tasks.selected_work_path(&thread_id).is_none() {
            if let Some(context) = self.tasks.work_context_for_thread(&thread_id) {
                if let Some(first) = context.entries.first() {
                    self.tasks.reduce(task::TaskAction::SelectWorkPath {
                        thread_id: thread_id.clone(),
                        path: Some(first.path.clone()),
                    });
                }
            }
        }
        self.request_preview_for_selected_path(&thread_id);
    }

    fn request_task_view_context(&mut self, target: &sidebar::SidebarItemTarget) {
        if let Some(thread_id) = self.target_thread_id(target) {
            self.send_daemon_command(DaemonCommand::RequestThreadTodos(thread_id.clone()));
            self.send_daemon_command(DaemonCommand::RequestThreadWorkContext(thread_id));
        }
    }

    pub(super) fn current_sidebar_snapshot(
        &self,
    ) -> Option<&widgets::sidebar::CachedSidebarSnapshot> {
        let area = self.pane_layout().sidebar?;
        self.sidebar_snapshot.as_ref().filter(|snapshot| {
            widgets::sidebar::cached_snapshot_matches_render(
                snapshot,
                area,
                &self.chat,
                &self.sidebar,
                &self.tasks,
                self.chat.active_thread_id(),
            )
        })
    }

    pub(super) fn selected_sidebar_file_path(&self) -> Option<String> {
        self.current_sidebar_snapshot()
            .and_then(|snapshot| snapshot.selected_file_path(self.sidebar.selected_item()))
            .or_else(|| {
                widgets::sidebar::selected_file_path(
                    &self.tasks,
                    &self.sidebar,
                    self.chat.active_thread_id(),
                )
            })
    }

    pub(super) fn filtered_sidebar_file_index(&self, path: &str) -> Option<usize> {
        self.current_sidebar_snapshot()
            .and_then(|snapshot| snapshot.filtered_file_index(path))
            .or_else(|| {
                widgets::sidebar::filtered_file_index(
                    &self.tasks,
                    &self.sidebar,
                    self.chat.active_thread_id(),
                    path,
                )
            })
    }

    pub(super) fn selected_sidebar_spawned_thread_id(&self) -> Option<String> {
        self.current_sidebar_snapshot()
            .and_then(|snapshot| snapshot.selected_spawned_thread_id(self.sidebar.selected_item()))
            .or_else(|| {
                widgets::sidebar::selected_spawned_thread_id(
                    &self.tasks,
                    &self.sidebar,
                    self.chat.active_thread_id(),
                )
            })
    }

    pub(super) fn first_openable_sidebar_spawned_index(&self) -> Option<usize> {
        self.current_sidebar_snapshot()
            .and_then(widgets::sidebar::CachedSidebarSnapshot::first_openable_spawned_index)
            .or_else(|| {
                widgets::sidebar::first_openable_spawned_index(
                    &self.tasks,
                    self.chat.active_thread_id(),
                )
            })
    }

    pub(super) fn selected_sidebar_pinned_message(
        &self,
    ) -> Option<crate::state::chat::PinnedThreadMessage> {
        self.current_sidebar_snapshot()
            .and_then(|snapshot| {
                snapshot.selected_pinned_message(&self.chat, self.sidebar.selected_item())
            })
            .or_else(|| widgets::sidebar::selected_pinned_message(&self.chat, &self.sidebar))
    }

    pub(super) fn sidebar_item_count(&self) -> usize {
        self.current_sidebar_snapshot()
            .map(widgets::sidebar::CachedSidebarSnapshot::item_count)
            .unwrap_or_else(|| {
                widgets::sidebar::body_item_count(
                    &self.tasks,
                    &self.chat,
                    &self.sidebar,
                    self.chat.active_thread_id(),
                )
            })
    }

    pub(super) fn activate_sidebar_tab(&mut self, tab: sidebar::SidebarTab) {
        self.sidebar.reduce(sidebar::SidebarAction::SwitchTab(tab));
        if tab == sidebar::SidebarTab::Spawned {
            if let Some(index) = self.first_openable_sidebar_spawned_index() {
                self.sidebar.select(index, self.sidebar_item_count());
            }
        }
    }

    pub(super) fn open_selected_spawned_thread(&mut self) {
        let Some(from_thread_id) = self.chat.active_thread_id().map(str::to_string) else {
            return;
        };
        let Some(to_thread_id) = self.selected_sidebar_spawned_thread_id() else {
            return;
        };

        self.cleanup_concierge_on_navigate();
        self.clear_chat_drag_selection();
        self.clear_work_context_drag_selection();
        self.clear_task_view_drag_selection();
        self.pending_new_thread_target_agent = None;

        if !self
            .chat
            .open_spawned_thread(&from_thread_id, &to_thread_id)
        {
            return;
        }

        self.set_mission_control_return_targets(
            self.current_goal_return_target(),
            Some(from_thread_id),
        );
        self.request_latest_thread_page(to_thread_id.clone(), true);
        self.main_pane_view = MainPaneView::Conversation;
        self.task_view_scroll = 0;
        self.focus = FocusArea::Chat;
        self.status_line = format!("Opened spawned thread {to_thread_id}");
    }

    pub(super) fn go_back_thread(&mut self) {
        if !self.chat.can_go_back_thread() {
            self.status_line = "No previous thread".to_string();
            return;
        }

        self.cleanup_concierge_on_navigate();
        self.clear_chat_drag_selection();
        self.clear_work_context_drag_selection();
        self.clear_task_view_drag_selection();
        self.pending_new_thread_target_agent = None;

        let Some(thread_id) = self.chat.go_back_thread() else {
            self.status_line = "No previous thread".to_string();
            return;
        };

        self.set_mission_control_return_to_thread_id(
            self.chat.thread_history_stack().last().cloned(),
        );
        self.request_latest_thread_page(thread_id.clone(), true);
        self.main_pane_view = MainPaneView::Conversation;
        self.task_view_scroll = 0;
        self.focus = FocusArea::Chat;
        self.status_line = format!("Returned to {thread_id}");
    }

    pub(super) fn open_sidebar_target(&mut self, target: sidebar::SidebarItemTarget) {
        self.clear_mission_control_return_context();
        self.cleanup_concierge_on_navigate();
        self.clear_task_view_drag_selection();
        if let sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. } = &target {
            self.request_authoritative_goal_run_refresh(goal_run_id.clone());
            if self.tasks.goal_run_by_id(goal_run_id).is_some_and(|run| {
                matches!(
                    run.status,
                    Some(task::GoalRunStatus::Queued)
                        | Some(task::GoalRunStatus::Planning)
                        | Some(task::GoalRunStatus::Running)
                        | Some(task::GoalRunStatus::AwaitingApproval)
                )
            }) {
                self.schedule_goal_hydration_refresh(goal_run_id.clone());
            }
            self.goal_workspace.set_plan_scroll(0);
        }
        self.request_task_view_context(&target);
        self.main_pane_view = MainPaneView::Task(target);
        self.reconcile_goal_sidebar_selection_for_active_goal_pane();
        self.sync_goal_workspace_selection_for_active_goal_pane();
        self.task_view_scroll = 0;
        self.sync_contextual_approval_overlay();
    }

    pub(super) fn sync_thread_picker_item_count(&mut self) {
        let count =
            widgets::thread_picker::filtered_threads(&self.chat, &self.modal, &self.subagents)
                .len()
                + 1;
        self.modal.set_picker_item_count(count);
    }

    pub(super) fn sync_goal_picker_item_count(&mut self) {
        self.modal
            .set_picker_item_count(self.filtered_goal_runs().len() + 1);
    }

    pub(super) fn selected_goal_step_context(
        &self,
    ) -> Option<(String, String, usize, crate::state::task::GoalRunStep)> {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun {
            goal_run_id,
            step_id,
        }) = &self.main_pane_view
        else {
            return None;
        };
        let run = self.tasks.goal_run_by_id(goal_run_id)?;
        let step = if let Some(step_id) = step_id {
            run.steps.iter().find(|step| step.id == *step_id)?.clone()
        } else {
            run.steps
                .iter()
                .find(|step| {
                    step.order as usize == run.current_step_index
                        || Some(step.title.as_str()) == run.current_step_title.as_deref()
                })
                .or_else(|| run.steps.iter().min_by_key(|step| step.order))
                .cloned()?
        };
        Some((run.id.clone(), run.title.clone(), step.order as usize, step))
    }

    pub(super) fn select_goal_step_in_active_run(&mut self, step_id: String) -> bool {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return false;
        };
        let Some(run) = self.tasks.goal_run_by_id(goal_run_id) else {
            return false;
        };
        let Some(step) = run.steps.iter().find(|step| step.id == step_id) else {
            return false;
        };
        let step_title = step.title.clone();
        let step_order = step.order;

        self.main_pane_view = MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun {
            goal_run_id: goal_run_id.clone(),
            step_id: Some(step.id.clone()),
        });
        self.reconcile_goal_sidebar_selection_for_active_goal_pane();
        self.sync_goal_workspace_selection_for_active_goal_pane();
        self.status_line = format!("Selected step {}: {}", step_order + 1, step_title);
        true
    }

    pub(super) fn step_goal_step_selection(&mut self, delta: i32) -> bool {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun {
            goal_run_id,
            step_id,
        }) = &self.main_pane_view
        else {
            return false;
        };
        let Some(run) = self.tasks.goal_run_by_id(goal_run_id) else {
            return false;
        };
        let mut steps = run.steps.clone();
        steps.sort_by_key(|step| step.order);
        if steps.is_empty() {
            return false;
        }

        let current_index = step_id
            .as_ref()
            .and_then(|selected| steps.iter().position(|step| step.id == *selected))
            .or_else(|| {
                steps.iter().position(|step| {
                    step.order as usize == run.current_step_index
                        || Some(step.title.as_str()) == run.current_step_title.as_deref()
                })
            })
            .unwrap_or(0);
        let next_index = if delta > 0 {
            current_index
                .saturating_add(delta as usize)
                .min(steps.len().saturating_sub(1))
        } else {
            current_index.saturating_sub((-delta) as usize)
        };
        let next_step = &steps[next_index];
        let step_title = next_step.title.clone();
        let step_order = next_step.order;
        self.main_pane_view = MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun {
            goal_run_id: goal_run_id.clone(),
            step_id: Some(next_step.id.clone()),
        });
        self.reconcile_goal_sidebar_selection_for_active_goal_pane();
        self.status_line = format!("Selected step {}: {}", step_order + 1, step_title);
        true
    }

    pub(super) fn request_selected_goal_step_retry_confirmation(&mut self) -> bool {
        if let Some((goal_run_id, goal_title, step_index, step)) = self.selected_goal_step_context()
        {
            self.open_pending_action_confirm(PendingConfirmAction::RetryGoalStep {
                goal_run_id,
                goal_title,
                step_index,
                step_title: step.title,
            });
            return true;
        }

        let Some((goal_run_id, goal_title)) = self.selected_goal_prompt_context() else {
            return false;
        };
        self.open_pending_action_confirm(PendingConfirmAction::RetryGoalPrompt {
            goal_run_id,
            goal_title,
        });
        true
    }

    pub(super) fn request_selected_goal_step_rerun_confirmation(&mut self) -> bool {
        if let Some((goal_run_id, goal_title, step_index, step)) = self.selected_goal_step_context()
        {
            self.open_pending_action_confirm(PendingConfirmAction::RerunGoalFromStep {
                goal_run_id,
                goal_title,
                step_index,
                step_title: step.title,
            });
            return true;
        }

        let Some((goal_run_id, goal_title)) = self.selected_goal_prompt_context() else {
            return false;
        };
        self.open_pending_action_confirm(PendingConfirmAction::RerunGoalPrompt {
            goal_run_id,
            goal_title,
        });
        true
    }

    pub(super) fn goal_action_picker_items(&self) -> Vec<GoalActionPickerItem> {
        let confirmation_items = self.runtime_assignment_confirmation_items();
        if !confirmation_items.is_empty() {
            return confirmation_items;
        }

        let mut items = Vec::new();
        if let Some(run) = self.selected_goal_run() {
            match run.status {
                Some(task::GoalRunStatus::Paused) => items.push(GoalActionPickerItem::ResumeGoal),
                Some(task::GoalRunStatus::Queued)
                | Some(task::GoalRunStatus::Planning)
                | Some(task::GoalRunStatus::Running)
                | Some(task::GoalRunStatus::AwaitingApproval) => {
                    items.push(GoalActionPickerItem::PauseGoal);
                    items.push(GoalActionPickerItem::StopGoal);
                }
                _ => {}
            }
            if !run.runtime_assignment_list.is_empty() || !run.launch_assignment_snapshot.is_empty()
            {
                items.push(GoalActionPickerItem::CycleRuntimeAssignment);
                items.push(GoalActionPickerItem::EditRuntimeProvider);
                items.push(GoalActionPickerItem::EditRuntimeModel);
                items.push(GoalActionPickerItem::EditRuntimeReasoning);
                items.push(GoalActionPickerItem::EditRuntimeRole);
                items.push(GoalActionPickerItem::ToggleRuntimeEnabled);
                items.push(GoalActionPickerItem::ToggleRuntimeInherit);
            }
        }

        if self.selected_goal_step_context().is_some()
            || self.selected_goal_prompt_context().is_some()
        {
            items.push(GoalActionPickerItem::RetryStep);
            items.push(GoalActionPickerItem::RerunFromStep);
        }

        items
    }

    fn selected_goal_prompt_context(&self) -> Option<(String, String)> {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return None;
        };
        let run = self.tasks.goal_run_by_id(goal_run_id)?;
        run.steps
            .is_empty()
            .then(|| (run.id.clone(), run.title.clone()))
    }

    pub(super) fn open_goal_step_action_picker(&mut self) -> bool {
        let items = self.goal_action_picker_items();
        if items.is_empty() {
            return false;
        }
        self.modal.reduce(modal::ModalAction::Push(
            modal::ModalKind::GoalStepActionPicker,
        ));
        self.modal.set_picker_item_count(items.len());
        true
    }

    pub(crate) fn open_queued_prompts_modal(&mut self) {
        if self.queued_prompts.is_empty() {
            self.status_line = "No queued messages".to_string();
            return;
        }
        if self.modal.top() != Some(modal::ModalKind::QueuedPrompts) {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::QueuedPrompts));
        }
        self.modal.set_picker_item_count(self.queued_prompts.len());
        self.queued_prompt_action = QueuedPromptAction::SendNow;
    }

    fn open_queued_prompt_viewer(&mut self, index: usize) {
        let Some(prompt) = self.queued_prompts.get(index) else {
            return;
        };
        let body = format_queued_prompt_viewer_body(prompt);

        self.prompt_modal_loading = false;
        self.prompt_modal_error = None;
        self.prompt_modal_scroll = 0;
        self.prompt_modal_title_override = Some("QUEUED MESSAGE".to_string());
        self.prompt_modal_body_override = Some(body);

        if self.modal.top() != Some(modal::ModalKind::PromptViewer) {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::PromptViewer));
        }
    }

    fn queue_prompt(&mut self, prompt: String) {
        self.queued_prompts.push(QueuedPrompt::new(prompt));
        self.status_line = format!("QUEUED ({})", self.queued_prompts.len());
        self.sync_queued_prompt_modal_state();
    }

    pub(super) fn queue_participant_suggestion(
        &mut self,
        thread_id: String,
        suggestion_id: String,
        target_agent_id: String,
        target_agent_name: String,
        prompt: String,
        force_send: bool,
    ) {
        if let Some(existing) = self.queued_prompts.iter_mut().find(|queued| {
            queued.thread_id.as_deref() == Some(thread_id.as_str())
                && queued.suggestion_id.as_deref() == Some(suggestion_id.as_str())
        }) {
            existing.text = prompt;
            existing.participant_agent_id = Some(target_agent_id);
            existing.participant_agent_name = Some(target_agent_name);
            existing.force_send = force_send;
        } else {
            self.queued_prompts.push(QueuedPrompt::new_with_agent(
                prompt,
                thread_id,
                suggestion_id,
                target_agent_id,
                target_agent_name,
                force_send,
            ));
        }
        self.status_line = format!("QUEUED ({})", self.queued_prompts.len());
        self.sync_queued_prompt_modal_state();
    }

    fn remove_queued_prompt_at(&mut self, index: usize) -> Option<QueuedPrompt> {
        if index >= self.queued_prompts.len() {
            return None;
        }
        let prompt = self.queued_prompts.remove(index);
        self.sync_queued_prompt_modal_state();
        Some(prompt)
    }

    pub(super) fn dispatch_next_queued_prompt_if_ready(&mut self) {
        if self.queue_barrier_active() {
            return;
        }
        let Some(index) = self
            .queued_prompts
            .iter()
            .position(|prompt| prompt.suggestion_id.is_none())
        else {
            return;
        };
        if let Some(prompt) = self.remove_queued_prompt_at(index) {
            self.submit_prompt(prompt.text);
        }
    }

    pub(super) fn sync_participant_queued_prompts_for_thread(
        &mut self,
        thread_id: &str,
        live_suggestion_ids: &std::collections::HashSet<String>,
    ) {
        let before = self.queued_prompts.len();
        self.queued_prompts.retain(|prompt| {
            let Some(prompt_thread_id) = prompt.thread_id.as_deref() else {
                return true;
            };
            let Some(suggestion_id) = prompt.suggestion_id.as_deref() else {
                return true;
            };
            if prompt_thread_id != thread_id {
                return true;
            }
            live_suggestion_ids.contains(suggestion_id)
        });
        if self.queued_prompts.len() != before {
            self.sync_queued_prompt_modal_state();
        }
    }

    fn interrupt_current_stream(&mut self) {
        let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) else {
            return;
        };
        self.cancelled_thread_id = Some(thread_id.clone());
        self.chat.reduce(chat::ChatAction::ForceStopStreaming);
        self.clear_active_thread_activity();
        self.pending_stop = false;
        self.send_daemon_command(DaemonCommand::StopStream { thread_id });
    }

    pub(super) fn execute_selected_queued_prompt_action(&mut self) {
        let index = self.modal.picker_cursor();
        let action = self.queued_prompt_action;
        match action {
            QueuedPromptAction::Expand => self.open_queued_prompt_viewer(index),
            QueuedPromptAction::SendNow => {
                let Some(prompt) = self.remove_queued_prompt_at(index) else {
                    return;
                };
                let should_interrupt =
                    self.assistant_busy() && (prompt.suggestion_id.is_none() || prompt.force_send);
                if should_interrupt {
                    self.interrupt_current_stream();
                }
                if let (Some(thread_id), Some(suggestion_id)) =
                    (prompt.thread_id.clone(), prompt.suggestion_id.clone())
                {
                    self.send_daemon_command(DaemonCommand::SendParticipantSuggestion {
                        thread_id,
                        suggestion_id,
                    });
                } else {
                    self.submit_prompt(prompt.text);
                }
            }
            QueuedPromptAction::Copy => {
                let Some(prompt) = self.queued_prompts.get_mut(index) else {
                    return;
                };
                conversion::copy_to_clipboard(&prompt.text);
                prompt.mark_copied(self.tick_counter.saturating_add(100));
                self.status_line = "Copied queued message".to_string();
            }
            QueuedPromptAction::Delete => {
                if let Some(prompt) = self.remove_queued_prompt_at(index) {
                    if let (Some(thread_id), Some(suggestion_id)) =
                        (prompt.thread_id, prompt.suggestion_id)
                    {
                        self.send_daemon_command(DaemonCommand::DismissParticipantSuggestion {
                            thread_id,
                            suggestion_id,
                        });
                    }
                    self.status_line = "Removed queued message".to_string();
                }
            }
        }
    }

    pub(super) fn open_new_goal_view(&mut self) {
        let current_goal_target = self.current_goal_target_for_mission_control();
        self.set_mission_control_source_goal_target(current_goal_target.clone());
        self.clear_mission_control_return_context();
        self.cleanup_concierge_on_navigate();
        let fallback_profile = self.current_conversation_agent_profile();
        let fallback_main_assignment = task::GoalAgentAssignment {
            role_id: amux_protocol::AGENT_ID_SWAROG.to_string(),
            enabled: true,
            provider: fallback_profile.provider,
            model: fallback_profile.model,
            reasoning_effort: fallback_profile.reasoning_effort,
            inherit_from_main: false,
        };
        let preferred_goal_snapshot = current_goal_target
            .as_ref()
            .and_then(|target| target_goal_run_id(self, target))
            .and_then(|goal_run_id| self.tasks.goal_run_by_id(&goal_run_id).cloned())
            .and_then(|run| {
                if !run.launch_assignment_snapshot.is_empty() {
                    Some(run.launch_assignment_snapshot)
                } else if !run.runtime_assignment_list.is_empty() {
                    Some(run.runtime_assignment_list)
                } else {
                    None
                }
            });
        let latest_goal_snapshot = self
            .tasks
            .goal_runs()
            .iter()
            .max_by_key(|run| run.updated_at)
            .and_then(|run| {
                if !run.launch_assignment_snapshot.is_empty() {
                    Some(run.launch_assignment_snapshot.clone())
                } else if !run.runtime_assignment_list.is_empty() {
                    Some(run.runtime_assignment_list.clone())
                } else {
                    None
                }
            });
        self.goal_mission_control = match preferred_goal_snapshot.or(latest_goal_snapshot) {
            Some(snapshot) => goal_mission_control::GoalMissionControlState::from_goal_snapshot(
                snapshot,
                fallback_main_assignment,
                "Previous goal snapshot",
            ),
            None => goal_mission_control::GoalMissionControlState::from_main_assignment(
                fallback_main_assignment.clone(),
                vec![fallback_main_assignment],
                "Main agent inheritance",
            ),
        };
        self.goal_mission_control.set_prompt_text(String::new());
        self.goal_mission_control.set_save_as_default_pending(false);
        self.main_pane_view = MainPaneView::GoalComposer;
        self.task_view_scroll = 0;
        self.focus = FocusArea::Input;
        self.set_input_text("");
        self.attachments.clear();
        self.status_line = "Mission Control preflight is ready".to_string();
    }

    pub(super) fn open_mission_control_goal_thread(&mut self) -> bool {
        let Some((thread_id, used_root_fallback)) = self.mission_control_thread_target() else {
            self.status_line = if self.mission_control_source_goal_target().is_some() {
                "Mission Control source goal has no active or root thread".to_string()
            } else {
                "Mission Control has no source goal thread to open".to_string()
            };
            return false;
        };

        self.open_thread_conversation(thread_id.clone());
        self.status_line = if used_root_fallback {
            "Opened root goal thread as fallback because no active goal thread was available"
                .to_string()
        } else {
            format!("Opened active goal thread {thread_id}")
        };
        true
    }

    pub(super) fn return_to_goal_from_mission_control(&mut self) -> bool {
        let Some(target) = self.mission_control_return_to_goal_target() else {
            return false;
        };

        self.clear_mission_control_return_context();
        self.open_sidebar_target(target);
        self.focus = FocusArea::Chat;
        self.status_line = "Returned to goal".to_string();
        true
    }

    pub(super) fn return_from_mission_control_navigation(&mut self) -> bool {
        if let Some(thread_id) = self.mission_control_return_to_thread_id() {
            self.set_mission_control_return_to_thread_id(None);
            self.cleanup_concierge_on_navigate();
            self.clear_chat_drag_selection();
            self.clear_work_context_drag_selection();
            self.clear_task_view_drag_selection();
            self.pending_new_thread_target_agent = None;
            self.chat
                .reduce(chat::ChatAction::SelectThread(thread_id.clone()));
            self.request_latest_thread_page(thread_id.clone(), true);
            self.main_pane_view = MainPaneView::Conversation;
            self.task_view_scroll = 0;
            self.focus = FocusArea::Chat;
            self.status_line = format!("Returned to {thread_id}");
            return true;
        }

        self.return_to_goal_from_mission_control()
    }

    pub(super) fn start_goal_run_from_prompt(&mut self, goal: String) {
        self.goal_mission_control.set_prompt_text(goal);
        self.start_goal_run_from_mission_control();
    }

    fn consume_attachments_for_text_prompt(
        &mut self,
        prompt: String,
    ) -> (String, Vec<serde_json::Value>) {
        let drained_attachments = self.attachments.drain(..).collect::<Vec<_>>();
        let mut content_blocks = Vec::new();
        let content_with_attachments = if drained_attachments.is_empty() {
            prompt
        } else {
            let mut parts: Vec<String> = Vec::new();
            for att in drained_attachments {
                match att.payload {
                    AttachmentPayload::Text(content) => parts.push(format!(
                        "<attached_file name=\"{}\">\n{}\n</attached_file>",
                        att.filename, content
                    )),
                    AttachmentPayload::ContentBlock(block) => content_blocks.push(block),
                }
            }
            parts.push(prompt);
            parts.join("\n\n")
        };
        (content_with_attachments, content_blocks)
    }

    pub(super) fn start_goal_run_from_mission_control(&mut self) {
        if !self.connected {
            self.status_line = "Not connected to daemon".to_string();
            return;
        }
        let raw_goal = self.goal_mission_control.prompt_text().trim().to_string();
        if raw_goal.is_empty() {
            self.status_line = "Enter a goal before launching".to_string();
            return;
        }
        self.cleanup_concierge_on_navigate();
        let (goal_with_attachments, _content_blocks) =
            self.consume_attachments_for_text_prompt(raw_goal);
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let goal = input_refs::append_referenced_files_footer(&goal_with_attachments, &cwd);
        let launch_assignments = if self
            .goal_mission_control
            .display_role_assignments()
            .is_empty()
        {
            let fallback_profile = self.current_conversation_agent_profile();
            vec![task::GoalAgentAssignment {
                role_id: amux_protocol::AGENT_ID_SWAROG.to_string(),
                enabled: true,
                provider: fallback_profile.provider,
                model: fallback_profile.model,
                reasoning_effort: fallback_profile.reasoning_effort,
                inherit_from_main: false,
            }]
        } else {
            self.goal_mission_control
                .display_role_assignments()
                .to_vec()
        };
        self.send_daemon_command(DaemonCommand::StartGoalRun {
            goal,
            thread_id: None,
            session_id: None,
            launch_assignments,
        });
        self.status_line = "Starting goal run...".to_string();
    }

    pub(super) fn sync_goal_mission_control_prompt_from_input(&mut self) {
        if matches!(self.main_pane_view, MainPaneView::GoalComposer)
            && self.focus == FocusArea::Input
        {
            self.goal_mission_control
                .set_prompt_text(self.input.buffer().to_string());
        }
    }

    pub(super) fn is_builtin_command(&self, command: &str) -> bool {
        matches!(
            command,
            "provider"
                | "model"
                | "image"
                | "tools"
                | "effort"
                | "thread"
                | "new"
                | "goal"
                | "tasks"
                | "conversation"
                | "chat"
                | "settings"
                | "view"
                | "status"
                | "statistics"
                | "stats"
                | "notifications"
                | "approvals"
                | "participants"
                | "compact"
                | "quit"
                | "prompt"
                | "new-goal"
                | "attach"
                | "plugins install"
                | "skills install"
                | "help"
                | "explain"
                | "diverge"
        )
    }

    pub(super) fn execute_command(&mut self, command: &str) {
        tracing::info!("execute_command: {:?}", command);
        match command {
            "provider" => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
                self.modal.set_picker_item_count(
                    widgets::provider_picker::available_provider_defs(&self.auth).len(),
                );
            }
            "model" => {
                let target = self
                    .settings_picker_target
                    .unwrap_or(SettingsPickerTarget::Model);
                self.open_provider_backed_model_picker(
                    target,
                    self.config.provider.clone(),
                    self.config.base_url.clone(),
                    self.config.api_key.clone(),
                    self.config.auth_source.clone(),
                );
            }
            "image" => {
                self.input.set_text("/image ");
                self.focus = FocusArea::Input;
                self.status_line = "Describe the image and press Enter".to_string();
            }
            "tools" => {
                self.open_settings_tab(SettingsTab::Tools);
            }
            "effort" => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::EffortPicker));
                self.modal.set_picker_item_count(6);
            }
            "thread" => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
                self.sync_thread_picker_item_count();
            }
            "goal" => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
                self.sync_goal_picker_item_count();
            }
            "new" => {
                self.start_new_thread_view_for_agent(Some(amux_protocol::AGENT_ID_SWAROG));
            }
            "tasks" => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
                self.sync_goal_picker_item_count();
            }
            "conversation" | "chat" => {
                self.main_pane_view = MainPaneView::Conversation;
            }
            "settings" => {
                self.open_settings_tab(SettingsTab::Auth);
            }
            "view" => {
                let next = match self.chat.transcript_mode() {
                    chat::TranscriptMode::Compact => chat::TranscriptMode::Tools,
                    chat::TranscriptMode::Tools => chat::TranscriptMode::Full,
                    chat::TranscriptMode::Full => chat::TranscriptMode::Compact,
                };
                self.chat.reduce(chat::ChatAction::SetTranscriptMode(next));
                self.status_line = format!("View: {:?}", next);
            }
            "status" => {
                self.open_status_modal_loading();
                self.send_daemon_command(DaemonCommand::RequestAgentStatus);
                self.status_line = "Requesting tamux status...".to_string();
            }
            "statistics" | "stats" => {
                self.request_statistics_window(self.statistics_modal_window);
            }
            "notifications" => {
                self.toggle_notifications_modal();
                self.status_line = "Viewing notifications".to_string();
            }
            "approvals" => {
                self.toggle_approval_center();
                self.status_line = "Viewing approvals".to_string();
            }
            "participants" => {
                self.open_thread_participants_modal();
                self.status_line = "Viewing thread participants".to_string();
            }
            "compact" => {
                let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) else {
                    self.status_line = "Open a thread first, then run /compact".to_string();
                    return;
                };
                self.send_daemon_command(DaemonCommand::ForceCompact { thread_id });
                self.status_line = "Forcing compaction...".to_string();
            }
            "quit" => self.pending_quit = true,
            "prompt" => {
                self.request_prompt_inspection(None);
            }
            "new-goal" => {
                self.open_new_goal_view();
            }
            "attach" => {
                self.status_line =
                    "Usage: /attach <path>  — attach a file to the next message".to_string();
            }
            "plugins install" => {
                self.input.set_text("tamux install plugin ");
                self.focus = FocusArea::Input;
                self.status_line = "Edit the plugin source and run it in the terminal".to_string();
            }
            "skills install" => {
                self.input.set_text("tamux skill import ");
                self.focus = FocusArea::Input;
                self.status_line = "Edit the skill source and run it in the terminal".to_string();
            }
            "help" => {
                self.help_modal_scroll = 0;
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::Help));
                self.modal.set_picker_item_count(100);
            }
            "explain" => {
                let action_id = self
                    .tasks
                    .goal_runs()
                    .iter()
                    .max_by_key(|run| run.updated_at)
                    .map(|run| run.id.clone());
                if let Some(action_id) = action_id {
                    self.send_daemon_command(DaemonCommand::ExplainAction {
                        action_id,
                        step_index: None,
                    });
                    self.status_line = "Requesting explainability report...".to_string();
                } else {
                    self.status_line = "No goal run available to explain".to_string();
                }
            }
            "diverge" => {
                if let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) {
                    self.input.set_text(&format!(
                        "/diverge-start {thread_id} Compare two implementation approaches for the current task"
                    ));
                    self.focus = FocusArea::Input;
                    self.status_line = "Edit /diverge-start prompt and press Enter".to_string();
                } else {
                    self.status_line = "Open a thread first, then run /diverge".to_string();
                }
            }
            _ => {
                // Unrecognized commands — insert into input so user can add
                // context before sending to the agent (plugin commands, etc.)
                self.input.set_text(&format!("/{command} "));
                self.focus = FocusArea::Chat;
            }
        }
    }

    pub(super) fn submit_image_prompt(&mut self, prompt: String) {
        if !self.connected {
            self.status_line = "Not connected to daemon".to_string();
            return;
        }

        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            self.execute_command("image");
            return;
        }

        self.cleanup_concierge_on_navigate();
        self.attachments.clear();

        let args_json = serde_json::json!({
            "thread_id": self.chat.active_thread_id().map(str::to_string),
            "prompt": trimmed,
        })
        .to_string();
        self.send_daemon_command(DaemonCommand::GenerateImage { args_json });

        self.main_pane_view = MainPaneView::Conversation;
        self.focus = FocusArea::Chat;
        self.input.set_mode(input::InputMode::Insert);
        self.status_line = "Generating image...".to_string();
        self.error_active = false;
    }

    pub(super) fn submit_prompt(&mut self, prompt: String) {
        if !self.connected {
            self.status_line = "Not connected to daemon".to_string();
            return;
        }
        if self.should_queue_submitted_prompt() {
            self.queue_prompt(prompt);
            return;
        }

        self.cleanup_concierge_on_navigate();

        let (content_with_attachments, mut content_blocks) =
            self.consume_attachments_for_text_prompt(prompt.clone());
        if !content_blocks.is_empty() {
            content_blocks.insert(
                0,
                serde_json::json!({
                    "type": "text",
                    "text": content_with_attachments.clone(),
                }),
            );
        }
        let content_blocks_json = (!content_blocks.is_empty())
            .then(|| serde_json::to_string(&content_blocks).ok())
            .flatten();
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let known_agent_aliases = self.known_agent_directive_aliases();
        if let Some(directive) = input_refs::parse_leading_agent_directive(
            &content_with_attachments,
            &known_agent_aliases,
        ) {
            if matches!(
                directive.agent_alias.to_ascii_lowercase().as_str(),
                "swarozyc" | "radogost" | "domowoj" | "swietowit" | "perun" | "mokosh" | "dazhbog"
            ) && !self.builtin_persona_configured(&directive.agent_alias)
            {
                self.open_builtin_persona_setup_flow(
                    &directive.agent_alias,
                    content_with_attachments.clone(),
                );
                return;
            }
            let directive_content =
                input_refs::append_referenced_files_footer(&directive.body, &cwd);
            match directive.kind {
                input_refs::LeadingAgentDirectiveKind::InternalDelegate => {
                    if let Some(thread_id) = self.chat.active_thread_id().map(String::from) {
                        if self.restore_prompt_and_show_budget_exceeded_notice(&thread_id, &prompt)
                        {
                            return;
                        }
                    }
                    self.send_daemon_command(DaemonCommand::InternalDelegate {
                        thread_id: self.chat.active_thread_id().map(String::from),
                        target_agent_id: directive.agent_alias.clone(),
                        content: directive_content,
                        session_id: None,
                    });
                    self.main_pane_view = MainPaneView::Conversation;
                    self.focus = FocusArea::Chat;
                    self.input.set_mode(input::InputMode::Insert);
                    self.status_line = format!("Delegated internally to {}", directive.agent_alias);
                    self.clear_active_thread_activity();
                    self.error_active = false;
                    return;
                }
                input_refs::LeadingAgentDirectiveKind::ParticipantUpsert => {
                    let participant_name = self.participant_display_name(&directive.agent_alias);
                    let Some(thread_id) = self.chat.active_thread_id().map(String::from) else {
                        self.status_line =
                            "Participant commands require an active thread".to_string();
                        self.show_input_notice(
                            format!(
                                "Open a thread before adding {participant_name} as a participant"
                            ),
                            InputNoticeKind::Warning,
                            120,
                            false,
                        );
                        return;
                    };
                    if self.restore_prompt_and_show_budget_exceeded_notice(&thread_id, &prompt) {
                        return;
                    }
                    self.send_daemon_command(DaemonCommand::ThreadParticipantCommand {
                        thread_id,
                        target_agent_id: directive.agent_alias.clone(),
                        action: "upsert".to_string(),
                        instruction: Some(directive_content),
                        session_id: None,
                    });
                    self.main_pane_view = MainPaneView::Conversation;
                    self.focus = FocusArea::Chat;
                    self.input.set_mode(input::InputMode::Insert);
                    self.status_line = format!("Participant {} updated", directive.agent_alias);
                    self.show_input_notice(
                        format!("Participant {participant_name} updated for this thread"),
                        InputNoticeKind::Success,
                        120,
                        false,
                    );
                    self.clear_active_thread_activity();
                    self.error_active = false;
                    return;
                }
                input_refs::LeadingAgentDirectiveKind::ParticipantDeactivate => {
                    let participant_name = self.participant_display_name(&directive.agent_alias);
                    let Some(thread_id) = self.chat.active_thread_id().map(String::from) else {
                        self.status_line =
                            "Participant commands require an active thread".to_string();
                        self.show_input_notice(
                            format!(
                                "Open a thread before removing {participant_name} as a participant"
                            ),
                            InputNoticeKind::Warning,
                            120,
                            false,
                        );
                        return;
                    };
                    if self.restore_prompt_and_show_budget_exceeded_notice(&thread_id, &prompt) {
                        return;
                    }
                    self.send_daemon_command(DaemonCommand::ThreadParticipantCommand {
                        thread_id,
                        target_agent_id: directive.agent_alias.clone(),
                        action: "deactivate".to_string(),
                        instruction: None,
                        session_id: None,
                    });
                    self.main_pane_view = MainPaneView::Conversation;
                    self.focus = FocusArea::Chat;
                    self.input.set_mode(input::InputMode::Insert);
                    self.status_line = format!("Participant {} stopped", directive.agent_alias);
                    self.show_input_notice(
                        format!("Participant {participant_name} removed from this thread"),
                        InputNoticeKind::Success,
                        120,
                        false,
                    );
                    self.clear_active_thread_activity();
                    self.error_active = false;
                    return;
                }
            }
        }

        let final_content =
            input_refs::append_referenced_files_footer(&content_with_attachments, &cwd);

        let goal_target = self.current_goal_target_for_mission_control();
        let goal_thread_target = self.goal_prompt_thread_target();
        if goal_target.is_some() && goal_thread_target.is_none() {
            self.input.set_text(&prompt);
            self.status_line =
                "Goal input accepts only slash commands until an active goal thread is available"
                    .to_string();
            self.show_input_notice(
                "Goal input needs an active step thread before it can send a prompt".to_string(),
                InputNoticeKind::Warning,
                120,
                false,
            );
            return;
        }

        let thread_id = goal_thread_target
            .as_ref()
            .map(|(_, thread_id)| thread_id.clone())
            .or_else(|| self.chat.active_thread_id().map(String::from));
        if let Some(thread_id) = thread_id.as_deref() {
            if self.restore_prompt_and_show_budget_exceeded_notice(thread_id, &prompt) {
                return;
            }
        }
        let target_agent_id = if thread_id.is_none() {
            self.pending_new_thread_target_agent.clone()
        } else {
            None
        };
        let local_target_agent_name = target_agent_id
            .as_deref()
            .map(|agent_id| self.participant_display_name(agent_id));
        if thread_id.as_deref() == self.cancelled_thread_id.as_deref() {
            self.cancelled_thread_id = None;
        }
        if let Some((target, thread_id)) = &goal_thread_target {
            self.set_mission_control_return_targets(Some(target.clone()), None);
            if self.chat.active_thread_id() != Some(thread_id.as_str()) {
                self.chat
                    .reduce(chat::ChatAction::SelectThread(thread_id.clone()));
            }
        }
        if thread_id.is_none() {
            let local_thread_id = format!("local-{}", self.tick_counter);
            let local_title = if prompt.len() > 40 {
                format!("{}...", &prompt[..40])
            } else {
                prompt.clone()
            };
            self.chat.reduce(chat::ChatAction::ThreadCreated {
                thread_id: local_thread_id.clone(),
                title: local_title.clone(),
            });
            if let Some(agent_name) = local_target_agent_name {
                self.chat
                    .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
                        id: local_thread_id,
                        agent_name: Some(agent_name),
                        title: local_title,
                        ..Default::default()
                    }));
            }
        }

        let optimistic_thread_id = thread_id
            .clone()
            .or_else(|| self.chat.active_thread_id().map(String::from));

        if let Some(thread_id) = optimistic_thread_id.as_ref() {
            let active_thread_id = thread_id.clone();
            self.reduce_chat_for_thread(
                Some(active_thread_id.as_str()),
                chat::ChatAction::AppendMessage {
                    thread_id: active_thread_id.clone(),
                    message: chat::AgentMessage {
                        role: chat::MessageRole::User,
                        content: final_content.clone(),
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0),
                        ..Default::default()
                    },
                },
            );
        }

        self.send_daemon_command(DaemonCommand::SendMessage {
            thread_id: thread_id.clone(),
            content: final_content,
            content_blocks_json,
            session_id: None,
            target_agent_id,
        });

        self.main_pane_view = MainPaneView::Conversation;
        self.focus = FocusArea::Chat;
        self.input.set_mode(input::InputMode::Insert);
        self.status_line = "Prompt sent".to_string();
        let activity_thread_id = optimistic_thread_id;
        if let Some(thread_id) = activity_thread_id.as_ref() {
            self.mark_pending_prompt_response_thread(thread_id.clone());
        }
        self.set_agent_activity_for(activity_thread_id, "thinking");
        self.error_active = false;
    }

    pub(super) fn focus_next(&mut self) {
        if matches!(self.main_pane_view, MainPaneView::Collaboration) {
            match self.focus {
                FocusArea::Chat => match self.collaboration.focus() {
                    CollaborationPaneFocus::Navigator => self.collaboration.reduce(
                        CollaborationAction::SetFocus(CollaborationPaneFocus::Detail),
                    ),
                    CollaborationPaneFocus::Detail => self.focus = FocusArea::Input,
                },
                FocusArea::Input => {
                    self.focus = FocusArea::Chat;
                    self.collaboration.reduce(CollaborationAction::SetFocus(
                        CollaborationPaneFocus::Navigator,
                    ));
                }
                FocusArea::Sidebar => self.focus = FocusArea::Input,
            }
            self.input.set_mode(input::InputMode::Insert);
            return;
        }

        if self.focus_next_goal_workspace_pane() {
            self.input.set_mode(input::InputMode::Insert);
            return;
        }

        self.focus = if self.sidebar_visible() {
            match self.focus {
                FocusArea::Chat => FocusArea::Sidebar,
                FocusArea::Sidebar => FocusArea::Input,
                FocusArea::Input => FocusArea::Chat,
            }
        } else {
            match self.focus {
                FocusArea::Chat | FocusArea::Sidebar => FocusArea::Input,
                FocusArea::Input => FocusArea::Chat,
            }
        };
        self.input.set_mode(input::InputMode::Insert);
    }

    pub(super) fn focus_prev(&mut self) {
        if matches!(self.main_pane_view, MainPaneView::Collaboration) {
            match self.focus {
                FocusArea::Input => {
                    self.focus = FocusArea::Chat;
                    self.collaboration.reduce(CollaborationAction::SetFocus(
                        CollaborationPaneFocus::Detail,
                    ));
                }
                FocusArea::Chat => match self.collaboration.focus() {
                    CollaborationPaneFocus::Detail => self.collaboration.reduce(
                        CollaborationAction::SetFocus(CollaborationPaneFocus::Navigator),
                    ),
                    CollaborationPaneFocus::Navigator => self.focus = FocusArea::Input,
                },
                FocusArea::Sidebar => {
                    self.focus = FocusArea::Chat;
                    self.collaboration.reduce(CollaborationAction::SetFocus(
                        CollaborationPaneFocus::Navigator,
                    ));
                }
            }
            self.input.set_mode(input::InputMode::Insert);
            return;
        }

        if self.focus == FocusArea::Input
            && matches!(
                self.main_pane_view,
                MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
            )
        {
            self.focus = FocusArea::Chat;
            self.goal_workspace
                .set_focused_pane(crate::state::goal_workspace::GoalWorkspacePane::CommandBar);
            self.input.set_mode(input::InputMode::Insert);
            return;
        }
        if self.focus_prev_goal_workspace_pane() {
            self.input.set_mode(input::InputMode::Insert);
            return;
        }

        self.focus = if self.sidebar_visible() {
            match self.focus {
                FocusArea::Chat => FocusArea::Input,
                FocusArea::Sidebar => FocusArea::Chat,
                FocusArea::Input => FocusArea::Sidebar,
            }
        } else {
            match self.focus {
                FocusArea::Chat | FocusArea::Sidebar => FocusArea::Input,
                FocusArea::Input => FocusArea::Chat,
            }
        };
        self.input.set_mode(input::InputMode::Insert);
    }

    pub(super) fn handle_sidebar_enter(&mut self) {
        if self.sidebar_uses_goal_sidebar() {
            let _ = self.handle_goal_sidebar_enter();
            return;
        }

        let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) else {
            return;
        };

        if self.should_toggle_work_context_from_sidebar(&thread_id) {
            self.set_main_pane_conversation(FocusArea::Sidebar);
            self.status_line = "Closed preview".to_string();
            return;
        }

        match self.sidebar.active_tab() {
            sidebar::SidebarTab::Files => {
                let Some(path) = self.selected_sidebar_file_path() else {
                    return;
                };
                let status_line = path.clone();
                self.open_work_context_for_thread(
                    thread_id.clone(),
                    Some(path),
                    Some(thread_id),
                    self.current_goal_return_target(),
                    status_line,
                );
            }
            sidebar::SidebarTab::Todos => {
                self.open_work_context_for_thread(
                    thread_id.clone(),
                    None,
                    Some(thread_id),
                    self.current_goal_return_target(),
                    "Todo details".to_string(),
                );
            }
            sidebar::SidebarTab::Spawned => {
                self.open_selected_spawned_thread();
            }
            sidebar::SidebarTab::Pinned => {
                let Some(pinned_message) = self.selected_sidebar_pinned_message() else {
                    return;
                };
                if let Some(message_index) = self
                    .chat
                    .resolve_active_pinned_message_to_loaded_index(&pinned_message)
                {
                    self.main_pane_view = MainPaneView::Conversation;
                    self.focus = FocusArea::Chat;
                    self.chat.select_message(Some(message_index));
                    self.status_line = "Pinned message".to_string();
                    return;
                }

                let Some(thread) = self.chat.active_thread() else {
                    return;
                };
                let total_messages = thread.total_message_count.max(thread.loaded_message_end);
                let page_size = self.chat_history_page_size().max(1);
                let end = pinned_message
                    .absolute_index
                    .saturating_add(1)
                    .max(page_size)
                    .min(total_messages);
                let start = end.saturating_sub(page_size);
                let limit = end.saturating_sub(start).max(1);
                let offset = total_messages.saturating_sub(end);
                self.pending_pinned_jump = Some(PendingPinnedJump {
                    thread_id: thread_id.clone(),
                    message_id: pinned_message.message_id.clone(),
                    absolute_index: pinned_message.absolute_index,
                });
                self.request_thread_page(thread_id, limit, offset, false);
                self.status_line = "Loading pinned message".to_string();
            }
        }
    }

    pub(super) fn submit_selected_collaboration_vote(&mut self) {
        if let (Some(session), Some(disagreement), Some(position)) = (
            self.collaboration.selected_session(),
            self.collaboration.selected_disagreement(),
            self.collaboration.selected_position(),
        ) {
            if let Some(parent_task_id) = session.parent_task_id.clone() {
                self.send_daemon_command(DaemonCommand::VoteOnCollaborationDisagreement {
                    parent_task_id,
                    disagreement_id: disagreement.id.clone(),
                    task_id: "operator".to_string(),
                    position: position.to_string(),
                    confidence: Some(1.0),
                });
                self.status_line = format!("Casting vote: {position}");
            }
        }
    }

    pub(super) fn copy_message(&mut self, index: usize) {
        let Some(thread) = self.chat.active_thread() else {
            return;
        };
        let Some(message) = thread.messages.get(index) else {
            return;
        };
        let mut text = String::new();
        if let Some(reasoning) = message
            .reasoning
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            text.push_str("Reasoning:\n");
            text.push_str(reasoning);
            if !message.content.is_empty() {
                text.push_str("\n\n-------\n\n");
            }
        }
        if !message.content.is_empty() {
            if !text.is_empty() {
                text.push_str("Content:\n");
            }
            text.push_str(&message.content);
        }
        if text.trim().is_empty() {
            return;
        }
        conversion::copy_to_clipboard(&text);
        self.chat
            .mark_message_copied(index, self.tick_counter.saturating_add(100));
        self.status_line = "Copied to clipboard".to_string();
    }

    pub(super) fn copy_work_context_content(&mut self) {
        let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) else {
            return;
        };

        let text = match self.sidebar.active_tab() {
            sidebar::SidebarTab::Files => {
                let Some(path) = self.selected_sidebar_file_path() else {
                    return;
                };
                let Some(entry) = self
                    .tasks
                    .work_context_for_thread(&thread_id)
                    .and_then(|context| context.entries.iter().find(|entry| entry.path == path))
                else {
                    return;
                };
                if let Some(repo_root) = entry.repo_root.as_deref() {
                    self.tasks
                        .diff_for_path(repo_root, &entry.path)
                        .map(str::to_string)
                        .filter(|value| !value.trim().is_empty())
                } else {
                    self.tasks
                        .preview_for_path(&entry.path)
                        .filter(|preview| preview.is_text)
                        .map(|preview| preview.content.clone())
                        .filter(|value| !value.trim().is_empty())
                }
            }
            sidebar::SidebarTab::Todos => self
                .tasks
                .todos_for_thread(&thread_id)
                .get(self.sidebar.selected_item())
                .map(|todo| todo.content.clone())
                .filter(|value| !value.trim().is_empty()),
            sidebar::SidebarTab::Spawned => None,
            sidebar::SidebarTab::Pinned => self
                .selected_sidebar_pinned_message()
                .map(|message| message.content)
                .filter(|value| !value.trim().is_empty()),
        };

        if let Some(text) = text {
            conversion::copy_to_clipboard(&text);
            self.status_line = "Copied to clipboard".to_string();
        }
    }

    pub(super) fn resend_message(&mut self, index: usize) {
        let content = self
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.get(index))
            .map(|message| message.content.clone());
        if let Some(content) = content.filter(|value| !value.trim().is_empty()) {
            self.submit_prompt(content);
        }
    }

    pub(super) fn pin_message_for_compaction(&mut self, index: usize) {
        let (thread_id, message_id) = {
            let Some(thread) = self.chat.active_thread() else {
                return;
            };
            let Some(message) = thread.messages.get(index) else {
                return;
            };
            let Some(message_id) = message.id.clone().filter(|id| !id.is_empty()) else {
                self.status_line = "Cannot pin message without a daemon id".to_string();
                return;
            };
            (thread.id.clone(), message_id)
        };

        self.send_daemon_command(DaemonCommand::PinThreadMessageForCompaction {
            thread_id,
            message_id,
        });
    }

    pub(super) fn unpin_message_for_compaction(&mut self, index: usize) {
        let (thread_id, message_id) = {
            let Some(thread) = self.chat.active_thread() else {
                return;
            };
            let Some(message) = thread.messages.get(index) else {
                return;
            };
            let Some(message_id) = message.id.clone().filter(|id| !id.is_empty()) else {
                self.status_line = "Cannot unpin message without a daemon id".to_string();
                return;
            };
            (thread.id.clone(), message_id)
        };

        let absolute_index = self
            .chat
            .active_thread()
            .map(|thread| thread.loaded_message_start.saturating_add(index));
        self.unpin_message_for_compaction_by_id(thread_id, message_id, absolute_index);
    }

    fn unpin_message_for_compaction_by_id(
        &mut self,
        thread_id: String,
        message_id: String,
        absolute_index: Option<usize>,
    ) {
        self.send_daemon_command(DaemonCommand::UnpinThreadMessageForCompaction {
            thread_id: thread_id.clone(),
            message_id: message_id.clone(),
        });
        self.chat
            .reduce(chat::ChatAction::UnpinMessageForCompaction {
                thread_id,
                message_id,
                absolute_index,
            });
        if self.sidebar.active_tab() == sidebar::SidebarTab::Pinned
            && !self.chat.active_thread_has_pinned_messages()
        {
            self.sidebar.reduce(sidebar::SidebarAction::SwitchTab(
                sidebar::SidebarTab::Todos,
            ));
        }
    }

    pub(super) fn unpin_selected_sidebar_message(&mut self) {
        let Some(pinned_message) = self.selected_sidebar_pinned_message() else {
            return;
        };
        let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) else {
            return;
        };
        self.unpin_message_for_compaction_by_id(
            thread_id,
            pinned_message.message_id,
            Some(pinned_message.absolute_index),
        );
    }

    pub(super) fn delete_message(&mut self, index: usize) {
        let (thread_id, msg_id) = {
            let Some(thread) = self.chat.active_thread() else {
                return;
            };
            if index >= thread.messages.len() {
                return;
            }
            let mid = thread.messages[index]
                .id
                .clone()
                .unwrap_or_else(|| format!("{}:{}", thread.id, index));
            (thread.id.clone(), mid)
        };

        self.send_daemon_command(DaemonCommand::DeleteMessages {
            thread_id,
            message_ids: vec![msg_id],
        });

        // Remove locally.
        self.chat.delete_active_message(index);
        self.status_line = format!("Deleted message {}", index + 1);
    }

    pub(super) fn regenerate_from_message(&mut self, index: usize) {
        let prompt = self.chat.active_thread().and_then(|thread| {
            thread
                .messages
                .iter()
                .take(index)
                .rev()
                .find(|message| {
                    message.role == chat::MessageRole::User && !message.content.trim().is_empty()
                })
                .map(|message| message.content.clone())
        });
        if let Some(prompt) = prompt {
            self.submit_prompt(prompt);
        }
    }
}

fn builtin_participant_display_name(agent_alias: &str) -> Option<String> {
    let normalized = agent_alias.trim().to_ascii_lowercase();
    if normalized == amux_protocol::AGENT_ID_SWAROG {
        return Some("Swarog".to_string());
    }
    if normalized == amux_protocol::AGENT_ID_RAROG {
        return Some(amux_protocol::AGENT_NAME_RAROG.to_string());
    }
    let canonical = match normalized.as_str() {
        "veles" => "weles",
        "weles" | "swarozyc" | "radogost" | "domowoj" | "swietowit" | "perun" | "mokosh"
        | "dazhbog" => normalized.as_str(),
        _ => return None,
    };
    Some(ascii_title_case(canonical))
}

fn ascii_title_case(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = String::with_capacity(value.len());
    out.push(first.to_ascii_uppercase());
    out.push_str(chars.as_str());
    out
}

fn format_queued_prompt_viewer_body(prompt: &QueuedPrompt) -> String {
    let mut body = String::new();

    if let Some(agent_name) = prompt.participant_agent_name.as_deref() {
        body.push_str(&format!("Participant: {agent_name}\n"));
    }
    if let Some(agent_id) = prompt.participant_agent_id.as_deref() {
        body.push_str(&format!("Agent ID: {agent_id}\n"));
    }
    if let Some(thread_id) = prompt.thread_id.as_deref() {
        body.push_str(&format!("Thread ID: {thread_id}\n"));
    }
    if prompt.force_send {
        body.push_str("Dispatch: forced after interrupting the current stream\n");
    }
    if !body.is_empty() {
        body.push_str("\n--------------------\n\n");
    }

    body.push_str(prompt.text.trim_end());
    body
}
