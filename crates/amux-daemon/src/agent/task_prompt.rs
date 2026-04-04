//! Task prompt building and session resolution.

use std::sync::Arc;

use super::agent_identity::{is_main_agent_scope, MAIN_AGENT_ID};
use super::task_scheduler::describe_scheduled_time;
use super::types::*;

pub(super) fn build_task_prompt(task: &AgentTask) -> String {
    let mut prompt = format!(
        "Execute the following queued task.\n\nTitle: {}\nDescription: {}",
        task.title, task.description
    );

    prompt.push_str(
        "\nUse execute_managed_command when work should run inside a daemon-managed terminal lane, needs a real PTY, or may require operator approval.",
    );
    prompt.push_str(
        "\nIf the task is more than a one-shot action, call update_todo immediately with a concise plan and keep it current as steps advance.",
    );
    prompt.push_str(
        "\nIf this task is large and parallelizable, use spawn_subagent for bounded child work items and monitor them with list_subagents.",
    );

    if let Some(command) = task.command.as_deref() {
        prompt.push_str(&format!("\nPreferred command or entrypoint: {command}"));
    }

    if let Some(session_id) = task.session_id.as_deref() {
        prompt.push_str(&format!("\nPreferred terminal session: {session_id}"));
    }

    if let Some(goal_run_id) = task.goal_run_id.as_deref() {
        prompt.push_str(&format!("\nGoal run context: {goal_run_id}"));
    }

    if let Some(parent_task_id) = task.parent_task_id.as_deref() {
        prompt.push_str(&format!(
            "\nParent task: {parent_task_id}\nYou are running as a supervised subagent. Stay tightly scoped to this assignment, avoid duplicating sibling work, and report concise results back through your normal response."
        ));
    }

    if let Some(parent_thread_id) = task.parent_thread_id.as_deref() {
        prompt.push_str(&format!("\nParent thread: {parent_thread_id}"));
    }

    prompt.push_str(&format!("\nAssigned runtime: {}", task.runtime));

    if let Some(scheduled_at) = task.scheduled_at {
        prompt.push_str(&format!(
            "\nOriginal schedule: {}",
            describe_scheduled_time(scheduled_at)
        ));
    }

    if !task.dependencies.is_empty() {
        prompt.push_str(&format!(
            "\nResolved dependencies: {}",
            task.dependencies.join(", ")
        ));
    }

    let recent_subagent_logs = task
        .logs
        .iter()
        .rev()
        .filter(|log| log.phase == "subagent")
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|log| {
            let details = log
                .details
                .as_deref()
                .map(|value| format!(" ({value})"))
                .unwrap_or_default();
            format!("- {}", log.message) + &details
        })
        .collect::<Vec<_>>();
    if !recent_subagent_logs.is_empty() {
        prompt.push_str("\nRecent subagent updates:\n");
        prompt.push_str(&recent_subagent_logs.join("\n"));
    }

    if task.retry_count > 0 {
        prompt.push_str(&format!(
            "\n\nThis is self-healing retry attempt {} of {}.",
            task.retry_count, task.max_retries
        ));
        if let Some(last_error) = task.last_error.as_deref() {
            prompt.push_str(&format!("\nLast failure: {last_error}"));
        }
        let recent_logs = task
            .logs
            .iter()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|log| format!("- [{}] {}", log.phase, log.message))
            .collect::<Vec<_>>();
        if !recent_logs.is_empty() {
            prompt.push_str("\nRecent task log:\n");
            prompt.push_str(&recent_logs.join("\n"));
        }
        prompt.push_str(
            "\nAnalyze the root cause, adapt the approach, and retry with the smallest viable correction.",
        );
    } else {
        prompt.push_str(
            "\n\nWork through this step by step. Use your tools as needed and report your progress clearly.",
        );
    }

    prompt
}

/// Append available sub-agent registry to a task prompt so the LLM
/// knows what specialist sub-agents it can delegate work to.
pub(super) fn append_sub_agent_registry(prompt: &mut String, sub_agents: &[SubAgentDefinition]) {
    let enabled: Vec<&SubAgentDefinition> = sub_agents.iter().filter(|sa| sa.enabled).collect();
    if enabled.is_empty() {
        return;
    }

    prompt.push_str("\n\n## Available Sub-Agents\n");
    prompt
        .push_str("You can delegate work to these specialist sub-agents via `spawn_subagent`:\n\n");
    for sa in &enabled {
        prompt.push_str(&format!("- **{}**", sa.name));
        if let Some(ref role) = sa.role {
            prompt.push_str(&format!(" (role: {role})"));
        }
        prompt.push_str(&format!(
            " — provider: {}, model: {}",
            sa.provider, sa.model
        ));
        if let Some(ref sp) = sa.system_prompt {
            let snippet: String = sp.chars().take(80).collect();
            prompt.push_str(&format!(" — \"{snippet}...\""));
        }
        prompt.push('\n');
    }
}

pub(super) async fn append_effective_sub_agent_registry(
    engine: &crate::agent::AgentEngine,
    prompt: &mut String,
) {
    let sub_agents = engine.list_sub_agents().await;
    append_sub_agent_registry(prompt, &sub_agents);
}

pub(super) async fn resolve_preferred_session_id(
    session_manager: &Arc<crate::session_manager::SessionManager>,
    session_hint: Option<&str>,
) -> Option<amux_protocol::SessionId> {
    let hint = session_hint?.trim();
    if hint.is_empty() {
        return None;
    }

    session_manager
        .list()
        .await
        .into_iter()
        .find(|session| {
            let session_id = session.id.to_string();
            session_id == hint || session_id.contains(hint)
        })
        .map(|session| session.id)
}

// -- Utility functions --

pub(super) fn agent_data_dir() -> std::path::PathBuf {
    let base = if cfg!(windows) {
        std::env::var("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .unwrap_or_default()
                    .join("AppData")
                    .join("Local")
            })
            .join("tamux")
    } else {
        dirs::home_dir().unwrap_or_default().join(".tamux")
    };
    base.join("agent")
}

pub(super) fn ordered_memory_dirs(agent_data_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let root = agent_data_dir.parent().unwrap_or(std::path::Path::new("."));
    let mut dirs = vec![root.join("agent-mission"), agent_data_dir.to_path_buf()];
    dirs.dedup();
    dirs
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MemoryPaths {
    pub memory_dir: std::path::PathBuf,
    pub memory_path: std::path::PathBuf,
    pub soul_path: std::path::PathBuf,
    pub user_path: std::path::PathBuf,
}

pub(super) fn memory_paths_for_scope(
    agent_data_dir: &std::path::Path,
    scope_id: &str,
) -> MemoryPaths {
    let shared_user_dir = active_memory_dir_for_scope(agent_data_dir, MAIN_AGENT_ID);
    let memory_dir = active_memory_dir_for_scope(agent_data_dir, scope_id);
    MemoryPaths {
        memory_path: memory_dir.join("MEMORY.md"),
        soul_path: memory_dir.join("SOUL.md"),
        user_path: shared_user_dir.join("USER.md"),
        memory_dir,
    }
}

pub(super) fn skills_dir(agent_data_dir: &std::path::Path) -> std::path::PathBuf {
    agent_data_dir
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("skills")
}

fn builtin_skills_source_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills")
}

/// Seed built-in skill documents into `~/.tamux/skills/builtin/`.
pub(super) fn seed_builtin_skills(agent_data_dir: &std::path::Path) {
    let root = skills_dir(agent_data_dir);
    let source = builtin_skills_source_dir();
    let target = root.join("builtin");

    match seed_skills_tree(&source, &target) {
        Ok(count) => tracing::debug!("seeded {} built-in skills into {}", count, target.display()),
        Err(e) => tracing::warn!(
            "failed to seed built-in skills from {} to {}: {e}",
            source.display(),
            target.display()
        ),
    }
}

fn seed_skills_tree(source: &std::path::Path, target: &std::path::Path) -> std::io::Result<usize> {
    std::fs::create_dir_all(target)?;

    let mut count = 0usize;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());

        if file_type.is_dir() {
            count += seed_skills_tree(&source_path, &target_path)?;
        } else if file_type.is_file() {
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&source_path, &target_path)?;
            count += 1;
        }
    }

    Ok(count)
}

pub(super) fn dir_has_memory_files(dir: &std::path::Path) -> bool {
    ["MEMORY.md", "SOUL.md", "USER.md"]
        .iter()
        .any(|name| dir.join(name).exists())
}

fn persona_memory_dir(agent_data_dir: &std::path::Path, scope_id: &str) -> std::path::PathBuf {
    agent_data_dir.join("personas").join(scope_id)
}

pub(super) fn active_memory_dir_for_scope(
    agent_data_dir: &std::path::Path,
    scope_id: &str,
) -> std::path::PathBuf {
    if !is_main_agent_scope(scope_id) {
        return persona_memory_dir(agent_data_dir, scope_id);
    }

    let dirs = ordered_memory_dirs(agent_data_dir);
    if let Some(path) = dirs.iter().find(|dir| dir_has_memory_files(dir)) {
        return path.clone();
    }
    if let Some(path) = dirs.iter().find(|dir| dir.exists()) {
        return path.clone();
    }
    dirs.first()
        .cloned()
        .unwrap_or_else(|| agent_data_dir.to_path_buf())
}

pub(super) fn active_memory_dir(agent_data_dir: &std::path::Path) -> std::path::PathBuf {
    active_memory_dir_for_scope(agent_data_dir, MAIN_AGENT_ID)
}

/// Read a string from a legacy frontend settings payload using either the nested
/// `/settings/<key>` path or a top-level `<key>` fallback.
pub(super) fn read_setting_str(v: &serde_json::Value, key: &str) -> String {
    let pointer = format!("/settings/{key}");
    v.pointer(&pointer)
        .or_else(|| v.get(key))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

pub(super) fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(super) async fn persist_json<T: serde::Serialize>(
    path: &std::path::Path,
    data: &T,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let json = serde_json::to_string_pretty(data)?;
    tokio::fs::write(path, json).await?;
    Ok(())
}

/// Load agent config using a shared HistoryStore, returning defaults if not found.
pub async fn load_config_from_history(
    history: &crate::history::HistoryStore,
) -> anyhow::Result<AgentConfig> {
    let items = history.list_agent_config_items().await?;
    super::config::load_config_from_items(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::agent_identity::{MAIN_AGENT_ID, RADOGOST_AGENT_ID};
    use tempfile::tempdir;

    #[test]
    fn memory_paths_for_main_scope_falls_back_to_legacy_root() {
        let root = std::path::Path::new("/tmp/tamux/agent");
        let paths = memory_paths_for_scope(root, MAIN_AGENT_ID);
        let legacy_root = root.parent().unwrap().join("agent-mission");
        assert_eq!(paths.memory_dir, legacy_root);
        assert_eq!(paths.memory_path, legacy_root.join("MEMORY.md"));
        assert_eq!(paths.soul_path, legacy_root.join("SOUL.md"));
        assert_eq!(paths.user_path, legacy_root.join("USER.md"));
    }

    #[test]
    fn memory_paths_for_persona_scope_use_persona_root_and_shared_user() {
        let root = std::path::Path::new("/tmp/tamux/agent");
        let paths = memory_paths_for_scope(root, RADOGOST_AGENT_ID);
        let shared_user_root = active_memory_dir_for_scope(root, MAIN_AGENT_ID);
        assert_eq!(
            paths.memory_dir,
            root.join("personas").join(RADOGOST_AGENT_ID)
        );
        assert_eq!(
            paths.memory_path,
            root.join("personas")
                .join(RADOGOST_AGENT_ID)
                .join("MEMORY.md")
        );
        assert_eq!(
            paths.soul_path,
            root.join("personas")
                .join(RADOGOST_AGENT_ID)
                .join("SOUL.md")
        );
        assert_eq!(paths.user_path, shared_user_root.join("USER.md"));
    }

    #[test]
    fn seed_builtin_skills_copies_repo_skill_docs() {
        let temp = tempdir().expect("tempdir should succeed");
        let agent_data_dir = temp.path().join("agent");

        seed_builtin_skills(&agent_data_dir);

        let builtin_root = skills_dir(&agent_data_dir).join("builtin");
        assert!(
            builtin_root.join("README.md").exists(),
            "expected built-in skills seed to copy the repo skills README"
        );
        assert!(
            builtin_root.join("operating").join("thread-compaction.md").exists(),
            "expected built-in skills seed to copy nested skill docs"
        );
    }

    #[test]
    fn builtin_skills_source_dir_points_at_repo_skills_tree() {
        let expected = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills");
        assert_eq!(builtin_skills_source_dir(), expected);
        assert!(
            builtin_skills_source_dir().join("README.md").exists(),
            "expected built-in skills source dir to resolve to the repo skills tree"
        );
    }
}
