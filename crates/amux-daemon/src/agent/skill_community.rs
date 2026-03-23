//! Community skill packaging and format conversion.

use std::fs::{self, File};
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tar::{Archive, Builder};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SkillFormat {
    TamuxNative,
    AgentSkillsIo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TamuxSkillFrontmatter {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_yaml::Value>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub tamux: TamuxExtensions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentSkillsFrontmatter {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_yaml::Value>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TamuxExtensions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maturity_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_hash: Option<String>,
    #[serde(default)]
    pub context_tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_trace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success_rate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PublishMetadata {
    pub publisher_id: String,
    pub origin_trace_summary: String,
    pub success_rate: f64,
    pub use_count: u32,
    pub created_at: u64,
    pub content_hash: String,
    pub tamux_version: String,
    pub maturity_at_publish: String,
}

pub(super) fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let rest = content.strip_prefix("---\n")?;
    let split_at = rest.find("\n---\n")?;
    let frontmatter = &rest[..split_at];
    let body = &rest[split_at + 5..];
    Some((frontmatter, body))
}

pub(super) fn detect_skill_format(frontmatter: &str) -> SkillFormat {
    if frontmatter.lines().any(|line| line.trim() == "tamux:") {
        SkillFormat::TamuxNative
    } else {
        SkillFormat::AgentSkillsIo
    }
}

pub(super) fn sanitize_name_for_agentskills(name: &str) -> String {
    let mut normalized = name.to_ascii_lowercase().replace('_', "-");
    while normalized.contains("--") {
        normalized = normalized.replace("--", "-");
    }
    normalized.trim_matches('-').to_string()
}

pub(super) fn to_agentskills_format(tamux: &TamuxSkillFrontmatter, body: &str) -> String {
    let export = AgentSkillsFrontmatter {
        name: sanitize_name_for_agentskills(&tamux.name),
        description: tamux.description.clone(),
        license: tamux.license.clone(),
        compatibility: tamux.compatibility.clone(),
        metadata: tamux.metadata.clone(),
        allowed_tools: tamux.allowed_tools.clone(),
    };
    let frontmatter = serde_yaml::to_string(&export).expect("serialize agentskills frontmatter");
    format!("---\n{}---\n{}", frontmatter, body)
}

pub(super) fn from_agentskills_format(content: &str) -> Result<TamuxSkillFrontmatter> {
    let (frontmatter, _) =
        split_frontmatter(content).ok_or_else(|| anyhow!("missing YAML frontmatter"))?;
    let parsed: AgentSkillsFrontmatter =
        serde_yaml::from_str(frontmatter).context("parse agentskills.io frontmatter")?;

    Ok(TamuxSkillFrontmatter {
        name: parsed.name,
        description: parsed.description,
        license: parsed.license,
        compatibility: parsed.compatibility,
        metadata: parsed.metadata,
        allowed_tools: parsed.allowed_tools,
        tamux: TamuxExtensions::default(),
    })
}

pub(super) fn content_hash(content: &str) -> String {
    hex_sha256(content.as_bytes())
}

pub(super) fn publisher_id(machine_id: &str) -> String {
    hex_sha256(format!("tamux-publisher:{machine_id}").as_bytes())[..16].to_string()
}

pub(super) fn pack_skill(skill_dir: &Path, output: &Path) -> Result<()> {
    let tar_gz =
        File::create(output).with_context(|| format!("create archive {}", output.display()))?;
    let encoder = GzEncoder::new(tar_gz, Compression::default());
    let mut builder = Builder::new(encoder);

    for entry in fs::read_dir(skill_dir)
        .with_context(|| format!("read skill dir {}", skill_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let name = path
                .file_name()
                .ok_or_else(|| anyhow!("invalid file name in {}", path.display()))?;
            builder
                .append_path_with_name(&path, name)
                .with_context(|| format!("append {} to archive", path.display()))?;
        }
    }

    let encoder = builder.into_inner().context("finish tar archive")?;
    encoder.finish().context("finish gzip archive")?;
    Ok(())
}

pub(super) fn unpack_skill(archive: &Path, target_dir: &Path) -> Result<()> {
    fs::create_dir_all(target_dir)
        .with_context(|| format!("create target dir {}", target_dir.display()))?;
    let tar_gz =
        File::open(archive).with_context(|| format!("open archive {}", archive.display()))?;
    let decoder = GzDecoder::new(tar_gz);
    let mut archive = Archive::new(decoder);
    archive
        .unpack(target_dir)
        .with_context(|| format!("unpack archive into {}", target_dir.display()))?;
    Ok(())
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn split_frontmatter_returns_frontmatter_and_body() {
        let result = split_frontmatter("---\nname: test\n---\nbody");
        assert_eq!(result, Some(("name: test", "body")));
    }

    #[test]
    fn split_frontmatter_returns_none_without_frontmatter() {
        assert_eq!(split_frontmatter("no frontmatter"), None);
    }

    #[test]
    fn detect_skill_format_distinguishes_tamux_and_agentskills() {
        assert_eq!(
            detect_skill_format("name: demo\ntamux:\n  variant_id: abc"),
            SkillFormat::TamuxNative
        );
        assert_eq!(
            detect_skill_format("name: demo\ndescription: sample"),
            SkillFormat::AgentSkillsIo
        );
    }

    #[test]
    fn sanitize_name_for_agentskills_normalizes_variant_suffix_and_separators() {
        assert_eq!(
            sanitize_name_for_agentskills("debug_rust--async"),
            "debug-rust-async"
        );
    }

    #[test]
    fn content_hash_is_stable_sha256_hex() {
        let first = content_hash("hello world");
        let second = content_hash("hello world");

        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn publisher_id_is_stable_truncated_sha256_hex() {
        let first = publisher_id("machine-123");
        let second = publisher_id("machine-123");

        assert_eq!(first, second);
        assert_eq!(first.len(), 16);
        assert!(first.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn to_agentskills_format_strips_tamux_extensions() {
        let skill = TamuxSkillFrontmatter {
            name: "debug_rust--async".to_string(),
            description: Some("Debug async Rust".to_string()),
            license: Some("MIT".to_string()),
            compatibility: Some(vec!["tamux>=0.1".to_string()]),
            metadata: Some(serde_yaml::from_str("category: debugging").expect("metadata yaml")),
            allowed_tools: vec!["read_file".to_string()],
            tamux: TamuxExtensions {
                maturity_status: Some("draft".to_string()),
                provenance_hash: Some("hash".to_string()),
                context_tags: vec!["rust".to_string()],
                variant_id: Some("variant-1".to_string()),
                origin_trace: Some("trace".to_string()),
                success_rate: Some(0.9),
                use_count: Some(12),
            },
        };

        let exported = to_agentskills_format(&skill, "Body");
        let (frontmatter, body) = split_frontmatter(&exported).expect("frontmatter present");

        assert_eq!(body, "Body");
        assert_eq!(detect_skill_format(frontmatter), SkillFormat::AgentSkillsIo);
        assert!(!frontmatter.contains("tamux:"));
        assert!(frontmatter.contains("allowed_tools:"));
    }

    #[test]
    fn from_agentskills_format_adds_default_tamux_extensions() {
        let imported = from_agentskills_format(
            "---\nname: debug-rust\ndescription: Debug async rust\nallowed_tools:\n  - read_file\n---\nBody",
        )
        .expect("agentskills import succeeds");

        assert_eq!(imported.name, "debug-rust");
        assert_eq!(imported.allowed_tools, vec!["read_file".to_string()]);
        assert_eq!(imported.tamux.context_tags, Vec::<String>::new());
        assert!(imported.tamux.maturity_status.is_none());
        assert!(imported.tamux.variant_id.is_none());
    }

    #[test]
    fn pack_and_unpack_skill_round_trip_tarball_contents() {
        let temp = tempfile::tempdir().expect("tempdir");
        let skill_dir = temp.path().join("skill");
        fs::create_dir_all(&skill_dir).expect("create skill dir");
        fs::write(skill_dir.join("SKILL.md"), "---\nname: demo\n---\nBody").expect("write skill");

        let archive = temp.path().join("demo.tar.gz");
        pack_skill(&skill_dir, &archive).expect("pack succeeds");
        assert!(archive.exists());

        let unpacked = temp.path().join("unpacked");
        unpack_skill(&archive, &unpacked).expect("unpack succeeds");

        let extracted = unpacked.join("SKILL.md");
        assert_eq!(
            fs::read_to_string(extracted).expect("read extracted"),
            "---\nname: demo\n---\nBody"
        );
    }
}
