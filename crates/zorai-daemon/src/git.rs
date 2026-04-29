use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::process::Command;
use zorai_protocol::{GitChangeEntry, GitInfo};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitLineStatusEntry {
    pub line: usize,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitLineStatusReport {
    pub repo_root: String,
    pub path: String,
    pub relative_path: String,
    pub tracked: bool,
    pub start_line: usize,
    pub end_line: usize,
    pub statuses: Vec<GitLineStatusEntry>,
}

/// Get git status for a working directory.
/// Uses `git` CLI to avoid a heavy libgit2 dependency.
pub fn get_git_status(path: &str) -> GitInfo {
    let default = GitInfo {
        branch: None,
        is_dirty: false,
        ahead: 0,
        behind: 0,
        untracked: 0,
        modified: 0,
        staged: 0,
    };

    // Check if it's a git repo
    let branch = match Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(path)
        .output()
    {
        Ok(output) if output.status.success() => {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if s.is_empty() {
                return default;
            }
            s
        }
        _ => return default,
    };

    // Get porcelain status
    let status_output = Command::new("git")
        .args(["status", "--porcelain=v1", "--branch"])
        .current_dir(path)
        .output();

    let (mut untracked, mut modified, mut staged, mut ahead, mut behind) =
        (0u32, 0u32, 0u32, 0u32, 0u32);

    if let Ok(output) = status_output {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if line.starts_with("## ") {
                // Parse ahead/behind from branch line: ## main...origin/main [ahead 1, behind 2]
                if let Some(idx) = line.find("[ahead ") {
                    let rest = &line[idx + 7..];
                    if let Some(end) = rest.find(']').or_else(|| rest.find(',')) {
                        ahead = rest[..end].trim().parse().unwrap_or(0);
                    }
                }
                if let Some(idx) = line.find("behind ") {
                    let rest = &line[idx + 7..];
                    if let Some(end) = rest.find(']') {
                        behind = rest[..end].trim().parse().unwrap_or(0);
                    }
                }
            } else if line.starts_with("??") {
                untracked += 1;
            } else if line.len() >= 2 {
                let bytes = line.as_bytes();
                // First char = index status, second = work tree status
                if bytes[0] != b' ' && bytes[0] != b'?' {
                    staged += 1;
                }
                if bytes[1] != b' ' && bytes[1] != b'?' {
                    modified += 1;
                }
            }
        }
    }

    let is_dirty = untracked > 0 || modified > 0 || staged > 0;

    GitInfo {
        branch: Some(branch),
        is_dirty,
        ahead,
        behind,
        untracked,
        modified,
        staged,
    }
}

pub fn find_git_root(path: &str) -> Option<String> {
    let resolved = fs::canonicalize(path).unwrap_or_else(|_| std::path::PathBuf::from(path));
    let workdir = if resolved.is_file() {
        resolved
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or(resolved)
    } else {
        resolved
    };
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(&workdir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if root.is_empty() {
        None
    } else {
        Some(root)
    }
}

pub fn list_git_changes(repo_path: &str) -> Vec<GitChangeEntry> {
    let Some(repo_root) = find_git_root(repo_path) else {
        return Vec::new();
    };

    let output = Command::new("git")
        .args(["status", "--short", "--untracked-files=all"])
        .current_dir(&repo_root)
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_git_change_line)
        .collect()
}

pub fn git_diff(repo_path: &str, file_path: Option<&str>) -> String {
    let Some(repo_root) = find_git_root(repo_path) else {
        return String::new();
    };

    let Some(relative_path) = file_path.map(str::trim).filter(|value| !value.is_empty()) else {
        return command_stdout_lossy(
            Command::new("git")
                .args(["diff", "--no-ext-diff", "HEAD"])
                .current_dir(&repo_root),
        );
    };

    let absolute_file_path = std::path::Path::new(&repo_root).join(relative_path);
    let head_exists = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(&repo_root)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    let tracked = Command::new("git")
        .args(["ls-files", "--error-unmatch", "--", relative_path])
        .current_dir(&repo_root)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    if !tracked && absolute_file_path.exists() {
        return command_stdout_lossy(
            Command::new("git")
                .args([
                    "diff",
                    "--no-index",
                    "--no-ext-diff",
                    "--",
                    "/dev/null",
                    absolute_file_path.to_string_lossy().as_ref(),
                ])
                .current_dir(&repo_root),
        );
    }

    let args = if head_exists {
        vec!["diff", "--no-ext-diff", "HEAD", "--", relative_path]
    } else {
        vec!["diff", "--no-ext-diff", "--cached", "--", relative_path]
    };
    command_stdout_lossy(Command::new("git").args(args).current_dir(&repo_root))
}

pub fn read_file_preview(path: &str, max_bytes: usize) -> (String, bool, bool) {
    let Ok(bytes) = fs::read(path) else {
        return (String::new(), false, false);
    };
    let truncated = bytes.len() > max_bytes;
    let slice = if truncated {
        &bytes[..max_bytes]
    } else {
        &bytes[..]
    };
    match std::str::from_utf8(slice) {
        Ok(text) => (text.to_string(), truncated, true),
        Err(_) => (String::new(), truncated, false),
    }
}

pub fn get_git_line_statuses(
    path: &str,
    start_line: usize,
    limit: usize,
) -> Result<GitLineStatusReport> {
    let requested_start = start_line.max(1);
    let requested_limit = limit.max(1);
    let absolute_path =
        fs::canonicalize(path).with_context(|| format!("failed to resolve file path `{path}`"))?;
    let content = fs::read_to_string(&absolute_path)
        .with_context(|| format!("failed to read text file `{}`", absolute_path.display()))?;
    let total_lines = content.lines().count();
    let repo_root = find_git_root(
        absolute_path
            .parent()
            .unwrap_or_else(|| Path::new(path))
            .to_string_lossy()
            .as_ref(),
    )
    .ok_or_else(|| {
        anyhow::anyhow!(
            "path is not inside a git repository: {}",
            absolute_path.display()
        )
    })?;
    let relative_path = absolute_path
        .strip_prefix(&repo_root)
        .unwrap_or(absolute_path.as_path())
        .to_string_lossy()
        .replace('\\', "/");
    let tracked = is_tracked_path(&repo_root, &relative_path);
    let end_line = if total_lines == 0 || requested_start > total_lines {
        requested_start.saturating_sub(1)
    } else {
        requested_start
            .saturating_add(requested_limit.saturating_sub(1))
            .min(total_lines)
    };

    let mut statuses = if end_line >= requested_start {
        (requested_start..=end_line)
            .map(|line| GitLineStatusEntry {
                line,
                status: "unchanged".to_string(),
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    if !statuses.is_empty() {
        if !tracked {
            for entry in &mut statuses {
                entry.status = "added".to_string();
            }
        } else {
            let diff = git_diff_for_line_statuses(&repo_root, &relative_path, &absolute_path);
            apply_diff_line_statuses(&diff, requested_start, &mut statuses);
        }
    }

    Ok(GitLineStatusReport {
        repo_root,
        path: absolute_path.to_string_lossy().to_string(),
        relative_path,
        tracked,
        start_line: requested_start,
        end_line,
        statuses,
    })
}

fn parse_git_change_line(line: &str) -> Option<GitChangeEntry> {
    if line.trim().is_empty() || line.len() < 4 {
        return None;
    }

    let code = line[..2].to_string();
    let raw_path = line[3..].trim();
    if raw_path.is_empty() {
        return None;
    }

    let rename_parts: Vec<&str> = raw_path.split(" -> ").collect();
    let previous_path = if rename_parts.len() > 1 {
        Some(rename_parts[0].trim().to_string()).filter(|value| !value.is_empty())
    } else {
        None
    };
    let path = rename_parts
        .last()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;

    Some(GitChangeEntry {
        code: code.clone(),
        path,
        previous_path,
        kind: classify_git_status(&code).to_string(),
    })
}

fn classify_git_status(code: &str) -> &'static str {
    let compact = code.replace(' ', "");
    if compact == "??" {
        return "untracked";
    }
    if compact.contains('U') {
        return "conflict";
    }
    if compact.contains('R') {
        return "renamed";
    }
    if compact.contains('C') {
        return "copied";
    }
    if compact.contains('A') {
        return "added";
    }
    if compact.contains('D') {
        return "deleted";
    }
    "modified"
}

fn is_tracked_path(repo_root: &str, relative_path: &str) -> bool {
    Command::new("git")
        .args(["ls-files", "--error-unmatch", "--", relative_path])
        .current_dir(repo_root)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn git_diff_for_line_statuses(
    repo_root: &str,
    relative_path: &str,
    absolute_path: &Path,
) -> String {
    let head_exists = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(repo_root)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    if !head_exists {
        return command_stdout_lossy(
            Command::new("git")
                .args([
                    "diff",
                    "--no-ext-diff",
                    "--cached",
                    "--unified=0",
                    "--",
                    relative_path,
                ])
                .current_dir(repo_root),
        );
    }

    let tracked = is_tracked_path(repo_root, relative_path);
    if !tracked && absolute_path.exists() {
        return command_stdout_lossy(
            Command::new("git")
                .args([
                    "diff",
                    "--no-index",
                    "--no-ext-diff",
                    "--unified=0",
                    "--",
                    "/dev/null",
                    absolute_path.to_string_lossy().as_ref(),
                ])
                .current_dir(repo_root),
        );
    }

    command_stdout_lossy(
        Command::new("git")
            .args([
                "diff",
                "--no-ext-diff",
                "--unified=0",
                "HEAD",
                "--",
                relative_path,
            ])
            .current_dir(repo_root),
    )
}

fn apply_diff_line_statuses(diff: &str, start_line: usize, statuses: &mut [GitLineStatusEntry]) {
    for line in diff.lines() {
        let Some((old_count, new_start, new_count)) = parse_hunk_header(line) else {
            continue;
        };
        if new_count == 0 {
            continue;
        }

        let status = if old_count == 0 { "added" } else { "modified" };
        let hunk_end = new_start + new_count.saturating_sub(1);
        let window_end = start_line + statuses.len().saturating_sub(1);
        let overlap_start = new_start.max(start_line);
        let overlap_end = hunk_end.min(window_end);
        if overlap_start > overlap_end {
            continue;
        }

        for line_no in overlap_start..=overlap_end {
            if let Some(entry) = statuses.get_mut(line_no - start_line) {
                entry.status = status.to_string();
            }
        }
    }
}

fn parse_hunk_header(line: &str) -> Option<(usize, usize, usize)> {
    let body = line.strip_prefix("@@ -")?;
    let (ranges, _) = body.split_once(" @@")?;
    let (old_range, new_range) = ranges.split_once(" +")?;
    let (_, old_count) = parse_hunk_range(old_range)?;
    let (new_start, new_count) = parse_hunk_range(new_range)?;
    Some((old_count, new_start, new_count))
}

fn parse_hunk_range(raw: &str) -> Option<(usize, usize)> {
    if let Some((start, count)) = raw.split_once(',') {
        Some((start.parse().ok()?, count.parse().ok()?))
    } else {
        Some((raw.parse().ok()?, 1))
    }
}

fn command_stdout_lossy(command: &mut Command) -> String {
    command
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
        .unwrap_or_default()
}
