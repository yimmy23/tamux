use amux_protocol::{GitChangeEntry, GitInfo};
use std::fs;
use std::process::Command;

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
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(&resolved)
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

fn command_stdout_lossy(command: &mut Command) -> String {
    command
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
        .unwrap_or_default()
}
