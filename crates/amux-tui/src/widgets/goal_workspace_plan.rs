use crate::state::goal_workspace::GoalWorkspaceState;
use crate::state::task::{TaskState, TodoStatus};
use ratatui::text::{Line, Span};

use super::GoalWorkspaceHitTarget;

pub(crate) struct GoalWorkspacePlanRow {
    pub(crate) line: Line<'static>,
    pub(crate) target: Option<GoalWorkspaceHitTarget>,
}

pub(crate) fn build_rows(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> Vec<GoalWorkspacePlanRow> {
    let mut rows = Vec::new();
    let run = tasks.goal_run_by_id(goal_run_id);
    for step in tasks.goal_steps_in_display_order(goal_run_id) {
        let expanded = state.is_step_expanded(&step.id);
        let active = run.is_some_and(|run| {
            run.current_step_index == step.order as usize
                || run.current_step_title.as_deref() == Some(step.title.as_str())
        });
        rows.push(GoalWorkspacePlanRow {
            line: Line::from(vec![
                Span::raw(if expanded { "▾ " } else { "▸ " }),
                Span::raw(if active { "◌ " } else { "○ " }),
                Span::raw(format!("{}. {}", step.order + 1, step.title)),
            ]),
            target: Some(GoalWorkspaceHitTarget::PlanStep(step.id.clone())),
        });

        if expanded {
            if !step.instructions.is_empty() {
                for line in wrap_plain_text(&step.instructions, 52) {
                    rows.push(GoalWorkspacePlanRow {
                        line: Line::from(vec![Span::raw("    "), Span::raw(line)]),
                        target: Some(GoalWorkspaceHitTarget::PlanStep(step.id.clone())),
                    });
                }
            }
            if let Some(summary) = step.summary.as_deref() {
                for line in wrap_plain_text(summary, 52) {
                    rows.push(GoalWorkspacePlanRow {
                        line: Line::from(vec![Span::raw("    "), Span::raw(line)]),
                        target: Some(GoalWorkspaceHitTarget::PlanStep(step.id.clone())),
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
                    target: Some(GoalWorkspaceHitTarget::PlanTodo {
                        step_id: step.id.clone(),
                        todo_id: todo.id,
                    }),
                });
            }
        }
    }

    if rows.is_empty() {
        rows.push(GoalWorkspacePlanRow {
            line: Line::from("No plan yet"),
            target: None,
        });
    }

    rows
}

fn todo_status_chip(status: Option<TodoStatus>) -> &'static str {
    match status {
        Some(TodoStatus::InProgress) => "[~]",
        Some(TodoStatus::Completed) => "[x]",
        Some(TodoStatus::Blocked) => "[!]",
        _ => "[ ]",
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
