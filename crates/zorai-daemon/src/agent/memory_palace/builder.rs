use std::path::Path;

use crate::agent::context::structural_memory::{
    MemoryGraphEdgeUpsert, MemoryGraphNodeUpsert, MemoryGraphUpdateBatch, ThreadStructuralMemory,
};
use crate::agent::semantic_env::SemanticPackageSummary;

fn node_id_for_relative_path(relative_path: &str) -> String {
    format!("node:file:{}", relative_path.replace('\\', "/"))
}

fn node_id_for_package(ecosystem: &str, package_name: &str) -> String {
    format!("node:package:{ecosystem}:{package_name}")
}

fn inferred_node_from_id(node_id: &str) -> MemoryGraphNodeUpsert {
    if let Some(relative_path) = node_id.strip_prefix("node:file:") {
        return MemoryGraphNodeUpsert {
            id: node_id.to_string(),
            label: relative_path.to_string(),
            node_type: "file".to_string(),
            summary_text: Some("structural path inferred from thread memory".to_string()),
        };
    }

    if let Some(task_id) = node_id.strip_prefix("node:task:") {
        return MemoryGraphNodeUpsert {
            id: node_id.to_string(),
            label: task_id.to_string(),
            node_type: "task".to_string(),
            summary_text: Some("task anchor inferred from thread memory".to_string()),
        };
    }

    if let Some(thread_id) = node_id.strip_prefix("node:thread:") {
        return MemoryGraphNodeUpsert {
            id: node_id.to_string(),
            label: thread_id.to_string(),
            node_type: "thread".to_string(),
            summary_text: Some("thread anchor inferred from thread memory".to_string()),
        };
    }

    if let Some(label) = node_id.strip_prefix("node:error:") {
        return MemoryGraphNodeUpsert {
            id: node_id.to_string(),
            label: label.to_string(),
            node_type: "error".to_string(),
            summary_text: Some("error node inferred from thread memory".to_string()),
        };
    }

    if let Some(package_key) = node_id.strip_prefix("node:package:") {
        let label = package_key
            .rsplit(':')
            .next()
            .unwrap_or(package_key)
            .to_string();
        return MemoryGraphNodeUpsert {
            id: node_id.to_string(),
            label,
            node_type: "package".to_string(),
            summary_text: Some("package inferred from thread memory".to_string()),
        };
    }

    MemoryGraphNodeUpsert {
        id: node_id.to_string(),
        label: node_id.to_string(),
        node_type: "memory".to_string(),
        summary_text: Some("memory graph node inferred from thread memory".to_string()),
    }
}

pub(crate) fn build_memory_palace_update_batch(
    thread_id: Option<&str>,
    task_id: Option<&str>,
    structural_memory: Option<&ThreadStructuralMemory>,
    semantic_packages: &[SemanticPackageSummary],
) -> MemoryGraphUpdateBatch {
    let mut batch = MemoryGraphUpdateBatch::default();

    if let Some(thread_id) = thread_id {
        batch.nodes.push(MemoryGraphNodeUpsert {
            id: format!("node:thread:{thread_id}"),
            label: thread_id.to_string(),
            node_type: "thread".to_string(),
            summary_text: Some("thread context anchor".to_string()),
        });
    }
    if let Some(task_id) = task_id {
        batch.nodes.push(MemoryGraphNodeUpsert {
            id: format!("node:task:{task_id}"),
            label: task_id.to_string(),
            node_type: "task".to_string(),
            summary_text: Some("task context anchor".to_string()),
        });
    }

    if let Some(memory) = structural_memory {
        for seed in &memory.workspace_seeds {
            batch.nodes.push(MemoryGraphNodeUpsert {
                id: seed.node_id.clone(),
                label: seed.relative_path.clone(),
                node_type: seed.kind.clone(),
                summary_text: Some(format!("workspace seed {}", seed.kind)),
            });
        }
        for file in &memory.observed_files {
            batch.nodes.push(MemoryGraphNodeUpsert {
                id: file.node_id.clone(),
                label: file.relative_path.clone(),
                node_type: "file".to_string(),
                summary_text: Some("observed from thread structural memory".to_string()),
            });
            if let Some(task_id) = task_id {
                batch.edges.push(MemoryGraphEdgeUpsert {
                    source_node_id: format!("node:task:{task_id}"),
                    target_node_id: file.node_id.clone(),
                    relation_type: "task_context_file".to_string(),
                    weight: 1.0,
                });
            }
        }
        for edge in &memory.edges {
            batch.nodes.push(inferred_node_from_id(&edge.from));
            batch.nodes.push(inferred_node_from_id(&edge.to));
            batch.edges.push(MemoryGraphEdgeUpsert {
                source_node_id: edge.from.clone(),
                target_node_id: edge.to.clone(),
                relation_type: edge.kind.clone(),
                weight: 1.0,
            });
        }
    }

    for package in semantic_packages {
        let manifest_node_id = node_id_for_relative_path(&package.manifest_path);
        let package_node_id = node_id_for_package(&package.ecosystem, &package.name);
        batch.nodes.push(MemoryGraphNodeUpsert {
            id: manifest_node_id.clone(),
            label: package.manifest_path.clone(),
            node_type: "file".to_string(),
            summary_text: Some("package manifest".to_string()),
        });
        batch.nodes.push(MemoryGraphNodeUpsert {
            id: package_node_id.clone(),
            label: package.name.clone(),
            node_type: "package".to_string(),
            summary_text: Some(format!(
                "{} package declared in {}",
                package.ecosystem, package.manifest_path
            )),
        });
        batch.edges.push(MemoryGraphEdgeUpsert {
            source_node_id: manifest_node_id.clone(),
            target_node_id: package_node_id.clone(),
            relation_type: "manifest_declares_package".to_string(),
            weight: 1.0,
        });

        let package_root = Path::new(&package.manifest_path)
            .parent()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if let Some(memory) = structural_memory {
            for file in &memory.observed_files {
                let normalized_file = file.relative_path.replace('\\', "/");
                if package_root.is_empty() || normalized_file.starts_with(&package_root) {
                    batch.edges.push(MemoryGraphEdgeUpsert {
                        source_node_id: file.node_id.clone(),
                        target_node_id: package_node_id.clone(),
                        relation_type: "file_in_package".to_string(),
                        weight: 1.0,
                    });
                }
            }
        }
    }

    let mut deduped = MemoryGraphUpdateBatch::default();
    for node in batch.nodes {
        if !deduped.nodes.iter().any(|existing| existing.id == node.id) {
            deduped.nodes.push(node);
        }
    }
    for edge in batch.edges {
        if let Some(existing) = deduped.edges.iter_mut().find(|candidate| {
            candidate.source_node_id == edge.source_node_id
                && candidate.target_node_id == edge.target_node_id
                && candidate.relation_type == edge.relation_type
        }) {
            existing.weight += edge.weight;
        } else {
            deduped.edges.push(edge);
        }
    }
    deduped
}
