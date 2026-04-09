use ratatui::style::Style;
use ratatui::text::{Line, Span};
use serde_json::Value;
use unicode_width::UnicodeWidthChar;

use crate::theme::ThemeTokens;

const MAX_STRUCTURED_FIELDS: usize = 24;

#[derive(Clone, Copy)]
pub(crate) enum ToolStructuredValueSource {
    Arguments,
    Result,
}

#[derive(Clone, Copy)]
enum ToolDiffLineKind {
    File,
    Hunk,
    Add,
    Remove,
    Context,
    Meta,
}

struct ToolDiffLine {
    kind: ToolDiffLineKind,
    text: String,
}

struct ToolDiffSection {
    lines: Vec<ToolDiffLine>,
}

pub(crate) fn render_tool_edit_diff(
    tool_name: &str,
    tool_arguments: &str,
    theme: &ThemeTokens,
    width: usize,
) -> Option<Vec<Line<'static>>> {
    let sections = build_tool_diff_sections(tool_name, tool_arguments)?;
    let mut rendered = Vec::new();

    for (index, section) in sections.into_iter().enumerate() {
        if index > 0 {
            rendered.push(Line::default());
        }
        for line in section.lines {
            for wrapped in wrap_preserving_whitespace(&line.text, width.max(1)) {
                rendered.push(Line::from(Span::styled(
                    wrapped,
                    style_for_kind(line.kind, theme),
                )));
            }
        }
    }

    Some(rendered)
}

fn build_tool_diff_sections(tool_name: &str, tool_arguments: &str) -> Option<Vec<ToolDiffSection>> {
    let args: Value = serde_json::from_str(tool_arguments).ok()?;
    match tool_name {
        "apply_patch" => build_apply_patch_sections(&args),
        "apply_file_patch" => build_apply_file_patch_sections(&args),
        "replace_in_file" => build_replace_in_file_sections(&args),
        "write_file" => build_write_like_sections(&args, "write"),
        "append_to_file" => build_write_like_sections(&args, "append"),
        _ => None,
    }
}

pub(crate) fn render_tool_structured_json(
    tool_name: &str,
    source: ToolStructuredValueSource,
    raw: &str,
    theme: &ThemeTokens,
    width: usize,
) -> Option<Vec<Line<'static>>> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let mut fields = Vec::new();
    flatten_structured_value(tool_name, source, String::new(), &value, &mut fields);
    if fields.is_empty() {
        return None;
    }

    let mut rendered = Vec::new();
    for (key, value) in fields {
        let prefix = format!("{key}: ");
        let prefix_width = prefix
            .chars()
            .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0).max(1))
            .sum::<usize>();
        let wrapped_values =
            wrap_preserving_whitespace(&value, width.saturating_sub(prefix_width).max(1));

        if let Some(first_line) = wrapped_values.first() {
            rendered.push(Line::from(vec![
                Span::styled(prefix.clone(), theme.fg_dim),
                Span::styled(first_line.clone(), theme.fg_active),
            ]));
            let indent = " ".repeat(prefix_width);
            for continuation in wrapped_values.iter().skip(1) {
                rendered.push(Line::from(vec![
                    Span::styled(indent.clone(), theme.fg_dim),
                    Span::styled(continuation.clone(), theme.fg_active),
                ]));
            }
        }
    }

    Some(rendered)
}

fn build_apply_patch_sections(args: &Value) -> Option<Vec<ToolDiffSection>> {
    if let Some(input) = get_string_arg(args, &["input", "patch"]) {
        return parse_harness_patch(input);
    }
    build_apply_file_patch_sections(args)
}

fn build_apply_file_patch_sections(args: &Value) -> Option<Vec<ToolDiffSection>> {
    let path = get_path_arg(args)?;
    let edits = args
        .get("edits")
        .or_else(|| args.get("patches"))?
        .as_array()?;
    if edits.is_empty() {
        return None;
    }

    let mut lines = vec![
        ToolDiffLine {
            kind: ToolDiffLineKind::File,
            text: format!("--- {path}"),
        },
        ToolDiffLine {
            kind: ToolDiffLineKind::File,
            text: format!("+++ {path}"),
        },
    ];

    for edit in edits {
        let old_text = get_string_arg(edit, &["old_text", "search", "find"])?;
        let new_text = get_string_arg(edit, &["new_text", "replace", "replacement"])?;
        lines.push(ToolDiffLine {
            kind: ToolDiffLineKind::Hunk,
            text: "@@ patch @@".to_string(),
        });
        push_prefixed_lines(&mut lines, old_text, '-', ToolDiffLineKind::Remove);
        push_prefixed_lines(&mut lines, new_text, '+', ToolDiffLineKind::Add);
    }

    Some(vec![ToolDiffSection { lines }])
}

fn build_replace_in_file_sections(args: &Value) -> Option<Vec<ToolDiffSection>> {
    let path = get_path_arg(args)?;
    let old_text = get_string_arg(args, &["old_text", "search", "find"])?;
    let new_text = get_string_arg(args, &["new_text", "replace", "replacement"])?;

    let mut lines = vec![
        ToolDiffLine {
            kind: ToolDiffLineKind::File,
            text: format!("--- {path}"),
        },
        ToolDiffLine {
            kind: ToolDiffLineKind::File,
            text: format!("+++ {path}"),
        },
        ToolDiffLine {
            kind: ToolDiffLineKind::Hunk,
            text: "@@ replace @@".to_string(),
        },
    ];
    push_prefixed_lines(&mut lines, old_text, '-', ToolDiffLineKind::Remove);
    push_prefixed_lines(&mut lines, new_text, '+', ToolDiffLineKind::Add);

    Some(vec![ToolDiffSection { lines }])
}

fn build_write_like_sections(args: &Value, action: &str) -> Option<Vec<ToolDiffSection>> {
    let path = get_path_arg(args)?;
    let content = get_string_arg(args, &["content", "contents", "text", "data", "body"])?;
    let mut lines = vec![ToolDiffLine {
        kind: ToolDiffLineKind::File,
        text: format!("+++ {path}"),
    }];
    lines.push(ToolDiffLine {
        kind: ToolDiffLineKind::Hunk,
        text: format!("@@ {action} @@"),
    });
    push_prefixed_lines(&mut lines, content, '+', ToolDiffLineKind::Add);
    Some(vec![ToolDiffSection { lines }])
}

fn parse_harness_patch(input: &str) -> Option<Vec<ToolDiffSection>> {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut sections = Vec::new();
    let mut current: Option<ToolDiffSection> = None;

    for raw_line in normalized.lines() {
        if raw_line == "*** Begin Patch" || raw_line == "*** End Patch" {
            continue;
        }

        if let Some(path) = raw_line.strip_prefix("*** Update File: ") {
            push_current_section(&mut sections, &mut current);
            let path = path.split(" -> ").next().unwrap_or(path).trim().to_string();
            current = Some(ToolDiffSection {
                lines: vec![
                    ToolDiffLine {
                        kind: ToolDiffLineKind::File,
                        text: format!("--- {path}"),
                    },
                    ToolDiffLine {
                        kind: ToolDiffLineKind::File,
                        text: format!("+++ {path}"),
                    },
                ],
            });
            continue;
        }

        if let Some(path) = raw_line.strip_prefix("*** Add File: ") {
            push_current_section(&mut sections, &mut current);
            let path = path.split(" -> ").next().unwrap_or(path).trim().to_string();
            current = Some(ToolDiffSection {
                lines: vec![ToolDiffLine {
                    kind: ToolDiffLineKind::File,
                    text: format!("+++ {path}"),
                }],
            });
            continue;
        }

        if let Some(path) = raw_line.strip_prefix("*** Delete File: ") {
            push_current_section(&mut sections, &mut current);
            let path = path.split(" -> ").next().unwrap_or(path).trim().to_string();
            current = Some(ToolDiffSection {
                lines: vec![ToolDiffLine {
                    kind: ToolDiffLineKind::File,
                    text: format!("--- {path}"),
                }],
            });
            continue;
        }

        let section = current.get_or_insert_with(|| ToolDiffSection { lines: Vec::new() });
        let line = if raw_line.starts_with("@@") {
            ToolDiffLine {
                kind: ToolDiffLineKind::Hunk,
                text: raw_line.to_string(),
            }
        } else if raw_line.starts_with('+') {
            ToolDiffLine {
                kind: ToolDiffLineKind::Add,
                text: raw_line.to_string(),
            }
        } else if raw_line.starts_with('-') {
            ToolDiffLine {
                kind: ToolDiffLineKind::Remove,
                text: raw_line.to_string(),
            }
        } else if raw_line.starts_with(' ') {
            ToolDiffLine {
                kind: ToolDiffLineKind::Context,
                text: raw_line.to_string(),
            }
        } else {
            ToolDiffLine {
                kind: ToolDiffLineKind::Meta,
                text: raw_line.to_string(),
            }
        };
        section.lines.push(line);
    }

    push_current_section(&mut sections, &mut current);
    if sections.is_empty() {
        None
    } else {
        Some(sections)
    }
}

fn push_current_section(
    sections: &mut Vec<ToolDiffSection>,
    current: &mut Option<ToolDiffSection>,
) {
    if let Some(section) = current.take() {
        if !section.lines.is_empty() {
            sections.push(section);
        }
    }
}

fn push_prefixed_lines(
    lines: &mut Vec<ToolDiffLine>,
    text: &str,
    prefix: char,
    kind: ToolDiffLineKind,
) {
    if text.is_empty() {
        lines.push(ToolDiffLine {
            kind,
            text: prefix.to_string(),
        });
        return;
    }

    for line in text.lines() {
        lines.push(ToolDiffLine {
            kind,
            text: format!("{prefix}{line}"),
        });
    }
}

fn get_path_arg(args: &Value) -> Option<&str> {
    get_string_arg(args, &["path", "file_path", "filepath", "filename", "file"])
}

fn get_string_arg<'a>(args: &'a Value, names: &[&str]) -> Option<&'a str> {
    names
        .iter()
        .find_map(|name| args.get(*name).and_then(|value| value.as_str()))
}

fn style_for_kind(kind: ToolDiffLineKind, theme: &ThemeTokens) -> Style {
    match kind {
        ToolDiffLineKind::File => theme.accent_primary,
        ToolDiffLineKind::Hunk => theme.accent_secondary,
        ToolDiffLineKind::Add => theme.accent_success,
        ToolDiffLineKind::Remove => theme.accent_danger,
        ToolDiffLineKind::Context => theme.fg_active,
        ToolDiffLineKind::Meta => theme.fg_dim,
    }
}

fn flatten_structured_value(
    tool_name: &str,
    source: ToolStructuredValueSource,
    prefix: String,
    value: &Value,
    fields: &mut Vec<(String, String)>,
) {
    if fields.len() >= MAX_STRUCTURED_FIELDS {
        return;
    }

    match value {
        Value::Null => fields.push((empty_key(&prefix), "null".to_string())),
        Value::Bool(boolean) => fields.push((empty_key(&prefix), boolean.to_string())),
        Value::Number(number) => fields.push((empty_key(&prefix), number.to_string())),
        Value::String(text) => fields.push((
            empty_key(&prefix),
            summarize_string_value(tool_name, source, &prefix, text),
        )),
        Value::Array(items) => fields.push((empty_key(&prefix), summarize_array_value(items))),
        Value::Object(entries) => {
            if entries.is_empty() {
                fields.push((empty_key(&prefix), "{}".to_string()));
                return;
            }
            for (key, nested) in entries {
                if fields.len() >= MAX_STRUCTURED_FIELDS {
                    break;
                }
                let next_prefix = if prefix.is_empty() {
                    key.to_string()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_structured_value(tool_name, source, next_prefix, nested, fields);
            }
        }
    }
}

fn summarize_string_value(
    tool_name: &str,
    source: ToolStructuredValueSource,
    key_path: &str,
    value: &str,
) -> String {
    let field_name = key_path.rsplit('.').next().unwrap_or(key_path);
    let is_create_file_content = tool_name == "create_file"
        && matches!(source, ToolStructuredValueSource::Arguments)
        && matches!(
            field_name,
            "content" | "contents" | "text" | "data" | "body"
        );

    if is_create_file_content {
        let line_count = if value.is_empty() {
            0
        } else {
            value.lines().count()
        };
        return format!("{} chars, {} lines", value.len(), line_count);
    }

    if value.contains('\n') {
        let lines: Vec<&str> = value.lines().collect();
        let preview = lines.iter().take(3).copied().collect::<Vec<_>>().join(" ");
        return if lines.len() > 3 {
            format!(
                "{preview} ... (+{} more lines)",
                lines.len().saturating_sub(3)
            )
        } else {
            preview
        };
    }

    if value.len() > 180 {
        return format!("{}... (+{} chars)", &value[..180], value.len() - 180);
    }

    value.to_string()
}

fn summarize_array_value(items: &[Value]) -> String {
    if items.is_empty() {
        return "[]".to_string();
    }

    if items
        .iter()
        .all(|item| item.is_null() || item.is_boolean() || item.is_number() || item.is_string())
    {
        let preview = items
            .iter()
            .take(5)
            .map(|item| match item {
                Value::Null => "null".to_string(),
                Value::Bool(boolean) => boolean.to_string(),
                Value::Number(number) => number.to_string(),
                Value::String(text) => text.clone(),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        return if items.len() > 5 {
            format!("[{preview}, +{} more]", items.len() - 5)
        } else {
            format!("[{preview}]")
        };
    }

    format!("{} items", items.len())
}

fn empty_key(prefix: &str) -> String {
    if prefix.is_empty() {
        "value".to_string()
    } else {
        prefix.to_string()
    }
}

fn wrap_preserving_whitespace(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut wrapped = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
        if current_width > 0 && current_width + ch_width > width {
            wrapped.push(current);
            current = String::new();
            current_width = 0;
        }
        current.push(ch);
        current_width += ch_width;
    }

    if !current.is_empty() {
        wrapped.push(current);
    }

    if wrapped.is_empty() {
        wrapped.push(String::new());
    }
    wrapped
}
