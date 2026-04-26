use super::*;

fn next_routine_run_at(schedule_expression: &str, after_ms: u64) -> Option<u64> {
    let cron: croner::Cron = schedule_expression.parse().ok()?;
    let base = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(after_ms as i64)
        .map(|dt| dt.with_timezone(&chrono::Local))
        .unwrap_or_else(chrono::Local::now);
    cron.find_next_occurrence(&base, false)
        .ok()
        .and_then(|dt| u64::try_from(dt.timestamp_millis()).ok())
}

fn routine_row_json(row: &crate::history::RoutineDefinitionRow) -> serde_json::Value {
    let target_payload = serde_json::from_str::<serde_json::Value>(&row.target_payload_json)
        .unwrap_or(serde_json::Value::Null);
    serde_json::json!({
        "id": row.id,
        "title": row.title,
        "description": row.description,
        "enabled": row.enabled,
        "paused_at": row.paused_at,
        "schedule_expression": row.schedule_expression,
        "target_kind": row.target_kind,
        "target_payload": target_payload,
        "next_run_at": row.next_run_at,
        "last_run_at": row.last_run_at,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}

impl AgentEngine {
    pub(crate) async fn list_routines_json(&self) -> Result<serde_json::Value> {
        let rows = self.history.list_routine_definitions().await?;
        Ok(serde_json::json!(rows
            .iter()
            .map(routine_row_json)
            .collect::<Vec<_>>()))
    }

    pub(crate) async fn get_routine_json(&self, routine_id: &str) -> Result<serde_json::Value> {
        let row = self
            .history
            .get_routine_definition(routine_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("routine {routine_id} not found"))?;
        Ok(routine_row_json(&row))
    }

    pub(crate) async fn materialize_due_routines(&self) -> Result<Vec<AgentTask>> {
        let now = now_millis();
        let due = self.history.list_due_routine_definitions(now).await?;
        let mut created = Vec::new();

        for mut row in due {
            if row.target_kind != "task" {
                tracing::warn!(routine_id = %row.id, target_kind = %row.target_kind, "skipping unsupported due routine target kind");
                continue;
            }

            let payload = serde_json::from_str::<serde_json::Value>(&row.target_payload_json)
                .context("parse routine target payload")?;
            let title = payload
                .get("title")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(row.title.as_str())
                .to_string();
            let description = payload
                .get("description")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(row.description.as_str())
                .to_string();
            let priority = payload
                .get("priority")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("normal");
            let command = payload
                .get("command")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned);
            let session_id = payload
                .get("session_id")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned);
            let dependencies = payload
                .get("dependencies")
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let runtime = payload
                .get("runtime")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned);
            let source = format!("routine:{}", row.id);

            let task = self
                .enqueue_task(
                    title,
                    description,
                    priority,
                    command,
                    session_id,
                    dependencies,
                    None,
                    &source,
                    None,
                    None,
                    None,
                    runtime,
                )
                .await;
            created.push(task);

            row.last_run_at = Some(now);
            row.next_run_at = next_routine_run_at(&row.schedule_expression, now)
                .or_else(|| Some(now.saturating_add(60_000)));
            row.updated_at = now;
            self.history.upsert_routine_definition(&row).await?;
        }

        Ok(created)
    }

    pub(crate) async fn create_routine_from_args(
        &self,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let title = args
            .get("title")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("missing 'title' argument"))?;
        let description = args
            .get("description")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("missing 'description' argument"))?;
        let schedule_expression = args
            .get("schedule_expression")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("missing 'schedule_expression' argument"))?;
        let target_kind = args
            .get("target_kind")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| matches!(*value, "task" | "goal" | "tool"))
            .ok_or_else(|| anyhow::anyhow!("missing or invalid 'target_kind' argument"))?;
        let target_payload = args
            .get("target_payload")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing 'target_payload' argument"))?;
        let enabled = args
            .get("enabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        let paused_at = args.get("paused_at").and_then(|value| value.as_u64());
        let next_run_at = args.get("next_run_at").and_then(|value| value.as_u64());
        let last_run_at = args.get("last_run_at").and_then(|value| value.as_u64());
        let routine_id = args
            .get("id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("routine-{}", uuid::Uuid::new_v4()));
        let now = now_millis();

        let row = crate::history::RoutineDefinitionRow {
            id: routine_id,
            title: title.to_string(),
            description: description.to_string(),
            enabled,
            paused_at,
            schedule_expression: schedule_expression.to_string(),
            target_kind: target_kind.to_string(),
            target_payload_json: serde_json::to_string(&target_payload)
                .context("serialize routine target payload")?,
            next_run_at,
            last_run_at,
            created_at: now,
            updated_at: now,
        };

        self.history.upsert_routine_definition(&row).await?;
        Ok(serde_json::json!({
            "status": "created",
            "routine": routine_row_json(&row),
        }))
    }

    pub(crate) async fn pause_routine_json(&self, routine_id: &str) -> Result<serde_json::Value> {
        let mut row = self
            .history
            .get_routine_definition(routine_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("routine {routine_id} not found"))?;
        let now = now_millis();
        row.paused_at = Some(now);
        row.updated_at = now;
        self.history.upsert_routine_definition(&row).await?;
        Ok(serde_json::json!({
            "status": "paused",
            "routine": routine_row_json(&row),
        }))
    }

    pub(crate) async fn resume_routine_json(&self, routine_id: &str) -> Result<serde_json::Value> {
        let mut row = self
            .history
            .get_routine_definition(routine_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("routine {routine_id} not found"))?;
        let now = now_millis();
        row.paused_at = None;
        row.updated_at = now;
        self.history.upsert_routine_definition(&row).await?;
        Ok(serde_json::json!({
            "status": "resumed",
            "routine": routine_row_json(&row),
        }))
    }

    pub(crate) async fn delete_routine_json(&self, routine_id: &str) -> Result<serde_json::Value> {
        let deleted = self.history.delete_routine_definition(routine_id).await?;
        if !deleted {
            return Err(anyhow::anyhow!("routine {routine_id} not found"));
        }
        Ok(serde_json::json!({
            "status": "deleted",
            "routine_id": routine_id,
        }))
    }
}
