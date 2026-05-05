use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

const GITHUB_GUIDELINES_TREE_URL: &str =
    "https://api.github.com/repos/mkurman/zorai/git/trees/main?recursive=1";
const GITHUB_RAW_MAIN_URL: &str = "https://raw.githubusercontent.com/mkurman/zorai/main";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RemoteGuidelineDocument {
    pub(super) relative_path: String,
    pub(super) content: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct GuidelineSyncSummary {
    pub(super) installed: usize,
    pub(super) skipped_existing: usize,
    pub(super) overwritten: usize,
}

#[derive(Debug, Deserialize)]
struct GitTreeResponse {
    tree: Vec<GitTreeItem>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Debug, Deserialize)]
struct GitTreeItem {
    path: String,
    #[serde(rename = "type")]
    kind: String,
}

pub(super) async fn fetch_remote_guideline_documents() -> Result<Vec<RemoteGuidelineDocument>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to create guideline sync HTTP client")?;
    let user_agent = format!("zorai-cli/{}", env!("CARGO_PKG_VERSION"));

    let response = client
        .get(GITHUB_GUIDELINES_TREE_URL)
        .header(reqwest::header::USER_AGENT, &user_agent)
        .send()
        .await
        .context("failed to query zorai guideline tree")?
        .error_for_status()
        .context("zorai guideline tree request returned an error status")?;

    let tree = response
        .json::<GitTreeResponse>()
        .await
        .context("failed to parse zorai guideline tree response")?;
    if tree.truncated {
        anyhow::bail!("zorai guideline tree response was truncated by GitHub");
    }

    let mut paths = tree
        .tree
        .into_iter()
        .filter(|item| item.kind == "blob")
        .filter_map(|item| {
            let relative = item.path.strip_prefix("guidelines/")?;
            if relative.trim().is_empty() {
                return None;
            }
            let relative_path = relative.to_string();
            Some((item.path, relative_path))
        })
        .collect::<Vec<_>>();
    paths.sort_by(|(_, left), (_, right)| left.cmp(right));

    let mut documents = Vec::with_capacity(paths.len());
    for (repo_path, relative_path) in paths {
        let raw_url = format!("{GITHUB_RAW_MAIN_URL}/{}", percent_encode_path(&repo_path));
        let content = client
            .get(&raw_url)
            .header(reqwest::header::USER_AGENT, &user_agent)
            .send()
            .await
            .with_context(|| format!("failed to download guideline {repo_path}"))?
            .error_for_status()
            .with_context(|| format!("guideline download returned an error status: {repo_path}"))?
            .bytes()
            .await
            .with_context(|| format!("failed to read guideline response: {repo_path}"))?
            .to_vec();
        documents.push(RemoteGuidelineDocument {
            relative_path,
            content,
        });
    }

    if documents.is_empty() {
        anyhow::bail!("no guideline files were found in the zorai repository");
    }
    Ok(documents)
}

pub(super) fn sync_guideline_documents(
    guidelines_root: &Path,
    documents: &[RemoteGuidelineDocument],
    force: bool,
) -> Result<GuidelineSyncSummary> {
    std::fs::create_dir_all(guidelines_root).with_context(|| {
        format!(
            "failed to create guidelines directory {}",
            guidelines_root.display()
        )
    })?;

    let mut summary = GuidelineSyncSummary {
        installed: 0,
        skipped_existing: 0,
        overwritten: 0,
    };

    for document in documents {
        let relative = validate_remote_guideline_path(&document.relative_path)?;
        let destination = guidelines_root.join(relative);
        let exists = destination.exists();
        if exists && !force {
            summary.skipped_existing += 1;
            continue;
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create guideline directory {}", parent.display())
            })?;
        }
        std::fs::write(&destination, &document.content)
            .with_context(|| format!("failed to write guideline {}", destination.display()))?;
        if exists {
            summary.overwritten += 1;
        } else {
            summary.installed += 1;
        }
    }

    Ok(summary)
}

fn validate_remote_guideline_path(value: &str) -> Result<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        anyhow::bail!("remote guideline path must not be empty");
    }
    if trimmed.contains('\\') {
        anyhow::bail!("remote guideline path must use forward slashes: {trimmed}");
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        anyhow::bail!("remote guideline path must be relative: {trimmed}");
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => anyhow::bail!("remote guideline path must not escape guidelines root: {trimmed}"),
        }
    }

    Ok(path.to_path_buf())
}

fn percent_encode_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_guidelines_rejects_paths_that_escape_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let documents = vec![RemoteGuidelineDocument {
            relative_path: "../outside.md".to_string(),
            content: b"# Outside\n".to_vec(),
        }];

        let error = sync_guideline_documents(temp.path(), &documents, true)
            .expect_err("path traversal should be rejected");

        assert!(error.to_string().contains("must not escape"));
    }

    #[test]
    fn sync_guidelines_accepts_support_files_under_guidelines_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let documents = vec![RemoteGuidelineDocument {
            relative_path: "references/schema.json".to_string(),
            content: b"{\"type\":\"object\"}\n".to_vec(),
        }];

        let summary = sync_guideline_documents(temp.path(), &documents, false).expect("sync");

        assert_eq!(summary.installed, 1);
        assert_eq!(
            std::fs::read_to_string(temp.path().join("references/schema.json")).expect("read"),
            "{\"type\":\"object\"}\n"
        );
    }
}
