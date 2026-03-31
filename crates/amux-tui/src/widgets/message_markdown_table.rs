use super::*;

pub(super) fn is_markdown_table_row(line: &str) -> bool {
    line.contains('|')
}

pub(super) fn is_markdown_table_separator(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && trimmed.contains('|')
        && trimmed
            .chars()
            .all(|ch| matches!(ch, '|' | '-' | ':' | ' '))
}

pub(super) fn is_markdown_table_start(lines: &[&str], idx: usize) -> bool {
    idx + 1 < lines.len()
        && is_markdown_table_row(lines[idx])
        && is_markdown_table_separator(lines[idx + 1])
}

fn parse_markdown_table_row(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

fn pad_cell(text: &str, width: usize) -> String {
    let current = UnicodeWidthStr::width(text);
    if current >= width {
        return text.to_string();
    }
    format!("{text}{}", " ".repeat(width - current))
}

fn wrap_table_cell(text: &str, width: usize) -> Vec<String> {
    wrap_styled_line(Line::from(text.to_string()), width.max(1))
        .into_iter()
        .map(|line| line.spans.into_iter().map(|span| span.content).collect())
        .collect()
}

fn fit_table_widths(rows: &[Vec<String>], width: usize) -> Vec<usize> {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    if cols == 0 {
        return Vec::new();
    }

    let gutter_width = cols.saturating_sub(1) * 3;
    let available = width.saturating_sub(gutter_width).max(cols);
    let mut widths = vec![1usize; cols];

    for row in rows {
        for (idx, cell) in row.iter().enumerate() {
            widths[idx] = widths[idx].max(UnicodeWidthStr::width(cell.as_str()));
        }
    }

    let total: usize = widths.iter().sum();
    if total <= available {
        return widths;
    }

    let mut assigned = vec![3usize; cols];
    let mut remaining = available.saturating_sub(assigned.iter().sum::<usize>());
    let mut growable: Vec<(usize, usize)> = widths
        .iter()
        .enumerate()
        .map(|(idx, natural)| (idx, natural.saturating_sub(3)))
        .collect();

    while remaining > 0 {
        let mut progressed = false;
        for (idx, extra) in &mut growable {
            if *extra > 0 && remaining > 0 {
                assigned[*idx] += 1;
                *extra -= 1;
                remaining -= 1;
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }

    assigned
}

pub(super) fn render_markdown_table(lines: &[&str], width: usize) -> Vec<Line<'static>> {
    if lines.is_empty() {
        return vec![];
    }

    let mut rows = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if idx == 1 && is_markdown_table_separator(line) {
            continue;
        }
        rows.push(parse_markdown_table_row(line));
    }

    let col_widths = fit_table_widths(&rows, width.max(1));
    if col_widths.is_empty() {
        return vec![];
    }

    let header_style = Style::default().add_modifier(Modifier::BOLD);
    let separator = col_widths
        .iter()
        .map(|col| "─".repeat(*col))
        .collect::<Vec<_>>()
        .join("─┼─");

    let mut rendered = Vec::new();
    for (row_idx, row) in rows.iter().enumerate() {
        let wrapped_cells = col_widths
            .iter()
            .enumerate()
            .map(|(col_idx, col_width)| {
                let cell = row.get(col_idx).map(String::as_str).unwrap_or("");
                wrap_table_cell(cell, *col_width)
            })
            .collect::<Vec<_>>();
        let row_height = wrapped_cells.iter().map(Vec::len).max().unwrap_or(1);

        for visual_row in 0..row_height {
            let mut spans = Vec::new();
            for (col_idx, col_width) in col_widths.iter().enumerate() {
                if col_idx > 0 {
                    spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
                }
                let cell_line = wrapped_cells
                    .get(col_idx)
                    .and_then(|lines| lines.get(visual_row))
                    .map(String::as_str)
                    .unwrap_or("");
                spans.push(Span::styled(
                    pad_cell(cell_line, *col_width),
                    if row_idx == 0 {
                        header_style
                    } else {
                        Style::default()
                    },
                ));
            }
            rendered.push(Line::from(spans));
        }
        if row_idx == 0 {
            rendered.push(Line::from(Span::styled(
                separator.clone(),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }
    rendered
}
