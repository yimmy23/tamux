use super::*;

pub(super) const QUIET_GOAL_IDLE_MS: u64 = 5 * 60 * 1000;
const QUIET_GOAL_RECOVERY_COOLDOWN_MS: u64 = QUIET_GOAL_IDLE_MS;

#[derive(Debug, Clone, Copy)]
pub(super) struct QuietGoalRecoveryState {
    pub(super) last_triggered_at: u64,
    pub(super) observed_activity_at: u64,
}

#[derive(Debug, Clone)]
struct QuietGoalRecoveryCandidate {
    goal_run_id: String,
    thread_id: String,
    task_id: String,
    current_step_index: usize,
    current_step_id: Option<String>,
    current_step_title: Option<String>,
    active_task_status: TaskStatus,
    active_task_progress: u8,
    todo_items: Vec<TodoItem>,
    last_message_excerpt: String,
    last_activity_at: u64,
}

impl AgentEngine {
    pub(super) async fn supervise_quiet_goal_runs(&self) -> Result<()> {
        let now = now_millis();
        let goal_runs = {
            let goal_runs = self.goal_runs.lock().await;
            goal_runs.iter().cloned().collect::<Vec<_>>()
        };
        let tasks = {
            let tasks = self.tasks.lock().await;
            tasks.iter().cloned().collect::<Vec<_>>()
        };
        let threads = self.threads.read().await.clone();
        let streams = self.stream_cancellations.lock().await.clone();
        let runtimes = self.subagent_runtime.read().await.clone();
        let todos = self.thread_todos.read().await.clone();
        let inflight = self.inflight_goal_runs.lock().await.clone();

        let mut states = self.quiet_goal_recovery.lock().await;
        let mut retained_goal_ids = std::collections::HashSet::new();

        for goal_run in goal_runs {
            retained_goal_ids.insert(goal_run.id.clone());
            let Some(candidate) = build_quiet_goal_recovery_candidate(
                &goal_run, &tasks, &threads, &streams, &runtimes, &todos, &inflight,
            ) else {
                states.remove(&goal_run.id);
                continue;
            };

            if now.saturating_sub(candidate.last_activity_at) < QUIET_GOAL_IDLE_MS {
                states.remove(&goal_run.id);
                continue;
            }

            let should_trigger = match states.get(&goal_run.id) {
                Some(state)
                    if state.observed_activity_at == candidate.last_activity_at
                        && now.saturating_sub(state.last_triggered_at)
                            < QUIET_GOAL_RECOVERY_COOLDOWN_MS =>
                {
                    false
                }
                _ => true,
            };
            if !should_trigger {
                continue;
            }

            drop(states);
            self.perform_quiet_goal_recovery(&candidate).await?;
            states = self.quiet_goal_recovery.lock().await;
            states.insert(
                goal_run.id.clone(),
                QuietGoalRecoveryState {
                    last_triggered_at: now,
                    observed_activity_at: candidate.last_activity_at,
                },
            );
        }

        states.retain(|goal_run_id, _| retained_goal_ids.contains(goal_run_id));
        Ok(())
    }

    async fn perform_quiet_goal_recovery(
        &self,
        candidate: &QuietGoalRecoveryCandidate,
    ) -> Result<()> {
        let prior_user_message = {
            let threads = self.threads.read().await;
            let Some(thread) = threads.get(&candidate.thread_id) else {
                anyhow::bail!(
                    "root goal thread {} disappeared before quiet-goal recovery",
                    candidate.thread_id
                );
            };
            thread
                .messages
                .iter()
                .rev()
                .find(|message| message.role == MessageRole::User)
                .map(|message| message.content.clone())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "thread {} has no prior user message for quiet-goal recovery",
                        candidate.thread_id
                    )
                })?
        };

        let recovery_message = quiet_goal_recovery_system_message(candidate);
        {
            let mut threads = self.threads.write().await;
            let Some(thread) = threads.get_mut(&candidate.thread_id) else {
                anyhow::bail!(
                    "root goal thread {} disappeared before quiet-goal recovery message append",
                    candidate.thread_id
                );
            };
            let mut msg = AgentMessage::user(recovery_message, now_millis());
            msg.role = MessageRole::System;
            thread.messages.push(msg);
            thread.updated_at = now_millis();
        }

        self.persist_thread_by_id(&candidate.thread_id).await;
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: candidate.thread_id.clone(),
        });
        self.emit_workflow_notice(
            &candidate.thread_id,
            "quiet-goal-recovery",
            "Goal execution was idle for over 5 minutes; forcing the main thread to continue.",
            Some(
                serde_json::json!({
                    "goal_run_id": candidate.goal_run_id,
                    "task_id": candidate.task_id,
                    "last_activity_at": candidate.last_activity_at,
                })
                .to_string(),
            ),
        );

        self.resend_existing_user_message_for_task(
            &candidate.thread_id,
            &prior_user_message,
            &candidate.task_id,
        )
        .await?;
        Ok(())
    }
}

fn build_quiet_goal_recovery_candidate(
    goal_run: &GoalRun,
    tasks: &[AgentTask],
    threads: &HashMap<String, AgentThread>,
    streams: &HashMap<String, StreamCancellationEntry>,
    runtimes: &HashMap<String, SubagentRuntimeStats>,
    todos: &HashMap<String, Vec<TodoItem>>,
    inflight: &HashSet<String>,
) -> Option<QuietGoalRecoveryCandidate> {
    if goal_run.status != GoalRunStatus::Running || inflight.contains(&goal_run.id) {
        return None;
    }

    let thread_id = goal_run
        .root_thread_id
        .as_deref()
        .or(goal_run.thread_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let task_id = goal_run.active_task_id.as_deref()?.to_string();
    let active_task = tasks.iter().find(|task| task.id == task_id)?;
    if active_task.source != "goal_run" || active_task.parent_task_id.is_some() {
        return None;
    }
    if !matches!(
        active_task.status,
        TaskStatus::InProgress | TaskStatus::Blocked
    ) {
        return None;
    }

    let mut activity_at = active_task
        .started_at
        .unwrap_or(active_task.created_at)
        .max(goal_run.started_at.unwrap_or(goal_run.created_at));

    let mut execution_thread_ids = goal_run.execution_thread_ids.clone();
    if !execution_thread_ids.iter().any(|id| id == &thread_id) {
        execution_thread_ids.push(thread_id.clone());
    }

    for execution_thread_id in execution_thread_ids {
        activity_at = activity_at
            .max(thread_activity_at(&execution_thread_id, threads, streams).unwrap_or(0));
    }

    for child_task in tasks.iter().filter(|task| {
        task.goal_run_id.as_deref() == Some(goal_run.id.as_str())
            && task.id != active_task.id
            && !matches!(
                task.status,
                TaskStatus::Completed
                    | TaskStatus::Failed
                    | TaskStatus::Cancelled
                    | TaskStatus::BudgetExceeded
                    | TaskStatus::FailedAnalyzing
                    | TaskStatus::AwaitingApproval
            )
    }) {
        activity_at = activity_at.max(child_task.started_at.unwrap_or(child_task.created_at));
        if let Some(child_thread_id) = child_task.thread_id.as_deref() {
            activity_at =
                activity_at.max(thread_activity_at(child_thread_id, threads, streams).unwrap_or(0));
        }
        if let Some(runtime) = runtimes.get(&child_task.id) {
            activity_at = activity_at.max(
                runtime
                    .last_progress_at
                    .or(runtime.last_tool_call_at)
                    .unwrap_or(runtime.updated_at),
            );
        }
    }

    let current_step = goal_run.steps.get(goal_run.current_step_index);
    let todo_items = todos.get(&thread_id).cloned().unwrap_or_default();
    let last_message_excerpt = threads
        .get(&thread_id)
        .and_then(|thread| {
            thread
                .messages
                .iter()
                .rev()
                .find(|message| !message.content.trim().is_empty())
        })
        .map(|message| summarize_goal_recovery_message(&message.content))
        .unwrap_or_default();

    Some(QuietGoalRecoveryCandidate {
        goal_run_id: goal_run.id.clone(),
        thread_id,
        task_id,
        current_step_index: goal_run.current_step_index,
        current_step_id: current_step.map(|step| step.id.clone()),
        current_step_title: current_step.map(|step| step.title.clone()),
        active_task_status: active_task.status,
        active_task_progress: active_task.progress,
        todo_items,
        last_message_excerpt,
        last_activity_at: activity_at,
    })
}

fn thread_activity_at(
    thread_id: &str,
    threads: &HashMap<String, AgentThread>,
    streams: &HashMap<String, StreamCancellationEntry>,
) -> Option<u64> {
    let thread_activity = threads.get(thread_id).map(|thread| thread.updated_at);
    let stream_activity = streams.get(thread_id).map(|entry| entry.last_progress_at);
    thread_activity.into_iter().chain(stream_activity).max()
}

fn summarize_goal_recovery_message(content: &str) -> String {
    content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(180)
        .collect()
}

fn quiet_goal_recovery_system_message(candidate: &QuietGoalRecoveryCandidate) -> String {
    let step_title = candidate
        .current_step_title
        .as_deref()
        .unwrap_or("unnamed step");
    let step_id = candidate
        .current_step_id
        .as_deref()
        .map(|value| format!(" ({value})"))
        .unwrap_or_default();
    let todo_block = if candidate.todo_items.is_empty() {
        "- No authoritative goal todos are currently recorded.".to_string()
    } else {
        candidate
            .todo_items
            .iter()
            .map(|item| {
                format!(
                    "- [{}] {}",
                    todo_status_label(item.status),
                    item.content.trim()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let last_message = if candidate.last_message_excerpt.trim().is_empty() {
        "none".to_string()
    } else {
        candidate.last_message_excerpt.clone()
    };

    format!(
        "WELES quiet-goal recovery: Goal run `{}` is still running, but the main thread and all related execution threads have been idle for over 5 minutes.\n\
         Current step {}: {}{}\n\
         Active main task `{}` status={:?} progress={}.\n\
         Authoritative goal todos:\n{}\n\
         Last main-thread message: {}\n\
         Resume this goal on the main thread now. Updating goal todos does not finish the goal step; continue until the current step is actually complete or blocked by operator approval.",
        candidate.goal_run_id,
        candidate.current_step_index.saturating_add(1),
        step_title,
        step_id,
        candidate.task_id,
        candidate.active_task_status,
        candidate.active_task_progress,
        todo_block,
        last_message,
    )
}

fn todo_status_label(status: TodoStatus) -> &'static str {
    match status {
        TodoStatus::Pending => "pending",
        TodoStatus::InProgress => "in_progress",
        TodoStatus::Completed => "completed",
        TodoStatus::Blocked => "blocked",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tests::spawn_goal_recording_server;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex as StdMutex};
    use tempfile::tempdir;

    fn assistant_message(content: &str, now: u64) -> AgentMessage {
        AgentMessage {
            id: generate_message_id(),
            role: MessageRole::Assistant,
            content: content.to_string(),
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_arguments: None,
            tool_status: None,
            weles_review: None,
            input_tokens: 0,
            output_tokens: 0,
            cost: None,
            provider: None,
            model: None,
            api_transport: None,
            response_id: None,
            upstream_message: None,
            provider_final_result: None,
            author_agent_id: None,
            author_agent_name: None,
            reasoning: None,
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: now,
        }
    }

    async fn build_test_engine(response_text: &str) -> Arc<AgentEngine> {
        let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.base_url =
            spawn_goal_recording_server(recorded_bodies, response_text.to_string()).await;
        config.model = "gpt-4o-mini".to_string();
        config.api_key = "test-key".to_string();
        config.api_transport = ApiTransport::ChatCompletions;
        config.auto_retry = false;
        config.max_retries = 0;
        config.max_tool_loops = 1;
        AgentEngine::new_test(manager, config, root.path()).await
    }

    fn sample_running_goal(now: u64, thread_id: &str, task_id: &str) -> GoalRun {
        GoalRun {
            id: "goal-quiet".to_string(),
            title: "Quiet goal".to_string(),
            goal: "Continue the goal even after thread-local work looks done".to_string(),
            client_request_id: None,
            status: GoalRunStatus::Running,
            priority: TaskPriority::Normal,
            created_at: now.saturating_sub(900_000),
            updated_at: now.saturating_sub(10_000),
            started_at: Some(now.saturating_sub(900_000)),
            completed_at: None,
            thread_id: Some(thread_id.to_string()),
            session_id: None,
            current_step_index: 0,
            current_step_title: Some("Implement plugin scaffold".to_string()),
            current_step_kind: Some(GoalRunStepKind::Command),
            planner_owner_profile: None,
            current_step_owner_profile: None,
            replan_count: 0,
            max_replans: 0,
            plan_summary: None,
            reflection_summary: None,
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
            active_task_id: Some(task_id.to_string()),
            duration_ms: None,
            steps: vec![GoalRunStep {
                id: "goal-step-1".to_string(),
                position: 0,
                title: "Implement plugin scaffold".to_string(),
                instructions: "Keep the goal moving.".to_string(),
                kind: GoalRunStepKind::Command,
                success_criteria: "Plugin scaffold exists".to_string(),
                session_id: None,
                status: GoalRunStepStatus::InProgress,
                task_id: Some(task_id.to_string()),
                summary: None,
                error: None,
                started_at: Some(now.saturating_sub(900_000)),
                completed_at: None,
            }],
            events: Vec::new(),
            dossier: None,
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            estimated_cost_usd: None,
            autonomy_level: Default::default(),
            authorship_tag: None,
            launch_assignment_snapshot: Vec::new(),
            runtime_assignment_list: Vec::new(),
            root_thread_id: Some(thread_id.to_string()),
            active_thread_id: Some(thread_id.to_string()),
            execution_thread_ids: vec![thread_id.to_string()],
        }
    }

    fn sample_main_goal_task(now: u64, thread_id: &str, task_id: &str) -> AgentTask {
        AgentTask {
            id: task_id.to_string(),
            title: "Implement plugin scaffold".to_string(),
            description: "Main goal executor".to_string(),
            status: TaskStatus::InProgress,
            priority: TaskPriority::Normal,
            progress: 72,
            created_at: now.saturating_sub(900_000),
            started_at: Some(now.saturating_sub(900_000)),
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some(thread_id.to_string()),
            source: "goal_run".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: Some("goal-quiet".to_string()),
            goal_run_title: Some("Quiet goal".to_string()),
            goal_step_id: Some("goal-step-1".to_string()),
            goal_step_title: Some("Implement plugin scaffold".to_string()),
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: None,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            override_provider: None,
            override_model: None,
            override_system_prompt: None,
            sub_agent_def_id: None,
        }
    }

    #[tokio::test]
    async fn supervise_quiet_goal_runs_resumes_root_thread_after_full_idle_window() {
        let engine = build_test_engine("Recovered.").await;
        let now = now_millis();
        let thread_id = "thread-goal-quiet";
        let task_id = "task-goal-quiet";

        engine
            .goal_runs
            .lock()
            .await
            .push_back(sample_running_goal(now, thread_id, task_id));
        engine
            .tasks
            .lock()
            .await
            .push_back(sample_main_goal_task(now, thread_id, task_id));
        engine.thread_todos.write().await.insert(
            thread_id.to_string(),
            vec![
                TodoItem {
                    id: "todo-1".to_string(),
                    content: "Write short implementation spec for autosearch plugin scaffold"
                        .to_string(),
                    status: TodoStatus::InProgress,
                    position: 0,
                    step_index: Some(0),
                    created_at: now.saturating_sub(600_000),
                    updated_at: now.saturating_sub(600_000),
                },
                TodoItem {
                    id: "todo-2".to_string(),
                    content: "Capture implementation artifact and validation notes".to_string(),
                    status: TodoStatus::Pending,
                    position: 1,
                    step_index: Some(0),
                    created_at: now.saturating_sub(600_000),
                    updated_at: now.saturating_sub(600_000),
                },
            ],
        );

        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                AgentThread {
                    id: thread_id.to_string(),
                    agent_name: None,
                    title: "Quiet goal thread".to_string(),
                    messages: vec![
                        AgentMessage::user(
                            "Continue the goal until it is actually complete",
                            now.saturating_sub(900_000),
                        ),
                        assistant_message(
                            "I finished my current thread job after updating the goal todos.",
                            now.saturating_sub(600_000),
                        ),
                    ],
                    pinned: false,
                    upstream_thread_id: None,
                    upstream_transport: None,
                    upstream_provider: None,
                    upstream_model: None,
                    upstream_assistant_id: None,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    created_at: now.saturating_sub(900_000),
                    updated_at: now.saturating_sub(600_000),
                },
            );
        }

        engine
            .supervise_quiet_goal_runs()
            .await
            .expect("quiet goal recovery should succeed");

        let threads = engine.threads.read().await;
        let thread = threads.get(thread_id).expect("goal thread should exist");
        assert!(thread.messages.iter().any(|message| {
            message.role == MessageRole::System
                && message.content.contains("WELES quiet-goal recovery")
                && message
                    .content
                    .contains("Updating goal todos does not finish the goal step")
                && message
                    .content
                    .contains("Write short implementation spec for autosearch plugin scaffold")
        }));
        assert!(thread.messages.iter().any(|message| {
            message.role == MessageRole::Assistant && message.content.contains("Recovered.")
        }));
    }

    #[tokio::test]
    async fn supervise_quiet_goal_runs_skips_recent_child_progress() {
        let engine = build_test_engine("Recovered.").await;
        let now = now_millis();
        let thread_id = "thread-goal-quiet-skip";
        let child_task_id = "task-goal-quiet-child";
        let task_id = "task-goal-quiet-root";

        let mut goal_run = sample_running_goal(now, thread_id, task_id);
        goal_run
            .execution_thread_ids
            .push("thread-goal-child".to_string());
        engine.goal_runs.lock().await.push_back(goal_run);
        engine
            .tasks
            .lock()
            .await
            .push_back(sample_main_goal_task(now, thread_id, task_id));
        engine.tasks.lock().await.push_back(AgentTask {
            id: child_task_id.to_string(),
            title: "Child task".to_string(),
            description: "Recent child progress".to_string(),
            status: TaskStatus::InProgress,
            priority: TaskPriority::Normal,
            progress: 40,
            created_at: now.saturating_sub(200_000),
            started_at: Some(now.saturating_sub(200_000)),
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some("thread-goal-child".to_string()),
            source: "subagent".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: Some("goal-quiet".to_string()),
            goal_run_title: Some("Quiet goal".to_string()),
            goal_step_id: Some("goal-step-1".to_string()),
            goal_step_title: Some("Implement plugin scaffold".to_string()),
            parent_task_id: Some(task_id.to_string()),
            parent_thread_id: Some(thread_id.to_string()),
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: None,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            override_provider: None,
            override_model: None,
            override_system_prompt: None,
            sub_agent_def_id: None,
        });
        engine.subagent_runtime.write().await.insert(
            child_task_id.to_string(),
            SubagentRuntimeStats {
                task_id: child_task_id.to_string(),
                parent_task_id: Some(task_id.to_string()),
                thread_id: Some("thread-goal-child".to_string()),
                started_at: now.saturating_sub(200_000),
                created_at: now.saturating_sub(200_000),
                max_duration_secs: None,
                context_budget_tokens: None,
                last_tool_call_at: Some(now.saturating_sub(60_000)),
                last_progress_at: Some(now.saturating_sub(60_000)),
                tool_calls_total: 2,
                tool_calls_succeeded: 2,
                tool_calls_failed: 0,
                consecutive_errors: 0,
                recent_tool_names: VecDeque::new(),
                tokens_consumed: 0,
                context_utilization_pct: 0,
                health_state: SubagentHealthState::Healthy,
                updated_at: now.saturating_sub(60_000),
            },
        );

        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                AgentThread {
                    id: thread_id.to_string(),
                    agent_name: None,
                    title: "Quiet goal thread".to_string(),
                    messages: vec![
                        AgentMessage::user(
                            "Continue the goal until it is actually complete",
                            now.saturating_sub(900_000),
                        ),
                        assistant_message(
                            "I finished my current thread job after updating the goal todos.",
                            now.saturating_sub(600_000),
                        ),
                    ],
                    pinned: false,
                    upstream_thread_id: None,
                    upstream_transport: None,
                    upstream_provider: None,
                    upstream_model: None,
                    upstream_assistant_id: None,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    created_at: now.saturating_sub(900_000),
                    updated_at: now.saturating_sub(600_000),
                },
            );
            threads.insert(
                "thread-goal-child".to_string(),
                AgentThread {
                    id: "thread-goal-child".to_string(),
                    agent_name: Some("hermes".to_string()),
                    title: "Child goal thread".to_string(),
                    messages: vec![assistant_message(
                        "Child progress is still happening.",
                        now.saturating_sub(60_000),
                    )],
                    pinned: false,
                    upstream_thread_id: None,
                    upstream_transport: None,
                    upstream_provider: None,
                    upstream_model: None,
                    upstream_assistant_id: None,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    created_at: now.saturating_sub(200_000),
                    updated_at: now.saturating_sub(60_000),
                },
            );
        }

        engine
            .supervise_quiet_goal_runs()
            .await
            .expect("recent child progress should be handled without forcing recovery");

        let threads = engine.threads.read().await;
        let thread = threads.get(thread_id).expect("goal thread should exist");
        assert!(
            !thread.messages.iter().any(|message| {
                message.role == MessageRole::System
                    && message.content.contains("WELES quiet-goal recovery")
            }),
            "recent child progress should suppress main-thread quiet-goal recovery"
        );
        assert!(
            !thread
                .messages
                .iter()
                .any(|message| message.role == MessageRole::Assistant
                    && message.content.contains("Recovered.")),
            "no forced continuation should run while child progress is fresh"
        );
    }
}
