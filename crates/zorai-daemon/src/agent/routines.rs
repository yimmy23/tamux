use super::*;
use serde_json::Value;

const ROUTINE_SCHEMA_VERSION: u32 = 1;
const DEFAULT_ROUTINE_HISTORY_LIMIT: usize = 10;
const FALLBACK_NEXT_RUN_DELAY_MS: u64 = 60_000;
const ALLOWED_NOTIFY_CHANNELS: &[&str] = &["in-app", "slack", "discord", "telegram", "whatsapp"];

#[derive(Debug, Clone)]
struct TaskRoutinePlan {
    title: String,
    description: String,
    priority: String,
    command: Option<String>,
    session_id: Option<String>,
    dependencies: Vec<String>,
    runtime: Option<String>,
    notify_on_complete: bool,
    notify_channels: Vec<String>,
}

#[derive(Debug, Clone)]
struct GoalRoutinePlan {
    goal: String,
    title: Option<String>,
    thread_id: Option<String>,
    session_id: Option<String>,
    priority: String,
    autonomy_level: Option<String>,
    requires_approval: bool,
    launch_assignments: Option<Vec<crate::agent::types::GoalAgentAssignment>>,
}

#[derive(Debug, Clone)]
enum RoutineExecutionTarget {
    Task(TaskRoutinePlan),
    Goal(GoalRoutinePlan),
}

#[derive(Debug, Clone)]
struct RoutineExecutionPlan {
    target_kind: String,
    materialized_payload: Value,
    delivery_fan_out: Value,
    approval_posture: Value,
    execution_target: RoutineExecutionTarget,
}

#[derive(Debug, Clone)]
struct RoutineExecutionOutcome {
    run: crate::history::RoutineRunRow,
    task: Option<AgentTask>,
    goal_run: Option<GoalRun>,
}

fn next_routine_run_at(schedule_expression: &str, after_ms: u64) -> Option<u64> {
    let cron: croner::Cron = schedule_expression.parse().ok()?;
    let base = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(after_ms as i64)
        .map(|dt| dt.with_timezone(&chrono::Local))
        .unwrap_or_else(chrono::Local::now);
    cron.find_next_occurrence(&base, false)
        .ok()
        .and_then(|dt| u64::try_from(dt.timestamp_millis()).ok())
}

fn next_routine_run_times(schedule_expression: &str, after_ms: u64, count: usize) -> Vec<u64> {
    let mut times = Vec::new();
    let mut cursor = after_ms;
    for _ in 0..count.max(1) {
        let Some(next) = next_routine_run_at(schedule_expression, cursor) else {
            break;
        };
        times.push(next);
        cursor = next.saturating_add(1);
    }
    times
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
        "schema_version": row.schema_version,
        "next_run_at": row.next_run_at,
        "last_run_at": row.last_run_at,
        "last_result": row.last_result,
        "last_error": row.last_error,
        "last_success_summary": row.last_success_summary,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}

fn routine_run_row_json(row: &crate::history::RoutineRunRow) -> serde_json::Value {
    let payload = serde_json::from_str::<serde_json::Value>(&row.payload_json)
        .unwrap_or(serde_json::Value::Null);
    serde_json::json!({
        "id": row.id,
        "routine_id": row.routine_id,
        "trigger_kind": row.trigger_kind,
        "status": row.status,
        "started_at": row.started_at,
        "finished_at": row.finished_at,
        "created_task_id": row.created_task_id,
        "created_goal_run_id": row.created_goal_run_id,
        "payload": payload,
        "result_summary": row.result_summary,
        "error": row.error,
        "rerun_of_run_id": row.rerun_of_run_id,
    })
}

fn trimmed_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn optional_non_empty_string_arg(args: &Value, key: &str) -> Result<Option<String>> {
    match args.get(key) {
        None => Ok(None),
        Some(value) => {
            let Some(value) = value.as_str() else {
                anyhow::bail!("'{key}' must be a string");
            };
            let value = value.trim();
            if value.is_empty() {
                anyhow::bail!("'{key}' must not be empty");
            }
            Ok(Some(value.to_string()))
        }
    }
}

fn optional_bool_arg(args: &Value, key: &str) -> Result<Option<bool>> {
    match args.get(key) {
        None => Ok(None),
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| anyhow::anyhow!("'{key}' must be a boolean")),
    }
}

fn parse_goal_launch_assignments_value(
    value: Option<&Value>,
) -> Result<Option<Vec<crate::agent::types::GoalAgentAssignment>>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let assignments = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("'launch_assignments' must be an array"))?;
    if assignments.is_empty() {
        return Ok(None);
    }

    assignments
        .iter()
        .enumerate()
        .map(|(index, assignment)| {
            let role_id = assignment
                .get("role_id")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "launch_assignments[{index}].role_id must be a non-empty string"
                    )
                })?
                .to_string();
            let provider = assignment
                .get("provider")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "launch_assignments[{index}].provider must be a non-empty string"
                    )
                })?
                .to_string();
            let model = assignment
                .get("model")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("launch_assignments[{index}].model must be a non-empty string")
                })?
                .to_string();
            let reasoning_effort = assignment
                .get("reasoning_effort")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            let enabled = assignment
                .get("enabled")
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            let inherit_from_main = assignment
                .get("inherit_from_main")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            Ok(crate::agent::types::GoalAgentAssignment {
                role_id,
                enabled,
                provider,
                model,
                reasoning_effort,
                inherit_from_main,
            })
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

fn normalize_notify_channels(payload: &Value) -> Result<Vec<String>> {
    let values = payload
        .get("notify_channels")
        .map(|value| {
            value
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("'target_payload.notify_channels' must be an array"))
        })
        .transpose()?;

    let mut seen = std::collections::HashSet::new();
    let mut channels = Vec::new();
    if let Some(values) = values {
        for value in values {
            let channel = value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "'target_payload.notify_channels' entries must be non-empty strings"
                    )
                })?
                .to_ascii_lowercase();
            if !ALLOWED_NOTIFY_CHANNELS.contains(&channel.as_str()) {
                anyhow::bail!(
                    "unsupported notify channel '{channel}' (allowed: {})",
                    ALLOWED_NOTIFY_CHANNELS.join(", ")
                );
            }
            if seen.insert(channel.clone()) {
                channels.push(channel);
            }
        }
    }
    if channels.is_empty() {
        channels.push("in-app".to_string());
    }
    Ok(channels)
}

fn parse_string_array(payload: &Value, key: &str) -> Result<Vec<String>> {
    payload
        .get(key)
        .map(|items| {
            items
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("'target_payload.{key}' must be an array"))?
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "'target_payload.{key}' entries must be non-empty strings"
                            )
                        })
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()
        .map(|value| value.unwrap_or_default())
}

fn build_task_execution_plan(
    routine_id: &str,
    row_title: &str,
    row_description: &str,
    payload: &Value,
) -> Result<RoutineExecutionPlan> {
    let payload = payload
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("'target_payload' must be an object for task routines"))?;
    let payload = Value::Object(payload.clone());

    let title = trimmed_string(payload.get("title")).unwrap_or_else(|| row_title.to_string());
    let description =
        trimmed_string(payload.get("description")).unwrap_or_else(|| row_description.to_string());
    if description.trim().is_empty() {
        anyhow::bail!("task routines require a non-empty description");
    }

    let priority = trimmed_string(payload.get("priority")).unwrap_or_else(|| "normal".to_string());
    let command = trimmed_string(payload.get("command"));
    let session_id = trimmed_string(payload.get("session_id"));
    let runtime = trimmed_string(payload.get("runtime"));
    let dependencies = parse_string_array(&payload, "dependencies")?;
    let notify_on_complete = payload
        .get("notify_on_complete")
        .map(|value| {
            value.as_bool().ok_or_else(|| {
                anyhow::anyhow!("'target_payload.notify_on_complete' must be a boolean")
            })
        })
        .transpose()?
        .unwrap_or(true);
    let notify_channels = normalize_notify_channels(&payload)?;

    let materialized_payload = serde_json::json!({
        "title": title,
        "description": description,
        "priority": priority,
        "command": command,
        "session_id": session_id,
        "dependencies": dependencies,
        "runtime": runtime,
        "notify_on_complete": notify_on_complete,
        "notify_channels": notify_channels,
        "source": format!("routine:{routine_id}"),
    });

    Ok(RoutineExecutionPlan {
        target_kind: "task".to_string(),
        delivery_fan_out: serde_json::json!({
            "kind": "task_notifications",
            "notify_on_complete": materialized_payload["notify_on_complete"],
            "channels": materialized_payload["notify_channels"],
            "channel_count": materialized_payload["notify_channels"].as_array().map(|items| items.len()).unwrap_or(0),
        }),
        approval_posture: serde_json::json!({
            "kind": "task_materialization",
            "requires_approval": false,
            "summary": "Routine materialization enqueues a task without mutating external systems. Downstream task execution may still request approvals.",
        }),
        execution_target: RoutineExecutionTarget::Task(TaskRoutinePlan {
            title: materialized_payload["title"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            description: materialized_payload["description"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            priority: materialized_payload["priority"]
                .as_str()
                .unwrap_or("normal")
                .to_string(),
            command: trimmed_string(materialized_payload.get("command")),
            session_id: trimmed_string(materialized_payload.get("session_id")),
            dependencies: materialized_payload["dependencies"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            runtime: trimmed_string(materialized_payload.get("runtime")),
            notify_on_complete,
            notify_channels: materialized_payload["notify_channels"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        }),
        materialized_payload,
    })
}

fn build_goal_execution_plan(row_title: &str, payload: &Value) -> Result<RoutineExecutionPlan> {
    let payload = payload
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("'target_payload' must be an object for goal routines"))?;
    let payload = Value::Object(payload.clone());

    let goal = trimmed_string(payload.get("goal"))
        .ok_or_else(|| anyhow::anyhow!("goal routines require 'target_payload.goal'"))?;
    let title = trimmed_string(payload.get("title")).unwrap_or_else(|| row_title.to_string());
    let priority = trimmed_string(payload.get("priority")).unwrap_or_else(|| "normal".to_string());
    let thread_id = trimmed_string(payload.get("thread_id"));
    let session_id = trimmed_string(payload.get("session_id"));
    let autonomy_level = trimmed_string(payload.get("autonomy_level"));
    let requires_approval = payload
        .get("requires_approval")
        .map(|value| {
            value.as_bool().ok_or_else(|| {
                anyhow::anyhow!("'target_payload.requires_approval' must be a boolean")
            })
        })
        .transpose()?
        .unwrap_or(false);
    let launch_assignments =
        parse_goal_launch_assignments_value(payload.get("launch_assignments"))?;

    let materialized_payload = serde_json::json!({
        "goal": goal,
        "title": title,
        "thread_id": thread_id,
        "session_id": session_id,
        "priority": priority,
        "autonomy_level": autonomy_level,
        "requires_approval": requires_approval,
        "launch_assignments": launch_assignments,
    });

    Ok(RoutineExecutionPlan {
        target_kind: "goal".to_string(),
        delivery_fan_out: serde_json::json!({
            "kind": "goal_visibility",
            "channels": ["in-app"],
            "channel_count": 1,
            "notify_on_complete": false,
        }),
        approval_posture: serde_json::json!({
            "kind": "goal_creation",
            "requires_approval": requires_approval,
            "summary": if requires_approval {
                "The materialized goal run is configured to wait for normal operator approval gates."
            } else {
                "The materialized goal run can start without a pre-create approval gate, but downstream steps may still request approvals."
            },
        }),
        execution_target: RoutineExecutionTarget::Goal(GoalRoutinePlan {
            goal: materialized_payload["goal"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            title: Some(
                materialized_payload["title"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            ),
            thread_id: trimmed_string(materialized_payload.get("thread_id")),
            session_id: trimmed_string(materialized_payload.get("session_id")),
            priority: materialized_payload["priority"]
                .as_str()
                .unwrap_or("normal")
                .to_string(),
            autonomy_level: trimmed_string(materialized_payload.get("autonomy_level")),
            requires_approval,
            launch_assignments,
        }),
        materialized_payload,
    })
}

fn build_execution_plan_from_payload(
    routine_id: &str,
    row_title: &str,
    row_description: &str,
    target_kind: &str,
    payload: &Value,
) -> Result<RoutineExecutionPlan> {
    match target_kind {
        "task" => build_task_execution_plan(routine_id, row_title, row_description, payload),
        "goal" => build_goal_execution_plan(row_title, payload),
        "tool" => anyhow::bail!(
            "target_kind 'tool' is reserved but not yet supported for routine execution"
        ),
        other => anyhow::bail!("unsupported target kind '{other}'"),
    }
}

fn routine_snapshot_json(plan: &RoutineExecutionPlan) -> Value {
    serde_json::json!({
        "target_kind": plan.target_kind,
        "materialized_payload": plan.materialized_payload,
    })
}

impl AgentEngine {
    async fn apply_materialized_task_notification_preferences(
        &self,
        task_id: &str,
        notify_on_complete: Option<bool>,
        notify_channels: Option<Vec<String>>,
    ) -> Option<AgentTask> {
        if notify_on_complete.is_none() && notify_channels.is_none() {
            return None;
        }

        let updated = {
            let mut tasks = self.tasks.lock().await;
            let task = tasks.iter_mut().find(|task| task.id == task_id)?;
            let mut changed = false;

            if let Some(notify_on_complete) = notify_on_complete {
                if task.notify_on_complete != notify_on_complete {
                    task.notify_on_complete = notify_on_complete;
                    changed = true;
                }
            }

            if let Some(notify_channels) = notify_channels {
                if task.notify_channels != notify_channels {
                    task.notify_channels = notify_channels;
                    changed = true;
                }
            }

            changed.then(|| task.clone())
        };

        if let Some(updated) = updated {
            self.persist_tasks().await;
            self.emit_task_update(&updated, Some(status_message(&updated).into()));
            Some(updated)
        } else {
            None
        }
    }

    fn build_routine_execution_plan(
        &self,
        row: &crate::history::RoutineDefinitionRow,
    ) -> Result<RoutineExecutionPlan> {
        if row.schema_version != ROUTINE_SCHEMA_VERSION {
            anyhow::bail!(
                "unsupported routine schema_version {} (expected {})",
                row.schema_version,
                ROUTINE_SCHEMA_VERSION
            );
        }
        let payload = serde_json::from_str::<Value>(&row.target_payload_json)
            .context("parse routine target payload")?;
        build_execution_plan_from_payload(
            &row.id,
            &row.title,
            &row.description,
            &row.target_kind,
            &payload,
        )
    }

    fn build_routine_execution_plan_from_snapshot(
        &self,
        routine_id: &str,
        row_title: &str,
        row_description: &str,
        snapshot: &Value,
    ) -> Result<RoutineExecutionPlan> {
        let target_kind = snapshot
            .get("target_kind")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("stored routine run payload is missing target_kind"))?;
        let payload = snapshot
            .get("materialized_payload")
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!("stored routine run payload is missing materialized_payload")
            })?;
        build_execution_plan_from_payload(
            routine_id,
            row_title,
            row_description,
            target_kind,
            &payload,
        )
    }

    async fn persist_routine_run_outcome(
        &self,
        row: &mut crate::history::RoutineDefinitionRow,
        trigger_kind: &str,
        rerun_of_run_id: Option<String>,
        started_at: u64,
        finished_at: u64,
        snapshot: Value,
        task: Option<&AgentTask>,
        goal_run: Option<&GoalRun>,
        error: Option<String>,
        advance_schedule: bool,
    ) -> Result<crate::history::RoutineRunRow> {
        let success = error.is_none();
        let status = if success { "success" } else { "failed" };
        let result_summary = if let Some(task) = task {
            Some(format!("Enqueued task {} ({})", task.id, task.title))
        } else if let Some(goal_run) = goal_run {
            Some(format!(
                "Started goal run {} ({})",
                goal_run.id, goal_run.title
            ))
        } else {
            None
        };

        row.last_run_at = Some(finished_at);
        row.last_result = Some(status.to_string());
        row.last_error = error.clone();
        if let Some(summary) = result_summary.clone() {
            row.last_success_summary = Some(summary);
        }
        if advance_schedule {
            row.next_run_at = next_routine_run_at(&row.schedule_expression, finished_at)
                .or_else(|| Some(finished_at.saturating_add(FALLBACK_NEXT_RUN_DELAY_MS)));
        }
        row.updated_at = finished_at;

        self.history.upsert_routine_definition(row).await?;

        let run = crate::history::RoutineRunRow {
            id: format!("routine-run-{}", uuid::Uuid::new_v4()),
            routine_id: row.id.clone(),
            trigger_kind: trigger_kind.to_string(),
            status: status.to_string(),
            started_at,
            finished_at: Some(finished_at),
            created_task_id: task.map(|task| task.id.clone()),
            created_goal_run_id: goal_run.map(|goal_run| goal_run.id.clone()),
            payload_json: snapshot.to_string(),
            result_summary: result_summary.clone(),
            error: error.clone(),
            rerun_of_run_id,
        };
        self.history.append_routine_run(&run).await?;

        self.record_provenance_event(
            "routine_run_recorded",
            "routine execution recorded",
            serde_json::json!({
                "routine_id": row.id,
                "run_id": run.id,
                "trigger_kind": run.trigger_kind,
                "status": run.status,
                "created_task_id": run.created_task_id,
                "created_goal_run_id": run.created_goal_run_id,
                "error": run.error,
            }),
            goal_run.map(|goal_run| goal_run.id.as_str()),
            task.map(|task| task.id.as_str()),
            None,
            None,
            None,
        )
        .await;

        Ok(run)
    }

    async fn execute_routine_plan(
        &self,
        row: &mut crate::history::RoutineDefinitionRow,
        plan: RoutineExecutionPlan,
        trigger_kind: &str,
        rerun_of_run_id: Option<String>,
        advance_schedule: bool,
    ) -> Result<RoutineExecutionOutcome> {
        let started_at = now_millis();
        let snapshot = routine_snapshot_json(&plan);

        let execution_result: Result<(Option<AgentTask>, Option<GoalRun>)> =
            match plan.execution_target {
                RoutineExecutionTarget::Task(task_plan) => {
                    let task = self
                        .enqueue_task(
                            task_plan.title,
                            task_plan.description,
                            &task_plan.priority,
                            task_plan.command,
                            task_plan.session_id,
                            task_plan.dependencies,
                            None,
                            &format!("routine:{}", row.id),
                            None,
                            None,
                            None,
                            task_plan.runtime,
                        )
                        .await;
                    let task = self
                        .apply_materialized_task_notification_preferences(
                            &task.id,
                            Some(task_plan.notify_on_complete),
                            Some(task_plan.notify_channels),
                        )
                        .await
                        .unwrap_or(task);
                    Ok((Some(task), None))
                }
                RoutineExecutionTarget::Goal(goal_plan) => {
                    let goal_run = self
                        .start_goal_run_with_surface_and_approval_policy(
                            goal_plan.goal,
                            goal_plan.title,
                            goal_plan.thread_id,
                            goal_plan.session_id,
                            Some(goal_plan.priority.as_str()),
                            None,
                            goal_plan.autonomy_level,
                            None,
                            goal_plan.requires_approval,
                            goal_plan.launch_assignments,
                        )
                        .await;
                    Ok((None, Some(goal_run)))
                }
            };

        let finished_at = now_millis();
        let (task, goal_run, error) = match execution_result {
            Ok((task, goal_run)) => (task, goal_run, None),
            Err(error) => (None, None, Some(error.to_string())),
        };
        let run = self
            .persist_routine_run_outcome(
                row,
                trigger_kind,
                rerun_of_run_id,
                started_at,
                finished_at,
                snapshot,
                task.as_ref(),
                goal_run.as_ref(),
                error,
                advance_schedule,
            )
            .await?;

        Ok(RoutineExecutionOutcome {
            run,
            task,
            goal_run,
        })
    }

    async fn execute_saved_routine(
        &self,
        row: &mut crate::history::RoutineDefinitionRow,
        trigger_kind: &str,
        rerun_of_run_id: Option<String>,
        advance_schedule: bool,
    ) -> Result<RoutineExecutionOutcome> {
        let started_at = now_millis();
        let plan = self.build_routine_execution_plan(row);
        let finished_at = now_millis();
        match plan {
            Ok(plan) => {
                self.execute_routine_plan(
                    row,
                    plan,
                    trigger_kind,
                    rerun_of_run_id,
                    advance_schedule,
                )
                .await
            }
            Err(error) => {
                let snapshot = serde_json::json!({
                    "target_kind": row.target_kind,
                    "materialized_payload": serde_json::from_str::<Value>(&row.target_payload_json).unwrap_or(Value::Null),
                });
                let run = self
                    .persist_routine_run_outcome(
                        row,
                        trigger_kind,
                        rerun_of_run_id,
                        started_at,
                        finished_at,
                        snapshot,
                        None,
                        None,
                        Some(error.to_string()),
                        advance_schedule,
                    )
                    .await?;
                Ok(RoutineExecutionOutcome {
                    run,
                    task: None,
                    goal_run: None,
                })
            }
        }
    }

    fn build_routine_row_from_args(
        &self,
        args: &Value,
        existing: Option<&crate::history::RoutineDefinitionRow>,
        routine_id: String,
        created_at: u64,
    ) -> Result<crate::history::RoutineDefinitionRow> {
        let title = optional_non_empty_string_arg(args, "title")?
            .or_else(|| existing.map(|row| row.title.clone()))
            .ok_or_else(|| anyhow::anyhow!("missing 'title' argument"))?;
        let description = optional_non_empty_string_arg(args, "description")?
            .or_else(|| existing.map(|row| row.description.clone()))
            .ok_or_else(|| anyhow::anyhow!("missing 'description' argument"))?;
        let schedule_expression = optional_non_empty_string_arg(args, "schedule_expression")?
            .or_else(|| existing.map(|row| row.schedule_expression.clone()))
            .ok_or_else(|| anyhow::anyhow!("missing 'schedule_expression' argument"))?;
        if next_routine_run_at(&schedule_expression, now_millis()).is_none() {
            anyhow::bail!("invalid 'schedule_expression': no next occurrence could be computed");
        }

        let target_kind = optional_non_empty_string_arg(args, "target_kind")?
            .or_else(|| existing.map(|row| row.target_kind.clone()))
            .ok_or_else(|| anyhow::anyhow!("missing 'target_kind' argument"))?;
        if !matches!(target_kind.as_str(), "task" | "goal" | "tool") {
            anyhow::bail!("missing or invalid 'target_kind' argument");
        }

        let target_payload = args
            .get("target_payload")
            .cloned()
            .or_else(|| {
                existing
                    .and_then(|row| serde_json::from_str::<Value>(&row.target_payload_json).ok())
            })
            .ok_or_else(|| anyhow::anyhow!("missing 'target_payload' argument"))?;

        let enabled = optional_bool_arg(args, "enabled")?
            .or_else(|| existing.map(|row| row.enabled))
            .unwrap_or(true);
        let paused_at = match args.get("paused_at") {
            Some(Value::Null) => None,
            Some(value) => Some(value.as_u64().ok_or_else(|| {
                anyhow::anyhow!("'paused_at' must be an unsigned integer or null")
            })?),
            None => existing.and_then(|row| row.paused_at),
        };
        let explicit_next_run_at = match args.get("next_run_at") {
            Some(Value::Null) => Some(None),
            Some(value) => Some(Some(value.as_u64().ok_or_else(|| {
                anyhow::anyhow!("'next_run_at' must be an unsigned integer or null")
            })?)),
            None => None,
        };
        let last_run_at = match args.get("last_run_at") {
            Some(Value::Null) => None,
            Some(value) => Some(value.as_u64().ok_or_else(|| {
                anyhow::anyhow!("'last_run_at' must be an unsigned integer or null")
            })?),
            None => existing.and_then(|row| row.last_run_at),
        };
        let now = now_millis();
        let next_run_at = explicit_next_run_at.unwrap_or_else(|| {
            if !enabled || paused_at.is_some() {
                existing.and_then(|row| row.next_run_at)
            } else {
                next_routine_run_at(&schedule_expression, now)
                    .or_else(|| Some(now.saturating_add(FALLBACK_NEXT_RUN_DELAY_MS)))
            }
        });

        let row = crate::history::RoutineDefinitionRow {
            id: routine_id,
            title,
            description,
            enabled,
            paused_at,
            schedule_expression,
            target_kind,
            target_payload_json: serde_json::to_string(&target_payload)
                .context("serialize routine target payload")?,
            schema_version: ROUTINE_SCHEMA_VERSION,
            next_run_at,
            last_run_at,
            last_result: existing.and_then(|row| row.last_result.clone()),
            last_error: existing.and_then(|row| row.last_error.clone()),
            last_success_summary: existing.and_then(|row| row.last_success_summary.clone()),
            created_at,
            updated_at: now,
        };

        let _ = self.build_routine_execution_plan(&row)?;
        Ok(row)
    }

    async fn preview_routine_row_json(
        &self,
        row: &crate::history::RoutineDefinitionRow,
        fire_count: usize,
    ) -> Result<Value> {
        let plan = self.build_routine_execution_plan(row)?;
        Ok(serde_json::json!({
            "routine": routine_row_json(row),
            "preview": {
                "dry_run": true,
                "next_fire_times": next_routine_run_times(&row.schedule_expression, now_millis(), fire_count),
                "materialized_payload": plan.materialized_payload,
                "delivery_fan_out": plan.delivery_fan_out,
                "approval_posture": plan.approval_posture,
                "would_mutate_state": false,
                "would_enqueue_work": true,
                "target_kind": row.target_kind,
            }
        }))
    }

    pub(crate) async fn list_routines_json(&self) -> Result<Value> {
        let rows = self.history.list_routine_definitions().await?;
        Ok(serde_json::json!(rows
            .iter()
            .map(routine_row_json)
            .collect::<Vec<_>>()))
    }

    pub(crate) async fn get_routine_json(&self, routine_id: &str) -> Result<Value> {
        let row = self
            .history
            .get_routine_definition(routine_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("routine {routine_id} not found"))?;
        let history = self
            .history
            .list_routine_runs(routine_id, DEFAULT_ROUTINE_HISTORY_LIMIT)
            .await?;
        Ok(serde_json::json!({
            "routine": routine_row_json(&row),
            "history": history.iter().map(routine_run_row_json).collect::<Vec<_>>(),
        }))
    }

    pub(crate) async fn materialize_due_routines(&self) -> Result<Vec<AgentTask>> {
        let now = now_millis();
        let due = self.history.list_due_routine_definitions(now).await?;
        let mut created = Vec::new();

        for mut row in due {
            if !row.enabled || row.paused_at.is_some() {
                continue;
            }
            let outcome = self
                .execute_saved_routine(&mut row, "scheduled", None, true)
                .await?;
            if let Some(task) = outcome.task {
                created.push(task);
            }
        }

        Ok(created)
    }

    pub(crate) async fn create_routine_from_args(&self, args: &Value) -> Result<Value> {
        let routine_id = optional_non_empty_string_arg(args, "id")?
            .unwrap_or_else(|| format!("routine-{}", uuid::Uuid::new_v4()));
        if self
            .history
            .get_routine_definition(&routine_id)
            .await?
            .is_some()
        {
            anyhow::bail!("routine {routine_id} already exists; use update_routine");
        }
        let row = self.build_routine_row_from_args(args, None, routine_id, now_millis())?;
        self.history.upsert_routine_definition(&row).await?;
        self.record_provenance_event(
            "routine_created",
            "routine definition created",
            serde_json::json!({ "routine_id": row.id, "target_kind": row.target_kind }),
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        Ok(serde_json::json!({
            "status": "created",
            "routine": routine_row_json(&row),
        }))
    }

    pub(crate) async fn update_routine_from_args(&self, args: &Value) -> Result<Value> {
        let routine_id = args
            .get("routine_id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("missing 'routine_id' argument"))?;
        let existing = self
            .history
            .get_routine_definition(routine_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("routine {routine_id} not found"))?;
        let row = self.build_routine_row_from_args(
            args,
            Some(&existing),
            existing.id.clone(),
            existing.created_at,
        )?;
        self.history.upsert_routine_definition(&row).await?;
        self.record_provenance_event(
            "routine_updated",
            "routine definition updated",
            serde_json::json!({ "routine_id": row.id, "target_kind": row.target_kind }),
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        Ok(serde_json::json!({
            "status": "updated",
            "routine": routine_row_json(&row),
        }))
    }

    pub(crate) async fn preview_routine_json(
        &self,
        routine_id: &str,
        fire_count: usize,
    ) -> Result<Value> {
        let row = self
            .history
            .get_routine_definition(routine_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("routine {routine_id} not found"))?;
        self.preview_routine_row_json(&row, fire_count).await
    }

    pub(crate) async fn run_routine_now_json(&self, routine_id: &str) -> Result<Value> {
        let mut row = self
            .history
            .get_routine_definition(routine_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("routine {routine_id} not found"))?;
        let outcome = self
            .execute_saved_routine(&mut row, "run_now", None, false)
            .await?;
        Ok(serde_json::json!({
            "status": outcome.run.status,
            "routine": routine_row_json(&row),
            "run": routine_run_row_json(&outcome.run),
            "created_task": outcome.task,
            "created_goal_run": outcome.goal_run,
        }))
    }

    pub(crate) async fn list_routine_history_json(
        &self,
        routine_id: &str,
        limit: usize,
    ) -> Result<Value> {
        if self
            .history
            .get_routine_definition(routine_id)
            .await?
            .is_none()
        {
            anyhow::bail!("routine {routine_id} not found");
        }
        let rows = self.history.list_routine_runs(routine_id, limit).await?;
        Ok(serde_json::json!({
            "routine_id": routine_id,
            "runs": rows.iter().map(routine_run_row_json).collect::<Vec<_>>(),
        }))
    }

    pub(crate) async fn rerun_routine_run_json(&self, run_id: &str) -> Result<Value> {
        let prior_run = self
            .history
            .get_routine_run(run_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("routine run {run_id} not found"))?;
        let mut row = self
            .history
            .get_routine_definition(&prior_run.routine_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("routine {} not found", prior_run.routine_id))?;
        let snapshot = serde_json::from_str::<Value>(&prior_run.payload_json)
            .context("parse stored routine run payload")?;
        let plan = self.build_routine_execution_plan_from_snapshot(
            &row.id,
            &row.title,
            &row.description,
            &snapshot,
        )?;
        let outcome = self
            .execute_routine_plan(&mut row, plan, "rerun", Some(prior_run.id.clone()), false)
            .await?;
        Ok(serde_json::json!({
            "status": outcome.run.status,
            "routine": routine_row_json(&row),
            "run": routine_run_row_json(&outcome.run),
            "rerun_of": routine_run_row_json(&prior_run),
            "created_task": outcome.task,
            "created_goal_run": outcome.goal_run,
        }))
    }

    pub(crate) async fn pause_routine_json(&self, routine_id: &str) -> Result<Value> {
        let mut row = self
            .history
            .get_routine_definition(routine_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("routine {routine_id} not found"))?;
        let now = now_millis();
        row.paused_at = Some(now);
        row.updated_at = now;
        self.history.upsert_routine_definition(&row).await?;
        self.record_provenance_event(
            "routine_paused",
            "routine paused",
            serde_json::json!({ "routine_id": row.id }),
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        Ok(serde_json::json!({
            "status": "paused",
            "routine": routine_row_json(&row),
        }))
    }

    pub(crate) async fn resume_routine_json(&self, routine_id: &str) -> Result<Value> {
        let mut row = self
            .history
            .get_routine_definition(routine_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("routine {routine_id} not found"))?;
        let now = now_millis();
        row.paused_at = None;
        row.next_run_at = match row.next_run_at {
            Some(next_run_at) if next_run_at <= now => Some(next_run_at),
            _ => next_routine_run_at(&row.schedule_expression, now)
                .or_else(|| Some(now.saturating_add(FALLBACK_NEXT_RUN_DELAY_MS))),
        };
        row.updated_at = now;
        self.history.upsert_routine_definition(&row).await?;
        self.record_provenance_event(
            "routine_resumed",
            "routine resumed",
            serde_json::json!({ "routine_id": row.id, "next_run_at": row.next_run_at }),
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        Ok(serde_json::json!({
            "status": "resumed",
            "routine": routine_row_json(&row),
        }))
    }

    pub(crate) async fn delete_routine_json(&self, routine_id: &str) -> Result<Value> {
        let deleted = self.history.delete_routine_definition(routine_id).await?;
        if !deleted {
            return Err(anyhow::anyhow!("routine {routine_id} not found"));
        }
        self.record_provenance_event(
            "routine_deleted",
            "routine deleted",
            serde_json::json!({ "routine_id": routine_id }),
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        Ok(serde_json::json!({
            "status": "deleted",
            "routine_id": routine_id,
        }))
    }
}
