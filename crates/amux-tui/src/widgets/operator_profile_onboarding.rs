use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::*;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Wrap};

use crate::theme::{ThemeTokens, ROUNDED_BORDER};

#[derive(Clone, Copy, Debug)]
pub struct OperatorProfileQuestionView<'a> {
    pub field_key: &'a str,
    pub prompt: &'a str,
    pub input_kind: &'a str,
    pub optional: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct OperatorProfileProgressView {
    pub answered: u32,
    pub remaining: u32,
    pub completion_ratio: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct OperatorProfileOnboardingView<'a> {
    pub session_kind: Option<&'a str>,
    pub question: Option<OperatorProfileQuestionView<'a>>,
    pub progress: Option<OperatorProfileProgressView>,
    pub loading: bool,
    pub warning: Option<&'a str>,
    pub input_value: &'a str,
    pub select_options: Option<&'a [&'a str]>,
}

fn completion_percent(progress: Option<OperatorProfileProgressView>) -> u8 {
    let Some(progress) = progress else {
        return 0;
    };
    (progress.completion_ratio.clamp(0.0, 1.0) * 100.0).round() as u8
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    view: &OperatorProfileOnboardingView<'_>,
    theme: &ThemeTokens,
) {
    if area.width < 40 || area.height < 12 {
        render_compact(frame, area, view, theme);
        return;
    }

    let card_height = area.height.min(21);
    let card_width = area.width.min(92);
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(card_height),
            Constraint::Fill(1),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(card_width),
            Constraint::Fill(1),
        ])
        .split(vertical[1]);
    let card = horizontal[1];

    let block = Block::default()
        .title(Span::styled(" Operator Profile ", theme.accent_primary))
        .borders(Borders::ALL)
        .border_type(ROUNDED_BORDER)
        .border_style(theme.accent_primary)
        .padding(Padding::new(2, 2, 1, 1));
    let inner = block.inner(card);
    frame.render_widget(block, card);

    let percent = completion_percent(view.progress);
    let (answered, remaining) = view
        .progress
        .map(|progress| (progress.answered, progress.remaining))
        .unwrap_or((0, if view.question.is_some() { 1 } else { 0 }));

    let mut lines = vec![
        Line::from(Span::styled(
            "First-run onboarding",
            theme.fg_active.add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("Session: ", theme.fg_dim),
            Span::styled(
                view.session_kind.unwrap_or("first_run_onboarding"),
                theme.accent_secondary,
            ),
        ]),
        Line::from(vec![
            Span::styled("Progress: ", theme.fg_dim),
            Span::styled(
                format!("{answered} answered • {remaining} remaining • {percent}%"),
                theme.accent_success,
            ),
        ]),
        Line::raw(""),
    ];

    if let Some(question) = view.question {
        lines.push(Line::from(Span::styled(question.prompt, theme.fg_active)));
        lines.push(Line::from(vec![
            Span::styled("Field: ", theme.fg_dim),
            Span::styled(question.field_key, theme.accent_secondary),
            Span::raw("  "),
            Span::styled("Type: ", theme.fg_dim),
            Span::styled(question.input_kind, theme.accent_secondary),
            Span::raw("  "),
            Span::styled(
                if question.optional {
                    "optional"
                } else {
                    "required"
                },
                if question.optional {
                    theme.fg_dim
                } else {
                    theme.accent_success
                },
            ),
        ]));
        if question.input_kind == "select" {
            if let Some(options) = view.select_options {
                if !options.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("Options: ", theme.fg_dim),
                        Span::styled(options.join(", "), theme.fg_active),
                    ]));
                }
            }
        } else if question.input_kind == "bool" {
            lines.push(Line::from(vec![
                Span::styled("Answer: ", theme.fg_dim),
                Span::styled("true/false (or yes/no)", theme.fg_active),
            ]));
        }
        lines.push(Line::from(vec![
            Span::styled("Current input: ", theme.fg_dim),
            Span::styled(
                if view.input_value.trim().is_empty() {
                    "—"
                } else {
                    view.input_value.trim()
                },
                theme.fg_active,
            ),
        ]));
    } else if view.loading {
        lines.push(Line::from(Span::styled(
            "Loading your next question…",
            theme.fg_dim,
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "No pending operator profile question.",
            theme.fg_dim,
        )));
    }

    if let Some(warning) = view.warning {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled("Warning:", theme.accent_danger)));
        lines.push(Line::from(Span::styled(warning, theme.accent_danger)));
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("Enter", theme.accent_success),
        Span::styled(" submit  ", theme.fg_dim),
        Span::styled("Ctrl+S", theme.accent_primary),
        Span::styled(" skip  ", theme.fg_dim),
        Span::styled("Ctrl+D", theme.accent_primary),
        Span::styled(" defer  ", theme.fg_dim),
        Span::styled("Ctrl+R", theme.accent_primary),
        Span::styled(" retry", theme.fg_dim),
    ]));

    let paragraph = Paragraph::new(lines)
        .style(Style::default())
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

fn render_compact(
    frame: &mut Frame,
    area: Rect,
    view: &OperatorProfileOnboardingView<'_>,
    theme: &ThemeTokens,
) {
    let pct = completion_percent(view.progress);
    let lines = vec![
        Line::from(Span::styled("Operator profile onboarding", theme.fg_active)),
        Line::from(Span::styled(format!("{pct}% complete"), theme.fg_dim)),
        Line::from(Span::styled("Enter submit • Ctrl+S skip", theme.fg_dim)),
        Line::from(Span::styled("Ctrl+D defer • Ctrl+R retry", theme.fg_dim)),
        Line::from(Span::styled(
            if view.loading {
                "Loading next question…"
            } else {
                view.question.map(|q| q.prompt).unwrap_or("No question")
            },
            theme.fg_dim,
        )),
    ];
    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
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
            session_kind: Some("first_run_onboarding"),
            question: Some(OperatorProfileQuestionView {
                field_key: "name",
                prompt: "What should I call you?",
                input_kind: "text",
                optional: false,
            }),
            progress: Some(OperatorProfileProgressView {
                answered: 0,
                remaining: 3,
                completion_ratio: 0.0,
            }),
            loading: false,
            warning: None,
            input_value: "Milan",
            select_options: None,
        };

        terminal
            .draw(|frame| render(frame, frame.area(), &view, &theme))
            .expect("render should not panic");
    }
}
