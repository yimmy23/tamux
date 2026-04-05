//! Lightweight semantic environment queries over local workspace manifests.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use walkdir::{DirEntry, WalkDir};

use crate::agent::AgentEngine;
use crate::history::HistoryStore;
use crate::session_manager::SessionManager;

use amux_protocol::SessionId;

#[cfg(test)]
use std::fs;

mod helpers;
mod history;
mod render;
mod scan;

use self::helpers::*;
use self::history::{render_conventions, render_temporal};
use self::render::*;
use self::scan::{resolve_query_root, scan_workspace_semantics};

const MAX_MANIFESTS: usize = 200;
const MAX_SERVICES: usize = 100;
const MAX_IMPORT_FILES: usize = 400;
const MAX_IMPORTS_PER_FILE: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
struct SemanticPackage {
    ecosystem: &'static str,
    name: String,
    manifest_path: String,
    dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SemanticService {
    name: String,
    compose_path: String,
    dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SemanticImportFile {
    language: &'static str,
    source_path: String,
    imports: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SemanticInfraResource {
    system: &'static str,
    kind: String,
    name: String,
    source_path: String,
    namespace: Option<String>,
    dependencies: Vec<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct SemanticGraph {
    packages: Vec<SemanticPackage>,
    services: Vec<SemanticService>,
    infra_resources: Vec<SemanticInfraResource>,
    import_files: Vec<SemanticImportFile>,
}

pub(super) async fn execute_semantic_query(
    args: &Value,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    history: &HistoryStore,
    agent_data_dir: &Path,
) -> Result<String> {
    let kind = args
        .get("kind")
        .and_then(|value| value.as_str())
        .unwrap_or("summary")
        .trim()
        .to_ascii_lowercase();
    let target = args
        .get("target")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .unwrap_or(20)
        .clamp(1, 100) as usize;

    let root = resolve_query_root(args, session_manager, session_id).await?;
    let graph = scan_workspace_semantics(&root)?;

    match kind.as_str() {
        "summary" => Ok(render_summary(&root, &graph)),
        "packages" => Ok(render_packages(&root, &graph, limit)),
        "dependencies" => render_dependencies(&root, &graph, target),
        "dependents" => render_dependents(&root, &graph, target),
        "services" => Ok(render_services(&root, &graph, limit)),
        "infra" => Ok(render_infra(&root, &graph, limit)),
        "service_dependencies" => render_service_dependencies(&root, &graph, target),
        "service_dependents" => render_service_dependents(&root, &graph, target),
        "imports" => render_imports(&root, &graph, target, limit),
        "imported_by" => render_imported_by(&root, &graph, target, limit),
        "conventions" => {
            render_conventions(&root, &graph, history, agent_data_dir, target, limit).await
        }
        "temporal" => render_temporal(&root, history, target, limit).await,
        other => Err(anyhow::anyhow!(
            "invalid semantic query kind `{other}`; expected summary, packages, dependencies, dependents, services, infra, service_dependencies, service_dependents, imports, imported_by, conventions, or temporal"
        )),
    }
}

pub(super) fn infer_workspace_context_tags(root: &Path) -> Vec<String> {
    let graph = scan_workspace_semantics(root).unwrap_or_default();
    let mut tags = BTreeSet::new();

    for package in &graph.packages {
        match package.ecosystem {
            "cargo" => {
                tags.insert("rust".to_string());
            }
            "npm" => {
                tags.insert("node".to_string());
            }
            _ => {}
        }

        for dependency in &package.dependencies {
            match dependency.as_str() {
                "tokio" | "async-std" | "futures" => {
                    tags.insert("async".to_string());
                }
                "wasm-bindgen" | "wasmtime" | "wasm-pack" => {
                    tags.insert("wasm32".to_string());
                }
                "react" | "next" | "vite" | "svelte" | "vue" => {
                    tags.insert("frontend".to_string());
                }
                "electron" | "tauri" => {
                    tags.insert("desktop".to_string());
                }
                "diesel" | "sqlx" | "postgres" | "prisma" | "sequelize" => {
                    tags.insert("database".to_string());
                }
                _ => {}
            }
        }
    }

    if !graph.services.is_empty() {
        tags.insert("docker".to_string());
    }

    for resource in &graph.infra_resources {
        tags.insert("infra".to_string());
        match resource.system {
            "terraform" => {
                tags.insert("terraform".to_string());
            }
            "kubernetes" => {
                tags.insert("kubernetes".to_string());
            }
            _ => {}
        }
    }

    tags.into_iter().collect()
}

impl AgentEngine {
    pub(crate) async fn semantic_query_text(&self, args: &Value) -> Result<String> {
        execute_semantic_query(
            args,
            &self.session_manager,
            None,
            &self.history,
            &self.data_dir,
        )
        .await
    }
}

#[cfg(test)]
mod tests;
