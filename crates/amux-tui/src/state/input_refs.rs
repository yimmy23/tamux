use std::env;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveToken {
    pub range: Range<usize>,
    pub text: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceEdit {
    pub range: Range<usize>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabCompletionOutcome {
    pub consumed: bool,
    pub notice: Option<String>,
    pub replacement: Option<ReferenceEdit>,
}

pub fn active_at_token(buffer: &str, cursor: usize) -> Option<ActiveToken> {
    let cursor = cursor.min(buffer.len());
    let mut start = None;

    for (index, ch) in buffer.char_indices() {
        if ch.is_whitespace() {
            if let Some(token_start) = start.take() {
                let token_end = index;
                let segment = &buffer[token_start..token_end];
                if segment.starts_with('@') && cursor >= token_start && cursor <= token_end {
                    return Some(ActiveToken {
                        range: token_start..token_end,
                        text: segment.to_string(),
                        path: segment[1..].to_string(),
                    });
                }
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }

    if let Some(token_start) = start {
        let token_end = buffer.len();
        let segment = &buffer[token_start..token_end];
        if segment.starts_with('@') && cursor >= token_start && cursor <= token_end {
            return Some(ActiveToken {
                range: token_start..token_end,
                text: segment.to_string(),
                path: segment[1..].to_string(),
            });
        }
    }

    None
}

pub fn resolve_reference_path(raw_path: &str, cwd: &Path) -> Option<PathBuf> {
    let expanded = expand_reference_path(raw_path, cwd);
    expanded.exists().then_some(expanded)
}

pub fn complete_active_at_token(buffer: &str, cursor: usize, cwd: &Path) -> TabCompletionOutcome {
    let Some(token) = active_at_token(buffer, cursor) else {
        return TabCompletionOutcome {
            consumed: false,
            notice: None,
            replacement: None,
        };
    };

    let Some(completed_path) = complete_reference_path(&token.path, cwd) else {
        return TabCompletionOutcome {
            consumed: false,
            notice: None,
            replacement: None,
        };
    };

    let completed_text = format!("@{}", completed_path);
    let notice = completion_notice(&token.path, cwd);
    if completed_text == token.text {
        return TabCompletionOutcome {
            consumed: true,
            notice,
            replacement: None,
        };
    }

    TabCompletionOutcome {
        consumed: true,
        notice,
        replacement: Some(ReferenceEdit {
            range: token.range,
            text: completed_text,
        }),
    }
}

fn complete_reference_path(raw_path: &str, cwd: &Path) -> Option<String> {
    let (search_dir, typed_prefix, raw_prefix, _raw_suffix) = completion_parts(raw_path, cwd)?;
    let mut matches = Vec::new();

    for entry in fs::read_dir(search_dir).ok()? {
        let entry = entry.ok()?;
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !file_name.starts_with(&typed_prefix) {
            continue;
        }
        let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        matches.push((file_name, is_dir));
    }

    if matches.is_empty() {
        return None;
    }

    let suffix = if matches.len() == 1 {
        let (name, is_dir) = &matches[0];
        let mut name = name.clone();
        if *is_dir {
            name.push(std::path::MAIN_SEPARATOR);
        }
        name
    } else {
        let names: Vec<String> = matches.iter().map(|(name, _)| name.clone()).collect();
        common_prefix(&names)
    };

    Some(format!("{raw_prefix}{suffix}"))
}

fn completion_notice(raw_path: &str, cwd: &Path) -> Option<String> {
    let (search_dir, typed_prefix, _, _) = completion_parts(raw_path, cwd)?;
    let mut matches = Vec::new();

    for entry in fs::read_dir(search_dir).ok()? {
        let entry = entry.ok()?;
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.starts_with(&typed_prefix) {
            matches.push(file_name);
        }
    }

    if matches.len() > 1 {
        Some(format!(
            "Multiple matches: {}",
            matches.into_iter().take(5).collect::<Vec<_>>().join(", ")
        ))
    } else {
        None
    }
}

fn completion_parts(raw_path: &str, cwd: &Path) -> Option<(PathBuf, String, String, String)> {
    if raw_path.is_empty() {
        return Some((
            cwd.to_path_buf(),
            String::new(),
            String::new(),
            String::new(),
        ));
    }

    if raw_path == "~" {
        let home = home_dir()?;
        return Some((home, String::new(), "~".to_string(), String::new()));
    }

    if raw_path.ends_with('/') || raw_path.ends_with('\\') {
        let search_dir = expand_reference_path(raw_path, cwd);
        return Some((
            search_dir,
            String::new(),
            raw_path.to_string(),
            String::new(),
        ));
    }

    let expanded = expand_reference_path(raw_path, cwd);
    let (raw_prefix, raw_suffix) = split_path_text(raw_path);

    let search_dir = if raw_path.starts_with('~') && !raw_path.starts_with("~/") {
        home_dir()?
    } else {
        expanded
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| cwd.to_path_buf())
    };

    let typed_prefix = if raw_path.starts_with('~') && raw_path.len() > 1 {
        raw_suffix.clone()
    } else {
        expanded
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| raw_suffix.clone())
    };

    Some((search_dir, typed_prefix, raw_prefix, raw_suffix))
}

fn expand_reference_path(raw_path: &str, cwd: &Path) -> PathBuf {
    if raw_path.starts_with('~') {
        if let Some(home) = home_dir() {
            return if raw_path == "~" {
                home
            } else {
                home.join(raw_path.trim_start_matches("~/"))
            };
        }
    }

    let path = Path::new(raw_path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn split_path_text(raw_path: &str) -> (String, String) {
    match last_separator_index(raw_path) {
        Some(index) => (
            raw_path[..=index].to_string(),
            raw_path[index + 1..].to_string(),
        ),
        None => (String::new(), raw_path.to_string()),
    }
}

fn last_separator_index(raw_path: &str) -> Option<usize> {
    raw_path.rfind(|ch| matches!(ch, '/' | '\\'))
}

fn common_prefix(values: &[String]) -> String {
    let Some(first) = values.first() else {
        return String::new();
    };

    let mut prefix = first.clone();
    for value in values.iter().skip(1) {
        let shared_len = prefix
            .chars()
            .zip(value.chars())
            .take_while(|(a, b)| a == b)
            .count();
        prefix = prefix.chars().take(shared_len).collect();
        if prefix.is_empty() {
            break;
        }
    }

    prefix
}
