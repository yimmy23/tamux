use amux_protocol::{WorkspaceActor, WorkspacePriority, WorkspaceTask, WorkspaceTaskStatus};

#[derive(Debug, Default)]
pub(crate) struct WorkspaceListFilter {
    pub(crate) status: Option<WorkspaceTaskStatus>,
    pub(crate) priority: Option<WorkspacePriority>,
    pub(crate) assignee: Option<Option<WorkspaceActor>>,
    pub(crate) reviewer: Option<Option<WorkspaceActor>>,
}

pub(crate) fn filter_workspace_tasks(
    tasks: Vec<WorkspaceTask>,
    filter: &WorkspaceListFilter,
) -> Vec<WorkspaceTask> {
    tasks
        .into_iter()
        .filter(|task| {
            filter
                .status
                .as_ref()
                .is_none_or(|status| &task.status == status)
        })
        .filter(|task| {
            filter
                .priority
                .as_ref()
                .is_none_or(|priority| &task.priority == priority)
        })
        .filter(|task| {
            filter
                .assignee
                .as_ref()
                .is_none_or(|assignee| task.assignee.as_ref() == assignee.as_ref())
        })
        .filter(|task| {
            filter
                .reviewer
                .as_ref()
                .is_none_or(|reviewer| task.reviewer.as_ref() == reviewer.as_ref())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use amux_protocol::WorkspaceTaskType;

    fn task(
        id: &str,
        status: WorkspaceTaskStatus,
        priority: WorkspacePriority,
        assignee: Option<WorkspaceActor>,
    ) -> WorkspaceTask {
        WorkspaceTask {
            id: id.to_string(),
            workspace_id: "main".to_string(),
            title: id.to_string(),
            task_type: WorkspaceTaskType::Thread,
            description: "Description".to_string(),
            definition_of_done: None,
            priority,
            status,
            sort_order: 0,
            reporter: WorkspaceActor::User,
            assignee,
            reviewer: Some(WorkspaceActor::User),
            thread_id: Some(format!("workspace-thread:{id}")),
            goal_run_id: None,
            runtime_history: Vec::new(),
            created_at: 1,
            updated_at: 1,
            started_at: None,
            completed_at: None,
            deleted_at: None,
            last_notice_id: None,
        }
    }

    #[test]
    fn workspace_task_filters_match_status_priority_and_assignee() {
        let tasks = vec![
            task(
                "keep",
                WorkspaceTaskStatus::InReview,
                WorkspacePriority::Urgent,
                Some(WorkspaceActor::Agent("swarog".to_string())),
            ),
            task(
                "drop-status",
                WorkspaceTaskStatus::Todo,
                WorkspacePriority::Urgent,
                Some(WorkspaceActor::Agent("swarog".to_string())),
            ),
            task(
                "drop-assignee",
                WorkspaceTaskStatus::InReview,
                WorkspacePriority::Urgent,
                None,
            ),
        ];

        let filtered = filter_workspace_tasks(
            tasks,
            &WorkspaceListFilter {
                status: Some(WorkspaceTaskStatus::InReview),
                priority: Some(WorkspacePriority::Urgent),
                assignee: Some(Some(WorkspaceActor::Agent("swarog".to_string()))),
                reviewer: None,
            },
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "keep");
    }
}
