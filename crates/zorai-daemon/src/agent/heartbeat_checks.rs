//! Built-in heartbeat check functions — structured data gathering (no LLM calls).
//!
//! Per D-01: Each check is a standalone async method on AgentEngine that reads
//! in-memory state and returns a HeartbeatCheckResult. Per D-02: four checks
//! ship in Phase 2.

use super::*;

impl AgentEngine {
    /// Check for TODOs that have been pending/in-progress longer than threshold. Per D-02/BEAT-02.
    pub(super) async fn check_stale_todos(&self, threshold_hours: u64) -> HeartbeatCheckResult {
        let now = now_millis();
        let threshold_ms = threshold_hours * 3600 * 1000;
        let todos = self.thread_todos.read().await;
        let stale: Vec<CheckDetail> = todos
            .values()
            .flat_map(|items| items.iter())
            .filter(|t| matches!(t.status, TodoStatus::Pending | TodoStatus::InProgress))
            .filter(|t| now.saturating_sub(t.updated_at) >= threshold_ms)
            .map(|t| {
                let age_h = (now.saturating_sub(t.updated_at)) as f64 / 3_600_000.0;
                CheckDetail {
                    id: t.id.clone(),
                    label: t.content.clone(),
                    age_hours: age_h,
                    severity: if age_h > (threshold_hours as f64 * 3.0) {
                        CheckSeverity::High
                    } else if age_h > (threshold_hours as f64 * 1.5) {
                        CheckSeverity::Medium
                    } else {
                        CheckSeverity::Low
                    },
                    context: format!(
                        "TODO '{}' ({:?}) last updated {:.1}h ago",
                        t.content, t.status, age_h
                    ),
                }
            })
            .collect();

        HeartbeatCheckResult {
            check_type: HeartbeatCheckType::StaleTodos,
            items_found: stale.len(),
            summary: if stale.is_empty() {
                "No stale TODOs.".into()
            } else {
                format!("{} TODO(s) older than {}h", stale.len(), threshold_hours)
            },
            details: stale,
        }
    }

    /// Check for goal runs stuck in Running/Planning/AwaitingApproval longer than threshold. Per D-02/BEAT-02.
    pub(super) async fn check_stuck_goal_runs(&self, threshold_hours: u64) -> HeartbeatCheckResult {
        let now = now_millis();
        let threshold_ms = threshold_hours * 3600 * 1000;
        let goal_runs = self.goal_runs.lock().await;
        let stuck: Vec<CheckDetail> = goal_runs
            .iter()
            .filter(|g| {
                matches!(
                    g.status,
                    GoalRunStatus::Running
                        | GoalRunStatus::Planning
                        | GoalRunStatus::AwaitingApproval
                )
            })
            .filter(|g| now.saturating_sub(g.updated_at) >= threshold_ms)
            .map(|g| {
                let age_h = (now.saturating_sub(g.updated_at)) as f64 / 3_600_000.0;
                CheckDetail {
                    id: g.id.clone(),
                    label: g.title.clone(),
                    age_hours: age_h,
                    severity: if age_h > (threshold_hours as f64 * 4.0) {
                        CheckSeverity::Critical
                    } else if age_h > (threshold_hours as f64 * 2.0) {
                        CheckSeverity::High
                    } else {
                        CheckSeverity::Medium
                    },
                    context: format!(
                        "Goal '{}' status {:?}, last update {:.1}h ago{}",
                        g.title,
                        g.status,
                        age_h,
                        g.last_error
                            .as_ref()
                            .map(|e| format!(", error: {}", e))
                            .unwrap_or_default()
                    ),
                }
            })
            .collect();

        HeartbeatCheckResult {
            check_type: HeartbeatCheckType::StuckGoalRuns,
            items_found: stuck.len(),
            summary: if stuck.is_empty() {
                "No stuck goal runs.".into()
            } else {
                format!(
                    "{} goal run(s) stuck for >{}h",
                    stuck.len(),
                    threshold_hours
                )
            },
            details: stuck,
        }
    }

    /// Check for unreplied gateway messages. Per D-02/BEAT-02/GATE-06.
    ///
    /// Compares `last_incoming_at` vs `last_response_at` per channel in GatewayState.
    /// A channel is considered "unreplied" when:
    /// 1. It has an incoming message timestamp newer than the last response timestamp
    ///    (or no response at all), AND
    /// 2. The incoming message is older than `threshold_hours` (prevents flagging
    ///    messages that just arrived — gives the agent time to respond).
    pub(super) async fn check_unreplied_messages(
        &self,
        threshold_hours: u64,
    ) -> HeartbeatCheckResult {
        let now = now_millis();
        let threshold_ms = threshold_hours * 3600 * 1000;

        // Read gateway_threads for sender context (maps thread_id -> gateway channel key)
        let gateway_threads = self.gateway_threads.read().await;

        // Read gateway_state for last_incoming_at and last_response_at
        let gw_lock = self.gateway_state.lock().await;

        let mut unreplied: Vec<CheckDetail> = Vec::new();

        if let Some(gw) = gw_lock.as_ref() {
            for (channel_key, &incoming_at) in &gw.last_incoming_at {
                // Check if we've responded after the incoming message
                let responded = gw
                    .last_response_at
                    .get(channel_key)
                    .map(|&resp_at| resp_at >= incoming_at)
                    .unwrap_or(false);

                if responded {
                    continue;
                }

                // Check if the incoming message is old enough to flag
                // (prevents flagging messages that just arrived)
                let elapsed_ms = now.saturating_sub(incoming_at);
                if elapsed_ms < threshold_ms {
                    continue;
                }

                let age_h = elapsed_ms as f64 / 3_600_000.0;

                // Try to find sender info from gateway_threads
                let sender = gateway_threads
                    .iter()
                    .find(|(_, v)| v.as_str() == channel_key)
                    .map(|(k, _)| k.clone())
                    .unwrap_or_else(|| "unknown".to_string());

                let severity = if age_h > (threshold_hours as f64 * 4.0) {
                    CheckSeverity::High
                } else if age_h > (threshold_hours as f64 * 2.0) {
                    CheckSeverity::Medium
                } else {
                    CheckSeverity::Low
                };

                unreplied.push(CheckDetail {
                    id: channel_key.clone(),
                    label: format!("Unreplied message on {channel_key}"),
                    age_hours: age_h,
                    severity,
                    context: format!(
                        "Message from '{}' on {} unreplied for {:.1}h",
                        sender, channel_key, age_h
                    ),
                });
            }
        }

        drop(gw_lock);

        HeartbeatCheckResult {
            check_type: HeartbeatCheckType::UnrepliedGatewayMessages,
            items_found: unreplied.len(),
            summary: if unreplied.is_empty() {
                "No unreplied gateway messages.".into()
            } else {
                format!(
                    "{} unreplied gateway conversation(s) for >{}h",
                    unreplied.len(),
                    threshold_hours
                )
            },
            details: unreplied,
        }
    }

    /// Check for repo changes using git status. Per D-05/BEAT-02.
    /// Uses spawn_blocking to avoid blocking the tokio reactor.
    pub(super) async fn check_repo_changes(&self) -> HeartbeatCheckResult {
        let data_dir = self.data_dir.clone();
        // Find the parent of data_dir as the likely project root
        let repo_path = data_dir
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_string_lossy().to_string());

        let repo_path = match repo_path {
            Some(p) => p,
            None => {
                return HeartbeatCheckResult {
                    check_type: HeartbeatCheckType::RepoChanges,
                    items_found: 0,
                    summary: "No repo path available.".into(),
                    details: vec![],
                };
            }
        };

        // Check if git is available on PATH
        let has_git = which::which("git").is_ok();
        if !has_git {
            return HeartbeatCheckResult {
                check_type: HeartbeatCheckType::RepoChanges,
                items_found: 0,
                summary: "Git not available on PATH.".into(),
                details: vec![],
            };
        }

        // Run git status in spawn_blocking to avoid blocking the reactor (Pitfall 2)
        let path_clone = repo_path.clone();
        let git_info = match tokio::task::spawn_blocking(move || {
            crate::git::get_git_status(&path_clone)
        })
        .await
        {
            Ok(info) => info,
            Err(e) => {
                tracing::warn!("git status check failed: {e}");
                return HeartbeatCheckResult {
                    check_type: HeartbeatCheckType::RepoChanges,
                    items_found: 0,
                    summary: format!("Git check failed: {e}"),
                    details: vec![],
                };
            }
        };

        let total_changes = git_info.modified + git_info.staged + git_info.untracked;
        let mut details = Vec::new();

        if git_info.modified > 0 {
            details.push(CheckDetail {
                id: "repo_modified".into(),
                label: format!("{} modified file(s)", git_info.modified),
                age_hours: 0.0,
                severity: CheckSeverity::Low,
                context: format!("{} modified file(s) in {}", git_info.modified, repo_path),
            });
        }
        if git_info.staged > 0 {
            details.push(CheckDetail {
                id: "repo_staged".into(),
                label: format!("{} staged file(s)", git_info.staged),
                age_hours: 0.0,
                severity: CheckSeverity::Low,
                context: format!("{} staged file(s) ready to commit", git_info.staged),
            });
        }
        if git_info.untracked > 0 {
            details.push(CheckDetail {
                id: "repo_untracked".into(),
                label: format!("{} untracked file(s)", git_info.untracked),
                age_hours: 0.0,
                severity: CheckSeverity::Low,
                context: format!("{} untracked file(s)", git_info.untracked),
            });
        }

        HeartbeatCheckResult {
            check_type: HeartbeatCheckType::RepoChanges,
            items_found: total_changes as usize,
            summary: if total_changes == 0 {
                format!(
                    "Repo clean on branch {}",
                    git_info.branch.as_deref().unwrap_or("unknown")
                )
            } else {
                format!(
                    "{} change(s) on branch {} ({} modified, {} staged, {} untracked)",
                    total_changes,
                    git_info.branch.as_deref().unwrap_or("unknown"),
                    git_info.modified,
                    git_info.staged,
                    git_info.untracked
                )
            },
            details,
        }
    }

    pub(super) async fn check_plugin_auth(&self) -> HeartbeatCheckResult {
        let Some(plugin_manager) = self.plugin_manager.get() else {
            return HeartbeatCheckResult {
                check_type: HeartbeatCheckType::PluginAuth,
                items_found: 0,
                summary: "Plugin auth monitoring unavailable.".into(),
                details: vec![],
            };
        };

        let issues = plugin_manager.monitor_auth_health().await;
        self.sync_plugin_auth_notifications(&issues).await;
        let details: Vec<CheckDetail> = issues
            .iter()
            .map(|issue| {
                let (severity, label) = match issue.status {
                    crate::plugin::PluginAuthStatus::ExpiringSoon => (
                        CheckSeverity::Medium,
                        format!("{} expires soon", issue.plugin_name),
                    ),
                    crate::plugin::PluginAuthStatus::NeedsReconnect => (
                        CheckSeverity::High,
                        format!("{} needs reconnect", issue.plugin_name),
                    ),
                    crate::plugin::PluginAuthStatus::Refreshable => (
                        CheckSeverity::Medium,
                        format!("{} auto-refresh failed", issue.plugin_name),
                    ),
                    crate::plugin::PluginAuthStatus::Connected
                    | crate::plugin::PluginAuthStatus::NotConfigured => {
                        (CheckSeverity::Low, issue.plugin_name.clone())
                    }
                };
                CheckDetail {
                    id: issue.plugin_name.clone(),
                    label,
                    age_hours: 0.0,
                    severity,
                    context: issue.message.clone(),
                }
            })
            .collect();

        let summary = if details.is_empty() {
            "Plugin auth healthy.".to_string()
        } else if details.iter().any(|detail| {
            matches!(
                detail.severity,
                CheckSeverity::High | CheckSeverity::Critical
            )
        }) {
            format!("{} plugin(s) need reconnect", details.len())
        } else if details
            .iter()
            .any(|detail| detail.label.contains("expires soon"))
        {
            format!("{} plugin(s) expire soon", details.len())
        } else {
            format!("{} plugin auto-refresh failed", details.len())
        };

        HeartbeatCheckResult {
            check_type: HeartbeatCheckType::PluginAuth,
            items_found: details.len(),
            summary,
            details,
        }
    }

    async fn sync_plugin_auth_notifications(
        &self,
        issues: &[crate::plugin::PluginAuthHealthIssue],
    ) {
        let now = now_millis() as i64;
        let existing = self.history.list_notifications(true, Some(500)).await;
        let Ok(existing) = existing else {
            return;
        };
        let existing_by_id: std::collections::HashMap<String, zorai_protocol::InboxNotification> =
            existing
                .iter()
                .cloned()
                .map(|notification| (notification.id.clone(), notification))
                .collect();
        let active_notifications: Vec<zorai_protocol::InboxNotification> = issues
            .iter()
            .map(|issue| {
                let (kind, title, severity) = match issue.status {
                    crate::plugin::PluginAuthStatus::ExpiringSoon => (
                        "plugin_expiring_soon",
                        format!("{} expires soon", issue.plugin_name),
                        "warning",
                    ),
                    crate::plugin::PluginAuthStatus::NeedsReconnect => (
                        "plugin_needs_reconnect",
                        format!("{} needs reconnect", issue.plugin_name),
                        "error",
                    ),
                    crate::plugin::PluginAuthStatus::Refreshable => (
                        "plugin_refresh_failed",
                        format!("{} auto-refresh failed", issue.plugin_name),
                        "warning",
                    ),
                    crate::plugin::PluginAuthStatus::Connected
                    | crate::plugin::PluginAuthStatus::NotConfigured => {
                        ("plugin_auth", issue.plugin_name.clone(), "info")
                    }
                };
                let id = format!("plugin-auth:{kind}:{}", issue.plugin_name);
                let existing = existing_by_id.get(&id);
                zorai_protocol::InboxNotification {
                    id,
                    source: "plugin_auth".to_string(),
                    kind: kind.to_string(),
                    title,
                    body: issue.message.clone(),
                    subtitle: Some(issue.plugin_name.clone()),
                    severity: severity.to_string(),
                    created_at: existing
                        .map(|notification| notification.created_at)
                        .unwrap_or(now),
                    updated_at: now,
                    read_at: existing.and_then(|notification| notification.read_at),
                    archived_at: None,
                    deleted_at: None,
                    actions: vec![crate::notifications::plugin_settings_action(
                        &issue.plugin_name,
                    )],
                    metadata_json: Some(
                        serde_json::json!({
                            "plugin_name": issue.plugin_name,
                            "status": issue.status.as_str(),
                            "auto_action_attempted": issue.auto_action_attempted,
                        })
                        .to_string(),
                    ),
                }
            })
            .collect();

        for notification in &active_notifications {
            let _ = self.upsert_inbox_notification(notification.clone()).await;
        }

        let active_ids: std::collections::HashSet<&str> = active_notifications
            .iter()
            .map(|notification| notification.id.as_str())
            .collect();

        for mut notification in existing.into_iter().filter(|notification| {
            notification.source == "plugin_auth"
                && notification.archived_at.is_none()
                && notification.deleted_at.is_none()
                && !active_ids.contains(notification.id.as_str())
        }) {
            notification.archived_at = Some(now);
            notification.updated_at = now;
            let _ = self.upsert_inbox_notification(notification).await;
        }
    }
}

#[cfg(test)]
#[path = "heartbeat_checks/tests.rs"]
mod tests;
