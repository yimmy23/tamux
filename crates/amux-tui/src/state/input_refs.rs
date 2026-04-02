use std::collections::HashSet;
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
    resolve_reference_path_with_home(raw_path, cwd, home_dir().as_deref())
}

pub(crate) fn resolve_reference_path_with_home(
    raw_path: &str,
    cwd: &Path,
    home: Option<&Path>,
) -> Option<PathBuf> {
    let expanded = expand_reference_path(raw_path, cwd, home);
    expanded.exists().then_some(expanded)
}

pub fn complete_active_at_token(buffer: &str, cursor: usize, cwd: &Path) -> TabCompletionOutcome {
    complete_active_at_token_with_home(buffer, cursor, cwd, home_dir().as_deref())
}

pub fn resolved_referenced_files(buffer: &str, cwd: &Path) -> Vec<PathBuf> {
    let mut resolved = Vec::new();
    let mut seen = HashSet::new();

    for segment in buffer.split_whitespace() {
        let Some(raw_path) = segment.strip_prefix('@') else {
            continue;
        };
        let Some(path) = resolve_reference_path(raw_path, cwd) else {
            continue;
        };
        let normalized = normalize_resolved_path(path, cwd);
        if seen.insert(normalized.clone()) {
            resolved.push(normalized);
        }
    }

    resolved
}

pub fn append_referenced_files_footer(buffer: &str, cwd: &Path) -> String {
    let referenced_files = resolved_referenced_files(buffer, cwd);
    if referenced_files.is_empty() {
        return buffer.to_string();
    }

    let footer = format!(
        "Referenced files: {}\nInspect these with read_file before making assumptions.",
        referenced_files
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );

    format!("{buffer}\n\n{footer}")
}

pub(crate) fn complete_active_at_token_with_home(
    buffer: &str,
    cursor: usize,
    cwd: &Path,
    home: Option<&Path>,
) -> TabCompletionOutcome {
    let Some(token) = active_at_token(buffer, cursor) else {
        return TabCompletionOutcome {
            consumed: false,
            notice: None,
            replacement: None,
        };
    };

    let Some(completed_path) = complete_reference_path(&token.path, cwd, home) else {
        return TabCompletionOutcome {
            consumed: true,
            notice: Some(format!("No matches for {}", token.text)),
            replacement: None,
        };
    };

    let completed_text = format!("@{}", completed_path);
    let notice = completion_notice(&token.path, cwd, home);
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

fn complete_reference_path(raw_path: &str, cwd: &Path, home: Option<&Path>) -> Option<String> {
    let (search_dir, typed_prefix, raw_prefix, _raw_suffix) =
        completion_parts(raw_path, cwd, home)?;
    let mut matches = Vec::new();

    for entry in fs::read_dir(search_dir).ok()? {
        let Ok(entry) = entry else {
            continue;
        };
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
            name.push(preferred_separator(raw_path));
        }
        name
    } else {
        let names: Vec<String> = matches.iter().map(|(name, _)| name.clone()).collect();
        common_prefix(&names)
    };

    Some(format!("{raw_prefix}{suffix}"))
}

fn completion_notice(raw_path: &str, cwd: &Path, home: Option<&Path>) -> Option<String> {
    let (search_dir, typed_prefix, _, _) = completion_parts(raw_path, cwd, home)?;
    let mut matches = Vec::new();

    for entry in fs::read_dir(search_dir).ok()? {
        let Ok(entry) = entry else {
            continue;
        };
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

fn completion_parts(
    raw_path: &str,
    cwd: &Path,
    home: Option<&Path>,
) -> Option<(PathBuf, String, String, String)> {
    if raw_path.is_empty() {
        return Some((
            cwd.to_path_buf(),
            String::new(),
            String::new(),
            String::new(),
        ));
    }

    if raw_path == "~" {
        let home = home.map(Path::to_path_buf).or_else(home_dir)?;
        return Some((home, String::new(), "~".to_string(), String::new()));
    }

    if raw_path.ends_with('/') || raw_path.ends_with('\\') {
        let search_dir = expand_reference_path(raw_path, cwd, home);
        return Some((
            search_dir,
            String::new(),
            raw_path.to_string(),
            String::new(),
        ));
    }

    let expanded = expand_reference_path(raw_path, cwd, home);
    let (raw_prefix, raw_suffix) = split_path_text(raw_path);

    let search_dir = expanded
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| cwd.to_path_buf());

    let typed_prefix = expanded
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| raw_suffix.clone());

    Some((search_dir, typed_prefix, raw_prefix, raw_suffix))
}

fn expand_reference_path(raw_path: &str, cwd: &Path, home: Option<&Path>) -> PathBuf {
    if raw_path == "~" {
        if let Some(home) = home {
            return home.to_path_buf();
        }
        if let Some(home) = home_dir() {
            return home;
        }
    }

    if let Some(rest) = raw_path
        .strip_prefix("~/")
        .or_else(|| raw_path.strip_prefix("~\\"))
    {
        let Some(home) = home.map(Path::to_path_buf).or_else(home_dir) else {
            return cwd.join(raw_path);
        };
        return join_normalized(&home, rest);
    }

    let path = Path::new(raw_path);
    if path.is_absolute() {
        path.to_path_buf()
    } else if raw_path.contains('\\') {
        join_normalized(cwd, raw_path)
    } else {
        cwd.join(path)
    }
}

fn normalize_resolved_path(path: PathBuf, cwd: &Path) -> PathBuf {
    if path.is_absolute() {
        path
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

fn preferred_separator(raw_path: &str) -> char {
    let last_forward = raw_path.rfind('/');
    let last_backslash = raw_path.rfind('\\');

    match (last_forward, last_backslash) {
        (Some(forward), Some(backslash)) if backslash > forward => '\\',
        (Some(_), Some(_)) => '/',
        (None, Some(_)) => '\\',
        (Some(_), None) => '/',
        (None, None) => std::path::MAIN_SEPARATOR,
    }
}

fn join_normalized(base: &Path, raw_path: &str) -> PathBuf {
    let mut path = base.to_path_buf();
    for component in raw_path.split(['/', '\\']) {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            path.pop();
            continue;
        }
        path.push(component);
    }
    path
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
