use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use std::collections::HashSet;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::state::workspace::WorkspaceState;
use crate::theme::ThemeTokens;

const TASK_COLLAPSED_ROW_HEIGHT: u16 = 7;
const TASK_EXPANDED_ROW_HEIGHT: u16 = 11;
const TASK_PRIMARY_ACTION_ROW: u16 = 4;
const TASK_SECONDARY_ACTION_ROW: u16 = 5;
const TASK_ASSIGN_ACTION_ROW: u16 = 6;
const TASK_DELETE_ACTION_ROW: u16 = 7;
const TASK_TITLE_MAX_LINES: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceBoardToolbarAction {
    NewTask,
    Refresh,
    ToggleOperator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceBoardAction {
    ToggleActions,
    Run,
    RunBlocked,
    Pause,
    Stop,
    MoveNext,
    Review,
    Assign,
    Reviewer,
    Details,
    OpenRuntime,
    History,
    Edit,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceBoardHitTarget {
    Column {
        status: amux_protocol::WorkspaceTaskStatus,
    },
    Task {
        task_id: String,
        status: amux_protocol::WorkspaceTaskStatus,
    },
    Action {
        task_id: String,
        status: amux_protocol::WorkspaceTaskStatus,
        action: WorkspaceBoardAction,
    },
    Toolbar(WorkspaceBoardToolbarAction),
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    workspace: &WorkspaceState,
    expanded_task_ids: &HashSet<String>,
    selected: Option<&WorkspaceBoardHitTarget>,
    theme: &ThemeTokens,
    focused: bool,
) {
    let projection = workspace.projection();
    let title = format!(
        " Workspace {}{} ",
        projection.workspace_id,
        projection
            .filter_summary
            .as_ref()
            .map(|summary| format!(" · filter {summary}"))
            .unwrap_or_default()
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(if focused {
            theme.accent_primary
        } else {
            theme.fg_dim
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 30 || inner.height < 4 {
        return;
    }

    render_toolbar(frame, inner, projection.operator.clone(), selected, theme);
    let board = board_area(inner);
    let columns = workspace_column_areas(board);

    for (index, column) in projection.columns.iter().enumerate() {
        let column_block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ({}) ", column.title, column.tasks.len()))
            .border_style(theme.fg_dim);
        let column_inner = column_block.inner(columns[index]);
        frame.render_widget(column_block, columns[index]);
        render_column_tasks(
            frame,
            column_inner,
            column,
            &projection.notice_summaries,
            expanded_task_ids,
            selected,
            theme,
        );
    }
}

pub fn selectable_targets(
    workspace: &WorkspaceState,
    expanded_task_ids: &HashSet<String>,
) -> Vec<WorkspaceBoardHitTarget> {
    let mut targets = vec![
        WorkspaceBoardHitTarget::Toolbar(WorkspaceBoardToolbarAction::NewTask),
        WorkspaceBoardHitTarget::Toolbar(WorkspaceBoardToolbarAction::Refresh),
        WorkspaceBoardHitTarget::Toolbar(WorkspaceBoardToolbarAction::ToggleOperator),
    ];
    for column in &workspace.projection().columns {
        for task in &column.tasks {
            targets.push(WorkspaceBoardHitTarget::Task {
                task_id: task.id.clone(),
                status: column.status.clone(),
            });
            targets.push(WorkspaceBoardHitTarget::Action {
                task_id: task.id.clone(),
                status: column.status.clone(),
                action: WorkspaceBoardAction::OpenRuntime,
            });
            targets.push(WorkspaceBoardHitTarget::Action {
                task_id: task.id.clone(),
                status: column.status.clone(),
                action: WorkspaceBoardAction::ToggleActions,
            });
            if !expanded_task_ids.contains(&task.id) {
                continue;
            }
            let run_action = if task.assignee.is_some() {
                WorkspaceBoardAction::Run
            } else {
                WorkspaceBoardAction::RunBlocked
            };
            for action in [
                run_action,
                WorkspaceBoardAction::Pause,
                WorkspaceBoardAction::Stop,
                WorkspaceBoardAction::MoveNext,
                WorkspaceBoardAction::Review,
                WorkspaceBoardAction::Assign,
                WorkspaceBoardAction::Reviewer,
                WorkspaceBoardAction::Details,
                WorkspaceBoardAction::History,
                WorkspaceBoardAction::Edit,
                WorkspaceBoardAction::Delete,
            ] {
                targets.push(WorkspaceBoardHitTarget::Action {
                    task_id: task.id.clone(),
                    status: column.status.clone(),
                    action,
                });
            }
        }
    }
    targets
}

pub fn step_selection(
    workspace: &WorkspaceState,
    expanded_task_ids: &HashSet<String>,
    current: Option<&WorkspaceBoardHitTarget>,
    delta: i32,
) -> Option<WorkspaceBoardHitTarget> {
    let targets = selectable_targets(workspace, expanded_task_ids);
    if targets.is_empty() {
        return None;
    }
    let current_index = current
        .and_then(|selected| targets.iter().position(|target| target == selected))
        .unwrap_or(0);
    let last = targets.len().saturating_sub(1) as i32;
    let next = (current_index as i32 + delta).clamp(0, last) as usize;
    targets.get(next).cloned()
}

pub fn hit_test(
    area: Rect,
    workspace: &WorkspaceState,
    expanded_task_ids: &HashSet<String>,
    position: Position,
) -> Option<WorkspaceBoardHitTarget> {
    let inner = block_inner(area);
    if !contains(inner, position) || inner.width < 30 || inner.height < 4 {
        return None;
    }
    if position.y == inner.y {
        return toolbar_action_at_x(inner.x, position.x).map(WorkspaceBoardHitTarget::Toolbar);
    }
    let board = board_area(inner);
    let columns = workspace_column_areas(board);
    for (index, rect) in columns.iter().enumerate() {
        if !contains(*rect, position) {
            continue;
        }
        let column = workspace.projection().columns.get(index)?;
        let body = block_inner(*rect);
        if !contains(body, position) {
            return Some(WorkspaceBoardHitTarget::Column {
                status: column.status.clone(),
            });
        }
        let Some((task_index, card_rect)) =
            task_card_at_position(body, &column.tasks, expanded_task_ids, position)
        else {
            return Some(WorkspaceBoardHitTarget::Column {
                status: column.status.clone(),
            });
        };
        let Some(task) = column.tasks.get(task_index) else {
            return Some(WorkspaceBoardHitTarget::Column {
                status: column.status.clone(),
            });
        };
        let card_body = block_inner(card_rect);
        let rows = task_action_rows(task, card_body.width);
        if !contains(card_body, position) {
            return Some(WorkspaceBoardHitTarget::Task {
                task_id: task.id.clone(),
                status: column.status.clone(),
            });
        }
        let row_in_task = position.y.saturating_sub(card_body.y);
        let actions_expanded = expanded_task_ids.contains(&task.id);
        if !actions_expanded && row_in_task == rows.primary {
            if let Some(action) = collapsed_controls_action_at_x(card_body.x, position.x) {
                return Some(WorkspaceBoardHitTarget::Action {
                    task_id: task.id.clone(),
                    status: column.status.clone(),
                    action,
                });
            }
        }
        if actions_expanded && rows.action_rows().contains(&row_in_task) {
            if let Some(action) = action_at_x(
                rows.base_row(row_in_task),
                card_body.x,
                position.x,
                task.assignee.is_none(),
            ) {
                return Some(WorkspaceBoardHitTarget::Action {
                    task_id: task.id.clone(),
                    status: column.status.clone(),
                    action,
                });
            }
        }
        if actions_expanded && row_in_task == rows.footer {
            if let Some(action) = expanded_footer_action_at_x(card_body.x, position.x) {
                return Some(WorkspaceBoardHitTarget::Action {
                    task_id: task.id.clone(),
                    status: column.status.clone(),
                    action,
                });
            }
        }
        return Some(WorkspaceBoardHitTarget::Task {
            task_id: task.id.clone(),
            status: column.status.clone(),
        });
    }
    None
}

fn render_toolbar(
    frame: &mut Frame,
    area: Rect,
    operator: amux_protocol::WorkspaceOperator,
    selected: Option<&WorkspaceBoardHitTarget>,
    theme: &ThemeTokens,
) {
    let label = toolbar_label(operator.clone());
    let spans = toolbar_spans(operator, selected, theme);
    frame.render_widget(
        Paragraph::new(Line::from(spans)).wrap(Wrap { trim: false }),
        Rect::new(area.x, area.y, area.width, 1),
    );
    let _ = label;
}

fn toolbar_label(operator: amux_protocol::WorkspaceOperator) -> String {
    format!("[New task] [Refresh] [operator: {operator:?}]")
}

fn toolbar_spans(
    operator: amux_protocol::WorkspaceOperator,
    selected: Option<&WorkspaceBoardHitTarget>,
    theme: &ThemeTokens,
) -> Vec<Span<'static>> {
    [
        (
            WorkspaceBoardToolbarAction::NewTask,
            "[New task]".to_string(),
        ),
        (
            WorkspaceBoardToolbarAction::Refresh,
            "[Refresh]".to_string(),
        ),
        (
            WorkspaceBoardToolbarAction::ToggleOperator,
            format!("[operator: {operator:?}]"),
        ),
    ]
    .into_iter()
    .enumerate()
    .flat_map(|(index, (action, label))| {
        let style = if selected == Some(&WorkspaceBoardHitTarget::Toolbar(action.clone())) {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let mut spans = vec![Span::styled(label, style)];
        if index < 2 {
            spans.push(Span::raw(" "));
        }
        spans
    })
    .collect()
}

fn render_column_tasks(
    frame: &mut Frame,
    area: Rect,
    column: &crate::state::workspace::WorkspaceColumn,
    notice_summaries: &std::collections::HashMap<String, String>,
    expanded_task_ids: &HashSet<String>,
    selected: Option<&WorkspaceBoardHitTarget>,
    theme: &ThemeTokens,
) {
    if column.tasks.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled("No tasks", theme.fg_dim))),
            area,
        );
        return;
    }
    for (index, task) in column.tasks.iter().enumerate() {
        let rect = task_card_rect(area, &column.tasks, expanded_task_ids, index);
        if rect.height < 3 || rect.y >= area.y.saturating_add(area.height) {
            continue;
        }
        let task_selected = selected.is_some_and(|target| match target {
            WorkspaceBoardHitTarget::Task { task_id, .. }
            | WorkspaceBoardHitTarget::Action { task_id, .. } => task_id == &task.id,
            WorkspaceBoardHitTarget::Column { .. } | WorkspaceBoardHitTarget::Toolbar(_) => false,
        });
        let card = Block::default()
            .borders(Borders::ALL)
            .border_style(task_card_border_style(
                task,
                notice_summaries.get(&task.id),
                task_selected,
                theme,
            ));
        frame.render_widget(
            Paragraph::new(task_lines_with_actions(
                task,
                notice_summaries,
                selected,
                theme,
                Some(block_inner(rect).width),
                expanded_task_ids.contains(&task.id),
            ))
            .block(card),
            rect,
        );
    }
}

fn task_card_border_style(
    task: &amux_protocol::WorkspaceTask,
    notice_summary: Option<&String>,
    selected: bool,
    theme: &ThemeTokens,
) -> Style {
    if selected {
        return theme.accent_primary;
    }
    if task.status == amux_protocol::WorkspaceTaskStatus::Done {
        return theme.accent_success;
    }
    if notice_summary.is_some_and(|summary| task_notice_is_failed(summary)) {
        return theme.accent_danger;
    }
    theme.fg_dim
}

fn task_notice_is_failed(summary: &str) -> bool {
    summary.starts_with("runtime_failed:")
        || summary.starts_with("run_failed:")
        || summary.starts_with("review_failed:")
}

fn task_lines(
    task: &amux_protocol::WorkspaceTask,
    notice_summaries: &std::collections::HashMap<String, String>,
    selected: Option<&WorkspaceBoardHitTarget>,
    theme: &ThemeTokens,
) -> Vec<Line<'static>> {
    task_lines_with_actions(task, notice_summaries, selected, theme, None, false)
}

fn task_lines_with_actions(
    task: &amux_protocol::WorkspaceTask,
    notice_summaries: &std::collections::HashMap<String, String>,
    selected: Option<&WorkspaceBoardHitTarget>,
    theme: &ThemeTokens,
    card_body_width: Option<u16>,
    expanded: bool,
) -> Vec<Line<'static>> {
    let assignee = task
        .assignee
        .as_ref()
        .map(|actor| format!("{actor:?}"))
        .unwrap_or_else(|| "unassigned".to_string());
    let id_line = notice_summaries
        .get(&task.id)
        .map(|notice| format!("id {} · {notice}", short_task_id(&task.id)))
        .unwrap_or_else(|| format!("id {}", short_task_id(&task.id)));
    let controls_line = action_line(
        task,
        selected,
        theme,
        &[
            (WorkspaceBoardAction::OpenRuntime, "[Open]"),
            (
                WorkspaceBoardAction::ToggleActions,
                if expanded {
                    "[Hide actions]"
                } else {
                    "[Actions]"
                },
            ),
        ],
    );
    let run_line = if task.assignee.is_some() {
        action_line(
            task,
            selected,
            theme,
            &[
                (WorkspaceBoardAction::Run, "[Run]"),
                (WorkspaceBoardAction::Pause, "[Pause]"),
                (WorkspaceBoardAction::Stop, "[Stop]"),
            ],
        )
    } else {
        action_line(
            task,
            selected,
            theme,
            &[
                (WorkspaceBoardAction::RunBlocked, "[Blocked]"),
                (WorkspaceBoardAction::Pause, "[Pause]"),
                (WorkspaceBoardAction::Stop, "[Stop]"),
            ],
        )
    };
    let mut lines = title_lines(&task.title, card_body_width, theme);
    lines.extend([
        Line::from(Span::styled(
            format!("{:?} · {:?}", task.task_type, task.priority),
            theme.fg_dim,
        )),
        Line::from(Span::styled(id_line, theme.fg_dim)),
        Line::from(Span::styled(format!("→ {assignee}"), theme.fg_dim)),
    ]);
    if !expanded {
        lines.push(controls_line);
        return lines;
    }
    lines.extend([
        run_line,
        action_line(
            task,
            selected,
            theme,
            &[
                (WorkspaceBoardAction::MoveNext, "[Move]"),
                (WorkspaceBoardAction::Review, "[Review]"),
            ],
        ),
        action_line(
            task,
            selected,
            theme,
            &[
                (WorkspaceBoardAction::Assign, "[Assign]"),
                (WorkspaceBoardAction::Reviewer, "[Reviewer]"),
            ],
        ),
        action_line(
            task,
            selected,
            theme,
            &[
                (WorkspaceBoardAction::Details, "[Details]"),
                (WorkspaceBoardAction::History, "[History]"),
                (WorkspaceBoardAction::Edit, "[Edit]"),
                (WorkspaceBoardAction::Delete, "[Delete]"),
            ],
        ),
        controls_line,
    ]);
    lines
}

fn action_line(
    task: &amux_protocol::WorkspaceTask,
    selected: Option<&WorkspaceBoardHitTarget>,
    theme: &ThemeTokens,
    actions: &[(WorkspaceBoardAction, &'static str)],
) -> Line<'static> {
    let spans = actions
        .iter()
        .enumerate()
        .flat_map(|(index, (action, label))| {
            let is_selected = selected.is_some_and(|target| {
                target
                    == &WorkspaceBoardHitTarget::Action {
                        task_id: task.id.clone(),
                        status: task.status.clone(),
                        action: action.clone(),
                    }
            });
            let style = if is_selected {
                theme.accent_primary
            } else {
                theme.fg_dim
            };
            let mut spans = vec![Span::styled(*label, style)];
            if index + 1 < actions.len() {
                spans.push(Span::raw(" "));
            }
            spans
        })
        .collect::<Vec<_>>();
    Line::from(spans)
}

fn title_lines(
    title: &str,
    card_body_width: Option<u16>,
    theme: &ThemeTokens,
) -> Vec<Line<'static>> {
    wrapped_title_text(title, card_body_width)
        .into_iter()
        .map(|line| Line::from(Span::styled(line, theme.fg_active)))
        .collect()
}

fn wrapped_title_text(title: &str, card_body_width: Option<u16>) -> Vec<String> {
    let Some(width) = card_body_width.map(usize::from).filter(|width| *width > 0) else {
        return vec![title.to_string()];
    };
    wrap_text_to_width(title, width, TASK_TITLE_MAX_LINES)
}

fn title_row_count(title: &str, card_body_width: u16) -> u16 {
    wrapped_title_text(title, Some(card_body_width))
        .len()
        .max(1) as u16
}

fn wrap_text_to_width(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    if max_lines == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if lines.len() >= max_lines {
            break;
        }
        if UnicodeWidthStr::width(word) > width {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
                if lines.len() >= max_lines {
                    break;
                }
            }
            for chunk in split_word_to_width(word, width) {
                lines.push(chunk);
                if lines.len() >= max_lines {
                    break;
                }
            }
            continue;
        }
        let next_width = if current.is_empty() {
            UnicodeWidthStr::width(word)
        } else {
            UnicodeWidthStr::width(current.as_str()) + 1 + UnicodeWidthStr::width(word)
        };
        if next_width <= width {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
            continue;
        }
        lines.push(std::mem::take(&mut current));
        if lines.len() >= max_lines {
            break;
        }
        current.push_str(word);
    }
    if lines.len() < max_lines && !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn split_word_to_width(word: &str, width: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;
    for ch in word.chars() {
        let char_width = ch.width().unwrap_or(0);
        if current_width > 0 && current_width + char_width > width {
            chunks.push(std::mem::take(&mut current));
            current_width = 0;
        }
        current.push(ch);
        current_width += char_width;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[derive(Debug, Clone, Copy)]
struct TaskActionRows {
    primary: u16,
    secondary: u16,
    assign: u16,
    delete: u16,
    footer: u16,
}

impl TaskActionRows {
    fn action_rows(self) -> [u16; 4] {
        [self.primary, self.secondary, self.assign, self.delete]
    }

    fn base_row(self, row: u16) -> u16 {
        row.saturating_sub(self.primary.saturating_sub(TASK_PRIMARY_ACTION_ROW))
    }
}

fn task_action_rows(task: &amux_protocol::WorkspaceTask, card_body_width: u16) -> TaskActionRows {
    let offset = title_row_count(&task.title, card_body_width).saturating_sub(1);
    TaskActionRows {
        primary: TASK_PRIMARY_ACTION_ROW.saturating_add(offset),
        secondary: TASK_SECONDARY_ACTION_ROW.saturating_add(offset),
        assign: TASK_ASSIGN_ACTION_ROW.saturating_add(offset),
        delete: TASK_DELETE_ACTION_ROW.saturating_add(offset),
        footer: TASK_DELETE_ACTION_ROW
            .saturating_add(1)
            .saturating_add(offset),
    }
}

fn short_task_id(task_id: &str) -> String {
    task_id.chars().take(12).collect()
}

fn workspace_column_areas(inner: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(inner)
}

fn task_card_height(
    task: &amux_protocol::WorkspaceTask,
    card_body_width: u16,
    expanded_task_ids: &HashSet<String>,
) -> u16 {
    let title_extra_rows = title_row_count(&task.title, card_body_width).saturating_sub(1);
    if expanded_task_ids.contains(&task.id) {
        TASK_EXPANDED_ROW_HEIGHT.saturating_add(title_extra_rows)
    } else {
        TASK_COLLAPSED_ROW_HEIGHT.saturating_add(title_extra_rows)
    }
}

fn task_card_rect(
    body: Rect,
    tasks: &[amux_protocol::WorkspaceTask],
    expanded_task_ids: &HashSet<String>,
    index: usize,
) -> Rect {
    let card_body_width = body.width.saturating_sub(2);
    let offset = tasks.iter().take(index).fold(0u16, |offset, task| {
        offset.saturating_add(task_card_height(task, card_body_width, expanded_task_ids))
    });
    let y = body.y.saturating_add(offset);
    let height = tasks
        .get(index)
        .map(|task| task_card_height(task, card_body_width, expanded_task_ids))
        .unwrap_or(TASK_COLLAPSED_ROW_HEIGHT);
    Rect::new(
        body.x,
        y,
        body.width,
        height.min(body.y.saturating_add(body.height).saturating_sub(y)),
    )
}

fn task_card_at_position(
    body: Rect,
    tasks: &[amux_protocol::WorkspaceTask],
    expanded_task_ids: &HashSet<String>,
    position: Position,
) -> Option<(usize, Rect)> {
    if !contains(body, position) {
        return None;
    }
    for index in 0..tasks.len() {
        let rect = task_card_rect(body, tasks, expanded_task_ids, index);
        if contains(rect, position) {
            return Some((index, rect));
        }
        if rect.y >= body.y.saturating_add(body.height) {
            break;
        }
    }
    None
}

fn board_area(inner: Rect) -> Rect {
    Rect::new(
        inner.x,
        inner.y.saturating_add(1),
        inner.width,
        inner.height.saturating_sub(1),
    )
}

fn block_inner(area: Rect) -> Rect {
    Rect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(1),
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    )
}

fn contains(area: Rect, position: Position) -> bool {
    position.x >= area.x
        && position.x < area.x.saturating_add(area.width)
        && position.y >= area.y
        && position.y < area.y.saturating_add(area.height)
}

fn action_at_x(
    row: u16,
    body_x: u16,
    position_x: u16,
    run_blocked: bool,
) -> Option<WorkspaceBoardAction> {
    let x = position_x.saturating_sub(body_x);
    match row {
        TASK_PRIMARY_ACTION_ROW if run_blocked => match x {
            0..=8 => Some(WorkspaceBoardAction::RunBlocked),
            10..=16 => Some(WorkspaceBoardAction::Pause),
            18..=23 => Some(WorkspaceBoardAction::Stop),
            _ => None,
        },
        TASK_PRIMARY_ACTION_ROW => match x {
            0..=4 => Some(WorkspaceBoardAction::Run),
            6..=12 => Some(WorkspaceBoardAction::Pause),
            14..=19 => Some(WorkspaceBoardAction::Stop),
            _ => None,
        },
        TASK_SECONDARY_ACTION_ROW => match x {
            0..=5 => Some(WorkspaceBoardAction::MoveNext),
            7..=14 => Some(WorkspaceBoardAction::Review),
            _ => None,
        },
        TASK_ASSIGN_ACTION_ROW => match x {
            0..=7 => Some(WorkspaceBoardAction::Assign),
            9..=18 => Some(WorkspaceBoardAction::Reviewer),
            _ => None,
        },
        TASK_DELETE_ACTION_ROW => match x {
            0..=8 => Some(WorkspaceBoardAction::Details),
            10..=18 => Some(WorkspaceBoardAction::History),
            20..=25 => Some(WorkspaceBoardAction::Edit),
            27..=34 => Some(WorkspaceBoardAction::Delete),
            _ => None,
        },
        _ => None,
    }
}

fn collapsed_controls_action_at_x(body_x: u16, position_x: u16) -> Option<WorkspaceBoardAction> {
    let x = position_x.saturating_sub(body_x);
    match x {
        0..=5 => Some(WorkspaceBoardAction::OpenRuntime),
        7..=15 => Some(WorkspaceBoardAction::ToggleActions),
        _ => None,
    }
}

fn expanded_footer_action_at_x(body_x: u16, position_x: u16) -> Option<WorkspaceBoardAction> {
    let x = position_x.saturating_sub(body_x);
    match x {
        0..=5 => Some(WorkspaceBoardAction::OpenRuntime),
        7..=20 => Some(WorkspaceBoardAction::ToggleActions),
        _ => None,
    }
}

fn toolbar_action_at_x(body_x: u16, position_x: u16) -> Option<WorkspaceBoardToolbarAction> {
    let x = position_x.saturating_sub(body_x);
    match x {
        0..=9 => Some(WorkspaceBoardToolbarAction::NewTask),
        11..=19 => Some(WorkspaceBoardToolbarAction::Refresh),
        21..=37 => Some(WorkspaceBoardToolbarAction::ToggleOperator),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
