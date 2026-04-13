use super::*;

impl TuiModel {
    pub(in super::super) fn target_thread_id(
        &self,
        target: &sidebar::SidebarItemTarget,
    ) -> Option<String> {
        match target {
            sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. } => self
                .tasks
                .goal_run_by_id(goal_run_id)
                .and_then(|run| run.thread_id.clone()),
            sidebar::SidebarItemTarget::Task { task_id } => {
                self.tasks.task_by_id(task_id).and_then(|task| {
                    task.thread_id.clone().or_else(|| {
                        task.goal_run_id
                            .as_deref()
                            .and_then(|goal_run_id| self.tasks.goal_run_by_id(goal_run_id))
                            .and_then(|run| run.thread_id.clone())
                    })
                })
            }
        }
        .or_else(|| self.chat.active_thread_id().map(str::to_string))
    }

    #[allow(dead_code)]
    fn preferred_task_target(&self) -> Option<sidebar::SidebarItemTarget> {
        if let MainPaneView::Task(target) = &self.main_pane_view {
            return Some(target.clone());
        }

        if let Some(active_thread_id) = self.chat.active_thread_id() {
            if let Some(run) = self
                .tasks
                .goal_runs()
                .iter()
                .filter(|run| run.thread_id.as_deref() == Some(active_thread_id))
                .max_by_key(|run| run.updated_at)
            {
                return Some(sidebar::SidebarItemTarget::GoalRun {
                    goal_run_id: run.id.clone(),
                    step_id: None,
                });
            }

            if let Some(task) = self
                .tasks
                .tasks()
                .iter()
                .rev()
                .find(|task| task.thread_id.as_deref() == Some(active_thread_id))
            {
                return Some(sidebar::SidebarItemTarget::Task {
                    task_id: task.id.clone(),
                });
            }
        }

        if let Some(run) = self
            .tasks
            .goal_runs()
            .iter()
            .max_by_key(|run| run.updated_at)
        {
            return Some(sidebar::SidebarItemTarget::GoalRun {
                goal_run_id: run.id.clone(),
                step_id: None,
            });
        }

        self.tasks
            .tasks()
            .last()
            .map(|task| sidebar::SidebarItemTarget::Task {
                task_id: task.id.clone(),
            })
    }

    #[allow(dead_code)]
    pub(in super::super) fn open_goal_runner_view(
        &mut self,
        target: Option<sidebar::SidebarItemTarget>,
    ) -> bool {
        let Some(target) = target.or_else(|| self.preferred_task_target()) else {
            self.status_line = "No goal/task activity yet".to_string();
            return false;
        };
        self.open_sidebar_target(target);
        true
    }
}
