use crate::state::goal_workspace::GoalWorkspaceState;
use crate::state::task::{GoalRunStatus, TaskState, TaskStatus, TodoStatus};
use ratatui::text::{Line, Span};

use super::GoalWorkspaceHitTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GoalWorkspacePlanMarkerState {
    Pending,
    Completed,
    Running,
    Error,
}

pub(crate) struct GoalWorkspacePlanRow {
    pub(crate) line: Line<'static>,
    pub(crate) selection: Option<crate::state::goal_workspace::GoalPlanSelection>,
    pub(crate) target: Option<GoalWorkspaceHitTarget>,
    pub(crate) marker_state: Option<GoalWorkspacePlanMarkerState>,
    pub(crate) marker_span_index: Option<usize>,
}

pub(crate) fn build_rows(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> Vec<GoalWorkspacePlanRow> {
    let mut rows = Vec::new();
    let run = tasks.goal_run_by_id(goal_run_id);
    let prompt_expanded = state.prompt_expanded();
    let prompt_button = if prompt_expanded { "[Hide]" } else { "[Show]" };
    rows.push(GoalWorkspacePlanRow {
        line: Line::from(vec![
            Span::raw(if prompt_expanded { "▾ " } else { "▸ " }),
            Span::raw("Goal Prompt  "),
            Span::raw(prompt_button),
        ]),
        selection: Some(crate::state::goal_workspace::GoalPlanSelection::PromptToggle),
        target: Some(GoalWorkspaceHitTarget::PlanPromptToggle),
        marker_state: None,
        marker_span_index: None,
    });
    if prompt_expanded {
        let goal = run
            .map(|run| run.goal.trim())
            .filter(|goal| !goal.is_empty())
            .unwrap_or("No goal prompt available.");
        for line in wrap_plain_text(goal, 52) {
            rows.push(GoalWorkspacePlanRow {
                line: Line::from(vec![Span::raw("    "), Span::raw(line)]),
                selection: None,
                target: None,
                marker_state: None,
                marker_span_index: None,
            });
        }
    }

    if let Some((thread_label, thread_id)) = run.and_then(|run| main_agent_thread(tasks, run)) {
        rows.push(GoalWorkspacePlanRow {
            line: Line::from(vec![
                Span::raw("[thread] "),
                Span::raw(format!("{thread_label}  ")),
                Span::raw(thread_id.clone()),
            ]),
            selection: Some(
                crate::state::goal_workspace::GoalPlanSelection::MainThread {
                    thread_id: thread_id.clone(),
                },
            ),
            target: Some(GoalWorkspaceHitTarget::PlanMainThread(thread_id)),
            marker_state: None,
            marker_span_index: None,
        });
    } else {
        rows.push(GoalWorkspacePlanRow {
            line: Line::from("No main agent thread yet."),
            selection: None,
            target: None,
            marker_state: None,
            marker_span_index: None,
        });
    }

    for step in tasks.goal_steps_in_display_order(goal_run_id) {
        let expanded = state.is_step_expanded(&step.id);
        let active = run.is_some_and(|run| {
            run.current_step_index == step.order as usize
                || run.current_step_title.as_deref() == Some(step.title.as_str())
        });
        let marker_state = step_marker_state(&step, active);
        rows.push(GoalWorkspacePlanRow {
            line: Line::from(vec![
                Span::raw(if expanded { "▾ " } else { "▸ " }),
                Span::raw("○ "),
                Span::raw(format!("{}. {}", step.order + 1, step.title)),
            ]),
            selection: Some(crate::state::goal_workspace::GoalPlanSelection::Step {
                step_id: step.id.clone(),
            }),
            target: Some(GoalWorkspaceHitTarget::PlanStep(step.id.clone())),
            marker_state: Some(marker_state),
            marker_span_index: Some(1),
        });

        if expanded {
            if !step.instructions.is_empty() {
                for line in wrap_plain_text(&step.instructions, 52) {
                    rows.push(GoalWorkspacePlanRow {
                        line: Line::from(vec![Span::raw("    "), Span::raw(line)]),
                        selection: None,
                        target: Some(GoalWorkspaceHitTarget::PlanStep(step.id.clone())),
                        marker_state: None,
                        marker_span_index: None,
                    });
                }
            }
            if let Some(summary) = step.summary.as_deref() {
                for line in wrap_plain_text(summary, 52) {
                    rows.push(GoalWorkspacePlanRow {
                        line: Line::from(vec![Span::raw("    "), Span::raw(line)]),
                        selection: None,
                        target: Some(GoalWorkspaceHitTarget::PlanStep(step.id.clone())),
                        marker_state: None,
                        marker_span_index: None,
                    });
                }
            }
            for todo in tasks.goal_step_todos(goal_run_id, step.order as usize) {
                rows.push(GoalWorkspacePlanRow {
                    line: Line::from(vec![
                        Span::raw("    "),
                        Span::raw(todo_status_chip(todo.status)),
                        Span::raw(" "),
                        Span::raw(todo.content),
                    ]),
                    selection: Some(crate::state::goal_workspace::GoalPlanSelection::Todo {
                        step_id: step.id.clone(),
                        todo_id: todo.id.clone(),
                    }),
                    target: Some(GoalWorkspaceHitTarget::PlanTodo {
                        step_id: step.id.clone(),
                        todo_id: todo.id,
                    }),
                    marker_state: None,
                    marker_span_index: None,
                });
            }
        }
    }

    if rows.is_empty() {
        rows.push(GoalWorkspacePlanRow {
            line: Line::from("No plan yet"),
            selection: None,
            target: None,
            marker_state: None,
            marker_span_index: None,
        });
    }

    rows
}

fn main_agent_thread(
    tasks: &TaskState,
    run: &crate::state::task::GoalRun,
) -> Option<(String, String)> {
    run.thread_id
        .clone()
        .map(|thread_id| ("Main agent".to_string(), thread_id))
        .or_else(|| {
            run.root_thread_id.clone().map(|thread_id| {
                (
                    run.planner_owner_profile
                        .as_ref()
                        .map(|owner| format!("Main agent ({})", owner.agent_label))
                        .unwrap_or_else(|| "Main agent".to_string()),
                    thread_id,
                )
            })
        })
        .or_else(|| {
            run.active_thread_id.clone().map(|thread_id| {
                (
                    run.current_step_owner_profile
                        .as_ref()
                        .map(|owner| format!("Main agent ({})", owner.agent_label))
                        .unwrap_or_else(|| "Main agent".to_string()),
                    thread_id,
                )
            })
        })
        .or_else(|| {
            run.execution_thread_ids
                .first()
                .cloned()
                .map(|thread_id| ("Main agent".to_string(), thread_id))
        })
        .or_else(|| {
            let mut goal_tasks = tasks
                .tasks()
                .iter()
                .filter(|task| task.goal_run_id.as_deref() == Some(run.id.as_str()))
                .filter_map(|task| {
                    task.thread_id.as_ref().map(|thread_id| {
                        let priority = match task.status {
                            Some(TaskStatus::InProgress) => 0,
                            Some(TaskStatus::AwaitingApproval) => 1,
                            Some(TaskStatus::Queued) => 2,
                            _ => 3,
                        };
                        (
                            priority,
                            std::cmp::Reverse(task.created_at),
                            thread_id.clone(),
                        )
                    })
                })
                .collect::<Vec<_>>();
            goal_tasks.sort();
            goal_tasks
                .into_iter()
                .next()
                .map(|(_, _, thread_id)| ("Main agent".to_string(), thread_id))
        })
}

fn todo_status_chip(status: Option<TodoStatus>) -> &'static str {
    match status {
        Some(TodoStatus::InProgress) => "[~]",
        Some(TodoStatus::Completed) => "[x]",
        Some(TodoStatus::Blocked) => "[!]",
        _ => "[ ]",
    }
}

fn step_marker_state(
    step: &crate::state::task::GoalRunStep,
    active: bool,
) -> GoalWorkspacePlanMarkerState {
    if step
        .error
        .as_deref()
        .is_some_and(|error| !error.trim().is_empty())
        || matches!(step.status, Some(GoalRunStatus::Failed))
    {
        GoalWorkspacePlanMarkerState::Error
    } else if matches!(step.status, Some(GoalRunStatus::Completed)) {
        GoalWorkspacePlanMarkerState::Completed
    } else if active
        || matches!(
            step.status,
            Some(
                GoalRunStatus::Running | GoalRunStatus::Planning | GoalRunStatus::AwaitingApproval
            )
        )
    {
        GoalWorkspacePlanMarkerState::Running
    } else {
        GoalWorkspacePlanMarkerState::Pending
    }
}

fn wrap_plain_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut wrapped = Vec::new();
    for raw_line in text.lines() {
        let mut current = String::new();
        for word in raw_line.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if candidate.chars().count() > width && !current.is_empty() {
                wrapped.push(current);
                current = word.to_string();
            } else {
                current = candidate;
            }
        }
        if current.is_empty() {
            wrapped.push(String::new());
        } else {
            wrapped.push(current);
        }
    }
    if wrapped.is_empty() {
        wrapped.push(String::new());
    }
    wrapped
}
