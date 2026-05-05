use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::*;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Wrap};

use crate::theme::ThemeTokens;

fn visible_window(cursor: usize, item_count: usize, list_height: usize) -> (usize, usize) {
    if item_count == 0 || list_height == 0 {
        return (0, 0);
    }

    let height = list_height.min(item_count);
    let max_start = item_count.saturating_sub(height);
    let start = cursor
        .saturating_sub(height.saturating_sub(1))
        .min(max_start);
    (start, height)
}

#[derive(Clone, Copy, Debug)]
pub struct OperatorProfileQuestionView<'a> {
    pub prompt: &'a str,
    pub input_kind: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct OperatorProfileProgressView {
    pub answered: u32,
    pub remaining: u32,
    pub completion_ratio: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct OperatorProfileOnboardingView<'a> {
    pub question: Option<OperatorProfileQuestionView<'a>>,
    pub progress: Option<OperatorProfileProgressView>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperatorProfileOnboardingHitTarget {
    BoolChoice(bool),
    Submit,
    Skip,
    Defer,
}

fn is_bool_question(view: &OperatorProfileOnboardingView<'_>) -> bool {
    view.question
        .is_some_and(|question| matches!(question.input_kind, "bool" | "boolean"))
}

fn completion_percent(progress: Option<OperatorProfileProgressView>) -> u8 {
    let Some(progress) = progress else {
        return 0;
    };
    (progress.completion_ratio.clamp(0.0, 1.0) * 100.0).round() as u8
}

fn action_rows(
    view: &OperatorProfileOnboardingView<'_>,
) -> Vec<(OperatorProfileOnboardingHitTarget, &'static str)> {
    if is_bool_question(view) {
        vec![
            (
                OperatorProfileOnboardingHitTarget::BoolChoice(true),
                "[yes]",
            ),
            (
                OperatorProfileOnboardingHitTarget::BoolChoice(false),
                "[no]",
            ),
            (OperatorProfileOnboardingHitTarget::Skip, "[skip]"),
            (OperatorProfileOnboardingHitTarget::Defer, "[defer]"),
        ]
    } else {
        vec![
            (OperatorProfileOnboardingHitTarget::Submit, "[submit]"),
            (OperatorProfileOnboardingHitTarget::Skip, "[skip]"),
            (OperatorProfileOnboardingHitTarget::Defer, "[defer]"),
        ]
    }
}

pub fn item_count(view: &OperatorProfileOnboardingView<'_>) -> usize {
    action_rows(view).len()
}

pub fn target_at_index(
    view: &OperatorProfileOnboardingView<'_>,
    index: usize,
) -> Option<OperatorProfileOnboardingHitTarget> {
    action_rows(view).get(index).map(|(target, _)| *target)
}

fn onboarding_block(theme: &ThemeTokens) -> Block<'_> {
    Block::default()
        .title(" Operator Profile ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary)
}

fn modal_layout(inner: Rect) -> [Rect; 6] {
    Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(inner)
}

fn list_area(area: Rect, theme: &ThemeTokens) -> Option<Rect> {
    if !area.is_empty() {
        let inner = onboarding_block(theme).inner(area);
        if inner.height >= 6 {
            return Some(modal_layout(inner)[5]);
        }
    }
    None
}

#[cfg(test)]
pub fn hit_test_regions(
    area: Rect,
    view: &OperatorProfileOnboardingView<'_>,
) -> Option<Vec<(OperatorProfileOnboardingHitTarget, Rect)>> {
    let theme = ThemeTokens::default();
    let list_row = list_area(area, &theme)?;
    let rows = action_rows(view);
    Some(
        rows.into_iter()
            .enumerate()
            .map(|(idx, (target, _))| {
                (
                    target,
                    Rect::new(
                        list_row.x,
                        list_row.y.saturating_add(idx as u16),
                        list_row.width,
                        1,
                    ),
                )
            })
            .collect(),
    )
}

#[cfg(test)]
pub fn hit_test(
    area: Rect,
    view: &OperatorProfileOnboardingView<'_>,
    position: Position,
) -> Option<OperatorProfileOnboardingHitTarget> {
    index_at_position(area, view, position).and_then(|index| target_at_index(view, index))
}

pub fn index_at_position(
    area: Rect,
    view: &OperatorProfileOnboardingView<'_>,
    position: Position,
) -> Option<usize> {
    if !area.contains(position) {
        return None;
    }
    let theme = ThemeTokens::default();
    let list_row = list_area(area, &theme)?;
    if !list_row.contains(position) {
        return None;
    }
    let row_idx = position.y.saturating_sub(list_row.y) as usize;
    (row_idx < item_count(view)).then_some(row_idx)
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    view: &OperatorProfileOnboardingView<'_>,
    selected_index: usize,
    theme: &ThemeTokens,
) {
    let block = onboarding_block(theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 6 || inner.width < 24 {
        return;
    }

    let [progress_row, _, question_label_row, question_row, _, list_row] = modal_layout(inner);
    let percent = completion_percent(view.progress);
    let (answered, remaining) = view
        .progress
        .map(|progress| (progress.answered, progress.remaining))
        .unwrap_or((0, if view.question.is_some() { 1 } else { 0 }));

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Progress: ", theme.fg_dim),
            Span::styled(
                format!("{answered} answered • {remaining} remaining • {percent}%"),
                theme.accent_success,
            ),
        ])),
        progress_row,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("Question", theme.fg_active))),
        question_label_row,
    );
    frame.render_widget(
        Paragraph::new(
            view.question
                .map(|question| question.prompt)
                .unwrap_or("Loading"),
        )
        .wrap(Wrap { trim: true }),
        question_row,
    );

    let rows = action_rows(view);
    let list_h = list_row.height as usize;
    let (visible_start, visible_len) = visible_window(selected_index, rows.len(), list_h);
    let items: Vec<ListItem> = (0..list_h)
        .map(|idx| {
            if idx < visible_len {
                let absolute_index = visible_start + idx;
                let (_, label) = rows[absolute_index];
                let item = if absolute_index == selected_index {
                    ListItem::new(Line::from(vec![Span::raw(format!(" > {label}"))]))
                        .style(Style::default().bg(Color::Indexed(178)).fg(Color::Black))
                } else {
                    ListItem::new(Line::from(vec![
                        Span::raw("   "),
                        Span::styled(label, theme.fg_active),
                    ]))
                };
                item
            } else {
                ListItem::new(Line::raw(""))
            }
        })
        .collect();
    frame.render_widget(List::new(items), list_row);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn completion_percent_is_clamped() {
        assert_eq!(
            completion_percent(Some(OperatorProfileProgressView {
                answered: 1,
                remaining: 1,
                completion_ratio: 1.4
            })),
            100
        );
        assert_eq!(
            completion_percent(Some(OperatorProfileProgressView {
                answered: 1,
                remaining: 1,
                completion_ratio: -0.2
            })),
            0
        );
    }

    #[test]
    fn render_operator_profile_card_does_not_panic() {
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        let theme = ThemeTokens::default();
        let view = OperatorProfileOnboardingView {
            question: Some(OperatorProfileQuestionView {
                prompt: "What should I call you?",
                input_kind: "text",
            }),
            progress: Some(OperatorProfileProgressView {
                answered: 0,
                remaining: 3,
                completion_ratio: 0.0,
            }),
        };

        terminal
            .draw(|frame| render(frame, frame.area(), &view, 0, &theme))
            .expect("render should not panic");
    }

    #[test]
    fn boolean_question_hit_test_returns_choice_and_defer_rows() {
        let area = Rect::new(0, 0, 120, 36);
        let view = OperatorProfileOnboardingView {
            question: Some(OperatorProfileQuestionView {
                prompt: "Enable operator modeling overall?",
                input_kind: "boolean",
            }),
            progress: Some(OperatorProfileProgressView {
                answered: 0,
                remaining: 5,
                completion_ratio: 0.0,
            }),
        };

        let button_regions = hit_test_regions(area, &view)
            .expect("regular onboarding card should expose button regions");
        let no_button = button_regions
            .iter()
            .find(|(target, _)| *target == OperatorProfileOnboardingHitTarget::BoolChoice(false))
            .map(|(_, region)| *region)
            .expect("no choice should be clickable");
        let defer_button = button_regions
            .iter()
            .find(|(target, _)| *target == OperatorProfileOnboardingHitTarget::Defer)
            .map(|(_, region)| *region)
            .expect("defer action should be clickable");

        assert_eq!(
            hit_test(area, &view, Position::new(no_button.x, no_button.y)),
            Some(OperatorProfileOnboardingHitTarget::BoolChoice(false))
        );
        assert_eq!(
            hit_test(area, &view, Position::new(defer_button.x, defer_button.y)),
            Some(OperatorProfileOnboardingHitTarget::Defer)
        );
    }
}
