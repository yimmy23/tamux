use super::*;

fn checkpoint_type_label(raw: &str) -> &'static str {
    match raw {
        "pre_step" => "pre-step",
        "post_step" => "post-step",
        "pre_recovery" => "recovery",
        "periodic" => "periodic",
        "manual" => "manual",
        _ => "checkpoint",
    }
}

fn short_checkpoint_id(id: &str) -> String {
    if id.chars().count() <= 18 {
        return id.to_string();
    }
    let tail: String = id
        .chars()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{tail}")
}

pub(super) fn render_checkpoints(
    rows: &mut Vec<RenderRow>,
    tasks: &TaskState,
    run: &GoalRun,
    theme: &ThemeTokens,
    width: usize,
) {
    push_section_title(
        rows,
        "Checkpoints",
        theme.accent_primary.add_modifier(Modifier::BOLD),
    );
    let checkpoints = tasks.checkpoints_for_goal_run(&run.id);
    if checkpoints.is_empty() {
        rows.push(RenderRow {
            line: Line::from(Span::styled("No checkpoints recorded yet.", theme.fg_dim)),
            work_path: None,
            close_preview: false,
        });
        return;
    }

    for checkpoint in checkpoints.iter().take(6) {
        let step_label = checkpoint
            .step_index
            .map(|step_index| format!("step {}", step_index + 1))
            .unwrap_or_else(|| "step ?".to_string());
        rows.push(RenderRow {
            line: Line::from(vec![
                Span::styled("[", theme.fg_dim),
                Span::styled(
                    checkpoint_type_label(&checkpoint.checkpoint_type),
                    theme.fg_active,
                ),
                Span::styled("]", theme.fg_dim),
                Span::raw(" "),
                Span::styled(step_label, theme.fg_dim),
                Span::raw("  "),
                Span::styled(format!("{} task(s)", checkpoint.task_count), theme.fg_dim),
                Span::raw("  "),
                Span::styled(short_checkpoint_id(&checkpoint.id), theme.accent_primary),
            ]),
            work_path: None,
            close_preview: false,
        });
        if let Some(preview) = checkpoint.context_summary_preview.as_deref() {
            push_wrapped_text(rows, preview, theme.fg_dim, width, 2);
        }
    }
}

pub(super) fn render_steps(
    rows: &mut Vec<RenderRow>,
    tasks: &TaskState,
    run: &GoalRun,
    selected_step_id: Option<&str>,
    theme: &ThemeTokens,
    width: usize,
) {
    push_section_title(
        rows,
        "Execution Plan",
        theme.accent_primary.add_modifier(Modifier::BOLD),
    );

    let mut steps = run.steps.clone();
    steps.sort_by_key(|step| step.order);

    if steps.is_empty() {
        rows.push(RenderRow {
            line: Line::from(Span::styled("No steps", theme.fg_dim)),
            work_path: None,
            close_preview: false,
        });
        return;
    }

    for step in &steps {
        let chip = match step.status {
            None
            | Some(GoalRunStatus::Queued)
            | Some(GoalRunStatus::Planning)
            | Some(GoalRunStatus::AwaitingApproval) => "[ ]",
            Some(GoalRunStatus::Running) => "[~]",
            Some(GoalRunStatus::Paused) => "[P]",
            Some(GoalRunStatus::Completed) => "[x]",
            Some(GoalRunStatus::Failed) => "[!]",
            Some(GoalRunStatus::Cancelled) => "[-]",
        };
        let mut line = Line::from(vec![
            Span::styled(chip, theme.fg_dim),
            Span::raw(" "),
            Span::styled(step.title.clone(), theme.fg_active),
        ]);
        if selected_step_id == Some(step.id.as_str()) {
            line = line.style(Style::default().bg(Color::Indexed(236)));
        }
        rows.push(RenderRow {
            line,
            work_path: None,
            close_preview: false,
        });

        if !step.instructions.is_empty() {
            push_wrapped_text(rows, &step.instructions, theme.fg_dim, width, 2);
        }
        if let Some(summary) = &step.summary {
            push_wrapped_text(rows, summary, theme.fg_active, width, 2);
        }
        if let Some(error) = &step.error {
            push_wrapped_text(rows, error, theme.accent_danger, width, 2);
        }

        for task in related_tasks_for_step(tasks, run, step) {
            rows.push(RenderRow {
                line: Line::from(vec![
                    Span::raw("  "),
                    Span::styled("• ", theme.fg_dim),
                    Span::styled(task.title.clone(), theme.fg_active),
                    Span::raw(" "),
                    Span::styled(task_status_label(task.status), theme.fg_dim),
                ]),
                work_path: None,
                close_preview: false,
            });
        }
    }
}

pub(super) fn render_step_timeline(
    rows: &mut Vec<RenderRow>,
    run: &GoalRun,
    theme: &ThemeTokens,
    width: usize,
) {
    if run.events.is_empty() {
        return;
    }

    push_section_title(
        rows,
        "Step Timeline",
        theme.accent_primary.add_modifier(Modifier::BOLD),
    );
    for event in run.events.iter().rev().take(18).rev() {
        let mut prefix = format!("[{}] {}", event.phase, event.message);
        if let Some(step_index) = event.step_index {
            prefix = format!("step {} • {}", step_index + 1, prefix);
        }
        push_wrapped_text(rows, &prefix, theme.fg_active, width, 0);
        if let Some(details) = &event.details {
            push_wrapped_text(rows, details, theme.fg_dim, width, 2);
        }
        if !event.todo_snapshot.is_empty() {
            push_todo_items(rows, &event.todo_snapshot, theme, width, 4);
        }
    }
}

pub(super) fn render_live_todos(
    rows: &mut Vec<RenderRow>,
    tasks: &TaskState,
    thread_id: Option<&str>,
    theme: &ThemeTokens,
    width: usize,
) {
    let Some(thread_id) = thread_id else {
        return;
    };
    push_section_title(
        rows,
        "Live Todos",
        theme.accent_primary.add_modifier(Modifier::BOLD),
    );
    push_todo_items(rows, tasks.todos_for_thread(thread_id), theme, width, 0);
}

pub(super) fn render_work_context(
    rows: &mut Vec<RenderRow>,
    tasks: &TaskState,
    thread_id: Option<&str>,
    theme: &ThemeTokens,
    width: usize,
) {
    let Some(thread_id) = thread_id else {
        return;
    };
    let Some(context) = tasks.work_context_for_thread(thread_id) else {
        return;
    };

    push_section_title(
        rows,
        "Files",
        theme.accent_primary.add_modifier(Modifier::BOLD),
    );
    if context.entries.is_empty() {
        rows.push(RenderRow {
            line: Line::from(Span::styled(
                "No file or artifact activity yet.",
                theme.fg_dim,
            )),
            work_path: None,
            close_preview: false,
        });
        return;
    }

    let selected_path = tasks.selected_work_path(thread_id);
    for entry in &context.entries {
        let label = entry
            .change_kind
            .as_deref()
            .unwrap_or_else(|| work_kind_label(entry.kind));
        let marker = if selected_path == Some(entry.path.as_str()) {
            ">"
        } else {
            " "
        };
        rows.push(RenderRow {
            line: Line::from(vec![
                Span::styled(marker, theme.accent_primary),
                Span::raw(" "),
                Span::styled(format!("[{}]", label), theme.fg_dim),
                Span::raw(" "),
                Span::styled(entry.path.clone(), theme.fg_active),
            ]),
            work_path: Some(entry.path.clone()),
            close_preview: false,
        });
        if let Some(previous_path) = &entry.previous_path {
            push_wrapped_text(
                rows,
                &format!("from {}", previous_path),
                theme.fg_dim,
                width,
                4,
            );
        }
    }

    let Some(selected_path) = selected_path else {
        return;
    };
    let Some(selected_entry) = context
        .entries
        .iter()
        .find(|entry| entry.path == selected_path)
    else {
        return;
    };

    push_section_title(
        rows,
        "Preview",
        theme.accent_primary.add_modifier(Modifier::BOLD),
    );
    rows.push(RenderRow {
        line: Line::from(vec![
            Span::styled("[x]", theme.accent_danger),
            Span::raw(" "),
            Span::styled("Close preview", theme.fg_dim),
        ]),
        work_path: None,
        close_preview: true,
    });
    if let Some(repo_root) = selected_entry.repo_root.as_deref() {
        if let Some(diff) = tasks.diff_for_path(repo_root, &selected_entry.path) {
            if diff.trim().is_empty() {
                rows.push(RenderRow {
                    line: Line::from(Span::styled(
                        "No diff preview available for the selected file.",
                        theme.fg_dim,
                    )),
                    work_path: None,
                    close_preview: false,
                });
            } else {
                push_wrapped_text(rows, diff, theme.fg_dim, width, 0);
            }
            return;
        }
    }

    let preview_key = if selected_entry.repo_root.is_some() {
        selected_entry
            .repo_root
            .as_deref()
            .map(|repo_root| format!("{repo_root}/{}", selected_entry.path))
            .unwrap_or_else(|| selected_entry.path.clone())
    } else {
        selected_entry.path.clone()
    };
    if let Some(preview) = tasks.preview_for_path(&preview_key) {
        if preview.is_text {
            push_preview_text(rows, &selected_entry.path, &preview.content, theme, width);
        } else {
            rows.push(RenderRow {
                line: Line::from(Span::styled(
                    "Binary file preview is not available.",
                    theme.fg_dim,
                )),
                work_path: None,
                close_preview: false,
            });
        }
    } else {
        rows.push(RenderRow {
            line: Line::from(Span::styled("Loading preview...", theme.fg_dim)),
            work_path: None,
            close_preview: false,
        });
    }
}
