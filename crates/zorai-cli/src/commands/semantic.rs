use anyhow::{bail, Result};
use zorai_protocol::{
    SemanticDocumentIndexSyncResultPublic, SemanticIndexRepairResultPublic,
    SkillDiscoveryResultPublic,
};

use crate::cli::{SemanticAction, SemanticRerankKind};
use crate::client;

pub(crate) async fn run(action: SemanticAction) -> Result<()> {
    match action {
        SemanticAction::Status { json } => {
            let config = client::send_config_get().await?;
            let status_config = semantic_status_config(&config);
            let status = client::send_semantic_index_status(
                &status_config.embedding_model,
                status_config.dimensions,
            )
            .await?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "embedding_enabled": status_config.embedding_enabled,
                        "embedding_model": status_config.embedding_model,
                        "dimensions": status_config.dimensions,
                        "index": status,
                    }))?
                );
            } else {
                println!("{}", render_semantic_status(&status_config, &status));
            }
        }
        SemanticAction::RepairIndex { yes, json } => {
            if !yes {
                bail!("semantic index repair requires --yes because it moves the local vector index and queues a rebuild");
            }
            let result = client::send_semantic_index_repair(true).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("{}", render_semantic_repair(&result));
            }
        }
        SemanticAction::Sync { json } => {
            let result = client::send_semantic_document_sync().await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("{}", render_semantic_sync(&result));
            }
        }
        SemanticAction::Rerank {
            kind,
            query,
            limit,
            json,
        } => {
            let limit = limit.clamp(1, 20);
            let result = match kind {
                SemanticRerankKind::Skill => {
                    client::send_skill_discover(&query, None, limit, None).await?
                }
                SemanticRerankKind::Guideline => {
                    client::send_guideline_discover(&query, None, limit, None).await?
                }
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("{}", render_semantic_rerank(kind, &result));
            }
        }
    }
    Ok(())
}

fn render_semantic_repair(result: &SemanticIndexRepairResultPublic) -> String {
    let mut lines = vec![
        "Semantic index repair completed.".to_string(),
        format!("Removed vector index: {}", result.removed_vector_index),
        format!("Cleared completions: {}", result.cleared_completions),
        format!("Cleared deletions: {}", result.cleared_deletions),
        format!("Reset failed jobs: {}", result.reset_failed_jobs),
    ];
    if let Some(path) = result.backup_path.as_deref() {
        lines.push(format!("Backup: {path}"));
    }
    lines.push("Run `zorai semantic status` to watch the rebuild queue drain.".to_string());
    lines.join("\n")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SemanticStatusConfig {
    embedding_enabled: bool,
    embedding_model: String,
    dimensions: u32,
}

fn semantic_status_config(config: &serde_json::Value) -> SemanticStatusConfig {
    let embedding = config
        .get("semantic")
        .and_then(|semantic| semantic.get("embedding"));
    SemanticStatusConfig {
        embedding_enabled: embedding
            .and_then(|value| value.get("enabled"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        embedding_model: embedding
            .and_then(|value| value.get("model"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        dimensions: embedding
            .and_then(|value| value.get("dimensions"))
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(0),
    }
}

fn render_semantic_status(
    config: &SemanticStatusConfig,
    status: &client::SemanticIndexStatus,
) -> String {
    let mut lines = vec![
        "Semantic index status".to_string(),
        format!(
            "Embedding: {}",
            if config.embedding_enabled {
                "enabled"
            } else {
                "disabled"
            }
        ),
        format!(
            "Model: {} ({} dimensions)",
            display_or_none(&config.embedding_model),
            config.dimensions
        ),
        format!("Queued jobs: {}", status.queued_jobs),
        format!("Pending for model: {}", status.pending_for_model),
        format!("Completed for model: {}", status.completed_for_model),
        format!("Queued deletions: {}", status.queued_deletions),
        format!("Failed jobs: {}", status.failed_jobs),
        format!("Failed deletions: {}", status.failed_deletions),
    ];

    if !config.embedding_enabled || config.embedding_model.is_empty() {
        lines.push("Document sync can discover files, but embeddings will not be processed until semantic embeddings are enabled with a model.".to_string());
    }

    lines.join("\n")
}

fn render_semantic_sync(result: &SemanticDocumentIndexSyncResultPublic) -> String {
    let mut lines = vec![
        "Semantic document sync completed.".to_string(),
        format!(
            "Embedding model: {} ({} dimensions)",
            display_or_none(&result.embedding_model),
            result.dimensions
        ),
        format!(
            "Skills: discovered {} | changed {} | queued embeddings {} | removed {}",
            result.skills.discovered,
            result.skills.changed,
            result.skills.queued_embeddings,
            result.skills.removed
        ),
        format!(
            "Guidelines: discovered {} | changed {} | queued embeddings {} | removed {}",
            result.guidelines.discovered,
            result.guidelines.changed,
            result.guidelines.queued_embeddings,
            result.guidelines.removed
        ),
    ];

    if result.embedding_model.trim().is_empty() {
        lines.push(
            "Semantic embeddings are disabled or no embedding model is configured.".to_string(),
        );
    }

    lines.join("\n")
}

fn render_semantic_rerank(kind: SemanticRerankKind, result: &SkillDiscoveryResultPublic) -> String {
    let label = match kind {
        SemanticRerankKind::Skill => "skills",
        SemanticRerankKind::Guideline => "guidelines",
    };
    let mut lines = vec![
        format!("Semantic rerank for {label}: {}", result.query),
        format!("Confidence: {}", display_or_none(&result.confidence_tier)),
        format!(
            "Next action: {}",
            display_or_none(&result.recommended_action)
        ),
    ];

    if result.candidates.is_empty() {
        lines.push(format!("No matching {label} found."));
        return lines.join("\n");
    }

    for (index, candidate) in result.candidates.iter().enumerate() {
        lines.push(format!(
            "{}. {} ({}) score {:.2}",
            index + 1,
            display_or_none(&candidate.skill_name),
            display_or_none(&candidate.relative_path),
            candidate.score
        ));
        if !candidate.reasons.is_empty() {
            lines.push(format!("   Reasons: {}", candidate.reasons.join(", ")));
        }
        if !candidate.recommended_action.is_empty() {
            lines.push(format!("   Next: {}", candidate.recommended_action));
        }
    }

    lines.join("\n")
}

fn display_or_none(value: &str) -> &str {
    if value.trim().is_empty() {
        "-"
    } else {
        value
    }
}
