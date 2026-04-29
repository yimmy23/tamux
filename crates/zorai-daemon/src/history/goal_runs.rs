use super::*;
use crate::agent::types::{GoalAgentAssignment, GoalRuntimeOwnerProfile};

fn serialize_goal_runtime_owner_profile(
    profile: &Option<GoalRuntimeOwnerProfile>,
) -> rusqlite::Result<Option<String>> {
    profile
        .as_ref()
        .map(|value| {
            serde_json::to_string(value)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        })
        .transpose()
}

fn deserialize_goal_runtime_owner_profile(
    profile_json: Option<String>,
) -> Option<GoalRuntimeOwnerProfile> {
    profile_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
}

fn deserialize_goal_agent_assignments(
    assignments_json: Option<String>,
) -> Vec<GoalAgentAssignment> {
    assignments_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default()
}

fn deserialize_goal_run_thread_ids(
    thread_id: &Option<String>,
    execution_thread_ids_json: Option<String>,
) -> Vec<String> {
    let execution_thread_ids: Vec<String> = execution_thread_ids_json
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    if execution_thread_ids.is_empty() {
        return thread_id.clone().into_iter().collect();
    }
    execution_thread_ids
}

impl HistoryStore {
    pub async fn upsert_goal_run(&self, goal_run: &GoalRun) -> Result<()> {
        let goal_run = goal_run.clone();
        self.conn.call(move |conn| {
            let transaction = conn.transaction()?;
            let memory_updates_json = serde_json::to_string(&goal_run.memory_updates).call_err()?;
            let child_task_ids_json = serde_json::to_string(&goal_run.child_task_ids).call_err()?;
            let launch_assignment_snapshot_json =
                serde_json::to_string(&goal_run.launch_assignment_snapshot).call_err()?;
            let runtime_assignment_list_json =
                serde_json::to_string(&goal_run.runtime_assignment_list).call_err()?;
            let execution_thread_ids_json =
                serde_json::to_string(&goal_run.execution_thread_ids).call_err()?;
            let model_usage_json = serde_json::to_string(&goal_run.model_usage).call_err()?;
            let dossier_json = goal_run
                .dossier
                .as_ref()
                .map(|dossier| serde_json::to_string(dossier).call_err())
                .transpose()?;
            let planner_owner_profile_json =
                serialize_goal_runtime_owner_profile(&goal_run.planner_owner_profile)?;
            let current_step_owner_profile_json =
                serialize_goal_runtime_owner_profile(&goal_run.current_step_owner_profile)?;
            let authorship_tag = goal_run.authorship_tag.map(authorship_tag_to_str);

            transaction.execute(
                "INSERT OR REPLACE INTO goal_runs \
                 (id, title, goal, client_request_id, status, priority, created_at, updated_at, started_at, completed_at, thread_id, session_id, root_thread_id, active_thread_id, execution_thread_ids_json, current_step_index, replan_count, max_replans, plan_summary, reflection_summary, memory_updates_json, generated_skill_path, last_error, failure_cause, stopped_reason, child_task_ids_json, child_task_count, approval_count, awaiting_approval_id, policy_fingerprint, approval_expires_at, containment_scope, compensation_status, compensation_summary, active_task_id, duration_ms, dossier_json, total_prompt_tokens, total_completion_tokens, estimated_cost_usd, model_usage_json, autonomy_level, authorship_tag, planner_owner_profile_json, current_step_owner_profile_json, launch_assignment_snapshot_json, runtime_assignment_list_json, deleted_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42, ?43, ?44, ?45, ?46, ?47, NULL)",
                params![
                    &goal_run.id,
                    &goal_run.title,
                    &goal_run.goal,
                    &goal_run.client_request_id,
                    goal_run_status_to_str(goal_run.status),
                    task_priority_to_str(goal_run.priority),
                    goal_run.created_at as i64,
                    goal_run.updated_at as i64,
                    goal_run.started_at.map(|value| value as i64),
                    goal_run.completed_at.map(|value| value as i64),
                    &goal_run.thread_id,
                    &goal_run.session_id,
                    &goal_run.root_thread_id,
                    &goal_run.active_thread_id,
                    execution_thread_ids_json,
                    goal_run.current_step_index as i64,
                    goal_run.replan_count as i64,
                    goal_run.max_replans as i64,
                    &goal_run.plan_summary,
                    &goal_run.reflection_summary,
                    memory_updates_json,
                    &goal_run.generated_skill_path,
                    &goal_run.last_error,
                    &goal_run.failure_cause,
                    &goal_run.stopped_reason,
                    child_task_ids_json,
                    goal_run.child_task_count as i64,
                    goal_run.approval_count as i64,
                    &goal_run.awaiting_approval_id,
                    &goal_run.policy_fingerprint,
                    goal_run.approval_expires_at.map(|value| value as i64),
                    &goal_run.containment_scope,
                    &goal_run.compensation_status,
                    &goal_run.compensation_summary,
                    &goal_run.active_task_id,
                    goal_run.duration_ms.map(|value| value as i64),
                    dossier_json,
                    goal_run.total_prompt_tokens as i64,
                    goal_run.total_completion_tokens as i64,
                    goal_run.estimated_cost_usd,
                    model_usage_json,
                    autonomy_level_to_str(goal_run.autonomy_level),
                    authorship_tag,
                    planner_owner_profile_json,
                    current_step_owner_profile_json,
                    launch_assignment_snapshot_json,
                    runtime_assignment_list_json,
                ],
            )?;

        transaction.execute(
            "UPDATE goal_run_steps SET deleted_at = ?2 WHERE goal_run_id = ?1 AND deleted_at IS NULL",
            params![&goal_run.id, now_ts() as i64],
        )?;
        for step in &goal_run.steps {
            transaction.execute(
                "INSERT OR REPLACE INTO goal_run_steps \
                 (id, goal_run_id, ordinal, title, instructions, kind, success_criteria, session_id, status, task_id, summary, error, started_at, completed_at, deleted_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, NULL)",
                params![
                    &step.id,
                    &goal_run.id,
                    step.position as i64,
                    &step.title,
                    &step.instructions,
                    goal_run_step_kind_to_str(&step.kind),
                    &step.success_criteria,
                    &step.session_id,
                    goal_run_step_status_to_str(step.status),
                    &step.task_id,
                    &step.summary,
                    &step.error,
                    step.started_at.map(|value| value as i64),
                    step.completed_at.map(|value| value as i64),
                ],
            )?;
        }

        transaction.execute(
            "UPDATE goal_run_events SET deleted_at = ?2 WHERE goal_run_id = ?1 AND deleted_at IS NULL",
            params![&goal_run.id, now_ts() as i64],
        )?;
        for event in &goal_run.events {
            let todo_snapshot_json = serde_json::to_string(&event.todo_snapshot).call_err()?;
            transaction.execute(
                "INSERT OR REPLACE INTO goal_run_events (id, goal_run_id, timestamp, phase, message, details, step_index, todo_snapshot_json, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)",
                params![
                    &event.id,
                    &goal_run.id,
                    event.timestamp as i64,
                    &event.phase,
                    &event.message,
                    &event.details,
                    event.step_index.map(|value| value as i64),
                    todo_snapshot_json,
                ],
            )?;
        }

            transaction.commit()?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_goal_runs(&self) -> Result<Vec<GoalRun>> {
        self.read_conn.call(move |conn| {
        let mut step_stmt = conn.prepare(
            "SELECT id, goal_run_id, ordinal, title, instructions, kind, success_criteria, session_id, status, task_id, summary, error, started_at, completed_at \
             FROM goal_run_steps WHERE deleted_at IS NULL ORDER BY goal_run_id ASC, ordinal ASC",
        )?;
        let step_rows = step_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                GoalRunStep {
                    id: row.get(0)?,
                    position: row.get::<_, i64>(2)? as usize,
                    title: row.get(3)?,
                    instructions: row.get(4)?,
                    kind: parse_goal_run_step_kind(&row.get::<_, String>(5)?),
                    success_criteria: row.get(6)?,
                    session_id: row.get(7)?,
                    status: parse_goal_run_step_status(&row.get::<_, String>(8)?),
                    task_id: row.get(9)?,
                    summary: row.get(10)?,
                    error: row.get(11)?,
                    started_at: row.get::<_, Option<i64>>(12)?.map(|value| value as u64),
                    completed_at: row.get::<_, Option<i64>>(13)?.map(|value| value as u64),
                },
            ))
        })?;
        let mut step_map = std::collections::HashMap::<String, Vec<GoalRunStep>>::new();
        for row in step_rows {
            let (goal_run_id, step) = row?;
            step_map.entry(goal_run_id).or_default().push(step);
        }

        let mut event_stmt = conn.prepare(
            "SELECT id, goal_run_id, timestamp, phase, message, details, step_index, todo_snapshot_json FROM goal_run_events WHERE deleted_at IS NULL ORDER BY timestamp ASC",
        )?;
        let event_rows = event_stmt.query_map([], |row| {
            let todo_snapshot_json: Option<String> = row.get(7)?;
            Ok((
                row.get::<_, String>(1)?,
                GoalRunEvent {
                    id: row.get(0)?,
                    timestamp: row.get::<_, i64>(2)? as u64,
                    phase: row.get(3)?,
                    message: row.get(4)?,
                    details: row.get(5)?,
                    step_index: row.get::<_, Option<i64>>(6)?.map(|value| value as usize),
                    todo_snapshot: todo_snapshot_json
                        .as_deref()
                        .and_then(|json| serde_json::from_str(json).ok())
                        .unwrap_or_default(),
                },
            ))
        })?;
        let mut event_map = std::collections::HashMap::<String, Vec<GoalRunEvent>>::new();
        for row in event_rows {
            let (goal_run_id, event) = row?;
            event_map.entry(goal_run_id).or_default().push(event);
        }

        let mut stmt = conn.prepare(
            "SELECT id, title, goal, client_request_id, status, priority, created_at, updated_at, started_at, completed_at, thread_id, session_id, root_thread_id, active_thread_id, execution_thread_ids_json, current_step_index, replan_count, max_replans, plan_summary, reflection_summary, memory_updates_json, generated_skill_path, last_error, failure_cause, stopped_reason, child_task_ids_json, child_task_count, approval_count, awaiting_approval_id, policy_fingerprint, approval_expires_at, containment_scope, compensation_status, compensation_summary, active_task_id, duration_ms, dossier_json, total_prompt_tokens, total_completion_tokens, estimated_cost_usd, model_usage_json, autonomy_level, authorship_tag, planner_owner_profile_json, current_step_owner_profile_json, launch_assignment_snapshot_json, runtime_assignment_list_json \
             FROM goal_runs WHERE deleted_at IS NULL ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let thread_id: Option<String> = row.get(10)?;
            let root_thread_id: Option<String> = row.get(12)?;
            let active_thread_id: Option<String> = row.get(13)?;
            let execution_thread_ids_json: Option<String> = row.get(14)?;
            let memory_updates_json: String = row.get(20)?;
            let dossier_json: Option<String> = row.get(36)?;
            let child_task_ids_json: String = row.get(25)?;
            let model_usage_json: String = row.get(40)?;
            let planner_owner_profile_json: Option<String> = row.get(43)?;
            let current_step_owner_profile_json: Option<String> = row.get(44)?;
            let launch_assignment_snapshot_json: Option<String> = row.get(45)?;
            let runtime_assignment_list_json: Option<String> = row.get(46)?;
            let child_task_ids = serde_json::from_str(&child_task_ids_json).unwrap_or_default();
            let root_thread_id = root_thread_id.or_else(|| thread_id.clone());
            let active_thread_id = active_thread_id.or_else(|| thread_id.clone());
            Ok(GoalRun {
                id,
                title: row.get(1)?,
                goal: row.get(2)?,
                client_request_id: row.get(3)?,
                status: parse_goal_run_status(&row.get::<_, String>(4)?),
                priority: parse_task_priority(&row.get::<_, String>(5)?),
                created_at: row.get::<_, i64>(6)? as u64,
                updated_at: row.get::<_, i64>(7)? as u64,
                started_at: row.get::<_, Option<i64>>(8)?.map(|value| value as u64),
                completed_at: row.get::<_, Option<i64>>(9)?.map(|value| value as u64),
                thread_id: thread_id.clone(),
                root_thread_id,
                active_thread_id,
                execution_thread_ids: deserialize_goal_run_thread_ids(
                    &thread_id,
                    execution_thread_ids_json,
                ),
                session_id: row.get(11)?,
                current_step_index: row.get::<_, i64>(15)? as usize,
                current_step_title: None,
                current_step_kind: None,
                launch_assignment_snapshot: deserialize_goal_agent_assignments(
                    launch_assignment_snapshot_json,
                ),
                runtime_assignment_list: deserialize_goal_agent_assignments(
                    runtime_assignment_list_json,
                ),
                planner_owner_profile: deserialize_goal_runtime_owner_profile(
                    planner_owner_profile_json,
                ),
                current_step_owner_profile: deserialize_goal_runtime_owner_profile(
                    current_step_owner_profile_json,
                ),
                replan_count: row.get::<_, i64>(16)? as u32,
                max_replans: row.get::<_, i64>(17)? as u32,
                plan_summary: row.get(18)?,
                reflection_summary: row.get(19)?,
                memory_updates: serde_json::from_str(&memory_updates_json).unwrap_or_default(),
                generated_skill_path: row.get(21)?,
                last_error: row.get(22)?,
                failure_cause: row.get(23)?,
                stopped_reason: row.get(24)?,
                child_task_ids,
                child_task_count: row.get::<_, i64>(26)? as u32,
                approval_count: row.get::<_, i64>(27)? as u32,
                awaiting_approval_id: row.get(28)?,
                policy_fingerprint: row.get(29)?,
                approval_expires_at: row.get::<_, Option<i64>>(30)?.map(|value| value as u64),
                containment_scope: row.get(31)?,
                compensation_status: row.get(32)?,
                compensation_summary: row.get(33)?,
                active_task_id: row.get(34)?,
                duration_ms: row.get::<_, Option<i64>>(35)?.map(|value| value as u64),
                steps: Vec::new(),
                events: Vec::new(),
                dossier: dossier_json
                    .as_deref()
                    .and_then(|json| serde_json::from_str(json).ok()),
                total_prompt_tokens: row.get::<_, i64>(37)? as u64,
                total_completion_tokens: row.get::<_, i64>(38)? as u64,
                estimated_cost_usd: row.get(39)?,
                model_usage: serde_json::from_str(&model_usage_json).unwrap_or_default(),
                autonomy_level: parse_autonomy_level(&row.get::<_, String>(41)?),
                authorship_tag: row
                    .get::<_, Option<String>>(42)?
                    .map(|value| parse_authorship_tag(&value)),
            })
        })?;

        let mut goal_runs = Vec::new();
        for row in rows {
            let mut goal_run = row?;
            goal_run.steps = step_map.remove(&goal_run.id).unwrap_or_default();
            goal_run.events = event_map.remove(&goal_run.id).unwrap_or_default();
            if goal_run.current_step_title.is_none() {
                goal_run.current_step_title = goal_run
                    .steps
                    .get(goal_run.current_step_index)
                    .map(|step| step.title.clone());
            }
            if goal_run.current_step_kind.is_none() {
                goal_run.current_step_kind = goal_run
                    .steps
                    .get(goal_run.current_step_index)
                    .map(|step| step.kind.clone());
            }
            goal_runs.push(goal_run);
        }
        Ok(goal_runs)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_goal_run(&self, goal_run_id: &str) -> Result<Option<GoalRun>> {
        Ok(self
            .list_goal_runs()
            .await?
            .into_iter()
            .find(|goal_run| goal_run.id == goal_run_id))
    }

    pub async fn delete_goal_run(&self, goal_run_id: &str) -> Result<()> {
        let goal_run_id = goal_run_id.to_string();
        self.conn
            .call(move |conn| {
                let transaction = conn.transaction()?;
                transaction.execute(
                    "UPDATE goal_run_events SET deleted_at = ?2 WHERE goal_run_id = ?1 AND deleted_at IS NULL",
                    params![&goal_run_id, now_ts() as i64],
                )?;
                transaction.execute(
                    "UPDATE goal_run_steps SET deleted_at = ?2 WHERE goal_run_id = ?1 AND deleted_at IS NULL",
                    params![&goal_run_id, now_ts() as i64],
                )?;
                transaction.execute(
                    "UPDATE goal_runs SET deleted_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
                    params![&goal_run_id, now_ts() as i64],
                )?;
                transaction.commit()?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
