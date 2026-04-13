use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_yaml::Value;

use crate::agent::skill_recommendation::extract_skill_metadata;

use super::types::SkillMeshDocument;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SkillMeshCompileMode {
    Deterministic,
    ProviderAssisted,
}

#[derive(Debug, Clone)]
pub struct SkillMeshCompileContext {
    pub mode: SkillMeshCompileMode,
    pub compile_version: u32,
    pub source_kind: String,
    pub trust_tier: String,
    pub provenance: String,
    pub risk_level: String,
}

#[allow(dead_code)]
pub fn sample_compile_context_for_tests(mode: SkillMeshCompileMode) -> SkillMeshCompileContext {
    SkillMeshCompileContext {
        mode,
        compile_version: 1,
        source_kind: "builtin".to_string(),
        trust_tier: "trusted".to_string(),
        provenance: "test".to_string(),
        risk_level: "low".to_string(),
    }
}

pub async fn compile_skill_document(
    source_path: PathBuf,
    content: &str,
    context: SkillMeshCompileContext,
) -> Result<SkillMeshDocument> {
    let relative_path = source_path.to_string_lossy().replace('\\', "/");
    let metadata = extract_skill_metadata(&relative_path, content);
    let frontmatter = parse_frontmatter(content);
    let skill_name = frontmatter
        .as_ref()
        .and_then(|value| frontmatter_string(value, "name"))
        .unwrap_or_else(|| derive_skill_name(&source_path));
    let summary = metadata.summary.clone();
    let capability_path = infer_capability_path(&source_path, &metadata.keywords);
    let synthetic_queries = build_synthetic_queries(
        &skill_name,
        summary.as_deref(),
        &metadata.triggers,
        &metadata.keywords,
    );
    let required_tools = metadata.keywords.clone();
    let required_platforms = vec!["linux".to_string()];
    let required_env_hints = metadata.triggers.clone();
    let workspace_affinities = metadata.keywords.clone();
    let source_kind = context.source_kind;
    let trust_tier = context.trust_tier;
    let provenance = context.provenance;
    let risk_level = context.risk_level;
    let content_hash = format!("compile-v{}:{}", context.compile_version, relative_path);

    let document = SkillMeshDocument {
        skill_id: skill_name.clone(),
        variant_id: None,
        skill_name,
        variant_name: None,
        source_path: relative_path,
        source_kind,
        content_hash,
        compile_version: context.compile_version,
        summary,
        capability_path,
        synthetic_queries,
        explicit_trigger_phrases: metadata.triggers,
        workspace_affinities,
        required_tools,
        required_platforms,
        required_env_hints,
        security_risk_level: risk_level,
        trust_tier,
        provenance,
        use_count: 0,
        success_count: 0,
        failure_count: 0,
        dismiss_count: 0,
        negative_feedback_weight: 0.0,
        embedding_records: Vec::new(),
    };

    if matches!(context.mode, SkillMeshCompileMode::ProviderAssisted) {
        // Provider-assisted compilation lands later; v1 stays deterministic when no provider is wired.
    }

    Ok(document)
}

fn parse_frontmatter(content: &str) -> Option<Value> {
    let mut lines = content.lines();
    if !matches!(lines.next().map(str::trim), Some("---")) {
        return None;
    }

    let mut yaml = Vec::new();
    for line in content.lines().skip(1) {
        if line.trim() == "---" {
            break;
        }
        yaml.push(line);
    }

    serde_yaml::from_str::<Value>(&yaml.join("\n")).ok()
}

fn frontmatter_string(frontmatter: &Value, key: &str) -> Option<String> {
    frontmatter
        .as_mapping()
        .and_then(|mapping| mapping.get(Value::String(key.to_string()).borrow()))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn derive_skill_name(source_path: &Path) -> String {
    source_path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|value| value.to_str())
        .unwrap_or("skill")
        .to_string()
}

fn infer_capability_path(source_path: &Path, keywords: &[String]) -> Vec<String> {
    let mut segments = source_path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|segment| *segment != "SKILL.md" && *segment != "skills")
        .map(|segment| segment.to_string())
        .collect::<Vec<_>>();

    if segments
        .last()
        .is_some_and(|segment| segment.eq_ignore_ascii_case("skill.md"))
    {
        segments.pop();
    }
    if segments.is_empty() {
        segments.push("general".to_string());
    }
    if segments.len() == 1 && !keywords.is_empty() {
        segments.extend(keywords.iter().take(2).cloned());
    }
    segments
}

fn build_synthetic_queries(
    skill_name: &str,
    summary: Option<&str>,
    triggers: &[String],
    keywords: &[String],
) -> Vec<String> {
    let mut queries: Vec<String> = Vec::new();
    if let Some(summary) = summary {
        queries.push(summary.to_ascii_lowercase());
    }
    queries.extend(
        triggers
            .iter()
            .map(|trigger| format!("help with {trigger}")),
    );
    queries.extend(keywords.iter().map(|keyword| format!("{keyword} workflow")));
    if queries.is_empty() {
        queries.push(format!("use {skill_name}"));
    }
    queries.sort();
    queries.dedup();
    queries
}

trait BorrowYamlKey {
    fn borrow(&self) -> &Self;
}

impl BorrowYamlKey for Value {
    fn borrow(&self) -> &Self {
        self
    }
}
