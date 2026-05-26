use super::*;
use crate::agent::types::{GoalAgentAssignment, GoalRuntimeOwnerProfile};

/// Narrow projection of `goal_runs` columns the concierge welcome actually
/// reads. Lets `concierge_goal_context` skip the heavy 46-column scan +
/// JSON-blob deserialization that `list_goal_runs_filtered` performs.
struct ConciergeGoalRunLean {
    id: String,
    title: String,
    goal: String,
    status_str: String,
    priority_str: String,
    created_at: u64,
    updated_at: u64,
    started_at: Option<u64>,
    completed_at: Option<u64>,
    plan_summary: Option<String>,
    reflection_summary: Option<String>,
    current_step_index: usize,
}

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

fn map_goal_run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GoalRun> {
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
        runtime_assignment_list: deserialize_goal_agent_assignments(runtime_assignment_list_json),
        planner_owner_profile: deserialize_goal_runtime_owner_profile(planner_owner_profile_json),
        current_step_owner_profile: deserialize_goal_runtime_owner_profile(
            current_step_owner_profile_json,
        ),
        step_owner_overrides: std::collections::BTreeMap::new(),
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
}

fn map_goal_run_thread_ref_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GoalRunThreadRef> {
    let thread_id: Option<String> = row.get(3)?;
    let root_thread_id: Option<String> = row.get(4)?;
    let active_thread_id: Option<String> = row.get(5)?;
    let execution_thread_ids_json: Option<String> = row.get(6)?;
    Ok(GoalRunThreadRef {
        id: row.get(0)?,
        status: parse_goal_run_status(&row.get::<_, String>(1)?),
        updated_at: row.get::<_, i64>(2)?.max(0) as u64,
        root_thread_id: root_thread_id.or_else(|| thread_id.clone()),
        active_thread_id: active_thread_id.or_else(|| thread_id.clone()),
        execution_thread_ids: deserialize_goal_run_thread_ids(
            &thread_id,
            execution_thread_ids_json,
        ),
        thread_id,
    })
}

/// Synchronous goal-run upsert body that runs inside an existing
/// transaction. Extracted so both `upsert_goal_run` and the batched
/// variant share one implementation. Keep INSERT column lists / step
/// reaping / event reaping in lockstep here.
fn upsert_goal_run_in_tx(
    transaction: &rusqlite::Transaction<'_>,
    goal_run: &GoalRun,
) -> std::result::Result<(), tokio_rusqlite::Error> {
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
    Ok(())
}

impl HistoryStore {
    pub async fn upsert_goal_run(&self, goal_run: &GoalRun) -> Result<()> {
        self.caches.latest_goal_run_for_thread.clear();
        let goal_run = goal_run.clone();
        self.conn
            .call(move |conn| {
                let transaction = conn.transaction()?;
                upsert_goal_run_in_tx(&transaction, &goal_run)?;
                transaction.commit()?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Batched upsert. Wraps N goal-runs in one transaction + one
    /// background-thread roundtrip. `persist_goal_runs`'s loop previously
    /// paid per-goal BEGIN/COMMIT and dispatch overhead.
    pub async fn upsert_goal_runs_batch(&self, goal_runs: &[GoalRun]) -> Result<()> {
        if goal_runs.is_empty() {
            return Ok(());
        }
        self.caches.latest_goal_run_for_thread.clear();
        let goal_runs = goal_runs.to_vec();
        self.conn
            .call(move |conn| {
                let transaction = conn.transaction()?;
                for goal_run in &goal_runs {
                    upsert_goal_run_in_tx(&transaction, goal_run)?;
                }
                transaction.commit()?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_goal_runs(&self) -> Result<Vec<GoalRun>> {
        self.list_goal_runs_filtered(None).await
    }

    pub(crate) async fn list_goal_runs_for_statuses(
        &self,
        statuses: &[GoalRunStatus],
    ) -> Result<Vec<GoalRun>> {
        self.list_goal_runs_for_statuses_limited(statuses, None)
            .await
    }

    pub(crate) async fn list_goal_runs_for_statuses_limited(
        &self,
        statuses: &[GoalRunStatus],
        limit: Option<usize>,
    ) -> Result<Vec<GoalRun>> {
        let goal_run_ids = self
            .list_goal_run_ids_for_statuses_limited(statuses, limit)
            .await?;

        let mut goal_runs = Vec::with_capacity(goal_run_ids.len());
        for goal_run_id in goal_run_ids {
            if let Some(goal_run) = self.get_goal_run(&goal_run_id).await? {
                goal_runs.push(goal_run);
            }
        }
        Ok(goal_runs)
    }

    pub(crate) async fn list_goal_run_ids_for_statuses(
        &self,
        statuses: &[GoalRunStatus],
    ) -> Result<Vec<String>> {
        self.list_goal_run_ids_for_statuses_limited(statuses, None)
            .await
    }

    pub(crate) async fn list_goal_run_ids_for_statuses_limited(
        &self,
        statuses: &[GoalRunStatus],
        limit: Option<usize>,
    ) -> Result<Vec<String>> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }
        let status_values = statuses
            .iter()
            .map(|status| goal_run_status_to_str(*status).to_string())
            .collect::<Vec<_>>();
        let limit = limit.map(|value| value.max(1) as i64);
        let goal_run_ids = self
            .read_conn
            .call(move |conn| {
                let placeholders = std::iter::repeat("?")
                    .take(status_values.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let mut sql = format!(
                    "SELECT id FROM goal_runs \
                     WHERE deleted_at IS NULL AND status IN ({placeholders}) \
                     ORDER BY updated_at DESC"
                );
                if limit.is_some() {
                    sql.push_str(" LIMIT ?");
                }
                let mut stmt = conn.prepare(&sql)?;
                let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = status_values
                    .into_iter()
                    .map(|status| Box::new(status) as Box<dyn rusqlite::types::ToSql>)
                    .collect();
                if let Some(limit) = limit {
                    params.push(Box::new(limit));
                }
                let param_refs = params
                    .iter()
                    .map(|param| param.as_ref())
                    .collect::<Vec<&dyn rusqlite::types::ToSql>>();
                let rows = stmt.query_map(param_refs.as_slice(), |row| row.get::<_, String>(0))?;
                let mut ids = Vec::new();
                for row in rows {
                    ids.push(row?);
                }
                Ok(ids)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(goal_run_ids)
    }

    pub(crate) async fn pause_interrupted_goal_runs_on_restart(
        &self,
        now_ms: u64,
    ) -> Result<usize> {
        self.conn
            .call(move |conn| {
                let transaction = conn.transaction()?;
                let mut stmt = transaction.prepare(
                    "SELECT id FROM goal_runs \
                     WHERE deleted_at IS NULL AND status IN (?1, ?2)",
                )?;
                let rows = stmt.query_map(
                    params![
                        goal_run_status_to_str(GoalRunStatus::Running),
                        goal_run_status_to_str(GoalRunStatus::Planning),
                    ],
                    |row| row.get::<_, String>(0),
                )?;
                let goal_run_ids = rows.collect::<std::result::Result<Vec<_>, _>>()?;
                drop(stmt);
                if goal_run_ids.is_empty() {
                    transaction.commit()?;
                    return Ok(0);
                }

                let placeholders = std::iter::repeat("?")
                    .take(goal_run_ids.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let update_sql = format!(
                    "UPDATE goal_runs SET status = ? \
                     WHERE deleted_at IS NULL AND id IN ({placeholders})"
                );
                let mut update_values = vec![rusqlite::types::Value::Text(
                    goal_run_status_to_str(GoalRunStatus::Paused).to_string(),
                )];
                update_values.extend(goal_run_ids.iter().cloned().map(rusqlite::types::Value::Text));
                transaction.execute(&update_sql, rusqlite::params_from_iter(update_values.iter()))?;

                for goal_run_id in &goal_run_ids {
                    transaction.execute(
                        "INSERT INTO goal_run_events \
                         (id, goal_run_id, timestamp, phase, message, details, step_index, todo_snapshot_json, deleted_at) \
                         VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, ?6, NULL)",
                        params![
                            uuid::Uuid::new_v4().to_string(),
                            goal_run_id,
                            now_ms as i64,
                            "restart",
                            "Daemon restarted; goal run paused for operator review.",
                            "[]",
                        ],
                    )?;
                }

                transaction.commit()?;
                Ok(goal_run_ids.len())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn has_goal_run_id(&self, goal_run_id: &str) -> Result<bool> {
        let goal_run_id = goal_run_id.to_string();
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT 1 FROM goal_runs WHERE id = ?1 AND deleted_at IS NULL LIMIT 1",
                )?;
                match stmt.query_row(params![goal_run_id], |_| Ok(())) {
                    Ok(()) => Ok(true),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
                    Err(error) => Err(error.into()),
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn goal_run_replan_count(&self, goal_run_id: &str) -> Result<Option<u32>> {
        let goal_run_id = goal_run_id.to_string();
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT replan_count FROM goal_runs WHERE id = ?1 AND deleted_at IS NULL LIMIT 1",
                )?;
                match stmt.query_row(params![goal_run_id], |row| row.get::<_, i64>(0)) {
                    Ok(value) => Ok(Some(value.max(0) as u32)),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(error) => Err(error.into()),
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn goal_run_thread_id(&self, goal_run_id: &str) -> Result<Option<String>> {
        let goal_run_id = goal_run_id.to_string();
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT thread_id FROM goal_runs WHERE id = ?1 AND deleted_at IS NULL LIMIT 1",
                )?;
                stmt.query_row(params![goal_run_id], |row| row.get(0))
                    .optional()
                    .map(Option::flatten)
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn goal_run_task_context(
        &self,
        goal_run_id: &str,
    ) -> Result<Option<GoalRunTaskContextRef>> {
        let goal_run_id = goal_run_id.to_string();
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT current_step_index, session_id FROM goal_runs WHERE id = ?1 AND deleted_at IS NULL LIMIT 1",
                )?;
                match stmt.query_row(params![goal_run_id], |row| {
                    Ok(GoalRunTaskContextRef {
                        current_step_index: row.get::<_, i64>(0)?.max(0) as usize,
                        session_id: row.get(1)?,
                    })
                }) {
                    Ok(context) => Ok(Some(context)),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(error) => Err(error.into()),
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn goal_run_current_step_title(
        &self,
        goal_run_id: &str,
    ) -> Result<Option<String>> {
        let goal_run_id = goal_run_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT (SELECT title FROM goal_run_steps
                               WHERE goal_run_steps.goal_run_id = goal_runs.id
                                 AND goal_run_steps.deleted_at IS NULL
                                 AND goal_run_steps.ordinal = goal_runs.current_step_index
                               LIMIT 1) AS current_step_title
                       FROM goal_runs
                      WHERE id = ?1 AND deleted_at IS NULL
                      LIMIT 1",
                    params![goal_run_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()
                .map(|value| value.flatten())
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn goal_run_progress_metrics(
        &self,
        goal_run_id: &str,
    ) -> Result<Option<GoalRunProgressMetricsRef>> {
        let goal_run_id = goal_run_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT
                         COUNT(goal_run_steps.id) AS steps_total,
                         SUM(CASE WHEN goal_run_steps.status = ?2 THEN 1 ELSE 0 END) AS steps_completed
                       FROM goal_runs
                       LEFT JOIN goal_run_steps
                         ON goal_run_steps.goal_run_id = goal_runs.id
                        AND goal_run_steps.deleted_at IS NULL
                      WHERE goal_runs.id = ?1
                        AND goal_runs.deleted_at IS NULL
                      GROUP BY goal_runs.id
                      LIMIT 1",
                    params![goal_run_id, goal_run_step_status_to_str(GoalRunStepStatus::Completed)],
                    |row| {
                        Ok(GoalRunProgressMetricsRef {
                            steps_total: row.get::<_, i64>(0)?.max(0) as usize,
                            steps_completed: row.get::<_, i64>(1)?.max(0) as usize,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn goal_run_policy_context(
        &self,
        goal_run_id: &str,
    ) -> Result<Option<GoalRunPolicyContextRef>> {
        let goal_run_id = goal_run_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT
                         goal_runs.goal,
                         goal_runs.title,
                         (SELECT title FROM goal_run_steps current_step
                           WHERE current_step.goal_run_id = goal_runs.id
                             AND current_step.deleted_at IS NULL
                             AND current_step.ordinal = goal_runs.current_step_index
                           LIMIT 1) AS current_step_title,
                         COUNT(all_steps.id) AS steps_total,
                         SUM(CASE WHEN all_steps.status = ?2 THEN 1 ELSE 0 END) AS steps_completed
                       FROM goal_runs
                       LEFT JOIN goal_run_steps all_steps
                         ON all_steps.goal_run_id = goal_runs.id
                        AND all_steps.deleted_at IS NULL
                      WHERE goal_runs.id = ?1
                        AND goal_runs.deleted_at IS NULL
                      GROUP BY goal_runs.id
                      LIMIT 1",
                    params![
                        goal_run_id,
                        goal_run_step_status_to_str(GoalRunStepStatus::Completed)
                    ],
                    |row| {
                        Ok(GoalRunPolicyContextRef {
                            goal: row.get(0)?,
                            title: row.get(1)?,
                            current_step_title: row.get(2)?,
                            steps_total: row.get::<_, i64>(3)?.max(0) as usize,
                            steps_completed: row.get::<_, i64>(4)?.max(0) as usize,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn goal_run_todo_context(
        &self,
        goal_run_id: &str,
        goal_step_id: Option<&str>,
    ) -> Result<Option<GoalRunTodoContextRef>> {
        let goal_run_id = goal_run_id.to_string();
        let goal_step_id = goal_step_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        self.read_conn
            .call(move |conn| {
                let Some(current_step_index) = conn
                    .query_row(
                        "SELECT current_step_index
                         FROM goal_runs
                         WHERE id = ?1 AND deleted_at IS NULL
                         LIMIT 1",
                        params![&goal_run_id],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()?
                else {
                    return Ok(None);
                };
                let current_step_index = current_step_index.max(0);

                let requested_step = match goal_step_id.as_deref() {
                    Some(goal_step_id) => conn
                        .query_row(
                            "SELECT ordinal, id, status
                             FROM goal_run_steps
                             WHERE goal_run_id = ?1
                               AND id = ?2
                               AND deleted_at IS NULL
                             LIMIT 1",
                            params![&goal_run_id, goal_step_id],
                            |row| {
                                Ok((
                                    row.get::<_, i64>(0)?.max(0) as usize,
                                    row.get::<_, String>(1)?,
                                    row.get::<_, String>(2)?,
                                ))
                            },
                        )
                        .optional()?,
                    None => None,
                };

                let selected_step = match requested_step {
                    Some(step) => Some(step),
                    None => conn
                        .query_row(
                            "SELECT ordinal, id, status
                             FROM goal_run_steps
                             WHERE goal_run_id = ?1
                               AND ordinal = ?2
                               AND deleted_at IS NULL
                             LIMIT 1",
                            params![&goal_run_id, current_step_index],
                            |row| {
                                Ok((
                                    row.get::<_, i64>(0)?.max(0) as usize,
                                    row.get::<_, String>(1)?,
                                    row.get::<_, String>(2)?,
                                ))
                            },
                        )
                        .optional()?,
                };

                Ok(Some(match selected_step {
                    Some((step_index, step_id, status)) => GoalRunTodoContextRef {
                        step_index,
                        step_id: Some(step_id),
                        step_status: Some(parse_goal_run_step_status(&status)),
                    },
                    None => GoalRunTodoContextRef {
                        step_index: current_step_index as usize,
                        step_id: None,
                        step_status: None,
                    },
                }))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn goal_run_workspace_runtime_ref(
        &self,
        goal_run_id: &str,
    ) -> Result<Option<GoalRunWorkspaceRuntimeRef>> {
        let goal_run_id = goal_run_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT id, status, last_error, reflection_summary, plan_summary
                     FROM goal_runs
                     WHERE id = ?1 AND deleted_at IS NULL
                     LIMIT 1",
                    params![goal_run_id],
                    |row| {
                        Ok(GoalRunWorkspaceRuntimeRef {
                            id: row.get(0)?,
                            status: parse_goal_run_status(&row.get::<_, String>(1)?),
                            last_error: row.get(2)?,
                            reflection_summary: row.get(3)?,
                            plan_summary: row.get(4)?,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn goal_run_compaction_scope_ref(
        &self,
        goal_run_id: &str,
    ) -> Result<Option<GoalRunCompactionScopeRef>> {
        let goal_run_id = goal_run_id.to_string();
        self.read_conn
            .call(move |conn| {
                let mut scope = match conn
                    .query_row(
                        "SELECT g.id, g.active_task_id, g.title, g.goal, g.status, \
                                g.root_thread_id, g.active_thread_id, g.execution_thread_ids_json, \
                                s.title, s.status, s.summary, g.plan_summary, g.last_error, \
                                g.failure_cause
                         FROM goal_runs g
                         LEFT JOIN goal_run_steps s
                           ON s.goal_run_id = g.id
                          AND s.ordinal = g.current_step_index
                          AND s.deleted_at IS NULL
                         WHERE g.id = ?1 AND g.deleted_at IS NULL
                         LIMIT 1",
                        params![&goal_run_id],
                        |row| {
                            let execution_thread_ids_json: Option<String> = row.get(7)?;
                            let last_error: Option<String> = row.get(12)?;
                            let failure_cause: Option<String> = row.get(13)?;
                            let step_status: Option<String> = row.get(9)?;
                            Ok(GoalRunCompactionScopeRef {
                                id: row.get(0)?,
                                active_task_id: row.get(1)?,
                                title: row.get(2)?,
                                goal: row.get(3)?,
                                status: parse_goal_run_status(&row.get::<_, String>(4)?),
                                root_thread_id: row.get(5)?,
                                active_thread_id: row.get(6)?,
                                execution_thread_ids: execution_thread_ids_json
                                    .as_deref()
                                    .and_then(|json| serde_json::from_str(json).ok())
                                    .unwrap_or_default(),
                                current_step_title: row.get(8)?,
                                current_step_status: step_status
                                    .as_deref()
                                    .map(parse_goal_run_step_status),
                                current_step_summary: row.get(10)?,
                                plan_summary: row.get(11)?,
                                latest_error: last_error.or(failure_cause),
                                recent_events: Vec::new(),
                            })
                        },
                    )
                    .optional()?
                {
                    Some(scope) => scope,
                    None => return Ok(None),
                };

                let mut event_stmt = conn.prepare(
                    "SELECT message
                     FROM (
                         SELECT message, timestamp
                         FROM goal_run_events
                         WHERE goal_run_id = ?1 AND deleted_at IS NULL
                         ORDER BY timestamp DESC
                         LIMIT 3
                     )
                     ORDER BY timestamp ASC",
                )?;
                let event_rows = event_stmt.query_map(params![&goal_run_id], |row| row.get(0))?;
                let mut recent_events = Vec::new();
                for row in event_rows {
                    recent_events.push(row?);
                }
                scope.recent_events = recent_events;
                Ok(Some(scope))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn latest_goal_run_status_reply_ref_for_thread_ids(
        &self,
        thread_ids: &[String],
    ) -> Result<Option<GoalRunStatusReplyRef>> {
        let thread_ids = thread_ids
            .iter()
            .map(|thread_id| thread_id.trim())
            .filter(|thread_id| !thread_id.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if thread_ids.is_empty() {
            return Ok(None);
        }

        self.read_conn
            .call(move |conn| {
                let placeholders = std::iter::repeat("?")
                    .take(thread_ids.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    "SELECT id, status, title, updated_at, \
                                                        (SELECT title FROM goal_run_steps \
                                                            WHERE goal_run_steps.goal_run_id = goal_runs.id \
                                                                AND goal_run_steps.deleted_at IS NULL \
                                                                AND goal_run_steps.ordinal = goal_runs.current_step_index \
                                                            LIMIT 1) AS current_step_title, \
                            plan_summary \
                       FROM goal_runs \
                      WHERE deleted_at IS NULL \
                        AND (thread_id IN ({placeholders}) \
                          OR root_thread_id IN ({placeholders}) \
                          OR active_thread_id IN ({placeholders}) \
                          OR EXISTS ( \
                             SELECT 1 \
                               FROM json_each(COALESCE(goal_runs.execution_thread_ids_json, '[]')) \
                              WHERE json_each.value IN ({placeholders}) \
                          )) \
                      ORDER BY updated_at DESC, id DESC \
                      LIMIT 1"
                );
                let mut values = Vec::with_capacity(thread_ids.len() * 4);
                for _ in 0..4 {
                    values.extend(thread_ids.iter().cloned().map(rusqlite::types::Value::Text));
                }
                let mut stmt = conn.prepare(&sql)?;
                stmt.query_row(rusqlite::params_from_iter(values.iter()), |row| {
                    Ok(GoalRunStatusReplyRef {
                        id: row.get(0)?,
                        status: parse_goal_run_status(&row.get::<_, String>(1)?),
                        title: row.get(2)?,
                        updated_at: row.get::<_, i64>(3)?.max(0) as u64,
                        current_step_title: row.get(4)?,
                        plan_summary: row.get(5)?,
                    })
                })
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn list_goal_run_status_refs_for_statuses(
        &self,
        statuses: &[GoalRunStatus],
    ) -> Result<Vec<(String, GoalRunStatus)>> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }
        let status_values = statuses
            .iter()
            .map(|status| goal_run_status_to_str(*status).to_string())
            .collect::<Vec<_>>();

        self.read_conn
            .call(move |conn| {
                let placeholders = std::iter::repeat("?")
                    .take(status_values.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    "SELECT id, status FROM goal_runs \
                     WHERE deleted_at IS NULL AND status IN ({placeholders}) \
                     ORDER BY updated_at DESC"
                );
                let mut stmt = conn.prepare(&sql)?;
                let values = status_values
                    .into_iter()
                    .map(rusqlite::types::Value::Text)
                    .collect::<Vec<_>>();
                let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        parse_goal_run_status(&row.get::<_, String>(1)?),
                    ))
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn list_goal_run_goal_refs_for_statuses(
        &self,
        statuses: &[GoalRunStatus],
    ) -> Result<Vec<(String, String)>> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }
        let status_values = statuses
            .iter()
            .map(|status| goal_run_status_to_str(*status).to_string())
            .collect::<Vec<_>>();

        self.read_conn
            .call(move |conn| {
                let placeholders = std::iter::repeat("?")
                    .take(status_values.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    "SELECT id, goal FROM goal_runs \
                     WHERE deleted_at IS NULL AND status IN ({placeholders}) \
                     ORDER BY updated_at DESC"
                );
                let mut stmt = conn.prepare(&sql)?;
                let values = status_values
                    .into_iter()
                    .map(rusqlite::types::Value::Text)
                    .collect::<Vec<_>>();
                let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn list_goal_run_operational_refs_for_statuses_limited(
        &self,
        statuses: &[GoalRunStatus],
        limit: Option<usize>,
    ) -> Result<Vec<GoalRunOperationalRef>> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }
        let status_values = statuses
            .iter()
            .map(|status| goal_run_status_to_str(*status).to_string())
            .collect::<Vec<_>>();
        let limit = limit.map(|value| value.max(1) as i64);

        self.read_conn
            .call(move |conn| {
                let placeholders = std::iter::repeat("?")
                    .take(status_values.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let mut sql = format!(
                    "SELECT id, status, title, current_step_index, \
                            (SELECT COUNT(*) FROM goal_run_steps \
                              WHERE goal_run_steps.goal_run_id = goal_runs.id \
                                AND goal_run_steps.deleted_at IS NULL) AS step_count \
                       FROM goal_runs \
                      WHERE deleted_at IS NULL AND status IN ({placeholders}) \
                      ORDER BY updated_at DESC"
                );
                if limit.is_some() {
                    sql.push_str(" LIMIT ?");
                }
                let mut values = status_values
                    .into_iter()
                    .map(rusqlite::types::Value::Text)
                    .collect::<Vec<_>>();
                if let Some(limit) = limit {
                    values.push(rusqlite::types::Value::Integer(limit));
                }
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
                    Ok(GoalRunOperationalRef {
                        id: row.get(0)?,
                        status: parse_goal_run_status(&row.get::<_, String>(1)?),
                        title: row.get(2)?,
                        current_step_index: row.get::<_, i64>(3)?.max(0) as usize,
                        step_count: row.get::<_, i64>(4)?.max(0) as usize,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn list_goal_run_quiet_recovery_refs_for_statuses(
        &self,
        statuses: &[GoalRunStatus],
    ) -> Result<Vec<GoalRunQuietRecoveryRef>> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }
        let status_values = statuses
            .iter()
            .map(|status| goal_run_status_to_str(*status).to_string())
            .collect::<Vec<_>>();

        self.read_conn
            .call(move |conn| {
                let placeholders = std::iter::repeat("?")
                    .take(status_values.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    "SELECT id, status, created_at, started_at, thread_id, root_thread_id, \
                            execution_thread_ids_json, current_step_index, active_task_id, \
                            (SELECT id FROM goal_run_steps \
                              WHERE goal_run_steps.goal_run_id = goal_runs.id \
                                AND goal_run_steps.deleted_at IS NULL \
                                AND goal_run_steps.ordinal = goal_runs.current_step_index \
                              LIMIT 1) AS current_step_id, \
                                                        (SELECT title FROM goal_run_steps \
                                                            WHERE goal_run_steps.goal_run_id = goal_runs.id \
                                                                AND goal_run_steps.deleted_at IS NULL \
                                                                AND goal_run_steps.ordinal = goal_runs.current_step_index \
                                                            LIMIT 1) AS current_step_title \
                       FROM goal_runs \
                      WHERE deleted_at IS NULL AND status IN ({placeholders}) \
                      ORDER BY updated_at DESC, id DESC"
                );
                let values = status_values
                    .into_iter()
                    .map(rusqlite::types::Value::Text)
                    .collect::<Vec<_>>();
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
                    let thread_id: Option<String> = row.get(4)?;
                    let execution_thread_ids_json: Option<String> = row.get(6)?;
                    Ok(GoalRunQuietRecoveryRef {
                        id: row.get(0)?,
                        status: parse_goal_run_status(&row.get::<_, String>(1)?),
                        created_at: row.get::<_, i64>(2)?.max(0) as u64,
                        started_at: row
                            .get::<_, Option<i64>>(3)?
                            .map(|value| value.max(0) as u64),
                        root_thread_id: row.get(5)?,
                        execution_thread_ids: deserialize_goal_run_thread_ids(
                            &thread_id,
                            execution_thread_ids_json,
                        ),
                        current_step_index: row.get::<_, i64>(7)?.max(0) as usize,
                        active_task_id: row.get(8)?,
                        current_step_id: row.get(9)?,
                        current_step_title: row.get(10)?,
                        thread_id,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn list_goal_run_brief_refs_for_statuses(
        &self,
        statuses: &[GoalRunStatus],
    ) -> Result<Vec<GoalRunBriefRef>> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }
        let status_values = statuses
            .iter()
            .map(|status| goal_run_status_to_str(*status).to_string())
            .collect::<Vec<_>>();

        self.read_conn
            .call(move |conn| {
                let placeholders = std::iter::repeat("?")
                    .take(status_values.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    "SELECT id, status, title, updated_at, thread_id, \
                            (SELECT title FROM goal_run_steps \
                              WHERE goal_run_steps.goal_run_id = goal_runs.id \
                                AND goal_run_steps.deleted_at IS NULL \
                                AND goal_run_steps.ordinal = goal_runs.current_step_index \
                              LIMIT 1) AS current_step_title \
                       FROM goal_runs \
                      WHERE deleted_at IS NULL AND status IN ({placeholders}) \
                      ORDER BY updated_at DESC"
                );
                let values = status_values
                    .into_iter()
                    .map(rusqlite::types::Value::Text)
                    .collect::<Vec<_>>();
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
                    Ok(GoalRunBriefRef {
                        id: row.get(0)?,
                        status: parse_goal_run_status(&row.get::<_, String>(1)?),
                        title: row.get(2)?,
                        updated_at: row.get::<_, i64>(3)? as u64,
                        thread_id: row.get(4)?,
                        current_step_title: row.get(5)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn list_goal_runs_for_statuses_updated_before(
        &self,
        statuses: &[GoalRunStatus],
        updated_at_lte: u64,
    ) -> Result<Vec<GoalRun>> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }
        let status_values = statuses
            .iter()
            .map(|status| goal_run_status_to_str(*status).to_string())
            .collect::<Vec<_>>();
        let goal_run_ids = self
            .read_conn
            .call(move |conn| {
                let placeholders = std::iter::repeat("?")
                    .take(status_values.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    "SELECT id FROM goal_runs \
                     WHERE deleted_at IS NULL \
                       AND updated_at <= ? \
                       AND status IN ({placeholders}) \
                     ORDER BY updated_at ASC, id ASC"
                );
                let mut values = vec![rusqlite::types::Value::Integer(updated_at_lte as i64)];
                values.extend(status_values.into_iter().map(rusqlite::types::Value::Text));
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
                    row.get::<_, String>(0)
                })?;
                let mut ids = Vec::new();
                for row in rows {
                    ids.push(row?);
                }
                Ok(ids)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let mut goal_runs = Vec::with_capacity(goal_run_ids.len());
        for goal_run_id in goal_run_ids {
            if let Some(goal_run) = self.get_goal_run(&goal_run_id).await? {
                goal_runs.push(goal_run);
            }
        }
        Ok(goal_runs)
    }

    pub(crate) async fn list_goal_run_stuck_check_refs_updated_before(
        &self,
        statuses: &[GoalRunStatus],
        updated_at_lte: u64,
    ) -> Result<Vec<GoalRunStuckCheckRef>> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }
        let status_values = statuses
            .iter()
            .map(|status| goal_run_status_to_str(*status).to_string())
            .collect::<Vec<_>>();

        self.read_conn
            .call(move |conn| {
                let placeholders = std::iter::repeat("?")
                    .take(status_values.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    "SELECT id, status, title, updated_at, last_error FROM goal_runs \
                     WHERE deleted_at IS NULL \
                       AND updated_at <= ? \
                       AND status IN ({placeholders}) \
                     ORDER BY updated_at ASC, id ASC"
                );
                let mut values = vec![rusqlite::types::Value::Integer(updated_at_lte as i64)];
                values.extend(status_values.into_iter().map(rusqlite::types::Value::Text));
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
                    Ok(GoalRunStuckCheckRef {
                        id: row.get(0)?,
                        status: parse_goal_run_status(&row.get::<_, String>(1)?),
                        title: row.get(2)?,
                        updated_at: row.get::<_, i64>(3)?.max(0) as u64,
                        last_error: row.get(4)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn list_goal_runs_for_thread_ids(
        &self,
        thread_ids: &[String],
    ) -> Result<Vec<GoalRun>> {
        let thread_ids = thread_ids
            .iter()
            .map(|thread_id| thread_id.trim())
            .filter(|thread_id| !thread_id.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if thread_ids.is_empty() {
            return Ok(Vec::new());
        }

        let goal_run_ids = self
            .read_conn
            .call(move |conn| {
                let placeholders = std::iter::repeat("?")
                    .take(thread_ids.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    "SELECT id FROM goal_runs \
                     WHERE deleted_at IS NULL \
                       AND (thread_id IN ({placeholders}) \
                         OR root_thread_id IN ({placeholders}) \
                         OR active_thread_id IN ({placeholders}) \
                         OR EXISTS ( \
                            SELECT 1 \
                              FROM json_each(COALESCE(goal_runs.execution_thread_ids_json, '[]')) \
                             WHERE json_each.value IN ({placeholders}) \
                         )) \
                     ORDER BY updated_at DESC, id DESC"
                );
                let mut values = Vec::with_capacity(thread_ids.len() * 4);
                for _ in 0..4 {
                    values.extend(thread_ids.iter().cloned().map(rusqlite::types::Value::Text));
                }
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
                    row.get::<_, String>(0)
                })?;
                let mut ids = Vec::new();
                for row in rows {
                    ids.push(row?);
                }
                Ok(ids)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let mut goal_runs = Vec::with_capacity(goal_run_ids.len());
        for goal_run_id in goal_run_ids {
            if let Some(goal_run) = self.get_goal_run(&goal_run_id).await? {
                goal_runs.push(goal_run);
            }
        }
        Ok(goal_runs)
    }

    pub(crate) async fn list_goal_run_thread_refs_for_thread_ids(
        &self,
        thread_ids: &[String],
    ) -> Result<Vec<GoalRunThreadRef>> {
        let thread_ids = thread_ids
            .iter()
            .map(|thread_id| thread_id.trim())
            .filter(|thread_id| !thread_id.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if thread_ids.is_empty() {
            return Ok(Vec::new());
        }

        self.read_conn
            .call(move |conn| {
                let placeholders = std::iter::repeat("?")
                    .take(thread_ids.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    "SELECT id, status, updated_at, thread_id, root_thread_id, active_thread_id, execution_thread_ids_json \
                     FROM goal_runs \
                     WHERE deleted_at IS NULL \
                       AND (thread_id IN ({placeholders}) \
                         OR root_thread_id IN ({placeholders}) \
                         OR active_thread_id IN ({placeholders}) \
                         OR EXISTS ( \
                            SELECT 1 \
                              FROM json_each(COALESCE(goal_runs.execution_thread_ids_json, '[]')) \
                             WHERE json_each.value IN ({placeholders}) \
                         )) \
                     ORDER BY updated_at DESC, id DESC"
                );
                let mut values = Vec::with_capacity(thread_ids.len() * 4);
                for _ in 0..4 {
                    values.extend(thread_ids.iter().cloned().map(rusqlite::types::Value::Text));
                }
                let mut stmt = conn.prepare(&sql)?;
                let rows =
                    stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
                        map_goal_run_thread_ref_row(row)
                    })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn list_goal_run_thread_refs_for_statuses(
        &self,
        statuses: &[GoalRunStatus],
    ) -> Result<Vec<GoalRunThreadRef>> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }
        let status_values = statuses
            .iter()
            .map(|status| goal_run_status_to_str(*status).to_string())
            .collect::<Vec<_>>();

        self.read_conn
            .call(move |conn| {
                let placeholders = std::iter::repeat("?")
                    .take(status_values.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    "SELECT id, status, updated_at, thread_id, root_thread_id, active_thread_id, execution_thread_ids_json \
                     FROM goal_runs \
                     WHERE deleted_at IS NULL AND status IN ({placeholders}) \
                     ORDER BY updated_at DESC, id DESC"
                );
                let values = status_values
                    .into_iter()
                    .map(rusqlite::types::Value::Text)
                    .collect::<Vec<_>>();
                let mut stmt = conn.prepare(&sql)?;
                let rows =
                    stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
                        map_goal_run_thread_ref_row(row)
                    })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn latest_goal_run_id_for_thread_ids(
        &self,
        thread_ids: &[String],
    ) -> Result<Option<String>> {
        self.latest_goal_run_id_for_thread_ids_and_statuses(thread_ids, &[])
            .await
    }

    pub(crate) async fn latest_goal_run_id_for_thread_ids_and_statuses(
        &self,
        thread_ids: &[String],
        statuses: &[GoalRunStatus],
    ) -> Result<Option<String>> {
        Ok(self
            .latest_goal_run_id_and_updated_at_for_thread_ids_and_statuses(thread_ids, statuses)
            .await?
            .map(|(goal_run_id, _)| goal_run_id))
    }

    pub(crate) async fn latest_goal_run_id_and_updated_at_for_thread_ids_and_statuses(
        &self,
        thread_ids: &[String],
        statuses: &[GoalRunStatus],
    ) -> Result<Option<(String, u64)>> {
        let thread_ids = thread_ids
            .iter()
            .map(|thread_id| thread_id.trim())
            .filter(|thread_id| !thread_id.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if thread_ids.is_empty() {
            return Ok(None);
        }
        let status_values = statuses
            .iter()
            .map(|status| goal_run_status_to_str(*status).to_string())
            .collect::<Vec<_>>();

        self.read_conn
            .call(move |conn| {
                let placeholders = std::iter::repeat("?")
                    .take(thread_ids.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let status_filter = if status_values.is_empty() {
                    String::new()
                } else {
                    let status_placeholders = std::iter::repeat("?")
                        .take(status_values.len())
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(" AND status IN ({status_placeholders})")
                };
                let sql = format!(
                    "SELECT id, updated_at FROM goal_runs \
                     WHERE deleted_at IS NULL{status_filter} \
                       AND (thread_id IN ({placeholders}) \
                         OR root_thread_id IN ({placeholders}) \
                         OR active_thread_id IN ({placeholders}) \
                         OR EXISTS ( \
                            SELECT 1 \
                              FROM json_each(COALESCE(goal_runs.execution_thread_ids_json, '[]')) \
                             WHERE json_each.value IN ({placeholders}) \
                         )) \
                     ORDER BY updated_at DESC, id DESC \
                     LIMIT 1"
                );
                let mut values = Vec::with_capacity(status_values.len() + thread_ids.len() * 4);
                values.extend(status_values.into_iter().map(rusqlite::types::Value::Text));
                for _ in 0..4 {
                    values.extend(thread_ids.iter().cloned().map(rusqlite::types::Value::Text));
                }
                conn.query_row(&sql, rusqlite::params_from_iter(values.iter()), |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?.max(0) as u64,
                    ))
                })
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn list_active_goal_runs_for_start_request(
        &self,
        thread_id: Option<String>,
        session_id: Option<String>,
        client_request_id: Option<String>,
    ) -> Result<Vec<GoalRun>> {
        let status_values = [
            GoalRunStatus::Queued,
            GoalRunStatus::Planning,
            GoalRunStatus::Running,
            GoalRunStatus::AwaitingApproval,
            GoalRunStatus::Paused,
        ]
        .into_iter()
        .map(|status| goal_run_status_to_str(status).to_string())
        .collect::<Vec<_>>();
        let goal_run_ids = self
            .read_conn
            .call(move |conn| {
                let status_placeholders = std::iter::repeat("?")
                    .take(status_values.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let mut sql = format!(
                    "SELECT goal_runs.id FROM goal_runs \
                     LEFT JOIN agent_threads \
                       ON agent_threads.id = goal_runs.thread_id \
                      AND agent_threads.deleted_at IS NULL \
                     WHERE goal_runs.deleted_at IS NULL \
                       AND ((? IS NULL AND goal_runs.session_id IS NULL) OR goal_runs.session_id = ?) \
                       AND goal_runs.status IN ({status_placeholders})"
                );
                let session_value = session_id
                    .clone()
                    .map(rusqlite::types::Value::Text)
                    .unwrap_or(rusqlite::types::Value::Null);
                let mut values = vec![session_value.clone(), session_value];
                values.extend(
                    status_values
                        .into_iter()
                        .map(rusqlite::types::Value::Text),
                );
                if let Some(client_request_id) = client_request_id {
                    sql.push_str(" AND goal_runs.client_request_id = ?");
                    values.push(rusqlite::types::Value::Text(client_request_id));
                } else {
                    let upstream_thread_expr = "\
                        CASE \
                            WHEN agent_threads.metadata_json IS NOT NULL \
                                 AND json_valid(agent_threads.metadata_json) \
                            THEN COALESCE(\
                                json_extract(agent_threads.metadata_json, '$.upstream_thread_id'), \
                                json_extract(agent_threads.metadata_json, '$.upstreamThreadId')\
                            ) \
                            ELSE NULL \
                        END";
                    sql.push_str(&format!(
                        " AND (\
                              (? IS NULL AND (goal_runs.thread_id IS NULL OR {upstream_thread_expr} IS NULL OR {upstream_thread_expr} = '')) \
                              OR goal_runs.thread_id = ? \
                              OR {upstream_thread_expr} = ?\
                          )"
                    ));
                    let thread_value = thread_id
                        .map(rusqlite::types::Value::Text)
                        .unwrap_or(rusqlite::types::Value::Null);
                    values.push(thread_value.clone());
                    values.push(thread_value.clone());
                    values.push(thread_value);
                }
                sql.push_str(" ORDER BY goal_runs.updated_at DESC, goal_runs.id DESC");

                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
                    row.get::<_, String>(0)
                })?;
                let mut ids = Vec::new();
                for row in rows {
                    ids.push(row?);
                }
                Ok(ids)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let mut goal_runs = Vec::with_capacity(goal_run_ids.len());
        for goal_run_id in goal_run_ids {
            if let Some(goal_run) = self.get_goal_run(&goal_run_id).await? {
                goal_runs.push(goal_run);
            }
        }
        Ok(goal_runs)
    }

    pub(crate) async fn concierge_goal_context(&self) -> Result<ConciergeGoalContext> {
        let started = std::time::Instant::now();
        let running_str = goal_run_status_to_str(GoalRunStatus::Running);
        let paused_str = goal_run_status_to_str(GoalRunStatus::Paused);

        let (latest_goal_run, running_goal_total, paused_goal_total) = self
            .interactive_read_conn
            .call(move |conn| {
                let (latest_id, running_total, paused_total): (Option<String>, i64, i64) =
                    conn.query_row(
                        "SELECT \
                             (SELECT id FROM goal_runs WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT 1), \
                             COALESCE(SUM(CASE WHEN status = ?1 THEN 1 ELSE 0 END), 0), \
                             COALESCE(SUM(CASE WHEN status = ?2 THEN 1 ELSE 0 END), 0) \
                         FROM goal_runs \
                         WHERE deleted_at IS NULL",
                        params![running_str, paused_str],
                        |row| {
                            Ok((
                                row.get::<_, Option<String>>(0)?,
                                row.get::<_, i64>(1)?,
                                row.get::<_, i64>(2)?,
                            ))
                        },
                    )?;

                let latest_goal_run = if let Some(goal_run_id) = latest_id {
                    let lean: ConciergeGoalRunLean = conn.query_row(
                        "SELECT id, title, goal, status, priority, created_at, updated_at, \
                                started_at, completed_at, plan_summary, reflection_summary, \
                                current_step_index \
                         FROM goal_runs WHERE id = ?1",
                        params![goal_run_id],
                        |row| {
                            Ok(ConciergeGoalRunLean {
                                id: row.get(0)?,
                                title: row.get(1)?,
                                goal: row.get(2)?,
                                status_str: row.get(3)?,
                                priority_str: row.get(4)?,
                                created_at: row.get::<_, i64>(5)? as u64,
                                updated_at: row.get::<_, i64>(6)? as u64,
                                started_at: row.get::<_, Option<i64>>(7)?.map(|v| v as u64),
                                completed_at: row.get::<_, Option<i64>>(8)?.map(|v| v as u64),
                                plan_summary: row.get(9)?,
                                reflection_summary: row.get(10)?,
                                current_step_index: row.get::<_, i64>(11)?.max(0) as usize,
                            })
                        },
                    )?;

                    let current_step_title: Option<String> = conn
                        .query_row(
                            "SELECT title FROM goal_run_steps \
                             WHERE goal_run_id = ?1 AND ordinal = ?2 AND deleted_at IS NULL",
                            params![lean.id, lean.current_step_index as i64],
                            |row| row.get::<_, String>(0),
                        )
                        .optional()?;

                    let latest_step: Option<(String, Option<String>, Option<String>, Option<i64>, Option<i64>, i64)> = conn
                        .query_row(
                            "SELECT title, summary, error, started_at, completed_at, ordinal \
                             FROM goal_run_steps \
                             WHERE goal_run_id = ?1 \
                               AND deleted_at IS NULL \
                               AND ((summary IS NOT NULL AND length(trim(summary)) > 0) \
                                    OR (error IS NOT NULL AND length(trim(error)) > 0)) \
                             ORDER BY COALESCE(completed_at, started_at, ordinal) DESC, ordinal DESC \
                             LIMIT 1",
                            params![lean.id],
                            |row| {
                                Ok((
                                    row.get::<_, String>(0)?,
                                    row.get::<_, Option<String>>(1)?,
                                    row.get::<_, Option<String>>(2)?,
                                    row.get::<_, Option<i64>>(3)?,
                                    row.get::<_, Option<i64>>(4)?,
                                    row.get::<_, i64>(5)?,
                                ))
                            },
                        )
                        .optional()?;

                    let mut steps: Vec<GoalRunStep> = Vec::new();
                    if let Some((title, summary, error, started_at, completed_at, ordinal)) =
                        latest_step
                    {
                        steps.push(GoalRunStep {
                            id: String::new(),
                            position: ordinal.max(0) as usize,
                            title,
                            instructions: String::new(),
                            kind: GoalRunStepKind::Reason,
                            success_criteria: String::new(),
                            session_id: None,
                            status: GoalRunStepStatus::Completed,
                            task_id: None,
                            summary,
                            error,
                            started_at: started_at.map(|v| v as u64),
                            completed_at: completed_at.map(|v| v as u64),
                        });
                    }

                    Some(GoalRun {
                        id: lean.id,
                        title: lean.title,
                        goal: lean.goal,
                        client_request_id: None,
                        status: parse_goal_run_status(&lean.status_str),
                        priority: parse_task_priority(&lean.priority_str),
                        created_at: lean.created_at,
                        updated_at: lean.updated_at,
                        started_at: lean.started_at,
                        completed_at: lean.completed_at,
                        thread_id: None,
                        root_thread_id: None,
                        active_thread_id: None,
                        execution_thread_ids: Vec::new(),
                        session_id: None,
                        current_step_index: lean.current_step_index,
                        current_step_title,
                        current_step_kind: None,
                        launch_assignment_snapshot: Vec::new(),
                        runtime_assignment_list: Vec::new(),
                        planner_owner_profile: None,
                        current_step_owner_profile: None,
                        step_owner_overrides: std::collections::BTreeMap::new(),
                        replan_count: 0,
                        max_replans: 0,
                        plan_summary: lean.plan_summary,
                        reflection_summary: lean.reflection_summary,
                        memory_updates: Vec::new(),
                        generated_skill_path: None,
                        last_error: None,
                        failure_cause: None,
                        stopped_reason: None,
                        child_task_ids: Vec::new(),
                        child_task_count: 0,
                        approval_count: 0,
                        awaiting_approval_id: None,
                        policy_fingerprint: None,
                        approval_expires_at: None,
                        containment_scope: None,
                        compensation_status: None,
                        compensation_summary: None,
                        active_task_id: None,
                        duration_ms: None,
                        steps,
                        events: Vec::new(),
                        dossier: None,
                        total_prompt_tokens: 0,
                        total_completion_tokens: 0,
                        estimated_cost_usd: None,
                        model_usage: Vec::new(),
                        autonomy_level: crate::agent::AutonomyLevel::default(),
                        authorship_tag: None,
                    })
                } else {
                    None
                };

                Ok((
                    latest_goal_run,
                    running_total.max(0) as usize,
                    paused_total.max(0) as usize,
                ))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        tracing::info!(
            elapsed_ms = started.elapsed().as_millis() as u64,
            running_goal_total,
            paused_goal_total,
            has_latest = latest_goal_run.is_some(),
            "concierge_goal_context: lean query completed"
        );
        Ok(ConciergeGoalContext {
            latest_goal_run,
            running_goal_total,
            paused_goal_total,
        })
    }

    async fn list_goal_runs_filtered(&self, goal_run_id: Option<String>) -> Result<Vec<GoalRun>> {
        self.read_conn.call(move |conn| {
        let mut step_sql = "SELECT id, goal_run_id, ordinal, title, instructions, kind, success_criteria, session_id, status, task_id, summary, error, started_at, completed_at \
             FROM goal_run_steps WHERE deleted_at IS NULL".to_string();
        let mut step_values = Vec::<rusqlite::types::Value>::new();
        if let Some(goal_run_id) = goal_run_id.as_deref() {
            step_sql.push_str(" AND goal_run_id = ?");
            step_values.push(rusqlite::types::Value::Text(goal_run_id.to_string()));
        }
        step_sql.push_str(" ORDER BY goal_run_id ASC, ordinal ASC");

        let mut step_stmt = conn.prepare(&step_sql)?;
        let step_rows = step_stmt.query_map(rusqlite::params_from_iter(step_values.iter()), |row| {
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

        let mut event_sql = "SELECT id, goal_run_id, timestamp, phase, message, details, step_index, todo_snapshot_json FROM goal_run_events WHERE deleted_at IS NULL".to_string();
        let mut event_values = Vec::<rusqlite::types::Value>::new();
        if let Some(goal_run_id) = goal_run_id.as_deref() {
            event_sql.push_str(" AND goal_run_id = ?");
            event_values.push(rusqlite::types::Value::Text(goal_run_id.to_string()));
        }
        event_sql.push_str(" ORDER BY timestamp ASC");

        let mut event_stmt = conn.prepare(&event_sql)?;
        let event_rows = event_stmt.query_map(rusqlite::params_from_iter(event_values.iter()), |row| {
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

        let mut goal_sql = "SELECT id, title, goal, client_request_id, status, priority, created_at, updated_at, started_at, completed_at, thread_id, session_id, root_thread_id, active_thread_id, execution_thread_ids_json, current_step_index, replan_count, max_replans, plan_summary, reflection_summary, memory_updates_json, generated_skill_path, last_error, failure_cause, stopped_reason, child_task_ids_json, child_task_count, approval_count, awaiting_approval_id, policy_fingerprint, approval_expires_at, containment_scope, compensation_status, compensation_summary, active_task_id, duration_ms, dossier_json, total_prompt_tokens, total_completion_tokens, estimated_cost_usd, model_usage_json, autonomy_level, authorship_tag, planner_owner_profile_json, current_step_owner_profile_json, launch_assignment_snapshot_json, runtime_assignment_list_json \
             FROM goal_runs WHERE deleted_at IS NULL".to_string();
        let mut goal_values = Vec::<rusqlite::types::Value>::new();
        if let Some(goal_run_id) = goal_run_id.as_deref() {
            goal_sql.push_str(" AND id = ?");
            goal_values.push(rusqlite::types::Value::Text(goal_run_id.to_string()));
        }
        goal_sql.push_str(" ORDER BY updated_at DESC");

        let mut stmt = conn.prepare(&goal_sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(goal_values.iter()), |row| {
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
                step_owner_overrides: std::collections::BTreeMap::new(),
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
            .list_goal_runs_filtered(Some(goal_run_id.to_string()))
            .await?
            .into_iter()
            .next())
    }

    pub(crate) async fn latest_goal_run_for_thread(
        &self,
        thread_id: &str,
    ) -> Result<Option<GoalRun>> {
        let thread_id_owned = thread_id.to_string();
        if let Some(cached) = self.caches.latest_goal_run_for_thread.get(&thread_id_owned) {
            return Ok(cached);
        }
        let thread_id_for_query = thread_id_owned.clone();
        let goal_run_id = self
            .read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT id
                     FROM goal_runs
                     WHERE deleted_at IS NULL
                       AND thread_id = ?1
                     ORDER BY updated_at DESC, id DESC
                     LIMIT 1",
                    params![thread_id_for_query],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let value = match goal_run_id {
            Some(goal_run_id) => self.get_goal_run(&goal_run_id).await?,
            None => None,
        };
        self.caches
            .latest_goal_run_for_thread
            .insert(thread_id_owned, value.clone());
        Ok(value)
    }

    pub(crate) async fn latest_goal_run_repo_context_for_thread(
        &self,
        thread_id: &str,
    ) -> Result<Option<GoalRunRepoContextRef>> {
        let thread_id = thread_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT id, session_id, current_step_index
                     FROM goal_runs
                     WHERE deleted_at IS NULL
                       AND thread_id = ?1
                     ORDER BY updated_at DESC, id DESC
                     LIMIT 1",
                    params![thread_id],
                    |row| {
                        Ok(GoalRunRepoContextRef {
                            id: row.get(0)?,
                            session_id: row.get(1)?,
                            current_step_index: row.get::<_, i64>(2)?.max(0) as usize,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn list_goal_runs_page(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<GoalRun>, usize)> {
        let limit = limit.max(1) as i64;
        let offset = offset as i64;
        self.interactive_read_conn
            .call(move |conn| {
                let total = conn.query_row(
                    "SELECT COUNT(*) FROM goal_runs WHERE deleted_at IS NULL",
                    [],
                    |row| row.get::<_, i64>(0),
                )?;
                let mut id_stmt = conn.prepare(
                    "SELECT id FROM goal_runs \
                     WHERE deleted_at IS NULL \
                     ORDER BY updated_at DESC \
                     LIMIT ?1 OFFSET ?2",
                )?;
                let id_rows =
                    id_stmt.query_map(params![limit, offset], |row| row.get::<_, String>(0))?;
                let mut goal_run_ids = Vec::new();
                for row in id_rows {
                    goal_run_ids.push(row?);
                }
                if goal_run_ids.is_empty() {
                    return Ok((Vec::new(), total.max(0) as usize));
                }

                let placeholders = std::iter::repeat("?")
                    .take(goal_run_ids.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let values = goal_run_ids
                    .iter()
                    .cloned()
                    .map(rusqlite::types::Value::Text)
                    .collect::<Vec<_>>();

                let step_sql = format!(
                    "SELECT id, goal_run_id, ordinal, title, instructions, kind, success_criteria, session_id, status, task_id, summary, error, started_at, completed_at \
                     FROM goal_run_steps \
                     WHERE deleted_at IS NULL AND goal_run_id IN ({placeholders}) \
                     ORDER BY goal_run_id ASC, ordinal ASC"
                );
                let mut step_stmt = conn.prepare(&step_sql)?;
                let step_rows =
                    step_stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
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
                                started_at: row
                                    .get::<_, Option<i64>>(12)?
                                    .map(|value| value as u64),
                                completed_at: row
                                    .get::<_, Option<i64>>(13)?
                                    .map(|value| value as u64),
                            },
                        ))
                    })?;
                let mut step_map = std::collections::HashMap::<String, Vec<GoalRunStep>>::new();
                for row in step_rows {
                    let (goal_run_id, step) = row?;
                    step_map.entry(goal_run_id).or_default().push(step);
                }

                let event_sql = format!(
                    "SELECT id, goal_run_id, timestamp, phase, message, details, step_index, todo_snapshot_json \
                     FROM goal_run_events \
                     WHERE deleted_at IS NULL AND goal_run_id IN ({placeholders}) \
                     ORDER BY goal_run_id ASC, timestamp ASC"
                );
                let mut event_stmt = conn.prepare(&event_sql)?;
                let event_rows = event_stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
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

                let goal_sql = format!(
                    "SELECT id, title, goal, client_request_id, status, priority, \
                            created_at, updated_at, started_at, completed_at, \
                            thread_id, session_id, root_thread_id, active_thread_id, \
                            current_step_index, replan_count, max_replans, \
                            plan_summary, reflection_summary, generated_skill_path, \
                            last_error, failure_cause, stopped_reason, \
                            child_task_count, approval_count, awaiting_approval_id, \
                            policy_fingerprint, approval_expires_at, containment_scope, \
                            compensation_status, compensation_summary, active_task_id, \
                            duration_ms, total_prompt_tokens, total_completion_tokens, \
                            estimated_cost_usd, autonomy_level, authorship_tag \
                     FROM goal_runs \
                     WHERE deleted_at IS NULL AND id IN ({placeholders}) \
                     ORDER BY updated_at DESC"
                );
                let mut stmt = conn.prepare(&goal_sql)?;
                let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
                    Ok(GoalRun {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        goal: row.get(2)?,
                        client_request_id: row.get(3)?,
                        status: parse_goal_run_status(&row.get::<_, String>(4)?),
                        priority: parse_task_priority(&row.get::<_, String>(5)?),
                        created_at: row.get::<_, i64>(6)? as u64,
                        updated_at: row.get::<_, i64>(7)? as u64,
                        started_at: row.get::<_, Option<i64>>(8)?.map(|v| v as u64),
                        completed_at: row.get::<_, Option<i64>>(9)?.map(|v| v as u64),
                        thread_id: row.get(10)?,
                        session_id: row.get(11)?,
                        root_thread_id: row.get(12)?,
                        active_thread_id: row.get(13)?,
                        execution_thread_ids: Vec::new(),
                        current_step_index: row.get::<_, i64>(14)?.max(0) as usize,
                        current_step_title: None,
                        current_step_kind: None,
                        launch_assignment_snapshot: Vec::new(),
                        runtime_assignment_list: Vec::new(),
                        planner_owner_profile: None,
                        current_step_owner_profile: None,
                        step_owner_overrides: std::collections::BTreeMap::new(),
                        replan_count: row.get::<_, i64>(15)?.max(0) as u32,
                        max_replans: row.get::<_, i64>(16)?.max(0) as u32,
                        plan_summary: row.get(17)?,
                        reflection_summary: row.get(18)?,
                        memory_updates: Vec::new(),
                        generated_skill_path: row.get(19)?,
                        last_error: row.get(20)?,
                        failure_cause: row.get(21)?,
                        stopped_reason: row.get(22)?,
                        child_task_ids: Vec::new(),
                        child_task_count: row.get::<_, i64>(23)?.max(0) as u32,
                        approval_count: row.get::<_, i64>(24)?.max(0) as u32,
                        awaiting_approval_id: row.get(25)?,
                        policy_fingerprint: row.get(26)?,
                        approval_expires_at: row.get::<_, Option<i64>>(27)?.map(|v| v as u64),
                        containment_scope: row.get(28)?,
                        compensation_status: row.get(29)?,
                        compensation_summary: row.get(30)?,
                        active_task_id: row.get(31)?,
                        duration_ms: row.get::<_, Option<i64>>(32)?.map(|v| v as u64),
                        steps: Vec::new(),
                        events: Vec::new(),
                        dossier: None,
                        total_prompt_tokens: row.get::<_, i64>(33)?.max(0) as u64,
                        total_completion_tokens: row.get::<_, i64>(34)?.max(0) as u64,
                        estimated_cost_usd: row.get(35)?,
                        model_usage: Vec::new(),
                        autonomy_level: parse_autonomy_level(
                            &row.get::<_, Option<String>>(36)?.unwrap_or_default(),
                        ),
                        authorship_tag: row
                            .get::<_, Option<String>>(37)?
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

                Ok((goal_runs, total.max(0) as usize))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn list_goal_run_ids_page(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<String>, usize)> {
        let limit = limit.max(1) as i64;
        let offset = offset as i64;
        self.read_conn
            .call(move |conn| {
                let total = conn.query_row(
                    "SELECT COUNT(*) FROM goal_runs WHERE deleted_at IS NULL",
                    [],
                    |row| row.get::<_, i64>(0),
                )?;
                let mut stmt = conn.prepare(
                    "SELECT id FROM goal_runs \
                     WHERE deleted_at IS NULL \
                     ORDER BY updated_at DESC \
                     LIMIT ?1 OFFSET ?2",
                )?;
                let rows = stmt.query_map(params![limit, offset], |row| row.get::<_, String>(0))?;
                let mut ids = Vec::new();
                for row in rows {
                    ids.push(row?);
                }
                Ok((ids, total.max(0) as usize))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn count_goal_runs(&self) -> Result<usize> {
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM goal_runs WHERE deleted_at IS NULL",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map(|count| count.max(0) as usize)
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_goal_run(&self, goal_run_id: &str) -> Result<()> {
        self.caches.latest_goal_run_for_thread.clear();
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
