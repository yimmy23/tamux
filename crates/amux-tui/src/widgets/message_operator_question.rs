use ratatui::text::{Line, Span};

use crate::state::chat::{AgentMessage, MessageRole};
use crate::theme::ThemeTokens;

use super::message::wrap_text;

#[derive(Debug, Default)]
struct ParsedOperatorQuestion {
    title: Option<String>,
    body_lines: Vec<String>,
    options: Vec<OperatorQuestionOption>,
}

#[derive(Debug)]
struct OperatorQuestionOption {
    label: String,
    text: String,
}

pub(crate) fn render_operator_question_message(
    msg: &AgentMessage,
    theme: &ThemeTokens,
    width: usize,
) -> Option<Vec<Line<'static>>> {
    if msg.role != MessageRole::Assistant || !msg.is_operator_question {
        return None;
    }

    let parsed = parse_operator_question_content(&msg.content);
    Some(render_operator_question(&parsed, msg, theme, width.max(1)))
}

fn parse_operator_question_content(content: &str) -> ParsedOperatorQuestion {
    let mut parsed = ParsedOperatorQuestion::default();

    for line in content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if parsed.title.is_none() {
            parsed.title = Some(line.to_string());
            continue;
        }

        if let Some(option) = parse_option_line(line) {
            parsed.options.push(option);
        } else {
            parsed.body_lines.push(line.to_string());
        }
    }

    if parsed.title.is_none() && parsed.body_lines.is_empty() && parsed.options.is_empty() {
        parsed.body_lines.push(content.trim().to_string());
    }

    parsed
}

fn parse_option_line(line: &str) -> Option<OperatorQuestionOption> {
    let trimmed = line.trim();
    for separator in [" - ", ": ", ") ", ". "] {
        if let Some((label, text)) = trimmed.split_once(separator) {
            let label = label.trim();
            let text = text.trim();
            if label.is_empty() || text.is_empty() || !is_option_label(label) {
                continue;
            }
            return Some(OperatorQuestionOption {
                label: label.to_string(),
                text: text.to_string(),
            });
        }
    }

    None
}

fn is_option_label(label: &str) -> bool {
    !label.is_empty() && label.len() <= 4 && label.chars().all(|ch| ch.is_ascii_alphanumeric())
}

fn render_operator_question(
    parsed: &ParsedOperatorQuestion,
    msg: &AgentMessage,
    theme: &ThemeTokens,
    width: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let answer = msg.operator_question_answer.as_deref().map(str::trim);
    let answer_summary = match answer {
        Some(answer) if !answer.is_empty() => {
            if let Some(option) = parsed
                .options
                .iter()
                .find(|option| option_matches_answer(option, answer))
            {
                format!("answered: [{}] {}", option.label, option.text)
            } else {
                format!("answered: {answer}")
            }
        }
        _ => "awaiting answer".to_string(),
    };

    push_wrapped_text(
        &mut lines,
        &format!("operator question {answer_summary}"),
        theme.fg_dim,
        width,
    );

    if let Some(title) = parsed
        .title
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        push_wrapped_text(&mut lines, title, theme.fg_active, width);
    }

    for body_line in &parsed.body_lines {
        push_wrapped_text(&mut lines, body_line, theme.fg_active, width);
    }

    if !parsed.options.is_empty() {
        lines.push(Line::from(vec![Span::styled("options", theme.fg_dim)]));
        for option in &parsed.options {
            let selected = answer.is_some_and(|answer| option_matches_answer(option, answer));
            let option_style = if selected {
                theme.accent_secondary
            } else {
                theme.fg_active
            };
            push_wrapped_text(
                &mut lines,
                &format!("[{}] {}", option.label, option.text),
                option_style,
                width,
            );
        }
    }

    if lines.len() == 1 && !msg.content.trim().is_empty() {
        for line in wrap_text(&msg.content, width) {
            lines.push(Line::from(vec![Span::styled(line, theme.fg_active)]));
        }
    }

    lines
}

fn push_wrapped_text(
    lines: &mut Vec<Line<'static>>,
    text: &str,
    style: ratatui::style::Style,
    width: usize,
) {
    for line in wrap_text(text, width.max(1)) {
        lines.push(Line::from(vec![Span::styled(line, style)]));
    }
}

fn option_matches_answer(option: &OperatorQuestionOption, answer: &str) -> bool {
    let answer = answer.trim();
    !answer.is_empty()
        && (option.label.eq_ignore_ascii_case(answer) || option.text.eq_ignore_ascii_case(answer))
}
