use amux_protocol::{
    WorkspaceActor, WorkspaceNotice, WorkspaceOperator, WorkspacePriority,
    WorkspaceReviewSubmission, WorkspaceReviewVerdict, WorkspaceTask, WorkspaceTaskCreate,
    WorkspaceTaskMove, WorkspaceTaskStatus, WorkspaceTaskType, WorkspaceTaskUpdate,
};
use anyhow::{bail, Result};

use crate::cli::WorkspaceAction;
use crate::client;
use crate::commands::workspace_filters::{filter_workspace_tasks, WorkspaceListFilter};

pub(crate) async fn run(action: WorkspaceAction) -> Result<()> {
    match action {
        WorkspaceAction::List {
            workspace,
            include_deleted,
            status,
            priority,
            assignee,
            reviewer,
            json,
        } => {
            let tasks = client::send_workspace_task_list(workspace, include_deleted).await?;
            let filter = WorkspaceListFilter {
                status: status.as_deref().map(parse_status).transpose()?,
                priority: priority.as_deref().map(parse_priority).transpose()?,
                assignee: assignee.as_deref().map(parse_actor_field).transpose()?,
                reviewer: reviewer.as_deref().map(parse_actor_field).transpose()?,
            };
            let tasks = filter_workspace_tasks(tasks, &filter);
            println!("{}", format_workspace_task_list(&tasks, json)?);
        }
        WorkspaceAction::Get { task_id, json } => {
            let task = client::send_workspace_task_get(task_id).await?;
            println!("{}", format_workspace_task_detail(task.as_ref(), json)?);
        }
        WorkspaceAction::New {
            task_type,
            title,
            description,
            workspace,
            dod,
            priority,
            assignee,
            reviewer,
            json,
        } => {
            let task = client::send_workspace_task_create(WorkspaceTaskCreate {
                workspace_id: workspace,
                title,
                task_type: parse_task_type(&task_type)?,
                description,
                definition_of_done: dod,
                priority: Some(parse_priority(&priority)?),
                assignee: assignee
                    .as_deref()
                    .map(parse_actor_field)
                    .transpose()?
                    .flatten(),
                reviewer: parse_actor_field(&reviewer)?,
            })
            .await?;
            println!("{}", format_workspace_task_detail(Some(&task), json)?);
        }
        WorkspaceAction::Update {
            task_id,
            title,
            description,
            dod,
            clear_dod,
            priority,
            assignee,
            reviewer,
            json,
        } => {
            let parsed_assignee = assignee.as_deref().map(parse_actor_field).transpose()?;
            if matches!(parsed_assignee, Some(Some(WorkspaceActor::User))) {
                bail!("workspace assignee must be an agent or subagent");
            }
            let update = WorkspaceTaskUpdate {
                title,
                description,
                definition_of_done: if clear_dod { Some(None) } else { dod.map(Some) },
                priority: priority.as_deref().map(parse_priority).transpose()?,
                assignee: parsed_assignee,
                reviewer: reviewer.as_deref().map(parse_actor_field).transpose()?,
            };
            if update.title.is_none()
                && update.description.is_none()
                && update.definition_of_done.is_none()
                && update.priority.is_none()
                && update.assignee.is_none()
                && update.reviewer.is_none()
            {
                bail!("workspace update requires at least one field");
            }
            let task = client::send_workspace_task_update(task_id, update).await?;
            println!("{}", format_workspace_task_detail(task.as_ref(), json)?);
        }
        WorkspaceAction::Operator {
            workspace,
            operator,
            json,
        } => {
            let settings = if let Some(operator) = operator {
                client::send_workspace_operator(workspace, parse_operator(&operator)?).await?
            } else {
                client::send_workspace_settings(workspace).await?
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&settings)?);
            } else {
                println!(
                    "Workspace {} operator: {}",
                    settings.workspace_id,
                    operator_label(&settings.operator)
                );
            }
        }
        WorkspaceAction::Assign {
            task_id,
            assignee,
            json,
        } => {
            let assignee = parse_actor_field(&assignee)?;
            if matches!(assignee, Some(WorkspaceActor::User)) {
                bail!("workspace assignee must be an agent or subagent");
            }
            let task = client::send_workspace_task_update(
                task_id,
                WorkspaceTaskUpdate {
                    assignee: Some(assignee),
                    ..Default::default()
                },
            )
            .await?;
            println!("{}", format_workspace_task_detail(task.as_ref(), json)?);
        }
        WorkspaceAction::Reviewer {
            task_id,
            reviewer,
            json,
        } => {
            let task = client::send_workspace_task_update(
                task_id,
                WorkspaceTaskUpdate {
                    reviewer: Some(parse_actor_field(&reviewer)?),
                    ..Default::default()
                },
            )
            .await?;
            println!("{}", format_workspace_task_detail(task.as_ref(), json)?);
        }
        WorkspaceAction::Run { task_id, json } => {
            let task = client::send_workspace_task_run(task_id).await?;
            println!("{}", format_workspace_task_detail(Some(&task), json)?);
        }
        WorkspaceAction::Pause { task_id, json } => {
            let task = client::send_workspace_task_pause(task_id).await?;
            println!("{}", format_workspace_task_detail(task.as_ref(), json)?);
        }
        WorkspaceAction::Stop { task_id, json } => {
            let task = client::send_workspace_task_stop(task_id).await?;
            println!("{}", format_workspace_task_detail(task.as_ref(), json)?);
        }
        WorkspaceAction::Move {
            task_id,
            status,
            sort_order,
            json,
        } => {
            let task = client::send_workspace_task_move(WorkspaceTaskMove {
                task_id,
                status: parse_status(&status)?,
                sort_order,
            })
            .await?;
            println!("{}", format_workspace_task_detail(task.as_ref(), json)?);
        }
        WorkspaceAction::Review {
            task_id,
            verdict,
            message,
            json,
        } => {
            let task = client::send_workspace_review(WorkspaceReviewSubmission {
                task_id,
                verdict: parse_verdict(&verdict)?,
                message: (!message.is_empty()).then(|| message.join(" ")),
            })
            .await?;
            println!("{}", format_workspace_task_detail(task.as_ref(), json)?);
        }
        WorkspaceAction::Delete { task_id, yes, json } => {
            if !yes {
                use std::io::{self, Write};
                print!("Delete workspace task {task_id}? [y/N] ");
                io::stdout().flush()?;
                let mut answer = String::new();
                io::stdin().read_line(&mut answer)?;
                let normalized = answer.trim().to_ascii_lowercase();
                if normalized != "y" && normalized != "yes" {
                    println!("Aborted.");
                    return Ok(());
                }
            }
            let (deleted_task_id, deleted_at) =
                client::send_workspace_task_delete(task_id.clone()).await?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "task_id": if deleted_task_id.is_empty() { task_id } else { deleted_task_id },
                        "deleted_at": deleted_at,
                    }))?
                );
            } else if deleted_at.is_some() {
                println!("Deleted workspace task {}.", deleted_task_id);
            } else {
                println!("Workspace task {} was not found.", task_id);
            }
        }
        WorkspaceAction::Notices {
            workspace,
            task,
            json,
        } => {
            let notices = client::send_workspace_notice_list(workspace, task).await?;
            println!("{}", format_workspace_notice_list(&notices, json)?);
        }
    }
    Ok(())
}

pub(crate) fn format_workspace_task_list(tasks: &[WorkspaceTask], json: bool) -> Result<String> {
    if json {
        return serde_json::to_string_pretty(tasks).map_err(Into::into);
    }
    if tasks.is_empty() {
        return Ok("No workspace tasks found.".to_string());
    }
    let mut rendered = String::new();
    for (status, title) in [
        (WorkspaceTaskStatus::Todo, "TODO"),
        (WorkspaceTaskStatus::InProgress, "IN PROGRESS"),
        (WorkspaceTaskStatus::InReview, "IN REVIEW"),
        (WorkspaceTaskStatus::Done, "DONE"),
    ] {
        let column_tasks = tasks
            .iter()
            .filter(|task| task.status == status)
            .collect::<Vec<_>>();
        rendered.push_str(&format!("{title} ({})\n", column_tasks.len()));
        for task in column_tasks {
            rendered.push_str(&format!(
                "- {} {} {} ({}) assignee:{} reviewer:{}\n",
                task.id,
                priority_label(&task.priority),
                task.title,
                type_label(&task.task_type),
                actor_option_label(task.assignee.as_ref()),
                actor_option_label(task.reviewer.as_ref()),
            ));
        }
        rendered.push('\n');
    }
    Ok(rendered.trim_end().to_string())
}

pub(crate) fn format_workspace_task_detail(
    task: Option<&WorkspaceTask>,
    json: bool,
) -> Result<String> {
    if json {
        return serde_json::to_string_pretty(&task).map_err(Into::into);
    }
    let Some(task) = task else {
        return Ok("Workspace task not found.".to_string());
    };
    let mut rendered = String::new();
    rendered.push_str(&format!("ID:       {}\n", task.id));
    rendered.push_str(&format!("Title:    {}\n", task.title));
    rendered.push_str(&format!("Type:     {}\n", type_label(&task.task_type)));
    rendered.push_str(&format!("Status:   {}\n", status_label(&task.status)));
    rendered.push_str(&format!("Priority: {}\n", priority_label(&task.priority)));
    rendered.push_str(&format!(
        "Assignee: {}\n",
        actor_option_label(task.assignee.as_ref())
    ));
    rendered.push_str(&format!(
        "Reviewer: {}\n",
        actor_option_label(task.reviewer.as_ref())
    ));
    rendered.push_str(&format!("Reporter: {}\n", actor_label(&task.reporter)));
    rendered.push_str(&format!("Description:\n{}\n", task.description));
    if let Some(dod) = task.definition_of_done.as_deref() {
        rendered.push_str(&format!("Definition of done:\n{}\n", dod));
    }
    if let Some(thread_id) = task.thread_id.as_deref() {
        rendered.push_str(&format!("Thread:   {}\n", thread_id));
    }
    if let Some(goal_run_id) = task.goal_run_id.as_deref() {
        rendered.push_str(&format!("Goal:     {}\n", goal_run_id));
    }
    Ok(rendered.trim_end().to_string())
}

pub(crate) fn format_workspace_notice_list(
    notices: &[WorkspaceNotice],
    json: bool,
) -> Result<String> {
    if json {
        return serde_json::to_string_pretty(notices).map_err(Into::into);
    }
    if notices.is_empty() {
        return Ok("No workspace notices found.".to_string());
    }
    let mut rendered = String::new();
    for notice in notices {
        rendered.push_str(&format!(
            "{} [{}] {} task:{} actor:{}\n",
            notice.id,
            notice.notice_type,
            notice.message.replace('\n', " "),
            notice.task_id,
            actor_option_label(notice.actor.as_ref()),
        ));
    }
    Ok(rendered.trim_end().to_string())
}

fn parse_operator(raw: &str) -> Result<WorkspaceOperator> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "user" => Ok(WorkspaceOperator::User),
        "svarog" | "swarog" | "auto" => Ok(WorkspaceOperator::Svarog),
        other => bail!("unknown workspace operator: {other}"),
    }
}

fn parse_task_type(raw: &str) -> Result<WorkspaceTaskType> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "thread" => Ok(WorkspaceTaskType::Thread),
        "goal" => Ok(WorkspaceTaskType::Goal),
        other => bail!("unknown workspace task type: {other}"),
    }
}

fn parse_status(raw: &str) -> Result<WorkspaceTaskStatus> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "todo" | "to-do" => Ok(WorkspaceTaskStatus::Todo),
        "progress" | "in-progress" | "in_progress" | "inprogress" => {
            Ok(WorkspaceTaskStatus::InProgress)
        }
        "review" | "in-review" | "in_review" | "inreview" => Ok(WorkspaceTaskStatus::InReview),
        "done" => Ok(WorkspaceTaskStatus::Done),
        other => bail!("unknown workspace status: {other}"),
    }
}

fn parse_priority(raw: &str) -> Result<WorkspacePriority> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "low" => Ok(WorkspacePriority::Low),
        "normal" | "medium" => Ok(WorkspacePriority::Normal),
        "high" => Ok(WorkspacePriority::High),
        "urgent" => Ok(WorkspacePriority::Urgent),
        other => bail!("unknown workspace priority: {other}"),
    }
}

fn parse_actor_field(raw: &str) -> Result<Option<WorkspaceActor>> {
    if matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "none" | "clear" | "-"
    ) {
        return Ok(None);
    }
    parse_actor(raw).map(Some)
}

fn parse_actor(raw: &str) -> Result<WorkspaceActor> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("user") {
        return Ok(WorkspaceActor::User);
    }
    if raw.eq_ignore_ascii_case("svarog") || raw.eq_ignore_ascii_case("swarog") {
        return Ok(WorkspaceActor::Agent(
            amux_protocol::AGENT_ID_SWAROG.to_string(),
        ));
    }
    if let Some(id) = raw
        .strip_prefix("agent:")
        .or_else(|| raw.strip_prefix("agent/"))
    {
        let id = id.trim();
        if !id.is_empty() {
            return Ok(WorkspaceActor::Agent(id.to_string()));
        }
    }
    if let Some(id) = raw
        .strip_prefix("subagent:")
        .or_else(|| raw.strip_prefix("subagent/"))
        .or_else(|| raw.strip_prefix("sub:"))
    {
        let id = id.trim();
        if !id.is_empty() {
            return Ok(WorkspaceActor::Subagent(id.to_string()));
        }
    }
    if raw.is_empty() {
        bail!("workspace actor cannot be empty");
    }
    Ok(WorkspaceActor::Agent(raw.to_string()))
}

fn parse_verdict(raw: &str) -> Result<WorkspaceReviewVerdict> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "pass" | "passed" | "approve" | "approved" => Ok(WorkspaceReviewVerdict::Pass),
        "fail" | "failed" | "reject" | "rejected" => Ok(WorkspaceReviewVerdict::Fail),
        other => bail!("unknown workspace review verdict: {other}"),
    }
}

fn operator_label(operator: &WorkspaceOperator) -> &'static str {
    match operator {
        WorkspaceOperator::User => "user",
        WorkspaceOperator::Svarog => "svarog",
    }
}

fn type_label(task_type: &WorkspaceTaskType) -> &'static str {
    match task_type {
        WorkspaceTaskType::Thread => "thread",
        WorkspaceTaskType::Goal => "goal",
    }
}

fn status_label(status: &WorkspaceTaskStatus) -> &'static str {
    match status {
        WorkspaceTaskStatus::Todo => "todo",
        WorkspaceTaskStatus::InProgress => "in-progress",
        WorkspaceTaskStatus::InReview => "in-review",
        WorkspaceTaskStatus::Done => "done",
    }
}

fn priority_label(priority: &WorkspacePriority) -> &'static str {
    match priority {
        WorkspacePriority::Low => "low",
        WorkspacePriority::Normal => "normal",
        WorkspacePriority::High => "high",
        WorkspacePriority::Urgent => "urgent",
    }
}

fn actor_option_label(actor: Option<&WorkspaceActor>) -> String {
    actor.map(actor_label).unwrap_or_else(|| "none".to_string())
}

fn actor_label(actor: &WorkspaceActor) -> String {
    match actor {
        WorkspaceActor::User => "user".to_string(),
        WorkspaceActor::Agent(id) => format!("agent:{id}"),
        WorkspaceActor::Subagent(id) => format!("subagent:{id}"),
    }
}

#[cfg(test)]
#[path = "workspace_format_tests.rs"]
mod workspace_format_tests;
