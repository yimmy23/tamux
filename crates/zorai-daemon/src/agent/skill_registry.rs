//! Community skill registry client.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use zorai_protocol::CommunitySkillEntry;

use super::skill_community::PublishMetadata;

const SEARCH_TIMEOUT_SECS: u64 = 30;
const FETCH_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Clone)]
pub(crate) struct RegistryClient {
    http: reqwest::Client,
    registry_url: String,
    pub(crate) cache_dir: PathBuf,
    pub(crate) index_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) struct RegistryIndex {
    pub version: u32,
    pub updated_at: u64,
    pub skills: Vec<RegistrySkillEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct RegistrySkillEntry {
    pub name: String,
    pub description: String,
    pub version: String,
    pub publisher_id: String,
    pub publisher_verified: bool,
    pub success_rate: f64,
    pub use_count: u32,
    pub content_hash: String,
    pub zorai_version: String,
    pub maturity_at_publish: String,
    pub tags: Vec<String>,
    pub published_at: u64,
}

impl RegistryClient {
    pub(crate) fn new(registry_url: String, data_dir: &Path) -> Self {
        let registry_dir = data_dir.join("registry");
        let cache_dir = registry_dir.join("cache");
        let _ = std::fs::create_dir_all(&cache_dir);

        Self {
            http: reqwest::Client::new(),
            registry_url: registry_url.trim_end_matches('/').to_string(),
            cache_dir,
            index_path: registry_dir.join("index.json"),
        }
    }

    pub(crate) async fn search(&self, query: &str) -> Result<Vec<RegistrySkillEntry>> {
        let index = match self.load_index().await {
            Ok(index) => index,
            Err(_) => return Ok(Vec::new()),
        };

        let query = query.trim().to_ascii_lowercase();
        if query.is_empty() {
            return Ok(index.skills);
        }

        Ok(index
            .skills
            .into_iter()
            .filter(|entry| entry_matches_query(entry, &query))
            .collect())
    }

    pub(crate) async fn refresh_index(&self) -> Result<()> {
        let url = format!("{}/index.json", self.registry_url);
        let bytes =
            tokio::time::timeout(std::time::Duration::from_secs(SEARCH_TIMEOUT_SECS), async {
                let response = self.http.get(&url).send().await?;
                let response = response.error_for_status()?;
                response.bytes().await
            })
            .await
            .context("registry index request timed out")??;

        if let Some(parent) = self.index_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("create registry dir {}", parent.display()))?;
        }
        tokio::fs::write(&self.index_path, bytes)
            .await
            .with_context(|| format!("write registry index {}", self.index_path.display()))?;
        Ok(())
    }

    pub(crate) async fn fetch_skill(&self, name: &str) -> Result<PathBuf> {
        tokio::fs::create_dir_all(&self.cache_dir)
            .await
            .with_context(|| format!("create cache dir {}", self.cache_dir.display()))?;

        let url = format!("{}/skills/{}.tar.gz", self.registry_url, name);
        let output_path = self.cache_dir.join(format!("{}.tar.gz", name));
        let bytes =
            tokio::time::timeout(std::time::Duration::from_secs(FETCH_TIMEOUT_SECS), async {
                let response = self.http.get(&url).send().await?;
                let response = response.error_for_status()?;
                response.bytes().await
            })
            .await
            .context("registry skill download timed out")??;

        tokio::fs::write(&output_path, bytes)
            .await
            .with_context(|| format!("write skill archive {}", output_path.display()))?;
        Ok(output_path)
    }

    pub(crate) async fn publish_skill(
        &self,
        tarball_path: &Path,
        metadata: &PublishMetadata,
    ) -> Result<()> {
        let token = std::env::var("ZORAI_REGISTRY_TOKEN")
            .or_else(|_| std::env::var("REGISTRY_TOKEN"))
            .context("registry publish requires ZORAI_REGISTRY_TOKEN or REGISTRY_TOKEN")?;
        let tarball_bytes = tokio::fs::read(tarball_path)
            .await
            .with_context(|| format!("read tarball {}", tarball_path.display()))?;
        let metadata_json =
            serde_json::to_string(metadata).context("serialize publish metadata")?;
        let filename = tarball_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("skill.tar.gz")
            .to_string();

        let form = reqwest::multipart::Form::new()
            .part(
                "tarball",
                reqwest::multipart::Part::bytes(tarball_bytes)
                    .file_name(filename)
                    .mime_str("application/gzip")?,
            )
            .text("metadata", metadata_json);

        let url = format!("{}/skills", self.registry_url);
        tokio::time::timeout(std::time::Duration::from_secs(FETCH_TIMEOUT_SECS), async {
            let response = self
                .http
                .post(&url)
                .bearer_auth(token)
                .multipart(form)
                .send()
                .await?;
            response.error_for_status()?;
            Ok::<(), anyhow::Error>(())
        })
        .await
        .context("registry publish request timed out")??;
        Ok(())
    }

    async fn load_index(&self) -> Result<RegistryIndex> {
        let content = tokio::fs::read_to_string(&self.index_path)
            .await
            .with_context(|| format!("read registry index {}", self.index_path.display()))?;
        serde_json::from_str(&content).context("parse registry index json")
    }
}

pub(crate) fn to_community_entry(entry: &RegistrySkillEntry) -> CommunitySkillEntry {
    CommunitySkillEntry {
        name: entry.name.clone(),
        description: entry.description.clone(),
        version: entry.version.clone(),
        publisher_id: entry.publisher_id.clone(),
        publisher_verified: entry.publisher_verified,
        success_rate: entry.success_rate,
        use_count: entry.use_count,
        content_hash: entry.content_hash.clone(),
        zorai_version: entry.zorai_version.clone(),
        maturity_at_publish: entry.maturity_at_publish.clone(),
        tags: entry.tags.clone(),
        published_at: entry.published_at,
    }
}

fn entry_matches_query(entry: &RegistrySkillEntry, query: &str) -> bool {
    entry.name.to_ascii_lowercase().contains(query)
        || entry.description.to_ascii_lowercase().contains(query)
        || entry
            .tags
            .iter()
            .any(|tag| tag.to_ascii_lowercase().contains(query))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(name: &str, description: &str, tags: &[&str]) -> RegistrySkillEntry {
        RegistrySkillEntry {
            name: name.to_string(),
            description: description.to_string(),
            version: "1.0.0".to_string(),
            publisher_id: "publisher-1".to_string(),
            publisher_verified: false,
            success_rate: 0.8,
            use_count: 12,
            content_hash: "abc123".to_string(),
            zorai_version: "0.1.10".to_string(),
            maturity_at_publish: "proven".to_string(),
            tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
            published_at: 1,
        }
    }

    #[tokio::test]
    async fn registry_search_filters_cached_entries_case_insensitively() {
        let temp = tempfile::tempdir().expect("tempdir");
        let client = RegistryClient::new("https://registry.zorai.dev".to_string(), temp.path());
        let index = RegistryIndex {
            version: 1,
            updated_at: 42,
            skills: vec![
                sample_entry("git-helper", "Git workflow assistant", &["git", "workflow"]),
                sample_entry("sql-helper", "SQL inspection helper", &["database"]),
            ],
        };

        std::fs::write(
            &client.index_path,
            serde_json::to_vec_pretty(&index).expect("serialize index"),
        )
        .expect("write index");

        let matches = client.search("WORKFLOW").await.expect("search succeeds");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "git-helper");

        let no_matches = client.search("python").await.expect("search succeeds");
        assert!(no_matches.is_empty());
    }

    #[test]
    fn registry_index_round_trips_through_json() {
        let index = RegistryIndex {
            version: 1,
            updated_at: 42,
            skills: vec![sample_entry(
                "git-helper",
                "Git workflow assistant",
                &["git"],
            )],
        };

        let encoded = serde_json::to_string(&index).expect("encode index");
        let decoded: RegistryIndex = serde_json::from_str(&encoded).expect("decode index");
        assert_eq!(decoded, index);
    }

    #[test]
    fn registry_entry_converts_to_protocol_entry() {
        let entry = sample_entry("git-helper", "Git workflow assistant", &["git"]);
        let converted = to_community_entry(&entry);

        assert_eq!(converted.name, entry.name);
        assert_eq!(converted.publisher_verified, entry.publisher_verified);
        assert_eq!(converted.tags, entry.tags);
    }
}
