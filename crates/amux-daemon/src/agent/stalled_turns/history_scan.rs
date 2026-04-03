use super::analysis::classify_stalled_turn;
use super::types::{ThreadStallObservation, TurnEvidence};
use super::*;

impl AgentEngine {
    pub(super) async fn collect_stalled_turn_observations(&self) -> Vec<ThreadStallObservation> {
        let active_streams = {
            let streams = self.stream_cancellations.lock().await;
            streams.keys().cloned().collect::<HashSet<_>>()
        };
        let threads = self.threads.read().await;
        let goal_runs = self.goal_runs.lock().await;
        let tasks = self.tasks.lock().await;

        threads
            .values()
            .filter(|thread| !active_streams.contains(&thread.id))
            .filter_map(|thread| latest_stalled_turn_observation(thread, &tasks, &goal_runs))
            .collect()
    }
}

fn latest_stalled_turn_observation(
    thread: &AgentThread,
    tasks: &VecDeque<AgentTask>,
    goal_runs: &VecDeque<GoalRun>,
) -> Option<ThreadStallObservation> {
    let last_message = thread.messages.last()?;
    if last_message.role != MessageRole::Assistant
        || last_message.tool_calls.is_some()
        || last_message.content.trim().is_empty()
    {
        return None;
    }

    let preceded_by_tool_result = thread
        .messages
        .iter()
        .rev()
        .skip(1)
        .find(|message| !matches!(message.role, MessageRole::System))
        .map(|message| message.role == MessageRole::Tool)
        .unwrap_or(false);

    let evidence = TurnEvidence {
        last_assistant_message: last_message.content.clone(),
        preceded_by_tool_result,
        new_tool_call_followed: false,
        new_substantive_assistant_message_followed: false,
        task_or_goal_progressed: goal_progressed_after(
            thread.id.as_str(),
            last_message.timestamp,
            tasks,
            goal_runs,
        ),
        user_replied: false,
    };
    let class = classify_stalled_turn(&evidence)?;

    Some(ThreadStallObservation {
        thread_id: thread.id.clone(),
        last_message_id: last_message.id.clone(),
        last_message_at: last_message.timestamp,
        last_assistant_message: last_message.content.clone(),
        class,
        task_id: active_task_id_for_thread(thread.id.as_str(), tasks),
        goal_run_id: goal_run_id_for_thread(thread.id.as_str(), goal_runs),
    })
}

fn active_task_id_for_thread(thread_id: &str, tasks: &VecDeque<AgentTask>) -> Option<String> {
    tasks
        .iter()
        .rev()
        .find(|task| {
            task.thread_id.as_deref() == Some(thread_id)
                && matches!(
                    task.status,
                    TaskStatus::InProgress | TaskStatus::Blocked | TaskStatus::AwaitingApproval
                )
        })
        .map(|task| task.id.clone())
}

fn goal_run_id_for_thread(thread_id: &str, goal_runs: &VecDeque<GoalRun>) -> Option<String> {
    goal_runs
        .iter()
        .find(|goal_run| goal_run.thread_id.as_deref() == Some(thread_id))
        .map(|goal_run| goal_run.id.clone())
}

fn goal_progressed_after(
    thread_id: &str,
    message_timestamp: u64,
    tasks: &VecDeque<AgentTask>,
    goal_runs: &VecDeque<GoalRun>,
) -> bool {
    tasks.iter().any(|task| {
        task.thread_id.as_deref() == Some(thread_id)
            && task_progressed_after(task, message_timestamp)
    }) || goal_runs.iter().any(|goal_run| {
        goal_run.thread_id.as_deref() == Some(thread_id) && goal_run.updated_at > message_timestamp
    })
}

fn task_progressed_after(task: &AgentTask, message_timestamp: u64) -> bool {
    let execution_started = matches!(task.status, TaskStatus::InProgress)
        && task.started_at.is_some_and(|at| at > message_timestamp);
    let lifecycle_advanced = task.completed_at.is_some_and(|at| at > message_timestamp)
        || task.scheduled_at.is_some_and(|at| at > message_timestamp)
        || task.next_retry_at.is_some_and(|at| at > message_timestamp);
    let logged_progress = task
        .logs
        .iter()
        .any(|entry| entry.timestamp > message_timestamp);

    execution_started || lifecycle_advanced || logged_progress
}
