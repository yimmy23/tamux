use super::*;
use crate::history::EventTriggerRow;

impl AgentEngine {
    pub(crate) async fn ensure_default_event_triggers(&self) -> Result<usize> {
        let now = now_millis();
        let defaults = vec![
            EventTriggerRow {
                id: "trigger-health-weles-degraded".to_string(),
                event_family: "health".to_string(),
                event_kind: "weles_health".to_string(),
                target_state: Some("degraded".to_string()),
                thread_id: None,
                enabled: true,
                cooldown_secs: 900,
                risk_label: "medium".to_string(),
                notification_kind: "weles_health_degraded".to_string(),
                title_template: "WELES health degraded".to_string(),
                body_template:
                    "WELES health changed to {state}. Review reason and consider intervention."
                        .to_string(),
                created_at: now,
                updated_at: now,
                last_fired_at: None,
            },
            EventTriggerRow {
                id: "trigger-health-subagent-stuck".to_string(),
                event_family: "health".to_string(),
                event_kind: "subagent_health".to_string(),
                target_state: Some("stuck".to_string()),
                thread_id: None,
                enabled: true,
                cooldown_secs: 300,
                risk_label: "medium".to_string(),
                notification_kind: "subagent_health_stuck".to_string(),
                title_template: "Subagent stuck".to_string(),
                body_template:
                    "Subagent {task_id} entered {state}. Review reason and decide whether to intervene."
                        .to_string(),
                created_at: now,
                updated_at: now,
                last_fired_at: None,
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
            .map(|row| serde_json::json!({
                "id": row.id,
                "event_family": row.event_family,
                "event_kind": row.event_kind,
                "target_state": row.target_state,
                "thread_id": row.thread_id,
                "enabled": row.enabled,
                "cooldown_secs": row.cooldown_secs,
                "risk_label": row.risk_label,
                "notification_kind": row.notification_kind,
                "title_template": row.title_template,
                "body_template": row.body_template,
                "created_at": row.created_at,
                "updated_at": row.updated_at,
                "last_fired_at": row.last_fired_at,
            }))
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
            target_state,
            thread_id,
            enabled,
            cooldown_secs,
            risk_label: risk_label.to_string(),
            notification_kind: notification_kind.to_string(),
            title_template: title_template.to_string(),
            body_template: body_template.to_string(),
            created_at: now,
            updated_at: now,
            last_fired_at: None,
        };

        self.history.upsert_event_trigger(&row).await?;
        Ok(serde_json::json!({
            "status": "created",
            "trigger": {
                "id": row.id,
                "event_family": row.event_family,
                "event_kind": row.event_kind,
                "target_state": row.target_state,
                "thread_id": row.thread_id,
                "enabled": row.enabled,
                "cooldown_secs": row.cooldown_secs,
                "risk_label": row.risk_label,
                "notification_kind": row.notification_kind,
                "title_template": row.title_template,
                "body_template": row.body_template,
                "created_at": row.created_at,
                "updated_at": row.updated_at,
                "last_fired_at": row.last_fired_at,
            }
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
            if row.target_state.as_deref().is_some()
                && row.target_state.as_deref() != state
            {
                continue;
            }
            if row.thread_id.as_deref().is_some() && row.thread_id.as_deref() != thread_id {
                continue;
            }
            if let Some(last_fired_at) = row.last_fired_at {
                let cooldown_ms = row.cooldown_secs.saturating_mul(1000);
                if now < last_fired_at.saturating_add(cooldown_ms) {
                    continue;
                }
            }

            let title = render_event_trigger_template(&row.title_template, state, &context);
            let body = render_event_trigger_template(&row.body_template, state, &context);
            self.emit_workflow_notice(
                thread_id.unwrap_or("system"),
                &row.notification_kind,
                title,
                Some(body),
            );
            self.history.record_event_trigger_fired(&row.id, now).await?;
            fired = fired.saturating_add(1);
        }

        Ok(fired)
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

