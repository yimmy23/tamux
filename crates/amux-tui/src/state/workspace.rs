use std::collections::HashMap;

use amux_protocol::{
    WorkspaceActor, WorkspaceNotice, WorkspaceOperator, WorkspacePriority, WorkspaceSettings,
    WorkspaceTask, WorkspaceTaskStatus,
};

#[derive(Debug, Clone)]
pub struct WorkspaceColumn {
    pub status: WorkspaceTaskStatus,
    pub title: &'static str,
    pub tasks: Vec<WorkspaceTask>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceProjection {
    pub workspace_id: String,
    pub operator: WorkspaceOperator,
    pub filter_summary: Option<String>,
    pub columns: Vec<WorkspaceColumn>,
    pub notice_summaries: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceFilter {
    pub status: Option<WorkspaceTaskStatus>,
    pub priority: Option<WorkspacePriority>,
    pub assignee: Option<WorkspaceActor>,
    pub reviewer: Option<WorkspaceActor>,
    pub include_deleted: bool,
}

impl WorkspaceFilter {
    pub fn is_empty(&self) -> bool {
        self.status.is_none()
            && self.priority.is_none()
            && self.assignee.is_none()
            && self.reviewer.is_none()
            && !self.include_deleted
    }

    pub fn summary(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        if let Some(status) = &self.status {
            parts.push(format!("status:{status:?}"));
        }
        if let Some(priority) = &self.priority {
            parts.push(format!("priority:{priority:?}"));
        }
        if let Some(assignee) = &self.assignee {
            parts.push(format!("assignee:{assignee:?}"));
        }
        if let Some(reviewer) = &self.reviewer {
            parts.push(format!("reviewer:{reviewer:?}"));
        }
        if self.include_deleted {
            parts.push("deleted:shown".to_string());
        }
        Some(parts.join(" "))
    }
}

impl Default for WorkspaceFilter {
    fn default() -> Self {
        Self {
            status: None,
            priority: None,
            assignee: None,
            reviewer: None,
            include_deleted: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceState {
    workspace_id: String,
    settings: Option<WorkspaceSettings>,
    settings_list: Vec<WorkspaceSettings>,
    tasks: Vec<WorkspaceTask>,
    notices: Vec<WorkspaceNotice>,
    filter: WorkspaceFilter,
    projection: WorkspaceProjection,
}

impl WorkspaceState {
    pub fn new() -> Self {
        Self {
            workspace_id: "main".to_string(),
            settings: None,
            settings_list: Vec::new(),
            tasks: Vec::new(),
            notices: Vec::new(),
            filter: WorkspaceFilter::default(),
            projection: empty_projection("main", WorkspaceOperator::User),
        }
    }

    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    pub fn operator(&self) -> WorkspaceOperator {
        self.settings
            .as_ref()
            .map(|settings| settings.operator.clone())
            .unwrap_or(WorkspaceOperator::User)
    }

    pub fn projection(&self) -> &WorkspaceProjection {
        &self.projection
    }

    pub fn filter(&self) -> &WorkspaceFilter {
        &self.filter
    }

    pub fn set_filter(&mut self, filter: WorkspaceFilter) {
        self.filter = filter;
        self.rebuild_projection();
    }

    pub fn clear_filter(&mut self) {
        self.set_filter(WorkspaceFilter::default());
    }

    pub fn switch_workspace(&mut self, workspace_id: &str) {
        self.workspace_id = workspace_id.trim().to_string();
        self.settings = None;
        self.tasks.clear();
        self.notices.clear();
        self.rebuild_projection();
    }

    pub fn append_sort_order(&self, status: &WorkspaceTaskStatus) -> i64 {
        self.tasks
            .iter()
            .filter(|task| task.deleted_at.is_none())
            .filter(|task| &task.status == status)
            .map(|task| task.sort_order)
            .max()
            .unwrap_or(0)
            + 1
    }

    pub fn sort_order_for_drop(
        &self,
        dragged_task_id: &str,
        status: &WorkspaceTaskStatus,
        target_task_id: Option<&str>,
    ) -> i64 {
        let Some(target_task_id) = target_task_id.filter(|id| *id != dragged_task_id) else {
            return self.append_sort_order(status);
        };
        self.tasks
            .iter()
            .filter(|task| task.deleted_at.is_none())
            .find(|task| task.id == target_task_id && &task.status == status)
            .map(|task| task.sort_order)
            .unwrap_or_else(|| self.append_sort_order(status))
    }

    pub fn drop_targets_same_task(
        &self,
        dragged_task_id: &str,
        status: &WorkspaceTaskStatus,
        target_task_id: Option<&str>,
    ) -> bool {
        target_task_id == Some(dragged_task_id)
            && self
                .tasks
                .iter()
                .any(|task| task.id == dragged_task_id && &task.status == status)
    }

    pub fn drop_to_in_progress_run_blocked(
        &self,
        dragged_task_id: &str,
        start_status: Option<&WorkspaceTaskStatus>,
        drop_status: &WorkspaceTaskStatus,
    ) -> bool {
        if start_status != Some(&WorkspaceTaskStatus::Todo)
            || drop_status != &WorkspaceTaskStatus::InProgress
        {
            return false;
        }
        self.tasks
            .iter()
            .find(|task| task.id == dragged_task_id && task.deleted_at.is_none())
            .is_some_and(|task| task.assignee.is_none())
    }

    pub fn task_run_blocked(&self, task_id: &str) -> bool {
        self.tasks
            .iter()
            .find(|task| task.id == task_id && task.deleted_at.is_none())
            .is_some_and(|task| task.assignee.is_none())
    }

    pub fn set_settings(&mut self, settings: WorkspaceSettings) {
        self.workspace_id = settings.workspace_id.clone();
        upsert_settings(&mut self.settings_list, settings.clone());
        self.settings = Some(settings);
        self.rebuild_projection();
    }

    pub fn set_operator(&mut self, operator: WorkspaceOperator) {
        let mut settings = self.settings.clone().unwrap_or_else(|| WorkspaceSettings {
            workspace_id: self.workspace_id.clone(),
            workspace_root: None,
            operator: operator.clone(),
            created_at: 0,
            updated_at: 0,
        });
        settings.operator = operator;
        self.set_settings(settings);
    }

    pub fn set_settings_list(&mut self, mut settings: Vec<WorkspaceSettings>) {
        settings.sort_by(|left, right| left.workspace_id.cmp(&right.workspace_id));
        if !settings
            .iter()
            .any(|settings| settings.workspace_id == self.workspace_id)
        {
            settings.insert(
                0,
                WorkspaceSettings {
                    workspace_id: self.workspace_id.clone(),
                    workspace_root: None,
                    operator: self.operator(),
                    created_at: 0,
                    updated_at: 0,
                },
            );
        }
        self.settings_list = settings;
    }

    pub fn workspace_picker_items(&self, query: &str) -> Vec<&WorkspaceSettings> {
        let query = query.trim().to_ascii_lowercase();
        self.settings_list
            .iter()
            .filter(|settings| {
                if query.is_empty() {
                    return true;
                }
                settings.workspace_id.to_ascii_lowercase().contains(&query)
                    || settings
                        .workspace_root
                        .as_deref()
                        .is_some_and(|root| root.to_ascii_lowercase().contains(&query))
                    || format!("{:?}", settings.operator)
                        .to_ascii_lowercase()
                        .contains(&query)
            })
            .collect()
    }

    pub fn selected_workspace_id(&self, cursor: usize, query: &str) -> Option<String> {
        self.workspace_picker_items(query)
            .get(cursor)
            .map(|settings| settings.workspace_id.clone())
    }

    pub fn review_task_id_for(&self, task_id: &str) -> Option<String> {
        self.notices
            .iter()
            .filter(|notice| notice.task_id == task_id && notice.notice_type == "review_requested")
            .max_by_key(|notice| notice.created_at)
            .and_then(|notice| review_task_id_from_notice(&notice.message))
            .or_else(|| {
                self.tasks
                    .iter()
                    .find(|task| task.id == task_id && task.deleted_at.is_none())
                    .and_then(|task| {
                        task.runtime_history
                            .iter()
                            .filter(|entry| entry.source.as_deref() == Some("workspace_review"))
                            .filter_map(|entry| {
                                entry
                                    .agent_task_id
                                    .as_deref()
                                    .map(|id| (entry.archived_at, id.to_string()))
                            })
                            .max_by_key(|(created_at, _)| *created_at)
                            .map(|(_, id)| id)
                    })
            })
    }

    pub fn set_tasks(&mut self, workspace_id: String, tasks: Vec<WorkspaceTask>) {
        self.workspace_id = workspace_id;
        self.tasks = tasks;
        self.rebuild_projection();
    }

    pub fn upsert_task(&mut self, task: WorkspaceTask) {
        self.workspace_id = task.workspace_id.clone();
        if let Some(existing) = self
            .tasks
            .iter_mut()
            .find(|existing| existing.id == task.id)
        {
            *existing = task;
        } else {
            self.tasks.push(task);
        }
        self.rebuild_projection();
    }

    pub fn mark_deleted(&mut self, task_id: &str, deleted_at: Option<u64>) {
        if let Some(task) = self.tasks.iter_mut().find(|task| task.id == task_id) {
            task.deleted_at = deleted_at;
        }
        self.rebuild_projection();
    }

    pub fn set_notices(&mut self, notices: Vec<WorkspaceNotice>) {
        self.notices = notices;
        self.rebuild_projection();
    }

    pub fn upsert_notice(&mut self, notice: WorkspaceNotice) {
        if let Some(existing) = self
            .notices
            .iter_mut()
            .find(|existing| existing.id == notice.id)
        {
            *existing = notice;
        } else {
            self.notices.push(notice);
        }
        self.rebuild_projection();
    }

    pub fn task_by_id(&self, task_id: &str) -> Option<&WorkspaceTask> {
        self.tasks.iter().find(|task| task.id == task_id)
    }

    pub fn task_detail_body(&self, task_id: &str) -> Option<String> {
        let task = self.task_by_id(task_id)?;
        let mut body = format!(
            "{}\n\nType: {:?}\nStatus: {:?}\nPriority: {:?}\nReporter: {}\nAssignee: {}\nReviewer: {}\nThread: {}\nGoal: {}\n\nDescription: {}\nDefinition of done: {}\n",
            task.title,
            task.task_type,
            task.status,
            task.priority,
            actor_label(Some(&task.reporter)),
            actor_label(task.assignee.as_ref()),
            actor_label(task.reviewer.as_ref()),
            task.thread_id.as_deref().unwrap_or("none"),
            task.goal_run_id.as_deref().unwrap_or("none"),
            task.description,
            task.definition_of_done.as_deref().unwrap_or("Not provided"),
        );
        let mut notices = self
            .notices
            .iter()
            .filter(|notice| notice.task_id == task.id)
            .collect::<Vec<_>>();
        notices.sort_by_key(|notice| std::cmp::Reverse(notice.created_at));
        if notices.is_empty() {
            body.push_str("\nNotices:\nnone\n");
        } else {
            body.push_str("\nNotices:\n");
            for notice in notices.into_iter().take(5) {
                body.push_str(&format!("- {}: {}\n", notice.notice_type, notice.message));
            }
        }
        Some(body)
    }

    fn rebuild_projection(&mut self) {
        let operator = self.operator();
        let mut projection = empty_projection(&self.workspace_id, operator);
        projection.filter_summary = self.filter.summary();
        projection.notice_summaries = latest_notice_summaries(&self.notices);
        for task in self.tasks.iter().filter(|task| self.matches_filter(task)) {
            if let Some(column) = projection
                .columns
                .iter_mut()
                .find(|column| column.status == task.status)
            {
                column.tasks.push(task.clone());
            }
        }
        for column in &mut projection.columns {
            column
                .tasks
                .sort_by_key(|task| (task.sort_order, task.created_at));
        }
        self.projection = projection;
    }

    fn matches_filter(&self, task: &WorkspaceTask) -> bool {
        if !self.filter.include_deleted && task.deleted_at.is_some() {
            return false;
        }
        if self
            .filter
            .status
            .as_ref()
            .is_some_and(|status| status != &task.status)
        {
            return false;
        }
        if self
            .filter
            .priority
            .as_ref()
            .is_some_and(|priority| priority != &task.priority)
        {
            return false;
        }
        if self
            .filter
            .assignee
            .as_ref()
            .is_some_and(|assignee| task.assignee.as_ref() != Some(assignee))
        {
            return false;
        }
        if self
            .filter
            .reviewer
            .as_ref()
            .is_some_and(|reviewer| task.reviewer.as_ref() != Some(reviewer))
        {
            return false;
        }
        true
    }
}

fn upsert_settings(settings: &mut Vec<WorkspaceSettings>, next: WorkspaceSettings) {
    if let Some(existing) = settings
        .iter_mut()
        .find(|settings| settings.workspace_id == next.workspace_id)
    {
        *existing = next;
    } else {
        settings.push(next);
        settings.sort_by(|left, right| left.workspace_id.cmp(&right.workspace_id));
    }
}

fn review_task_id_from_notice(message: &str) -> Option<String> {
    let marker = "queued review task ";
    let (_, suffix) = message.rsplit_once(marker)?;
    suffix
        .split_whitespace()
        .next()
        .map(|value| value.trim_matches(|ch: char| ch == '.' || ch == ',' || ch == ';'))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn latest_notice_summaries(notices: &[WorkspaceNotice]) -> HashMap<String, String> {
    let mut latest: HashMap<String, &WorkspaceNotice> = HashMap::new();
    for notice in notices {
        let replace = latest
            .get(&notice.task_id)
            .is_none_or(|existing| notice.created_at >= existing.created_at);
        if replace {
            latest.insert(notice.task_id.clone(), notice);
        }
    }
    latest
        .into_iter()
        .map(|(task_id, notice)| (task_id, notice_summary(notice)))
        .collect()
}

fn notice_summary(notice: &WorkspaceNotice) -> String {
    format!("{}: {}", notice.notice_type, notice.message)
}

fn actor_label(actor: Option<&WorkspaceActor>) -> String {
    match actor {
        None => "none".to_string(),
        Some(WorkspaceActor::User) => "user".to_string(),
        Some(WorkspaceActor::Agent(id)) => format!("agent:{id}"),
        Some(WorkspaceActor::Subagent(id)) => format!("subagent:{id}"),
    }
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self::new()
    }
}

fn empty_projection(workspace_id: &str, operator: WorkspaceOperator) -> WorkspaceProjection {
    WorkspaceProjection {
        workspace_id: workspace_id.to_string(),
        operator,
        filter_summary: None,
        columns: vec![
            WorkspaceColumn {
                status: WorkspaceTaskStatus::Todo,
                title: "Todo",
                tasks: Vec::new(),
            },
            WorkspaceColumn {
                status: WorkspaceTaskStatus::InProgress,
                title: "In Progress",
                tasks: Vec::new(),
            },
            WorkspaceColumn {
                status: WorkspaceTaskStatus::InReview,
                title: "In Review",
                tasks: Vec::new(),
            },
            WorkspaceColumn {
                status: WorkspaceTaskStatus::Done,
                title: "Done",
                tasks: Vec::new(),
            },
        ],
        notice_summaries: HashMap::new(),
    }
}

#[cfg(test)]
mod tests;
