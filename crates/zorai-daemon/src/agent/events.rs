use super::*;
use crate::history::{EventLogRow, EventTriggerRow};

impl AgentEngine {
    pub(crate) async fn ensure_default_event_triggers(&self) -> Result<usize> {
        let now = now_millis();
        let defaults = vec![
            EventTriggerRow {
                id: "trigger-health-weles-degraded".to_string(),
                event_family: "health".to_string(),
                event_kind: "weles_health".to_string(),
                agent_id: Some("weles".to_string()),
                target_state: Some("degraded".to_string()),
                thread_id: None,
                enabled: true,
                cooldown_secs: 900,
                risk_label: "medium".to_string(),
                notification_kind: "weles_health_degraded".to_string(),
                prompt_template: Some(
                    "WELES health changed to {state}. Review the reason, diagnose the failing background path, and take the safest recovery action."
                        .to_string(),
                ),
                tool_name: None,
                tool_payload_json: None,
                title_template: "WELES health degraded".to_string(),
                body_template:
                    "WELES health changed to {state}. Review reason and consider intervention."
                        .to_string(),
                created_at: now,
                updated_at: now,
                last_fired_at: None,
                max_retries: 3,
            },
            EventTriggerRow {
                id: "trigger-health-subagent-stuck".to_string(),
                event_family: "health".to_string(),
                event_kind: "subagent_health".to_string(),
                agent_id: Some("weles".to_string()),
                target_state: Some("stuck".to_string()),
                thread_id: None,
                enabled: true,
                cooldown_secs: 300,
                risk_label: "medium".to_string(),
                notification_kind: "subagent_health_stuck".to_string(),
                prompt_template: Some(
                    "Subagent {task_id} entered {state} because {reason}. Assess recovery options, prefer safe automated continuation, and escalate only if recovery is not credible."
                        .to_string(),
                ),
                tool_name: None,
                tool_payload_json: None,
                title_template: "Subagent stuck".to_string(),
                body_template:
                    "Subagent {task_id} entered {state}. Review reason and decide whether to intervene."
                        .to_string(),
                created_at: now,
                updated_at: now,
                last_fired_at: None,
                max_retries: 3,
            },
            EventTriggerRow {
                id: "trigger-filesystem-file-changed".to_string(),
                event_family: "filesystem".to_string(),
                event_kind: "file_changed".to_string(),
                agent_id: Some("weles".to_string()),
                target_state: Some("detected".to_string()),
                thread_id: None,
                enabled: true,
                cooldown_secs: 300,
                risk_label: "low".to_string(),
                notification_kind: "file_changed".to_string(),
                prompt_template: Some(
                    "The file at {path} changed. Review whether the operator likely needs follow-up."
                        .to_string(),
                ),
                tool_name: None,
                tool_payload_json: None,
                title_template: "File changed: {path}".to_string(),
                body_template: "Observed file change for {path}".to_string(),
                created_at: now,
                updated_at: now,
                last_fired_at: None,
                max_retries: 3,
            },
            EventTriggerRow {
                id: "trigger-system-disk-pressure".to_string(),
                event_family: "system".to_string(),
                event_kind: "disk_pressure".to_string(),
                agent_id: Some("weles".to_string()),
                target_state: Some("critical".to_string()),
                thread_id: None,
                enabled: true,
                cooldown_secs: 600,
                risk_label: "high".to_string(),
                notification_kind: "disk_pressure".to_string(),
                prompt_template: Some(
                    "Disk pressure detected on {mount} at {usage_pct}. Investigate and suggest cleanup actions."
                        .to_string(),
                ),
                tool_name: None,
                tool_payload_json: None,
                title_template: "Disk pressure on {mount}".to_string(),
                body_template: "Disk usage on {mount} is {usage_pct}".to_string(),
                created_at: now,
                updated_at: now,
                last_fired_at: None,
                max_retries: 3,
            },
        ];

        for row in &defaults {
            self.history.upsert_event_trigger(row).await?;
        }

        Ok(defaults.len())
    }

    pub(crate) async fn list_event_triggers_json(&self) -> Result<serde_json::Value> {
        let rows = self.history.list_event_triggers(None, None).await?;
        Ok(serde_json::json!(rows
            .into_iter()
            .map(|row| {
                let source = match row.id.as_str() {
                    "trigger-health-weles-degraded"
                    | "trigger-health-subagent-stuck"
                    | "trigger-filesystem-file-changed"
                    | "trigger-system-disk-pressure" => "packaged_default",
                    _ => "custom",
                };
                serde_json::json!({
                    "id": row.id,
                    "event_family": row.event_family,
                    "event_kind": row.event_kind,
                    "agent_id": row.agent_id,
                    "target_state": row.target_state,
                    "thread_id": row.thread_id,
                    "enabled": row.enabled,
                    "cooldown_secs": row.cooldown_secs,
                    "risk_label": row.risk_label,
                    "notification_kind": row.notification_kind,
                    "prompt_template": row.prompt_template,
                    "tool_name": row.tool_name,
                    "tool_payload": row.tool_payload_json.as_deref().and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok()),
                    "title_template": row.title_template,
                    "body_template": row.body_template,
                    "created_at": row.created_at,
                    "updated_at": row.updated_at,
                    "last_fired_at": row.last_fired_at,
                    "source": source,
                })
            })
            .collect::<Vec<_>>()))
    }

    pub(crate) async fn add_event_trigger_from_args(
        &self,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let event_family = args
            .get("event_family")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("missing 'event_family' argument"))?;
        let event_kind = args
            .get("event_kind")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("missing 'event_kind' argument"))?;
        let notification_kind = args
            .get("notification_kind")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("missing 'notification_kind' argument"))?;
        let title_template = args
            .get("title_template")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("missing 'title_template' argument"))?;
        let body_template = args
            .get("body_template")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("missing 'body_template' argument"))?;
        let cooldown_secs = args
            .get("cooldown_secs")
            .and_then(|value| value.as_u64())
            .unwrap_or(300);
        let risk_label = args
            .get("risk_label")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| matches!(*value, "low" | "medium" | "high"))
            .unwrap_or("low");
        let agent_id = args
            .get("agent_id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let target_state = args
            .get("target_state")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let thread_id = args
            .get("thread_id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let enabled = args
            .get("enabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        let prompt_template = args
            .get("prompt_template")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let tool_name = args
            .get("tool_name")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let tool_payload_json = match args.get("tool_payload") {
            None => None,
            Some(serde_json::Value::Object(map)) => {
                Some(serde_json::Value::Object(map.clone()).to_string())
            }
            Some(_) => anyhow::bail!("'tool_payload' must be an object when provided"),
        };
        if tool_payload_json.is_some() && tool_name.is_none() {
            anyhow::bail!("'tool_payload' requires 'tool_name'");
        }
        let trigger_id = args
            .get("id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("trigger-{}-{}", event_family, uuid::Uuid::new_v4()));
        let now = now_millis();

        let row = EventTriggerRow {
            id: trigger_id.clone(),
            event_family: event_family.to_string(),
            event_kind: event_kind.to_string(),
            agent_id,
            target_state,
            thread_id,
            enabled,
            cooldown_secs,
            risk_label: risk_label.to_string(),
            notification_kind: notification_kind.to_string(),
            prompt_template,
            tool_name,
            tool_payload_json,
            title_template: title_template.to_string(),
            body_template: body_template.to_string(),
            created_at: now,
            updated_at: now,
            last_fired_at: None,
            max_retries: 3,
        };

        self.history.upsert_event_trigger(&row).await?;
        Ok(serde_json::json!({
            "status": "created",
            "trigger": {
                "id": row.id,
                "event_family": row.event_family,
                "event_kind": row.event_kind,
                "agent_id": row.agent_id,
                "target_state": row.target_state,
                "thread_id": row.thread_id,
                "enabled": row.enabled,
                "cooldown_secs": row.cooldown_secs,
                "risk_label": row.risk_label,
                "notification_kind": row.notification_kind,
                "prompt_template": row.prompt_template,
                "tool_name": row.tool_name,
                "tool_payload": row.tool_payload_json.as_deref().and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok()),
                "title_template": row.title_template,
                "body_template": row.body_template,
                "created_at": row.created_at,
                "updated_at": row.updated_at,
                "last_fired_at": row.last_fired_at,
                "source": "custom",
            }
        }))
    }

    pub(crate) async fn ingest_webhook_event_json(
        &self,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let event_family = args
            .get("event_family")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("missing 'event_family' argument"))?;
        let event_kind = args
            .get("event_kind")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("missing 'event_kind' argument"))?;
        let state = args
            .get("state")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let thread_id = args
            .get("thread_id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let payload = args
            .get("payload")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        let fired = self
            .maybe_fire_event_trigger(event_family, event_kind, state, thread_id, payload.clone())
            .await?;

        Ok(serde_json::json!({
            "status": "accepted",
            "event_family": event_family,
            "event_kind": event_kind,
            "state": state,
            "thread_id": thread_id,
            "fired": fired,
            "payload": payload,
        }))
    }

    pub(crate) async fn maybe_fire_event_trigger(
        &self,
        event_family: &str,
        event_kind: &str,
        state: Option<&str>,
        thread_id: Option<&str>,
        context: serde_json::Value,
    ) -> Result<usize> {
        let rows = self
            .history
            .list_event_triggers(Some(event_family), Some(event_kind))
            .await?;
        let now = now_millis();
        let mut fired = 0usize;

        for row in rows {
            if !row.enabled {
                continue;
            }
            if row.target_state.as_deref().is_some() && row.target_state.as_deref() != state {
                continue;
            }
            if row.thread_id.as_deref().is_some() && row.thread_id.as_deref() != thread_id {
                continue;
            }
            if let Some(last_fired_at) = row.last_fired_at {
                let cooldown_ms = row.cooldown_secs.saturating_mul(1000);
                if now < last_fired_at.saturating_add(cooldown_ms) {
                    // Record cooldown suppression
                    if let Err(error) = self
                        .history
                        .insert_trigger_fire_history(&crate::history::TriggerFireHistoryRow {
                            id: format!("trigger_fire_{}", uuid::Uuid::new_v4()),
                            trigger_id: row.id.clone(),
                            event_family: row.event_family.clone(),
                            event_kind: row.event_kind.clone(),
                            status: "suppressed".to_string(),
                            fired_at_ms: now,
                            completed_at_ms: None,
                            retry_count: 0,
                            error_message: Some("trigger suppressed by cooldown".to_string()),
                            created_task_id: None,
                            notice_id: None,
                            payload_json: serde_json::to_string(&context).unwrap_or_default(),
                        })
                        .await
                    {
                        tracing::warn!(
                            trigger_id = %row.id,
                            error = %error,
                            "failed to record suppressed trigger fire history"
                        );
                    }
                    continue;
                }
            }

            // Determine retry count from recent failed attempts for this trigger + event context
            let recent_attempts = self
                .history
                .list_trigger_fire_history(Some(&row.id), Some("failed"), 10)
                .await
                .unwrap_or_default();
            let retry_count = recent_attempts.len() as u64;
            let is_dead = retry_count >= row.max_retries as u64;

            if is_dead {
                // Record dead_letter and skip execution
                if let Err(error) = self
                    .history
                    .insert_trigger_fire_history(&crate::history::TriggerFireHistoryRow {
                        id: format!("trigger_fire_{}", uuid::Uuid::new_v4()),
                        trigger_id: row.id.clone(),
                        event_family: row.event_family.clone(),
                        event_kind: row.event_kind.clone(),
                        status: "dead_letter".to_string(),
                        fired_at_ms: now,
                        completed_at_ms: Some(now),
                        retry_count,
                        error_message: Some(format!("max retries ({}) exhausted", row.max_retries)),
                        created_task_id: None,
                        notice_id: None,
                        payload_json: serde_json::to_string(&context).unwrap_or_default(),
                    })
                    .await
                {
                    tracing::warn!(
                        trigger_id = %row.id,
                        error = %error,
                        "failed to record dead_letter trigger fire history"
                    );
                }
                continue;
            }

            let fire_id = format!("trigger_fire_{}", uuid::Uuid::new_v4());
            let fire_history_recorded = match self
                .history
                .insert_trigger_fire_history(&crate::history::TriggerFireHistoryRow {
                    id: fire_id.clone(),
                    trigger_id: row.id.clone(),
                    event_family: row.event_family.clone(),
                    event_kind: row.event_kind.clone(),
                    status: "fired".to_string(),
                    fired_at_ms: now,
                    completed_at_ms: None,
                    retry_count,
                    error_message: None,
                    created_task_id: None,
                    notice_id: None,
                    payload_json: serde_json::to_string(&context).unwrap_or_default(),
                })
                .await
            {
                Ok(()) => true,
                Err(error) => {
                    tracing::warn!(
                        trigger_id = %row.id,
                        error = %error,
                        "failed to record trigger fire history"
                    );
                    false
                }
            };

            let title = render_event_trigger_template(&row.title_template, state, &context);
            let body = render_event_trigger_template(&row.body_template, state, &context);
            self.emit_workflow_notice(
                thread_id.unwrap_or("system"),
                &row.notification_kind,
                title,
                Some(body),
            );

            let execution_result: Result<Option<AgentTask>> = async {
                self.log_fired_event_trigger(&row, state, thread_id, &context, now)
                    .await?;
                self.maybe_execute_event_trigger_tool(&row, state, thread_id, &context)
                    .await?;
                let created_task = if row.tool_name.is_none() {
                    self.maybe_queue_event_trigger_task(&row, state, thread_id, &context)
                        .await?
                } else {
                    None
                };
                self.history
                    .record_event_trigger_fired(&row.id, now)
                    .await?;
                Ok(created_task)
            }
            .await;

            match execution_result {
                Ok(created_task) => {
                    if fire_history_recorded {
                        if let Err(error) = self
                            .history
                            .update_trigger_fire_status(
                                &fire_id,
                                "succeeded",
                                None,
                                created_task.as_ref().map(|task| task.id.as_str()),
                            )
                            .await
                        {
                            tracing::warn!(
                                trigger_id = %row.id,
                                fire_id = %fire_id,
                                error = %error,
                                "failed to update trigger fire history to succeeded"
                            );
                        }
                    }
                    fired = fired.saturating_add(1);
                }
                Err(error) => {
                    let error_message = error.to_string();
                    if fire_history_recorded {
                        if let Err(update_error) = self
                            .history
                            .update_trigger_fire_status(
                                &fire_id,
                                "failed",
                                Some(&error_message),
                                None,
                            )
                            .await
                        {
                            tracing::warn!(
                                trigger_id = %row.id,
                                fire_id = %fire_id,
                                error = %update_error,
                                "failed to update trigger fire history to failed"
                            );
                        }
                    }
                    self.emit_workflow_notice(
                        thread_id.unwrap_or("system"),
                        "event_trigger_failed",
                        format!("Trigger `{}` failed", row.id),
                        Some(error_message.clone()),
                    );
                    return Err(error);
                }
            }
        }

        Ok(fired)
    }

    async fn log_fired_event_trigger(
        &self,
        row: &EventTriggerRow,
        state: Option<&str>,
        thread_id: Option<&str>,
        context: &serde_json::Value,
        handled_at_ms: u64,
    ) -> Result<()> {
        self.history
            .insert_event_log(&EventLogRow {
                id: format!("event_{}", uuid::Uuid::new_v4()),
                event_family: row.event_family.clone(),
                event_kind: row.event_kind.clone(),
                state: state.map(str::to_string),
                thread_id: thread_id
                    .map(str::to_string)
                    .or_else(|| row.thread_id.clone()),
                payload_json: serde_json::to_string(context)?,
                risk_label: row.risk_label.clone(),
                handled_at_ms,
            })
            .await
    }

    async fn maybe_queue_event_trigger_task(
        &self,
        row: &EventTriggerRow,
        state: Option<&str>,
        thread_id: Option<&str>,
        context: &serde_json::Value,
    ) -> Result<Option<AgentTask>> {
        let Some(target_agent_id) = normalize_event_trigger_target_agent(row.agent_id.as_deref())
        else {
            return Ok(None);
        };

        let description = row
            .prompt_template
            .as_deref()
            .map(|template| render_event_trigger_template(template, state, context))
            .unwrap_or_else(|| render_event_trigger_template(&row.body_template, state, context));
        let effective_thread_id = thread_id
            .map(str::to_string)
            .or_else(|| row.thread_id.clone());
        let title = format!(
            "Handle trigger: {}",
            render_event_trigger_template(&row.title_template, state, context)
        );
        let task = self
            .enqueue_task(
                title,
                description.clone(),
                match row.risk_label.as_str() {
                    "high" => "high",
                    "medium" => "normal",
                    _ => "low",
                },
                Some(description.clone()),
                None,
                Vec::new(),
                None,
                "event_trigger",
                None,
                None,
                effective_thread_id.clone(),
                Some("daemon".to_string()),
            )
            .await;
        let task = self
            .assign_event_trigger_task_target(
                &task.id,
                effective_thread_id.as_deref(),
                &target_agent_id,
            )
            .await
            .unwrap_or(task);

        if row.risk_label != "high" {
            return Ok(Some(task));
        }

        let pending_approval = ToolPendingApproval {
            approval_id: format!("event-trigger-approval-{}", uuid::Uuid::new_v4()),
            execution_id: format!("event-trigger-exec-{}", uuid::Uuid::new_v4()),
            command: description,
            rationale: format!(
                "event trigger {} requested a {}-risk background action",
                row.id, row.risk_label
            ),
            risk_level: row.risk_label.clone(),
            blast_radius: if effective_thread_id.is_some() {
                "thread".to_string()
            } else {
                "workspace".to_string()
            },
            reasons: vec![format!(
                "fired trigger {}:{}",
                row.event_family, row.event_kind
            )],
            session_id: None,
        };
        let approval_thread_id = effective_thread_id.as_deref().unwrap_or("system");
        if self
            .auto_approve_task_if_rule_matches(&task.id, approval_thread_id, &pending_approval)
            .await
        {
            return Ok(Some(task));
        }
        self.mark_task_awaiting_approval(&task.id, approval_thread_id, &pending_approval)
            .await;
        self.record_operator_approval_requested(&pending_approval)
            .await?;
        Ok(Some(task))
    }

    async fn assign_event_trigger_task_target(
        &self,
        task_id: &str,
        thread_id: Option<&str>,
        target_agent_id: &str,
    ) -> Option<AgentTask> {
        if target_agent_id == crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID {
            let mut updated = self.retarget_task_to_weles(task_id).await?;
            if let Some(thread_id) = thread_id {
                updated = self
                    .set_event_trigger_task_thread(task_id, thread_id)
                    .await
                    .unwrap_or(updated);
            }
            return Some(updated);
        }

        let updated = {
            let mut tasks = self.tasks.lock().await;
            let task = tasks.iter_mut().find(|task| task.id == task_id)?;
            task.sub_agent_def_id = Some(target_agent_id.to_string());
            if let Some(thread_id) = thread_id {
                task.thread_id = Some(thread_id.to_string());
            }
            task.clone()
        };
        self.persist_tasks().await;
        Some(updated)
    }

    async fn set_event_trigger_task_thread(
        &self,
        task_id: &str,
        thread_id: &str,
    ) -> Option<AgentTask> {
        let updated = {
            let mut tasks = self.tasks.lock().await;
            let task = tasks.iter_mut().find(|task| task.id == task_id)?;
            task.thread_id = Some(thread_id.to_string());
            task.clone()
        };
        self.persist_tasks().await;
        Some(updated)
    }

    async fn maybe_execute_event_trigger_tool(
        &self,
        row: &EventTriggerRow,
        state: Option<&str>,
        thread_id: Option<&str>,
        context: &serde_json::Value,
    ) -> Result<()> {
        let Some(tool_name) = row
            .tool_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(());
        };

        let effective_thread_id = thread_id.or(row.thread_id.as_deref()).unwrap_or("system");
        let mut tool_args = serde_json::json!({
            "payload": context.clone(),
        });
        if let Some(state) = state {
            tool_args["state"] = serde_json::Value::String(state.to_string());
        }
        if let Some(thread_id) = thread_id.or(row.thread_id.as_deref()) {
            tool_args["thread_id"] = serde_json::Value::String(thread_id.to_string());
        }
        if let Some(tool_payload_json) = row.tool_payload_json.as_deref() {
            let payload =
                serde_json::from_str::<serde_json::Value>(tool_payload_json).map_err(|error| {
                    anyhow::anyhow!("invalid tool payload for trigger {}: {}", row.id, error)
                })?;
            if !payload.is_object() {
                anyhow::bail!(
                    "event trigger {} tool payload must be a JSON object",
                    row.id
                );
            }
            merge_event_trigger_json_value(
                &mut tool_args,
                render_event_trigger_value_templates(&payload, state, context),
            );
        }

        match tool_name {
            zorai_protocol::tool_names::RUN_WORKFLOW_PACK => {
                let execution = self
                    .run_workflow_pack_json(&tool_args, Some(effective_thread_id), None)
                    .await?;
                if let Some(pending_approval) = execution.pending_approval.as_ref() {
                    self.remember_pending_approval_command(pending_approval)
                        .await;
                    self.record_operator_approval_requested(pending_approval)
                        .await?;
                    let _ = self
                        .maybe_send_gateway_thread_approval_request(
                            effective_thread_id,
                            pending_approval,
                        )
                        .await;
                }
                Ok(())
            }
            other => anyhow::bail!(
                "event trigger direct tool `{other}` is not supported; use run_workflow_pack"
            ),
        }
    }
}

fn render_event_trigger_template(
    template: &str,
    state: Option<&str>,
    context: &serde_json::Value,
) -> String {
    let mut rendered = template.replace("{state}", state.unwrap_or("unknown"));
    if let Some(object) = context.as_object() {
        for (key, value) in object {
            let replacement = value
                .as_str()
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| value.to_string());
            rendered = rendered.replace(&format!("{{{key}}}"), &replacement);
        }
    }
    rendered
}

fn render_event_trigger_value_templates(
    value: &serde_json::Value,
    state: Option<&str>,
    context: &serde_json::Value,
) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => {
            serde_json::Value::String(render_event_trigger_template(text, state, context))
        }
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .iter()
                .map(|item| render_event_trigger_value_templates(item, state, context))
                .collect(),
        ),
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        render_event_trigger_value_templates(value, state, context),
                    )
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

fn merge_event_trigger_json_value(base: &mut serde_json::Value, patch: serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(patch_map)) => {
            for (key, patch_value) in patch_map {
                match base_map.get_mut(&key) {
                    Some(base_value) => merge_event_trigger_json_value(base_value, patch_value),
                    None => {
                        base_map.insert(key, patch_value);
                    }
                }
            }
        }
        (base_slot, patch_value) => {
            *base_slot = patch_value;
        }
    }
}

fn normalize_event_trigger_target_agent(agent_id: Option<&str>) -> Option<String> {
    let normalized = agent_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("weles")
        .to_ascii_lowercase();
    if normalized == "svarog"
        || normalized == "main"
        || normalized == "main_agent"
        || normalized == crate::agent::agent_identity::MAIN_AGENT_ID
        || normalized == "weles"
        || normalized == crate::agent::agent_identity::WELES_AGENT_ID
    {
        Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID.to_string())
    } else {
        Some(normalized)
    }
}
