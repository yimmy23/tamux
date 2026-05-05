use anyhow::Result;
use zorai_protocol::{SemanticDocumentIndexSyncResultPublic, SkillDiscoveryResultPublic};

use crate::cli::{SemanticAction, SemanticRerankKind};
use crate::client;

pub(crate) async fn run(action: SemanticAction) -> Result<()> {
    match action {
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
