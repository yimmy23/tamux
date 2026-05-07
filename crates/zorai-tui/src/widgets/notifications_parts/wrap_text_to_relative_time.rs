use ratatui::style::Style;

use crate::theme::ThemeTokens;

pub(super) fn wrap_text(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current = word.to_string();
        } else if current.len() + 1 + word.len() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_string();
        }
        if lines.len() + 1 >= max_lines && current.len() >= width.saturating_sub(1) {
            break;
        }
    }
    if !current.is_empty() && lines.len() < max_lines {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    if lines.len() == max_lines && text.split_whitespace().count() > 1 {
        if let Some(last) = lines.last_mut() {
            if last.len() + 1 < width {
                last.push('…');
            }
        }
    }
    lines
}

pub(super) fn truncate_display(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    text.chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "…"
}

pub(super) fn severity_style(severity: &str, theme: &ThemeTokens) -> Style {
    match severity {
        "error" => theme.accent_danger,
        "warning" | "alert" => theme.accent_secondary,
        "info" => theme.accent_primary,
        _ => theme.fg_active,
    }
}

pub(super) fn relative_time(timestamp_ms: i64) -> String {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0);
    let elapsed_ms = now_ms.saturating_sub(timestamp_ms).max(0);
    let elapsed_secs = elapsed_ms / 1000;
    if elapsed_secs < 60 {
        format!("{}s", elapsed_secs)
    } else if elapsed_secs < 3600 {
        format!("{}m", elapsed_secs / 60)
    } else if elapsed_secs < 86_400 {
        format!("{}h", elapsed_secs / 3600)
    } else {
        format!("{}d", elapsed_secs / 86_400)
    }
}
