use anyhow::{bail, Result};
use zorai_protocol::{WorkspaceActor, WorkspaceTaskUpdate};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorkspaceUpdateDraft {
    pub(super) task_id: String,
    pub(super) update: WorkspaceTaskUpdate,
}

pub(super) fn parse_workspace_update_args(args: &str) -> Result<WorkspaceUpdateDraft> {
    let mut parts = args.trim().splitn(2, char::is_whitespace);
    let task_id = parts.next().unwrap_or("").trim();
    if task_id.is_empty() {
        bail!(usage());
    }
    let payload = parts.next().unwrap_or("").trim();
    let mut update = WorkspaceTaskUpdate::default();
    for (key, value) in split_options(payload) {
        apply_option(&mut update, key, value)?;
    }
    if update.title.is_none()
        && update.description.is_none()
        && update.definition_of_done.is_none()
        && update.priority.is_none()
        && update.assignee.is_none()
        && update.reviewer.is_none()
    {
        bail!("Workspace update requires at least one field");
    }
    Ok(WorkspaceUpdateDraft {
        task_id: task_id.to_string(),
        update,
    })
}

fn usage() -> &'static str {
    "Usage: /workspace-update <task_id> [--title text] [--description text] [--priority low|normal|high|urgent] [--assignee svarog|agent:id|subagent:id|none] [--reviewer user|svarog|agent:id|subagent:id|none] [--dod text|--clear-dod]"
}

fn split_options(payload: &str) -> Vec<(&str, &str)> {
    let mut options = Vec::new();
    let mut remaining = payload.trim();
    while let Some(stripped) = remaining.strip_prefix("--") {
        let key_end = stripped.find(char::is_whitespace).unwrap_or(stripped.len());
        let key = stripped[..key_end].trim();
        let after_key = stripped[key_end..].trim_start();
        if key.is_empty() {
            break;
        }
        if key == "clear-dod" {
            options.push((key, ""));
            remaining = after_key;
            continue;
        }
        let next_option = after_key.find(" --");
        let (value, next_remaining) = match next_option {
            Some(index) => (&after_key[..index], after_key[index + 1..].trim_start()),
            None => (after_key, ""),
        };
        options.push((key, value.trim()));
        remaining = next_remaining;
    }
    options
}

fn apply_option(update: &mut WorkspaceTaskUpdate, key: &str, value: &str) -> Result<()> {
    match key.trim().to_ascii_lowercase().as_str() {
        "title" => {
            if value.trim().is_empty() {
                bail!("Workspace title cannot be empty");
            }
            update.title = Some(value.trim().to_string());
        }
        "description" => {
            if value.trim().is_empty() {
                bail!("Workspace description cannot be empty");
            }
            update.description = Some(value.trim().to_string());
        }
        "priority" => {
            update.priority = Some(
                crate::app::commands::parse_workspace_priority(first_word(value))
                    .ok_or_else(|| anyhow::anyhow!("Unknown workspace priority: {value}"))?,
            );
        }
        "assignee" => {
            update.assignee = Some(
                crate::app::commands::parse_workspace_actor_field(first_word(value))
                    .ok_or_else(|| anyhow::anyhow!("Unknown workspace assignee: {value}"))?,
            );
            if matches!(update.assignee, Some(Some(WorkspaceActor::User))) {
                bail!("Workspace assignee must be an agent or subagent");
            }
        }
        "reviewer" => {
            update.reviewer = Some(
                crate::app::commands::parse_workspace_actor_field(first_word(value))
                    .ok_or_else(|| anyhow::anyhow!("Unknown workspace reviewer: {value}"))?,
            );
        }
        "dod" | "definition-of-done" | "definition_of_done" => {
            if value.trim().is_empty() {
                bail!("Workspace definition of done cannot be empty");
            }
            update.definition_of_done = Some(Some(value.trim().to_string()));
        }
        "clear-dod" | "clear_dod" => {
            update.definition_of_done = Some(None);
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
    use zorai_protocol::{WorkspaceActor, WorkspacePriority};

    #[test]
    fn parses_workspace_update_fields() {
        let draft = parse_workspace_update_args(
            "wtask_123 --title New title --description New description --priority high --assignee svarog --reviewer none --dod Tests pass",
        )
        .expect("draft");

        assert_eq!(draft.task_id, "wtask_123");
        assert_eq!(draft.update.title.as_deref(), Some("New title"));
        assert_eq!(draft.update.description.as_deref(), Some("New description"));
        assert_eq!(
            draft.update.definition_of_done.as_ref(),
            Some(&Some("Tests pass".to_string()))
        );
        assert_eq!(draft.update.priority, Some(WorkspacePriority::High));
        assert_eq!(
            draft.update.assignee,
            Some(Some(WorkspaceActor::Agent(
                zorai_protocol::AGENT_ID_SWAROG.to_string()
            )))
        );
        assert_eq!(draft.update.reviewer, Some(None));
    }

    #[test]
    fn parses_workspace_update_clear_dod() {
        let draft = parse_workspace_update_args("wtask_123 --clear-dod").expect("draft");
        assert_eq!(draft.update.definition_of_done, Some(None));
    }
}
