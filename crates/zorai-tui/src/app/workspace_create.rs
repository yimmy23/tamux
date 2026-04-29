use anyhow::{bail, Result};
use zorai_protocol::{WorkspaceActor, WorkspacePriority, WorkspaceTaskType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorkspaceCreateDraft {
    pub(super) task_type: WorkspaceTaskType,
    pub(super) title: String,
    pub(super) description: String,
    pub(super) definition_of_done: Option<String>,
    pub(super) priority: Option<WorkspacePriority>,
    pub(super) assignee: Option<WorkspaceActor>,
    pub(super) reviewer: Option<WorkspaceActor>,
}

pub(super) fn parse_workspace_create_args(args: &str) -> Result<WorkspaceCreateDraft> {
    let mut parts = args.trim().splitn(2, char::is_whitespace);
    let task_type = match parts.next().unwrap_or("").trim() {
        "thread" => WorkspaceTaskType::Thread,
        "goal" => WorkspaceTaskType::Goal,
        _ => bail!(usage()),
    };
    let rest = parts.next().unwrap_or("").trim();
    let Some((title, payload)) = rest.split_once("--") else {
        bail!(usage());
    };
    let title = title.trim();
    if title.is_empty() {
        bail!("Workspace task title and description are required");
    }

    let sections = split_sections(payload);
    let Some((_, description)) = sections.first() else {
        bail!("Workspace task title and description are required");
    };
    let description = description.trim();
    if description.is_empty() {
        bail!("Workspace task title and description are required");
    }

    let mut draft = WorkspaceCreateDraft {
        task_type,
        title: title.to_string(),
        description: description.to_string(),
        definition_of_done: None,
        priority: None,
        assignee: None,
        reviewer: Some(WorkspaceActor::User),
    };

    for (key, value) in sections.into_iter().skip(1) {
        apply_option(&mut draft, key, value)?;
    }

    Ok(draft)
}

fn usage() -> &'static str {
    "Usage: /new-workspace <thread|goal> <title> -- <description> [--priority low|normal|high|urgent] [--assignee svarog|agent:id|subagent:id|none] [--reviewer user|svarog|agent:id|subagent:id] [--dod text]"
}

fn split_sections(payload: &str) -> Vec<(&str, &str)> {
    let mut sections = Vec::new();
    sections.push(("", payload));
    let mut remaining = payload;
    while let Some(index) = remaining.find(" --") {
        let (_, after_marker) = remaining.split_at(index + 3);
        let key_end = after_marker
            .find(char::is_whitespace)
            .unwrap_or(after_marker.len());
        let key = after_marker[..key_end].trim();
        let value_start = key_end.min(after_marker.len());
        let after_value_start = after_marker[value_start..].trim_start();
        if key.is_empty() {
            break;
        }

        if sections.len() == 1 {
            sections[0].1 = remaining[..index].trim_end();
        } else if let Some(last) = sections.last_mut() {
            last.1 = remaining[..index].trim_end();
        }
        sections.push((key, after_value_start));
        remaining = after_value_start;
    }
    sections
}

fn apply_option(draft: &mut WorkspaceCreateDraft, key: &str, value: &str) -> Result<()> {
    let value = value.trim();
    match key.trim().to_ascii_lowercase().as_str() {
        "priority" => {
            draft.priority = Some(
                crate::app::commands::parse_workspace_priority(first_word(value))
                    .ok_or_else(|| anyhow::anyhow!("Unknown workspace priority: {value}"))?,
            );
        }
        "assignee" => {
            draft.assignee =
                crate::app::commands::parse_workspace_actor_field(first_word(value))
                    .ok_or_else(|| anyhow::anyhow!("Unknown workspace assignee: {value}"))?;
            if matches!(draft.assignee, Some(WorkspaceActor::User)) {
                bail!("Workspace assignee must be an agent or subagent");
            }
        }
        "reviewer" => {
            draft.reviewer =
                crate::app::commands::parse_workspace_actor_field(first_word(value))
                    .ok_or_else(|| anyhow::anyhow!("Unknown workspace reviewer: {value}"))?;
            if draft.reviewer.is_none() {
                bail!("Workspace reviewer is required");
            }
        }
        "dod" | "definition-of-done" | "definition_of_done" => {
            if value.is_empty() {
                bail!("Workspace definition of done cannot be empty");
            }
            draft.definition_of_done = Some(value.to_string());
        }
        other => bail!("Unknown workspace task option: {other}"),
    }
    Ok(())
}

fn first_word(value: &str) -> &str {
    value.split_whitespace().next().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use zorai_protocol::{WorkspaceActor, WorkspacePriority, WorkspaceTaskType};

    #[test]
    fn parses_workspace_create_optional_fields() {
        let draft = parse_workspace_create_args(
            "goal Ship board -- Build the workspace board --priority urgent --assignee svarog --reviewer subagent:qa --dod Tests pass",
        )
        .expect("draft");

        assert_eq!(draft.task_type, WorkspaceTaskType::Goal);
        assert_eq!(draft.title, "Ship board");
        assert_eq!(draft.description, "Build the workspace board");
        assert_eq!(draft.definition_of_done.as_deref(), Some("Tests pass"));
        assert_eq!(draft.priority, Some(WorkspacePriority::Urgent));
        assert_eq!(
            draft.assignee,
            Some(WorkspaceActor::Agent(
                zorai_protocol::AGENT_ID_SWAROG.to_string()
            ))
        );
        assert_eq!(
            draft.reviewer,
            Some(WorkspaceActor::Subagent("qa".to_string()))
        );
    }

    #[test]
    fn rejects_workspace_create_clear_reviewer() {
        let err = parse_workspace_create_args("thread Fix bug -- Repro and fix it --reviewer none")
            .unwrap_err();

        assert_eq!(err.to_string(), "Workspace reviewer is required");
    }
}
