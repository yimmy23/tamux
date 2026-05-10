use std::collections::HashMap;

use zorai_protocol::{
    WorkspaceActor, WorkspaceNotice, WorkspaceOperator, WorkspaceSettings, WorkspaceTaskStatus,
};

use super::{WorkspaceColumn, WorkspaceProjection, WorkspaceState};

pub(super) fn upsert_settings(settings: &mut Vec<WorkspaceSettings>, next: WorkspaceSettings) {
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

pub(super) fn push_unique_id(ids: &mut Vec<String>, id: &str) {
    if !id.is_empty() && !ids.iter().any(|candidate| candidate == id) {
        ids.push(id.to_string());
    }
}

pub(super) fn review_task_id_from_notice(message: &str) -> Option<String> {
    let marker = "queued review task ";
    let (_, suffix) = message.rsplit_once(marker)?;
    suffix
        .split_whitespace()
        .next()
        .map(|value| value.trim_matches(|ch: char| ch == '.' || ch == ',' || ch == ';'))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn latest_notice_summaries(notices: &[WorkspaceNotice]) -> HashMap<String, String> {
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

pub(super) fn notice_summary(notice: &WorkspaceNotice) -> String {
    format!("{}: {}", notice.notice_type, notice.message)
}

pub(super) fn actor_label(actor: Option<&WorkspaceActor>) -> String {
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

pub(super) fn empty_projection(
    workspace_id: &str,
    operator: WorkspaceOperator,
) -> WorkspaceProjection {
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
