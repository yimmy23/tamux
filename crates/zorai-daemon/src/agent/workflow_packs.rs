use super::workspace_support::workspace_priority_label;
use super::*;
use anyhow::{anyhow, bail, Result};
use chrono::{Duration as ChronoDuration, SecondsFormat, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;
use zorai_protocol::{WorkspacePriority, WorkspaceTask, WorkspaceTaskStatus};

#[derive(Debug, Clone)]
pub(crate) struct WorkflowPackExecution {
    pub payload: Value,
    pub pending_approval: Option<ToolPendingApproval>,
}

#[derive(Debug, Clone)]
struct ConnectorInfo {
    plugin_name: String,
    readiness_state: String,
    readiness_message: Option<String>,
    setup_hint: Option<String>,
    docs_path: Option<String>,
    read_actions: Vec<String>,
    write_actions: Vec<String>,
}

impl ConnectorInfo {
    fn is_ready(&self) -> bool {
        self.readiness_state == "ready" || self.readiness_state == "degraded"
    }
}

impl AgentEngine {
    pub(crate) async fn list_browser_profiles_with_current_health(
        &self,
    ) -> Result<Vec<crate::history::BrowserProfileRow>> {
        self.history
            .detect_and_classify_expired_profiles(now_millis())
            .await?;
        self.history.list_browser_profiles().await
    }

    pub(crate) async fn run_workflow_pack_json(
        &self,
        args: &Value,
        thread_id: Option<&str>,
        task_id: Option<&str>,
    ) -> Result<WorkflowPackExecution> {
        let pack_name = normalized_pack_name(args)?;
        match pack_name.as_str() {
            "daily-brief" => self.run_daily_brief_pack(args).await,
            "pr-issue-triage" => self.run_pr_issue_triage_pack(args).await,
            "inbox-calendar-triage" => self.run_inbox_calendar_triage_pack(args).await,
            "watch-monitor" => self.run_watch_monitor_pack(args).await,
            "standup" => self.run_standup_pack(args).await,
            "approval-checkpoint-long-task" => {
                self.run_approval_checkpoint_long_task_pack(args, thread_id, task_id)
                    .await
            }
            other => bail!("unsupported workflow pack `{other}`"),
        }
    }

    async fn connector_catalog(&self) -> HashMap<String, ConnectorInfo> {
        let Some(plugin_manager) = self.plugin_manager.get() else {
            return HashMap::new();
        };

        plugin_manager
            .list_plugins()
            .await
            .into_iter()
            .filter_map(|plugin| {
                let kind = plugin.connector_kind.clone()?;
                Some((
                    kind,
                    ConnectorInfo {
                        plugin_name: plugin.name,
                        readiness_state: plugin.readiness_state,
                        readiness_message: plugin.readiness_message,
                        setup_hint: plugin.setup_hint,
                        docs_path: plugin.docs_path,
                        read_actions: plugin.read_actions,
                        write_actions: plugin.write_actions,
                    },
                ))
            })
            .collect()
    }

    async fn connector_read_text(
        &self,
        connectors: &HashMap<String, ConnectorInfo>,
        connector_kind: &str,
        endpoint_name: &str,
        params: Value,
    ) -> Result<String> {
        let connector = connectors
            .get(connector_kind)
            .ok_or_else(|| anyhow!("connector `{connector_kind}` is unavailable"))?;
        let plugin_manager = self
            .plugin_manager
            .get()
            .ok_or_else(|| anyhow!("plugin system not available"))?;
        Ok(plugin_manager
            .api_call(&connector.plugin_name, endpoint_name, params)
            .await
            .map_err(|error| anyhow!(error.to_string()))?)
    }

    async fn run_daily_brief_pack(&self, args: &Value) -> Result<WorkflowPackExecution> {
        let delivery_channel =
            string_arg(args, &["delivery_channel"]).unwrap_or_else(|| "in-app".to_string());
        let deliver_now = bool_arg(args, "deliver_now").unwrap_or(false);
        if deliver_now && delivery_channel != "in-app" {
            let pending = workflow_pack_pending_approval(
                "daily-brief",
                format!("deliver brief via {delivery_channel}"),
                "message delivery is an external side effect",
                "medium",
            );
            return Ok(WorkflowPackExecution {
                payload: json!({
                    "status": "approval_required",
                    "pack_name": "daily-brief",
                    "reason": "external delivery requires fresh approval",
                    "delivery_channel": delivery_channel,
                    "approval_id": pending.approval_id,
                }),
                pending_approval: Some(pending),
            });
        }

        let workspace_id =
            string_arg(args, &["workspace_id"]).unwrap_or_else(|| "main".to_string());
        let mode = string_arg(args, &["mode"]).unwrap_or_else(|| "standard".to_string());
        let include_inbox = bool_arg(args, "include_inbox").unwrap_or(false);
        let include_calendar = bool_arg(args, "include_calendar").unwrap_or(false);
        let connectors = self.connector_catalog().await;
        let tasks = self
            .list_workspace_tasks(&workspace_id, false)
            .await
            .unwrap_or_default();
        let notices = self
            .list_workspace_notices(&workspace_id, None)
            .await
            .unwrap_or_default();
        let routines = self
            .history
            .list_routine_definitions()
            .await
            .unwrap_or_default();
        let pending_approvals = self.pending_operator_approvals.read().await;
        let pending_approval_count = pending_approvals.len();
        drop(pending_approvals);

        let top_priorities = summarize_workspace_tasks(&tasks);
        let routine_health = summarize_routines(&routines);
        let notice_lines = notices
            .iter()
            .take(5)
            .map(|notice| format!("- [{}] {}", notice.notice_type, notice.message))
            .collect::<Vec<_>>();

        let inbox_section = if include_inbox {
            resolve_connector_section(
                text_arg(args, &["inbox_preview"]),
                connectors.get("gmail"),
                self,
                &connectors,
                "gmail",
                "list_inbox",
                json!({
                    "max_results": 5,
                    "query": string_arg(args, &["gmail_query"])
                        .unwrap_or_else(|| "in:inbox newer_than:7d".to_string()),
                }),
            )
            .await
        } else {
            PackSection::omitted("Inbox section not requested.")
        };

        let calendar_section = if include_calendar {
            let (time_min, time_max) = pack_time_range(args);
            resolve_connector_section(
                text_arg(args, &["calendar_preview"]),
                connectors.get("calendar"),
                self,
                &connectors,
                "calendar",
                "list_schedule_items",
                json!({
                    "calendar_id": string_arg(args, &["calendar_id"])
                        .unwrap_or_else(|| "primary".to_string()),
                    "time_min": time_min,
                    "time_max": time_max,
                    "max_results": 6,
                }),
            )
            .await
        } else {
            PackSection::omitted("Calendar section not requested.")
        };

        let quiet_day = top_priorities.is_empty()
            && routine_health.is_empty()
            && notice_lines.is_empty()
            && inbox_section.text.is_none()
            && calendar_section.text.is_none();

        let summary = if quiet_day {
            "Quiet day: no urgent workspace items, failing routines, or notable notices."
                .to_string()
        } else {
            format!(
                "Daily brief ({mode}) with {} top priorities, {} pending approval(s), {} routine signal(s), and {} notice(s).",
                top_priorities.len(),
                pending_approval_count,
                routine_health.len(),
                notice_lines.len(),
            )
        };

        Ok(WorkflowPackExecution {
            payload: json!({
                "status": "ok",
                "pack_name": "daily-brief",
                "mode": mode,
                "workspace_id": workspace_id,
                "delivery_channel": delivery_channel,
                "mobile_safe": true,
                "summary": summary,
                "top_priorities": top_priorities,
                "pending_approvals": pending_approval_count,
                "routine_health": routine_health,
                "notices": notice_lines,
                "sections": {
                    "inbox": inbox_section.to_json(),
                    "calendar": calendar_section.to_json(),
                },
                "connector_health": connector_health_subset(&connectors, &["gmail", "calendar"]),
            }),
            pending_approval: None,
        })
    }

    async fn run_pr_issue_triage_pack(&self, args: &Value) -> Result<WorkflowPackExecution> {
        if bool_arg(args, "perform_writeback").unwrap_or(false)
            || string_arg(args, &["writeback_action"]).is_some()
        {
            let pending = workflow_pack_pending_approval(
                "pr-issue-triage",
                string_arg(args, &["writeback_action"])
                    .unwrap_or_else(|| "execute write-back suggestion".to_string()),
                "repo or tracker write-backs require fresh approval",
                "high",
            );
            return Ok(WorkflowPackExecution {
                payload: json!({
                    "status": "approval_required",
                    "pack_name": "pr-issue-triage",
                    "reason": "write-back actions are approval-gated",
                    "approval_id": pending.approval_id,
                }),
                pending_approval: Some(pending),
            });
        }

        let connectors = self.connector_catalog().await;
        let repo_connector = string_arg(args, &["repo_connector"])
            .or_else(|| choose_ready_connector(&connectors, &["github", "gitlab"]));
        let tracker_connector = string_arg(args, &["tracker_connector"])
            .filter(|value| value != "none")
            .or_else(|| choose_ready_connector(&connectors, &["linear", "jira"]));

        let repo_section = if let Some(preview) = text_arg(args, &["repo_preview"]) {
            PackSection::available(preview, json!({"source": "provided-preview"}))
        } else if let Some(repo_connector) = repo_connector.as_deref() {
            build_repo_triage_section(self, &connectors, repo_connector, args).await?
        } else {
            bail!("PR/Issue Triage requires a ready repo connector or a repo_preview override");
        };

        let tracker_section = if let Some(preview) = text_arg(args, &["tracker_preview"]) {
            PackSection::available(preview, json!({"source": "provided-preview"}))
        } else if let Some(tracker_connector) = tracker_connector.as_deref() {
            build_tracker_triage_section(self, &connectors, tracker_connector, args)
                .await
                .unwrap_or_else(|error| PackSection::degraded(error.to_string()))
        } else {
            PackSection::degraded(
                "Tracker enrichment unavailable; continuing with repo-only triage.",
            )
        };

        let suggestions = if bool_arg(args, "include_writeback_suggestions").unwrap_or(false) {
            vec![
                "approval-required: comment on blocked review items only after explicit approval"
                    .to_string(),
                "approval-required: assign or relabel stale work only after explicit approval"
                    .to_string(),
            ]
        } else {
            Vec::new()
        };

        Ok(WorkflowPackExecution {
            payload: json!({
                "status": "ok",
                "pack_name": "pr-issue-triage",
                "summary": format!(
                    "PR/Issue triage prepared with repo source {}{}.",
                    repo_connector.unwrap_or_else(|| "preview".to_string()),
                    tracker_connector
                        .as_ref()
                        .map(|value| format!(" and tracker source {value}"))
                        .unwrap_or_default(),
                ),
                "repo_section": repo_section.to_json(),
                "tracker_section": tracker_section.to_json(),
                "writeback_suggestions": suggestions,
                "approval_required_for_writebacks": true,
                "connector_health": connector_health_subset(&connectors, &["github", "gitlab", "linear", "jira"]),
            }),
            pending_approval: None,
        })
    }

    async fn run_inbox_calendar_triage_pack(&self, args: &Value) -> Result<WorkflowPackExecution> {
        if bool_arg(args, "send_reply").unwrap_or(false)
            || bool_arg(args, "create_event").unwrap_or(false)
            || bool_arg(args, "update_event").unwrap_or(false)
        {
            let pending = workflow_pack_pending_approval(
                "inbox-calendar-triage",
                "send replies or create/update events".to_string(),
                "reply sending and calendar mutations require fresh approval",
                "high",
            );
            return Ok(WorkflowPackExecution {
                payload: json!({
                    "status": "approval_required",
                    "pack_name": "inbox-calendar-triage",
                    "reason": "sending replies or changing calendar state is approval-gated",
                    "approval_id": pending.approval_id,
                }),
                pending_approval: Some(pending),
            });
        }

        let connectors = self.connector_catalog().await;
        let (time_min, time_max) = pack_time_range(args);
        let inbox_section = resolve_connector_section(
            text_arg(args, &["inbox_preview"]),
            connectors.get("gmail"),
            self,
            &connectors,
            "gmail",
            "list_inbox",
            json!({
                "max_results": 5,
                "query": string_arg(args, &["gmail_query"])
                    .unwrap_or_else(|| "in:inbox newer_than:7d".to_string()),
            }),
        )
        .await;
        let calendar_section = resolve_connector_section(
            text_arg(args, &["calendar_preview"]),
            connectors.get("calendar"),
            self,
            &connectors,
            "calendar",
            "list_schedule_items",
            json!({
                "calendar_id": string_arg(args, &["calendar_id"])
                    .unwrap_or_else(|| "primary".to_string()),
                "time_min": time_min,
                "time_max": time_max,
                "max_results": 6,
            }),
        )
        .await;

        if inbox_section.text.is_none() && calendar_section.text.is_none() {
            bail!(
                "Inbox + Calendar Triage requires at least one ready connector or preview override"
            );
        }

        let draft_suggestions = if bool_arg(args, "include_reply_drafts").unwrap_or(false)
            && inbox_section.text.is_some()
        {
            vec![
                "Draft-only suggestion: acknowledge the newest actionable thread after reviewing full context.".to_string(),
                "Draft-only suggestion: convert any commitment-bearing reply into an explicit draft for later approval.".to_string(),
            ]
        } else {
            Vec::new()
        };

        Ok(WorkflowPackExecution {
            payload: json!({
                "status": "ok",
                "pack_name": "inbox-calendar-triage",
                "summary": "Inbox + Calendar triage prepared with draft-only suggestions and graceful connector degradation.",
                "privacy_mode": string_arg(args, &["privacy_mode"]).unwrap_or_else(|| "standard".to_string()),
                "sections": {
                    "inbox": inbox_section.to_json(),
                    "calendar": calendar_section.to_json(),
                },
                "draft_suggestions": draft_suggestions,
                "approval_required_for_send_or_event_mutation": true,
                "connector_health": connector_health_subset(&connectors, &["gmail", "calendar"]),
            }),
            pending_approval: None,
        })
    }

    async fn run_watch_monitor_pack(&self, args: &Value) -> Result<WorkflowPackExecution> {
        if bool_arg(args, "remediate").unwrap_or(false)
            || bool_arg(args, "perform_writeback").unwrap_or(false)
        {
            let pending = workflow_pack_pending_approval(
                "watch-monitor",
                "execute remediation for a watch result".to_string(),
                "remediation or external side effects from a watch result require fresh approval",
                "high",
            );
            return Ok(WorkflowPackExecution {
                payload: json!({
                    "status": "approval_required",
                    "pack_name": "watch-monitor",
                    "reason": "watch remediation is approval-gated",
                    "approval_id": pending.approval_id,
                }),
                pending_approval: Some(pending),
            });
        }

        let connectors = self.connector_catalog().await;
        let watch_source =
            string_arg(args, &["watch_source"]).unwrap_or_else(|| "event".to_string());
        let previous_snapshot = pack_snapshot_arg(args, &["previous_snapshot", "previous_value"]);
        let current_snapshot = if let Some(snapshot) =
            pack_snapshot_arg(args, &["current_snapshot", "current_value"])
        {
            snapshot
        } else if let Some(payload) = args.get("payload") {
            stringify_value(payload)
        } else if watch_source == "repo" {
            if let Some(preview) = text_arg(args, &["repo_preview"]) {
                preview
            } else if let Some(repo_connector) = string_arg(args, &["repo_connector"])
                .or_else(|| choose_ready_connector(&connectors, &["github", "gitlab"]))
            {
                let repo_section =
                    build_repo_triage_section(self, &connectors, &repo_connector, args).await?;
                repo_section
                    .text
                    .unwrap_or_else(|| "(empty repo snapshot)".to_string())
            } else {
                bail!("Watch/Monitor repo source requires repo_preview or a ready repo connector");
            }
        } else {
            bail!("Watch/Monitor requires current_snapshot/current_value/payload input");
        };

        let suppression_rules = string_array_arg(args, "suppression_rules");
        let unchanged = previous_snapshot.as_deref() == Some(current_snapshot.as_str());
        let suppressed = unchanged
            || suppression_rules.iter().any(|rule| {
                let normalized = rule.trim().to_ascii_lowercase();
                normalized == "suppress-unchanged"
                    || (!normalized.is_empty()
                        && current_snapshot.to_ascii_lowercase().contains(&normalized))
            });

        let (meaningful_change, summary, baseline) = if previous_snapshot.is_none() {
            (
                false,
                "No previous snapshot was available; emitted a baseline snapshot instead of an alert.".to_string(),
                true,
            )
        } else if suppressed {
            (
                false,
                "Change was suppressed by watch rules or because the snapshot did not meaningfully change.".to_string(),
                false,
            )
        } else {
            (
                true,
                "Meaningful change detected and surfaced as a mobile-safe watch summary."
                    .to_string(),
                false,
            )
        };

        Ok(WorkflowPackExecution {
            payload: json!({
                "status": "ok",
                "pack_name": "watch-monitor",
                "watch_source": watch_source,
                "baseline": baseline,
                "meaningful_change": meaningful_change,
                "suppressed": suppressed,
                "summary": summary,
                "source_ref": string_arg(args, &["source_ref", "path", "url", "event_kind"]),
                "previous_snapshot": previous_snapshot,
                "current_snapshot": current_snapshot,
                "suppression_rules": suppression_rules,
                "connector_health": connector_health_subset(&connectors, &["github", "gitlab"]),
            }),
            pending_approval: None,
        })
    }

    async fn run_approval_checkpoint_long_task_pack(
        &self,
        args: &Value,
        thread_id: Option<&str>,
        parent_task_id: Option<&str>,
    ) -> Result<WorkflowPackExecution> {
        let task_kind = string_arg(args, &["task_kind"]).unwrap_or_else(|| "task".to_string());
        let checkpoint_titles = string_array_arg(args, "checkpoint_titles");
        let rollback_notes = string_array_arg(args, "rollback_notes");
        let summary_cadence = string_arg(args, &["summary_cadence"])
            .unwrap_or_else(|| "before each checkpoint".to_string());
        let first_checkpoint = checkpoint_titles
            .first()
            .cloned()
            .unwrap_or_else(|| "checkpoint-1".to_string());
        let title = string_arg(args, &["title"])
            .unwrap_or_else(|| "Approval-Checkpoint Long Task".to_string());
        let priority = string_arg(args, &["priority"]).unwrap_or_else(|| "normal".to_string());
        let effective_thread_id = thread_id.unwrap_or("system");

        match task_kind.as_str() {
            "goal" => {
                let goal = string_arg(args, &["goal"]).unwrap_or_else(|| {
                    format!(
                        "Use the Approval-Checkpoint Long Task pack with checkpoints: {}",
                        checkpoint_titles.join(", ")
                    )
                });
                let goal_text = format!(
                    "{goal}\n\nCheckpoint cadence: {summary_cadence}\nCheckpoints:\n{}\nRollback notes:\n{}",
                    bullet_block(&checkpoint_titles),
                    bullet_block(&rollback_notes),
                );
                let goal_run = self
                    .start_goal_run_with_surface_and_approval_policy(
                        goal_text,
                        Some(title.clone()),
                        Some(effective_thread_id.to_string()),
                        None,
                        Some(priority.as_str()),
                        None,
                        Some("supervised".to_string()),
                        None,
                        true,
                        None,
                    )
                    .await;
                Ok(WorkflowPackExecution {
                    payload: json!({
                        "status": "ok",
                        "pack_name": "approval-checkpoint-long-task",
                        "task_kind": "goal",
                        "summary": "Created a supervised goal run with approval checkpoints.",
                        "goal_run": goal_run,
                        "checkpoint_titles": checkpoint_titles,
                        "rollback_notes": rollback_notes,
                    }),
                    pending_approval: None,
                })
            }
            "task" => {
                let task_description = format!(
                    "Governed long task created by the Approval-Checkpoint Long Task pack.\n\nSummary cadence: {summary_cadence}\nCheckpoints:\n{}\nRollback notes:\n{}",
                    bullet_block(&checkpoint_titles),
                    bullet_block(&rollback_notes),
                );
                let task = self
                    .enqueue_task(
                        title.clone(),
                        task_description,
                        &priority,
                        None,
                        None,
                        Vec::new(),
                        None,
                        "workflow_pack",
                        None,
                        parent_task_id.map(ToOwned::to_owned),
                        Some(effective_thread_id.to_string()),
                        None,
                    )
                    .await;
                let pending = workflow_pack_pending_approval(
                    "approval-checkpoint-long-task",
                    format!("enter checkpoint `{first_checkpoint}`"),
                    "the first risky transition must pause for explicit approval",
                    "medium",
                );
                self.remember_pending_approval_command(&pending).await;
                self.record_operator_approval_requested(&pending).await?;
                if !self
                    .auto_approve_task_if_rule_matches(&task.id, effective_thread_id, &pending)
                    .await
                {
                    self.mark_task_awaiting_approval(&task.id, effective_thread_id, &pending)
                        .await;
                }
                let created_task = self
                    .list_tasks()
                    .await
                    .into_iter()
                    .find(|candidate| candidate.id == task.id)
                    .unwrap_or(task);
                Ok(WorkflowPackExecution {
                    payload: json!({
                        "status": "ok",
                        "pack_name": "approval-checkpoint-long-task",
                        "task_kind": "task",
                        "summary": "Created a governed daemon task and paused it at the first approval checkpoint.",
                        "created_task": created_task,
                        "approval_id": pending.approval_id,
                        "checkpoint_titles": checkpoint_titles,
                        "rollback_notes": rollback_notes,
                    }),
                    pending_approval: None,
                })
            }
            other => bail!("unsupported task_kind `{other}` for approval-checkpoint-long-task"),
        }
    }

    async fn run_standup_pack(&self, args: &Value) -> Result<WorkflowPackExecution> {
        let delivery_channel =
            string_arg(args, &["delivery_channel"]).unwrap_or_else(|| "in-app".to_string());
        let deliver_now = bool_arg(args, "deliver_now").unwrap_or(false);
        if deliver_now && delivery_channel != "in-app" {
            let pending = workflow_pack_pending_approval(
                "standup",
                format!("deliver standup via {delivery_channel}"),
                "message delivery is an external side effect",
                "medium",
            );
            return Ok(WorkflowPackExecution {
                payload: json!({
                    "status": "approval_required",
                    "pack_name": "standup",
                    "reason": "external delivery requires fresh approval",
                    "delivery_channel": delivery_channel,
                    "approval_id": pending.approval_id,
                }),
                pending_approval: Some(pending),
            });
        }

        let workspace_id =
            string_arg(args, &["workspace_id"]).unwrap_or_else(|| "main".to_string());
        let mode = string_arg(args, &["mode"]).unwrap_or_else(|| "standard".to_string());
        let connectors = self.connector_catalog().await;
        let tasks = self
            .list_workspace_tasks(&workspace_id, false)
            .await
            .unwrap_or_default();
        let notices = self
            .list_workspace_notices(&workspace_id, None)
            .await
            .unwrap_or_default();
        let routines = self
            .history
            .list_routine_definitions()
            .await
            .unwrap_or_default();
        let pending_approvals = self.pending_operator_approvals.read().await;
        let pending_approval_count = pending_approvals.len();
        drop(pending_approvals);

        // Trigger fire activity (last 24h)
        let trigger_fires = self
            .history
            .list_trigger_fire_history(None, None, 50)
            .await
            .unwrap_or_default();
        let recent_fires: Vec<_> = trigger_fires
            .iter()
            .filter(|fire| {
                let age_ms = Utc::now().timestamp_millis() as u64 - fire.fired_at_ms;
                age_ms < 86_400_000 // 24h
            })
            .take(10)
            .collect();

        // Browser profile health
        let browser_profiles = self
            .list_browser_profiles_with_current_health()
            .await
            .unwrap_or_default();
        let unhealthy_browser_profiles: Vec<_> = browser_profiles
            .iter()
            .filter(|profile| profile.health_state != "healthy")
            .collect();

        // Build sections
        let task_summary = summarize_workspace_tasks(&tasks);
        let routine_health = summarize_routines(&routines);
        let notice_lines: Vec<_> = notices
            .iter()
            .take(5)
            .map(|notice| format!("- [{}] {}", notice.notice_type, notice.message))
            .collect();

        let trigger_summary: Vec<_> = recent_fires
            .iter()
            .map(|fire| {
                format!(
                    "- {} trigger `{}` [{}]",
                    fire.event_family, fire.event_kind, fire.status,
                )
            })
            .collect();

        let browser_health_summary: Vec<_> = if unhealthy_browser_profiles.is_empty() {
            vec!["- All browser profiles healthy.".to_string()]
        } else {
            unhealthy_browser_profiles
                .iter()
                .map(|profile| {
                    let reason = profile
                        .last_auth_failure_reason
                        .as_deref()
                        .unwrap_or("no failure detail");
                    format!(
                        "- {} is [{}] — {}",
                        profile.label, profile.health_state, reason,
                    )
                })
                .collect()
        };

        // Knowledge connector path
        let knowledge_connectors: Vec<_> = connectors
            .iter()
            .filter(|(_, info)| {
                // Show knowledge connectors and any connector that isn't a standard repo/tracker/mail/calendar
                let kind = info.plugin_name.to_ascii_lowercase();
                kind.contains("notion") || kind.contains("confluence") || kind.contains("knowledge")
            })
            .map(|(kind, info)| {
                json!({
                    "kind": kind,
                    "plugin_name": info.plugin_name,
                    "readiness_state": info.readiness_state,
                    "readiness_message": info.readiness_message,
                    "setup_hint": info.setup_hint,
                    "docs_path": info.docs_path,
                })
            })
            .collect();

        let knowledge_connector_path = if knowledge_connectors.is_empty() {
            json!({
                "status": "not_configured",
                "message": "No knowledge connectors (Notion, Confluence) are currently installed. Install the Notion or Confluence plugin to enable knowledge-system integration for daily standup and research workflows.",
                "available_connectors": ["notion", "confluence"],
                "setup_guidance": "Install via the plugin system. Once installed, they will appear here with readiness state, setup hints, and workflow primitives."
            })
        } else {
            json!({
                "status": "configured",
                "connectors": knowledge_connectors,
                "message": "Knowledge connectors are installed. Check readiness state for each."
            })
        };

        // All connector health (not just subset)
        let all_connector_health: Vec<_> = connectors
            .iter()
            .map(|(kind, info)| {
                json!({
                    "kind": kind,
                    "plugin_name": info.plugin_name,
                    "readiness_state": info.readiness_state,
                    "readiness_message": info.readiness_message,
                    "setup_hint": info.setup_hint,
                    "docs_path": info.docs_path,
                    "read_actions": info.read_actions,
                    "write_actions": info.write_actions,
                })
            })
            .collect();

        let has_signal = !task_summary.is_empty()
            || !routine_health.is_empty()
            || !notice_lines.is_empty()
            || !trigger_summary.is_empty()
            || !unhealthy_browser_profiles.is_empty()
            || pending_approval_count > 0;

        let summary = if has_signal {
            format!(
                "Standup ({mode}): {} active task(s), {} pending approval(s), {} routine signal(s), {} trigger(s) in 24h, {} browser alert(s), {} notice(s), {} connector(s).",
                task_summary.len(),
                pending_approval_count,
                routine_health.len(),
                trigger_summary.len(),
                unhealthy_browser_profiles.len(),
                notice_lines.len(),
                all_connector_health.len(),
            )
        } else {
            "Standup: quiet period with no active tasks, approvals, routine signals, recent triggers, browser alerts, or notices."
                .to_string()
        };

        Ok(WorkflowPackExecution {
            payload: json!({
                "status": "ok",
                "pack_name": "standup",
                "mode": mode,
                "workspace_id": workspace_id,
                "delivery_channel": delivery_channel,
                "mobile_safe": true,
                "summary": summary,
                "sections": {
                    "tasks": task_summary,
                    "pending_approvals": pending_approval_count,
                    "routine_health": routine_health,
                    "recent_triggers": trigger_summary,
                    "browser_health": browser_health_summary,
                    "notices": notice_lines,
                    "knowledge_connectors": knowledge_connector_path,
                },
                "connector_health": all_connector_health,
            }),
            pending_approval: None,
        })
    }

    pub(crate) async fn get_cost_summary_json(
        &self,
        window: Option<&str>,
    ) -> Result<serde_json::Value> {
        let window = match window.unwrap_or("last7days") {
            "today" => zorai_protocol::AgentStatisticsWindow::Today,
            "last7days" | "last-7-days" | "7days" => zorai_protocol::AgentStatisticsWindow::Last7Days,
            "last30days" | "last-30-days" | "30days" => zorai_protocol::AgentStatisticsWindow::Last30Days,
            "all" | "alltime" | "all-time" => zorai_protocol::AgentStatisticsWindow::All,
            other => bail!("unsupported cost summary window `{other}`; use today, last7days, last30days, or all"),
        };

        let stats = self.history.get_agent_statistics(window).await?;

        // Recent activity context
        let tasks = self.list_tasks().await;
        let recent_tasks: Vec<_> = tasks
            .iter()
            .take(10)
            .map(|task| {
                json!({
                    "id": task.id,
                    "title": task.title,
                    "status": format!("{:?}", task.status),
                    "priority": format!("{:?}", task.priority),
                })
            })
            .collect();

        let routines = self
            .history
            .list_routine_definitions()
            .await
            .unwrap_or_default();
        let routine_summary: Vec<_> = routines
            .iter()
            .take(5)
            .map(|routine| {
                json!({
                    "id": routine.id,
                    "title": routine.title,
                    "enabled": routine.enabled,
                    "last_result": routine.last_result,
                })
            })
            .collect();

        let trigger_fires = self
            .history
            .list_trigger_fire_history(None, None, 20)
            .await
            .unwrap_or_default();
        let recent_trigger_summary: Vec<_> = trigger_fires
            .iter()
            .take(10)
            .map(|fire| {
                json!({
                    "trigger_id": fire.trigger_id,
                    "event_family": fire.event_family,
                    "event_kind": fire.event_kind,
                    "status": fire.status,
                    "fired_at_ms": fire.fired_at_ms,
                    "retry_count": fire.retry_count,
                })
            })
            .collect();

        Ok(json!({
            "window": format!("{:?}", stats.window),
            "generated_at": stats.generated_at,
            "has_incomplete_cost_history": stats.has_incomplete_cost_history,
            "totals": {
                "input_tokens": stats.totals.input_tokens,
                "output_tokens": stats.totals.output_tokens,
                "total_tokens": stats.totals.total_tokens,
                "cost_usd": stats.totals.cost_usd,
                "provider_count": stats.totals.provider_count,
                "model_count": stats.totals.model_count,
            },
            "providers": stats.providers.iter().map(|p| json!({
                "provider": p.provider,
                "input_tokens": p.input_tokens,
                "output_tokens": p.output_tokens,
                "total_tokens": p.total_tokens,
                "cost_usd": p.cost_usd,
            })).collect::<Vec<_>>(),
            "top_models_by_cost": stats.top_models_by_cost.iter().map(|m| json!({
                "provider": m.provider,
                "model": m.model,
                "total_tokens": m.total_tokens,
                "cost_usd": m.cost_usd,
            })).collect::<Vec<_>>(),
            "top_models_by_tokens": stats.top_models_by_tokens.iter().map(|m| json!({
                "provider": m.provider,
                "model": m.model,
                "total_tokens": m.total_tokens,
                "cost_usd": m.cost_usd,
            })).collect::<Vec<_>>(),
            "recent_activity": {
                "tasks": recent_tasks,
                "routines": routine_summary,
                "recent_triggers": recent_trigger_summary,
            },
            "replay_guidance": {
                "message": "Use thread history and task/goal inspection to drill into specific activity. This summary shows aggregate costs and recent system activity. For per-task or per-thread cost detail, query individual threads and tasks.",
                "drill_down_tools": [zorai_protocol::tool_names::LIST_TASKS, zorai_protocol::tool_names::LIST_GOAL_RUNS, zorai_protocol::tool_names::LIST_ROUTINE_HISTORY, zorai_protocol::tool_names::LIST_TRIGGER_FIRE_HISTORY, zorai_protocol::tool_names::SESSION_SEARCH]
            }
        }))
    }
}

#[derive(Debug, Clone)]
struct PackSection {
    state: String,
    text: Option<String>,
    metadata: Value,
}

impl PackSection {
    fn available(text: String, metadata: Value) -> Self {
        Self {
            state: "available".to_string(),
            text: Some(text),
            metadata,
        }
    }

    fn degraded(message: impl Into<String>) -> Self {
        Self {
            state: "degraded".to_string(),
            text: Some(message.into()),
            metadata: Value::Null,
        }
    }

    fn omitted(message: impl Into<String>) -> Self {
        Self {
            state: "omitted".to_string(),
            text: Some(message.into()),
            metadata: Value::Null,
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "state": self.state,
            "text": self.text,
            "metadata": self.metadata,
        })
    }
}

async fn resolve_connector_section(
    preview: Option<String>,
    connector: Option<&ConnectorInfo>,
    engine: &AgentEngine,
    connectors: &HashMap<String, ConnectorInfo>,
    connector_kind: &str,
    endpoint_name: &str,
    params: Value,
) -> PackSection {
    if let Some(preview) = preview {
        return PackSection::available(preview, json!({"source": "preview"}));
    }

    let Some(connector) = connector else {
        return PackSection::degraded("Required connector is unavailable.");
    };

    if !connector.is_ready() {
        return PackSection::degraded(connector_message(connector));
    }

    match engine
        .connector_read_text(connectors, connector_kind, endpoint_name, params)
        .await
    {
        Ok(text) => PackSection::available(
            text,
            json!({
                "source": "connector",
                "docs_path": connector.docs_path,
                "readiness_state": connector.readiness_state,
            }),
        ),
        Err(error) => PackSection::degraded(error.to_string()),
    }
}

async fn build_repo_triage_section(
    engine: &AgentEngine,
    connectors: &HashMap<String, ConnectorInfo>,
    repo_connector: &str,
    args: &Value,
) -> Result<PackSection> {
    let text = match repo_connector {
        "github" => {
            let owner = string_arg(args, &["owner"])
                .ok_or_else(|| anyhow!("missing owner for github triage"))?;
            let repo = string_arg(args, &["repo"])
                .ok_or_else(|| anyhow!("missing repo for github triage"))?;
            let review_items = engine
                .connector_read_text(
                    connectors,
                    "github",
                    "list_review_items",
                    json!({
                        "owner": owner,
                        "repo": repo,
                        "per_page": 20,
                    }),
                )
                .await?;
            let work_items = engine
                .connector_read_text(
                    connectors,
                    "github",
                    "list_work_items",
                    json!({
                        "owner": string_arg(args, &["owner"]).unwrap_or_default(),
                        "repo": string_arg(args, &["repo"]).unwrap_or_default(),
                        "per_page": 20,
                    }),
                )
                .await?;
            format!("{review_items}\n\n{work_items}")
        }
        "gitlab" => {
            let project = string_arg(args, &["project"])
                .ok_or_else(|| anyhow!("missing project for gitlab triage"))?;
            let review_items = engine
                .connector_read_text(
                    connectors,
                    "gitlab",
                    "list_review_items",
                    json!({
                        "project": project,
                        "per_page": 20,
                    }),
                )
                .await?;
            let work_items = engine
                .connector_read_text(
                    connectors,
                    "gitlab",
                    "list_work_items",
                    json!({
                        "project": string_arg(args, &["project"]).unwrap_or_default(),
                        "per_page": 20,
                    }),
                )
                .await?;
            format!("{review_items}\n\n{work_items}")
        }
        other => bail!("unsupported repo connector `{other}`"),
    };

    Ok(PackSection::available(
        text,
        json!({ "source": repo_connector }),
    ))
}

async fn build_tracker_triage_section(
    engine: &AgentEngine,
    connectors: &HashMap<String, ConnectorInfo>,
    tracker_connector: &str,
    args: &Value,
) -> Result<PackSection> {
    let text = match tracker_connector {
        "linear" => {
            engine
                .connector_read_text(
                    connectors,
                    "linear",
                    "list_work_items",
                    json!({ "first": 20 }),
                )
                .await?
        }
        "jira" => {
            engine
                .connector_read_text(
                    connectors,
                    "jira",
                    "list_work_items",
                    json!({
                        "project_key": string_arg(args, &["project_key", "jira_project_key"]).unwrap_or_default(),
                        "max_results": 20,
                    }),
                )
                .await?
        }
        other => bail!("unsupported tracker connector `{other}`"),
    };

    Ok(PackSection::available(
        text,
        json!({ "source": tracker_connector }),
    ))
}

fn normalized_pack_name(args: &Value) -> Result<String> {
    let raw = string_arg(args, &["pack_name", "pack", "name"])
        .ok_or_else(|| anyhow!("missing `pack_name` argument"))?;
    let normalized = raw
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-")
        .replace(' ', "-");
    let normalized = match normalized.as_str() {
        "dailybrief" => "daily-brief".to_string(),
        "prissue-triage" | "pr-issue-triage" => "pr-issue-triage".to_string(),
        "inbox-calendar" | "inbox-calendar-triage" => "inbox-calendar-triage".to_string(),
        "watch" | "watch-monitor" | "monitor" => "watch-monitor".to_string(),
        "standup" | "status-report" | "status" => "standup".to_string(),
        "approval-checkpoint" | "approval-checkpoint-long-task" => {
            "approval-checkpoint-long-task".to_string()
        }
        other => other.to_string(),
    };
    Ok(normalized)
}

fn workflow_pack_pending_approval(
    pack_name: &str,
    action: String,
    rationale: &str,
    risk_level: &str,
) -> ToolPendingApproval {
    ToolPendingApproval {
        approval_id: format!("workflow-pack-approval-{}", uuid::Uuid::new_v4()),
        execution_id: format!("workflow-pack-exec-{}", uuid::Uuid::new_v4()),
        command: format!("run_workflow_pack {pack_name} -- {action}"),
        rationale: rationale.to_string(),
        risk_level: risk_level.to_string(),
        blast_radius: "external connector or delivery surface".to_string(),
        reasons: vec![format!(
            "workflow pack `{pack_name}` requested an approval-gated side effect"
        )],
        session_id: None,
    }
}

fn string_arg(args: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        args.get(*key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn text_arg(args: &Value, keys: &[&str]) -> Option<String> {
    string_arg(args, keys)
}

fn bool_arg(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(|value| value.as_bool())
}

fn string_array_arg(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn pack_snapshot_arg(args: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| args.get(*key))
        .map(stringify_value)
}

fn stringify_value(value: &Value) -> String {
    value.as_str().map(ToOwned::to_owned).unwrap_or_else(|| {
        serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
    })
}

fn pack_time_range(args: &Value) -> (String, String) {
    let now = Utc::now();
    let upper = match string_arg(args, &["time_window"]).as_deref() {
        Some("next 8h") => now + ChronoDuration::hours(8),
        Some("next 24h") => now + ChronoDuration::hours(24),
        _ => now + ChronoDuration::hours(24),
    };
    (
        now.to_rfc3339_opts(SecondsFormat::Secs, true),
        upper.to_rfc3339_opts(SecondsFormat::Secs, true),
    )
}

fn summarize_workspace_tasks(tasks: &[WorkspaceTask]) -> Vec<String> {
    let mut prioritized = tasks
        .iter()
        .filter(|task| {
            task.status == WorkspaceTaskStatus::InProgress
                || task.status == WorkspaceTaskStatus::Todo
        })
        .collect::<Vec<_>>();
    prioritized.sort_by_key(|task| (workspace_priority_rank(&task.priority), task.sort_order));
    prioritized
        .into_iter()
        .take(5)
        .map(|task| {
            format!(
                "- {} [{} {:?}]",
                task.title,
                workspace_priority_label(&task.priority),
                task.status
            )
        })
        .collect()
}

fn summarize_routines(routines: &[crate::history::RoutineDefinitionRow]) -> Vec<String> {
    routines
        .iter()
        .take(5)
        .map(|routine| {
            let status = routine
                .last_result
                .as_deref()
                .unwrap_or(if routine.enabled {
                    "never-run"
                } else {
                    "disabled"
                });
            format!("- {} [{}]", routine.title, status)
        })
        .collect()
}

fn connector_message(connector: &ConnectorInfo) -> String {
    connector
        .readiness_message
        .clone()
        .or_else(|| connector.setup_hint.clone())
        .unwrap_or_else(|| "Connector is unavailable.".to_string())
}

fn connector_health_subset(connectors: &HashMap<String, ConnectorInfo>, kinds: &[&str]) -> Value {
    Value::Array(
        kinds
            .iter()
            .filter_map(|kind| {
                let connector = connectors.get(*kind)?;
                Some(json!({
                    "kind": kind,
                    "plugin_name": connector.plugin_name,
                    "readiness_state": connector.readiness_state,
                    "readiness_message": connector.readiness_message,
                    "setup_hint": connector.setup_hint,
                    "docs_path": connector.docs_path,
                    "read_actions": connector.read_actions,
                    "write_actions": connector.write_actions,
                }))
            })
            .collect(),
    )
}

fn choose_ready_connector(
    connectors: &HashMap<String, ConnectorInfo>,
    preferred: &[&str],
) -> Option<String> {
    preferred.iter().find_map(|kind| {
        connectors
            .get(*kind)
            .filter(|connector| connector.is_ready())
            .map(|_| (*kind).to_string())
    })
}

fn bullet_block(lines: &[String]) -> String {
    if lines.is_empty() {
        "- (none)".to_string()
    } else {
        lines
            .iter()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn workspace_priority_rank(priority: &WorkspacePriority) -> u8 {
    match priority {
        WorkspacePriority::Urgent => 0,
        WorkspacePriority::High => 1,
        WorkspacePriority::Normal => 2,
        WorkspacePriority::Low => 3,
    }
}
