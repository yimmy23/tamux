//! Lightweight semantic environment queries over local workspace manifests.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use walkdir::{DirEntry, WalkDir};

use crate::history::HistoryStore;
use crate::session_manager::SessionManager;

use amux_protocol::SessionId;

#[cfg(test)]
use std::fs;

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

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct SemanticGraph {
    packages: Vec<SemanticPackage>,
    services: Vec<SemanticService>,
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
        "service_dependencies" => render_service_dependencies(&root, &graph, target),
        "service_dependents" => render_service_dependents(&root, &graph, target),
        "imports" => render_imports(&root, &graph, target, limit),
        "imported_by" => render_imported_by(&root, &graph, target, limit),
        "conventions" => render_conventions(&root, &graph, history, agent_data_dir, target, limit).await,
        "temporal" => render_temporal(&root, history, target, limit).await,
        other => Err(anyhow::anyhow!(
            "invalid semantic query kind `{other}`; expected summary, packages, dependencies, dependents, services, service_dependencies, service_dependents, imports, imported_by, conventions, or temporal"
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

    tags.into_iter().collect()
}

async fn resolve_query_root(
    args: &Value,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
) -> Result<PathBuf> {
    if let Some(raw_path) = args.get("path").and_then(|value| value.as_str()) {
        let trimmed = raw_path.trim();
        if trimmed.is_empty() {
            anyhow::bail!("semantic query path must not be empty");
        }
        let path = PathBuf::from(trimmed);
        let resolved = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()?.join(path)
        };
        if resolved.is_dir() {
            return Ok(resolved);
        }
        anyhow::bail!(
            "semantic query path is not a directory: {}",
            resolved.display()
        );
    }

    if let Some(session_id) = session_id {
        let sessions = session_manager.list().await;
        if let Some(cwd) = sessions
            .iter()
            .find(|session| session.id == session_id)
            .and_then(|session| session.cwd.clone())
        {
            let root = PathBuf::from(cwd);
            if root.is_dir() {
                return Ok(root);
            }
        }
    }

    std::env::current_dir().context("failed to resolve current directory for semantic query")
}

fn scan_workspace_semantics(root: &Path) -> Result<SemanticGraph> {
    let mut packages = Vec::new();
    let mut services = Vec::new();
    let mut import_files = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(should_visit_semantic_entry)
        .filter_map(|entry| entry.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        match name.as_ref() {
            "Cargo.toml" => {
                if let Some(package) = parse_cargo_manifest(entry.path())? {
                    packages.push(package);
                }
            }
            "package.json" => {
                if let Some(package) = parse_package_manifest(entry.path())? {
                    packages.push(package);
                }
            }
            "docker-compose.yml" | "docker-compose.yaml" | "compose.yml" | "compose.yaml" => {
                services.extend(parse_compose_services(entry.path())?);
            }
            _ => {
                if is_supported_import_file(entry.path()) {
                    if let Some(import_file) = parse_import_file(entry.path())? {
                        import_files.push(import_file);
                    }
                }
            }
        }
        if packages.len() >= MAX_MANIFESTS {
            break;
        }
        if services.len() >= MAX_SERVICES {
            services.truncate(MAX_SERVICES);
        }
        if import_files.len() >= MAX_IMPORT_FILES {
            break;
        }
    }

    packages.sort_by(|left, right| left.name.cmp(&right.name));
    services.sort_by(|left, right| left.name.cmp(&right.name));
    import_files.sort_by(|left, right| left.source_path.cmp(&right.source_path));
    Ok(SemanticGraph {
        packages,
        services,
        import_files,
    })
}

fn should_visit_semantic_entry(entry: &DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy();
    !matches!(
        name.as_ref(),
        ".git"
            | "node_modules"
            | "target"
            | "dist"
            | "dist-release"
            | "release"
            | ".next"
            | ".turbo"
            | ".cache"
    )
}

fn parse_cargo_manifest(path: &Path) -> Result<Option<SemanticPackage>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut section = String::new();
    let mut package_name = None;
    let mut dependencies = BTreeSet::new();

    for raw_line in content.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line.trim_matches(['[', ']']).trim().to_ascii_lowercase();
            continue;
        }

        if section == "package" && line.starts_with("name") {
            if let Some(value) = parse_manifest_string_value(line) {
                package_name = Some(value);
            }
            continue;
        }

        if !matches!(
            section.as_str(),
            "dependencies"
                | "dev-dependencies"
                | "build-dependencies"
                | "workspace.dependencies"
                | "target.'cfg(unix)'.dependencies"
                | "target.'cfg(windows)'.dependencies"
        ) {
            continue;
        }

        if let Some((key, _)) = line.split_once('=') {
            let normalized = normalize_package_name(key);
            if !normalized.is_empty() {
                dependencies.insert(normalized);
            }
        }
    }

    let Some(name) = package_name.map(|value| normalize_package_name(&value)) else {
        return Ok(None);
    };
    if name.is_empty() {
        return Ok(None);
    }

    Ok(Some(SemanticPackage {
        ecosystem: "cargo",
        name,
        manifest_path: path.display().to_string(),
        dependencies: dependencies.into_iter().collect(),
    }))
}

fn parse_package_manifest(path: &Path) -> Result<Option<SemanticPackage>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let json: Value =
        serde_json::from_str(&content).with_context(|| format!("invalid {}", path.display()))?;
    let Some(name) = json.get("name").and_then(|value| value.as_str()) else {
        return Ok(None);
    };

    let mut dependencies = BTreeSet::new();
    for field in ["dependencies", "devDependencies", "peerDependencies"] {
        if let Some(object) = json.get(field).and_then(|value| value.as_object()) {
            for key in object.keys() {
                let normalized = normalize_package_name(key);
                if !normalized.is_empty() {
                    dependencies.insert(normalized);
                }
            }
        }
    }

    Ok(Some(SemanticPackage {
        ecosystem: "npm",
        name: normalize_package_name(name),
        manifest_path: path.display().to_string(),
        dependencies: dependencies.into_iter().collect(),
    }))
}

#[derive(Debug, Deserialize)]
struct ComposeFile {
    services: Option<BTreeMap<String, ComposeService>>,
}

#[derive(Debug, Deserialize)]
struct ComposeService {
    depends_on: Option<Value>,
}

fn parse_compose_services(path: &Path) -> Result<Vec<SemanticService>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed: ComposeFile =
        serde_yaml::from_str(&content).with_context(|| format!("invalid {}", path.display()))?;
    let mut services = Vec::new();
    for (name, service) in parsed.services.unwrap_or_default() {
        let dependencies = parse_compose_depends_on(service.depends_on);
        services.push(SemanticService {
            name: normalize_package_name(&name),
            compose_path: path.display().to_string(),
            dependencies,
        });
    }
    Ok(services)
}

fn parse_compose_depends_on(depends_on: Option<Value>) -> Vec<String> {
    let mut deps = BTreeSet::new();
    match depends_on {
        Some(Value::Array(items)) => {
            for item in items {
                if let Some(name) = item.as_str() {
                    let normalized = normalize_package_name(name);
                    if !normalized.is_empty() {
                        deps.insert(normalized);
                    }
                }
            }
        }
        Some(Value::Object(map)) => {
            for key in map.keys() {
                let normalized = normalize_package_name(key);
                if !normalized.is_empty() {
                    deps.insert(normalized);
                }
            }
        }
        _ => {}
    }
    deps.into_iter().collect()
}

fn is_supported_import_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default(),
        "rs" | "ts" | "tsx" | "js" | "jsx"
    )
}

fn parse_import_file(path: &Path) -> Result<Option<SemanticImportFile>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default();
    let (language, imports) = match extension {
        "rs" => ("rust", parse_rust_imports(&content)),
        "ts" | "tsx" => ("typescript", parse_script_imports(&content)),
        "js" | "jsx" => ("javascript", parse_script_imports(&content)),
        _ => return Ok(None),
    };
    if imports.is_empty() {
        return Ok(None);
    }
    Ok(Some(SemanticImportFile {
        language,
        source_path: path.display().to_string(),
        imports,
    }))
}

fn parse_rust_imports(content: &str) -> Vec<String> {
    let mut imports = BTreeSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        let candidate = trimmed
            .strip_prefix("use ")
            .or_else(|| trimmed.strip_prefix("pub use "));
        let Some(candidate) = candidate else {
            continue;
        };
        let path = candidate
            .split(';')
            .next()
            .unwrap_or(candidate)
            .split('{')
            .next()
            .unwrap_or(candidate)
            .trim();
        if path.is_empty() {
            continue;
        }
        imports.insert(path.to_string());
        if imports.len() >= MAX_IMPORTS_PER_FILE {
            break;
        }
    }
    imports.into_iter().collect()
}

fn parse_script_imports(content: &str) -> Vec<String> {
    let mut imports = BTreeSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        let source = if let Some(index) = trimmed.find(" from ") {
            trimmed[index + 6..].trim()
        } else if let Some(rest) = trimmed.strip_prefix("import ") {
            rest.trim()
        } else if let Some(rest) = trimmed.strip_prefix("export * from ") {
            rest.trim()
        } else {
            continue;
        };
        let source = source
            .trim_end_matches(';')
            .trim_matches('"')
            .trim_matches('\'')
            .trim();
        if source.is_empty() || source == "type" {
            continue;
        }
        imports.insert(source.to_string());
        if imports.len() >= MAX_IMPORTS_PER_FILE {
            break;
        }
    }
    imports.into_iter().collect()
}

fn parse_manifest_string_value(line: &str) -> Option<String> {
    let (_, rhs) = line.split_once('=')?;
    let trimmed = rhs.trim().trim_matches('"').trim_matches('\'').trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_package_name(raw: &str) -> String {
    raw.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_ascii_lowercase()
}

fn render_summary(root: &Path, graph: &SemanticGraph) -> String {
    if graph.packages.is_empty() && graph.services.is_empty() && graph.import_files.is_empty() {
        return format!(
            "No semantic manifests, compose services, or import edges found under {}.",
            root.display()
        );
    }

    let mut by_ecosystem = BTreeMap::<&str, usize>::new();
    for package in &graph.packages {
        *by_ecosystem.entry(package.ecosystem).or_default() += 1;
    }

    let ecosystems = by_ecosystem
        .into_iter()
        .map(|(ecosystem, count)| format!("{ecosystem}: {count}"))
        .collect::<Vec<_>>()
        .join(", ");
    let local_edges = graph
        .packages
        .iter()
        .map(|package| {
            package
                .dependencies
                .iter()
                .filter(|dependency| {
                    graph
                        .packages
                        .iter()
                        .any(|candidate| candidate.name == **dependency)
                })
                .count()
        })
        .sum::<usize>();
    let service_edges = graph
        .services
        .iter()
        .map(|service| service.dependencies.len())
        .sum::<usize>();
    let import_edges = graph
        .import_files
        .iter()
        .map(|file| file.imports.len())
        .sum::<usize>();

    format!(
        "Semantic workspace summary for {}:\n- packages: {}\n- services: {}\n- import files: {}\n- ecosystems: {}\n- local dependency edges: {}\n- service dependency edges: {}\n- code import edges: {}",
        root.display(),
        graph.packages.len(),
        graph.services.len(),
        graph.import_files.len(),
        ecosystems,
        local_edges,
        service_edges,
        import_edges
    )
}

fn render_packages(root: &Path, graph: &SemanticGraph, limit: usize) -> String {
    if graph.packages.is_empty() {
        return format!("No semantic packages found under {}.", root.display());
    }

    let mut lines = vec![format!("Semantic packages under {}:", root.display())];
    for package in graph.packages.iter().take(limit) {
        lines.push(format!(
            "- [{}] {} ({} deps) {}",
            package.ecosystem,
            package.name,
            package.dependencies.len(),
            package.manifest_path
        ));
    }
    if graph.packages.len() > limit {
        lines.push(format!(
            "- ... {} more package(s) omitted",
            graph.packages.len() - limit
        ));
    }
    lines.join("\n")
}

fn render_dependencies(root: &Path, graph: &SemanticGraph, target: Option<&str>) -> Result<String> {
    let package = resolve_target_package(graph, target)?;
    if package.dependencies.is_empty() {
        return Ok(format!(
            "{} has no direct {} dependencies in {}.",
            package.name,
            package.ecosystem,
            root.display()
        ));
    }
    Ok(format!(
        "Direct dependencies for {} [{}]:\n{}",
        package.name,
        package.ecosystem,
        package
            .dependencies
            .iter()
            .map(|dependency| format!("- {dependency}"))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

fn render_dependents(root: &Path, graph: &SemanticGraph, target: Option<&str>) -> Result<String> {
    let Some(target) = target
        .map(normalize_package_name)
        .filter(|value| !value.is_empty())
    else {
        anyhow::bail!("dependents queries require a non-empty `target` package name");
    };

    let dependents = graph
        .packages
        .iter()
        .filter(|package| {
            package
                .dependencies
                .iter()
                .any(|dependency| dependency == &target)
        })
        .collect::<Vec<_>>();
    if dependents.is_empty() {
        return Ok(format!(
            "No local packages under {} depend on {}.",
            root.display(),
            target
        ));
    }

    Ok(format!(
        "Local dependents of {}:\n{}",
        target,
        dependents
            .into_iter()
            .map(|package| format!(
                "- [{}] {} ({})",
                package.ecosystem, package.name, package.manifest_path
            ))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

fn render_services(root: &Path, graph: &SemanticGraph, limit: usize) -> String {
    if graph.services.is_empty() {
        return format!("No compose services found under {}.", root.display());
    }

    let mut lines = vec![format!("Compose services under {}:", root.display())];
    for service in graph.services.iter().take(limit) {
        lines.push(format!(
            "- {} ({} deps) {}",
            service.name,
            service.dependencies.len(),
            service.compose_path
        ));
    }
    if graph.services.len() > limit {
        lines.push(format!(
            "- ... {} more service(s) omitted",
            graph.services.len() - limit
        ));
    }
    lines.join("\n")
}

fn render_service_dependencies(
    root: &Path,
    graph: &SemanticGraph,
    target: Option<&str>,
) -> Result<String> {
    let service = resolve_target_service(graph, target)?;
    if service.dependencies.is_empty() {
        return Ok(format!(
            "Service {} has no direct compose dependencies in {}.",
            service.name,
            root.display()
        ));
    }
    Ok(format!(
        "Compose dependencies for {}:\n{}",
        service.name,
        service
            .dependencies
            .iter()
            .map(|dependency| format!("- {dependency}"))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

fn render_service_dependents(
    root: &Path,
    graph: &SemanticGraph,
    target: Option<&str>,
) -> Result<String> {
    let Some(target) = target
        .map(normalize_package_name)
        .filter(|value| !value.is_empty())
    else {
        anyhow::bail!("service_dependents queries require a non-empty `target` service name");
    };

    let dependents = graph
        .services
        .iter()
        .filter(|service| {
            service
                .dependencies
                .iter()
                .any(|dependency| dependency == &target)
        })
        .collect::<Vec<_>>();
    if dependents.is_empty() {
        return Ok(format!(
            "No compose services under {} depend on {}.",
            root.display(),
            target
        ));
    }

    Ok(format!(
        "Compose dependents of {}:\n{}",
        target,
        dependents
            .into_iter()
            .map(|service| format!("- {} ({})", service.name, service.compose_path))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

fn render_imports(
    root: &Path,
    graph: &SemanticGraph,
    target: Option<&str>,
    limit: usize,
) -> Result<String> {
    let Some(target) = target.map(str::trim).filter(|value| !value.is_empty()) else {
        anyhow::bail!("imports queries require a non-empty `target` file path or module name");
    };
    let normalized_target = normalize_convention_text(target);
    let matches = graph
        .import_files
        .iter()
        .filter(|file| normalize_convention_text(&file.source_path).contains(&normalized_target))
        .collect::<Vec<_>>();
    let Some(file) = matches.first().copied() else {
        return Ok(format!(
            "No import file matched `{target}` under {}.",
            root.display()
        ));
    };

    Ok(format!(
        "Imports for {} [{}]:\n{}",
        file.source_path,
        file.language,
        file.imports
            .iter()
            .take(limit)
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

fn render_imported_by(
    root: &Path,
    graph: &SemanticGraph,
    target: Option<&str>,
    limit: usize,
) -> Result<String> {
    let Some(target) = target.map(str::trim).filter(|value| !value.is_empty()) else {
        anyhow::bail!("imported_by queries require a non-empty `target` module or path fragment");
    };
    let normalized_target = normalize_convention_text(target);
    let importers = graph
        .import_files
        .iter()
        .filter(|file| {
            file.imports
                .iter()
                .any(|item| normalize_convention_text(item).contains(&normalized_target))
        })
        .take(limit)
        .collect::<Vec<_>>();
    if importers.is_empty() {
        return Ok(format!(
            "No local files under {} import `{target}`.",
            root.display()
        ));
    }

    Ok(format!(
        "Files importing `{target}`:\n{}",
        importers
            .into_iter()
            .map(|file| format!("- [{}] {}", file.language, file.source_path))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

async fn render_conventions(
    root: &Path,
    graph: &SemanticGraph,
    history: &HistoryStore,
    agent_data_dir: &Path,
    target: Option<&str>,
    limit: usize,
) -> Result<String> {
    let target_tokens = target.map(tokenize_convention_query).unwrap_or_default();
    let report = history.memory_provenance_report(None, limit.saturating_mul(4).max(20)).await?;
    let mut matching_entries = report
        .entries
        .into_iter()
        .filter(|entry| entry.mode != "remove")
        .filter(|entry| entry.status != "retracted")
        .filter(|entry| convention_entry_matches(entry, &target_tokens))
        .collect::<Vec<_>>();
    matching_entries.sort_by(|left, right| {
        right
            .confidence
            .partial_cmp(&left.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.created_at.cmp(&left.created_at))
    });

    let skill_matches = collect_matching_skills(&super::skills_dir(agent_data_dir), target, limit);
    let package_match = target
        .and_then(|value| resolve_target_package(graph, Some(value)).ok())
        .map(|package| {
            let dependent_count = graph
                .packages
                .iter()
                .filter(|candidate| candidate.dependencies.iter().any(|dep| dep == &package.name))
                .count();
            format!(
                "- Package context: [{}] {} has {} direct dependency(ies) and {} local dependent(s).",
                package.ecosystem,
                package.name,
                package.dependencies.len(),
                dependent_count
            )
        });

    if matching_entries.is_empty() && skill_matches.is_empty() && package_match.is_none() {
        return Ok(match target {
            Some(target) => format!(
                "No durable conventions matched `{target}` under {}.",
                root.display()
            ),
            None => format!("No durable conventions found for {}.", root.display()),
        });
    }

    let mut lines = vec![match target {
        Some(target) => format!("Conventions related to `{target}`:"),
        None => format!("Conventions for {}:", root.display()),
    }];

    for entry in matching_entries.into_iter().take(limit) {
        lines.push(format!(
            "- [{} | {} | {:.0}% confidence | {:.1}d old] {}",
            entry.target,
            entry.source_kind,
            entry.confidence * 100.0,
            entry.age_days,
            summarize_text(&entry.content, 140)
        ));
    }

    if let Some(package_line) = package_match {
        lines.push(package_line);
    }

    if !skill_matches.is_empty() {
        lines.push("Relevant local skills/workflows:".to_string());
        for skill in skill_matches {
            lines.push(format!("- {}", skill));
        }
    }

    Ok(lines.join("\n"))
}

async fn render_temporal(
    root: &Path,
    history: &HistoryStore,
    target: Option<&str>,
    limit: usize,
) -> Result<String> {
    let normalized_target = target.map(normalize_convention_text);
    let commands = history
        .query_command_log(None, None, Some(limit.saturating_mul(12).max(40))).await?
        .into_iter()
        .filter(|entry| {
            entry
                .cwd
                .as_deref()
                .map(|cwd| path_is_under_root(root, cwd))
                .unwrap_or(false)
                || entry
                    .path
                    .as_deref()
                    .map(|path| path_is_under_root(root, path))
                    .unwrap_or(false)
        })
        .filter(|entry| {
            normalized_target.as_ref().is_none_or(|target| {
                target_matches_text(&entry.command, target)
                    || entry
                        .path
                        .as_deref()
                        .is_some_and(|path| target_matches_text(path, target))
                    || entry
                        .cwd
                        .as_deref()
                        .is_some_and(|cwd| target_matches_text(cwd, target))
            })
        })
        .collect::<Vec<_>>();

    let transcripts = if normalized_target.is_some() {
        history
            .list_transcript_index(None).await?
            .into_iter()
            .filter(|entry| {
                normalized_target.as_ref().is_some_and(|target| {
                    target_matches_text(&entry.filename, target)
                        || entry
                            .preview
                            .as_deref()
                            .is_some_and(|preview| target_matches_text(preview, target))
                })
            })
            .take(limit)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let memory_matches = if normalized_target.is_some() {
        history
            .memory_provenance_report(target, limit.saturating_mul(2).max(12)).await?
            .entries
            .into_iter()
            .filter(|entry| entry.mode != "remove" && entry.status != "retracted")
            .take(limit)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    if commands.is_empty() && transcripts.is_empty() && memory_matches.is_empty() {
        return Ok(match target {
            Some(target) => format!(
                "No temporal workspace history matched `{target}` under {}.",
                root.display()
            ),
            None => format!(
                "No temporal workspace history found under {}.",
                root.display()
            ),
        });
    }

    let success_count = commands
        .iter()
        .filter(|entry| entry.exit_code.unwrap_or_default() == 0)
        .count();
    let failure_count = commands
        .iter()
        .filter(|entry| entry.exit_code.is_some_and(|code| code != 0))
        .count();

    let mut lines = vec![match target {
        Some(target) => format!("Temporal history related to `{target}`:"),
        None => format!("Temporal history for {}:", root.display()),
    }];
    if !commands.is_empty() {
        lines.push(format!(
            "- Recent matching commands: {} total, {} success, {} failure.",
            commands.len(),
            success_count,
            failure_count
        ));
        for entry in commands.iter().take(limit) {
            lines.push(format!(
                "- [cmd @ {}] {}{}",
                entry.timestamp,
                summarize_text(&entry.command, 100),
                match entry.exit_code {
                    Some(code) => format!(" (exit {code})"),
                    None => String::new(),
                }
            ));
        }
    }
    if !transcripts.is_empty() {
        lines.push("Global matching transcript/session captures:".to_string());
        for entry in transcripts {
            lines.push(format!(
                "- [capture @ {}] {}{}",
                entry.captured_at,
                entry.filename,
                entry
                    .preview
                    .as_deref()
                    .map(|preview| format!(" — {}", summarize_text(preview, 80)))
                    .unwrap_or_default()
            ));
        }
    }
    if !memory_matches.is_empty() {
        lines.push("Global durable learned history matching the target:".to_string());
        for entry in memory_matches {
            lines.push(format!(
                "- [{} | {:.0}% confidence | {:.1}d old] {}",
                entry.source_kind,
                entry.confidence * 100.0,
                entry.age_days,
                summarize_text(&entry.content, 100)
            ));
        }
    }

    Ok(lines.join("\n"))
}

fn resolve_target_package<'a>(
    graph: &'a SemanticGraph,
    target: Option<&str>,
) -> Result<&'a SemanticPackage> {
    let Some(target) = target
        .map(normalize_package_name)
        .filter(|value| !value.is_empty())
    else {
        anyhow::bail!("semantic dependency queries require a non-empty `target` package name");
    };

    if let Some(exact) = graph.packages.iter().find(|package| package.name == target) {
        return Ok(exact);
    }

    let partial_matches = graph
        .packages
        .iter()
        .filter(|package| package.name.contains(&target))
        .collect::<Vec<_>>();
    match partial_matches.as_slice() {
        [single] => Ok(*single),
        [] => Err(anyhow::anyhow!("no local package matched `{target}`")),
        _ => Err(anyhow::anyhow!(
            "multiple packages matched `{target}`; be more specific"
        )),
    }
}

fn resolve_target_service<'a>(
    graph: &'a SemanticGraph,
    target: Option<&str>,
) -> Result<&'a SemanticService> {
    let Some(target) = target
        .map(normalize_package_name)
        .filter(|value| !value.is_empty())
    else {
        anyhow::bail!("service queries require a non-empty `target` service name");
    };

    if let Some(exact) = graph.services.iter().find(|service| service.name == target) {
        return Ok(exact);
    }

    let partial_matches = graph
        .services
        .iter()
        .filter(|service| service.name.contains(&target))
        .collect::<Vec<_>>();
    match partial_matches.as_slice() {
        [single] => Ok(*single),
        [] => Err(anyhow::anyhow!("no compose service matched `{target}`")),
        _ => Err(anyhow::anyhow!(
            "multiple compose services matched `{target}`; be more specific"
        )),
    }
}

fn convention_entry_matches(
    entry: &crate::history::MemoryProvenanceReportEntry,
    target_tokens: &[String],
) -> bool {
    if target_tokens.is_empty() {
        return matches!(entry.target.as_str(), "MEMORY.md" | "USER.md");
    }

    let haystack = normalize_convention_text(&format!(
        "{} {} {} {}",
        entry.target,
        entry.source_kind,
        entry.content,
        entry.fact_keys.join(" ")
    ));
    target_tokens.iter().all(|token| haystack.contains(token))
        || target_tokens
            .iter()
            .any(|token| entry.fact_keys.iter().any(|key| key.contains(token)))
}

fn tokenize_convention_query(target: &str) -> Vec<String> {
    normalize_convention_text(target)
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect()
}

fn normalize_convention_text(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '-' | '_') {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
}

fn target_matches_text(value: &str, normalized_target: &str) -> bool {
    normalize_convention_text(value).contains(normalized_target)
}

fn path_is_under_root(root: &Path, candidate: &str) -> bool {
    let candidate_path = Path::new(candidate);
    if candidate_path.is_absolute() {
        candidate_path.starts_with(root)
    } else {
        false
    }
}

fn collect_matching_skills(skills_root: &Path, target: Option<&str>, limit: usize) -> Vec<String> {
    let mut matches = Vec::new();
    let target_tokens = target.map(tokenize_convention_query).unwrap_or_default();
    for entry in WalkDir::new(skills_root)
        .follow_links(false)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry
            .path()
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            != "md"
        {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(skills_root)
            .unwrap_or(entry.path())
            .display()
            .to_string();
        let haystack = normalize_convention_text(&relative);
        if !target_tokens.is_empty() && !target_tokens.iter().all(|token| haystack.contains(token))
        {
            continue;
        }
        matches.push(relative);
        if matches.len() >= limit {
            break;
        }
    }
    matches
}

fn summarize_text(content: &str, max_chars: usize) -> String {
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let char_count = normalized.chars().count();
    if char_count <= max_chars {
        return normalized;
    }
    let truncated = normalized.chars().take(max_chars).collect::<String>();
    format!("{truncated}...")
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_temp_dir() -> Result<PathBuf> {
        let root = std::env::temp_dir().join(format!("tamux-semantic-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root)?;
        Ok(root)
    }

    #[test]
    fn parse_cargo_manifest_extracts_name_and_dependencies() -> Result<()> {
        let root = make_temp_dir()?;
        let manifest = root.join("Cargo.toml");
        fs::write(
            &manifest,
            r#"[package]
name = "daemon-core"

[dependencies]
serde = "1"
tokio = { version = "1" }
"#,
        )?;

        let package = parse_cargo_manifest(&manifest)?.expect("cargo package should parse");
        assert_eq!(package.name, "daemon-core");
        assert_eq!(
            package.dependencies,
            vec!["serde".to_string(), "tokio".to_string()]
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn parse_package_manifest_extracts_dependencies() -> Result<()> {
        let root = make_temp_dir()?;
        let manifest = root.join("package.json");
        fs::write(
            &manifest,
            r#"{"name":"frontend","dependencies":{"react":"18"},"devDependencies":{"vite":"5"}}"#,
        )?;

        let package = parse_package_manifest(&manifest)?.expect("npm package should parse");
        assert_eq!(package.name, "frontend");
        assert_eq!(
            package.dependencies,
            vec!["react".to_string(), "vite".to_string()]
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn parse_compose_services_extracts_services_and_dependencies() -> Result<()> {
        let root = make_temp_dir()?;
        let compose = root.join("docker-compose.yml");
        fs::write(
            &compose,
            r#"
services:
  api:
    depends_on:
      - db
  worker:
    depends_on:
      redis:
        condition: service_started
  db: {}
"#,
        )?;

        let services = parse_compose_services(&compose)?;
        assert_eq!(services.len(), 3);
        let api = services
            .iter()
            .find(|service| service.name == "api")
            .expect("api service should parse");
        let worker = services
            .iter()
            .find(|service| service.name == "worker")
            .expect("worker service should parse");
        assert_eq!(api.dependencies, vec!["db".to_string()]);
        assert_eq!(worker.dependencies, vec!["redis".to_string()]);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn parse_script_imports_extracts_modules() {
        let imports = parse_script_imports(
            r#"
import React from "react";
import { api } from "./lib/api";
export * from "../shared/types";
"#,
        );

        assert!(imports.iter().any(|item| item == "react"));
        assert!(imports.iter().any(|item| item == "./lib/api"));
        assert!(imports.iter().any(|item| item == "../shared/types"));
    }

    #[test]
    fn render_service_dependents_lists_reverse_service_edges() {
        let graph = SemanticGraph {
            packages: Vec::new(),
            services: vec![
                SemanticService {
                    name: "api".to_string(),
                    compose_path: "/tmp/docker-compose.yml".to_string(),
                    dependencies: vec!["db".to_string()],
                },
                SemanticService {
                    name: "db".to_string(),
                    compose_path: "/tmp/docker-compose.yml".to_string(),
                    dependencies: vec![],
                },
            ],
            import_files: Vec::new(),
        };

        let rendered = render_service_dependents(Path::new("/tmp"), &graph, Some("db")).unwrap();
        assert!(rendered.contains("api"));
    }

    #[test]
    fn render_imported_by_lists_matching_files() {
        let graph = SemanticGraph {
            packages: Vec::new(),
            services: Vec::new(),
            import_files: vec![
                SemanticImportFile {
                    language: "typescript",
                    source_path: "/tmp/src/main.ts".to_string(),
                    imports: vec!["./lib/api".to_string(), "react".to_string()],
                },
                SemanticImportFile {
                    language: "rust",
                    source_path: "/tmp/src/lib.rs".to_string(),
                    imports: vec!["crate::db".to_string()],
                },
            ],
        };

        let rendered = render_imported_by(Path::new("/tmp"), &graph, Some("api"), 10).unwrap();
        assert!(rendered.contains("/tmp/src/main.ts"));
    }

    #[test]
    fn render_dependents_lists_local_reverse_edges() {
        let graph = SemanticGraph {
            packages: vec![
                SemanticPackage {
                    ecosystem: "cargo",
                    name: "amux-daemon".to_string(),
                    manifest_path: "/tmp/daemon/Cargo.toml".to_string(),
                    dependencies: vec!["amux-protocol".to_string()],
                },
                SemanticPackage {
                    ecosystem: "cargo",
                    name: "amux-protocol".to_string(),
                    manifest_path: "/tmp/protocol/Cargo.toml".to_string(),
                    dependencies: vec![],
                },
            ],
            services: Vec::new(),
            import_files: Vec::new(),
        };

        let rendered = render_dependents(Path::new("/tmp"), &graph, Some("amux-protocol")).unwrap();
        assert!(rendered.contains("amux-daemon"));
    }

    #[test]
    fn convention_entry_matches_fact_keys_and_content() {
        let entry = crate::history::MemoryProvenanceReportEntry {
            id: "1".to_string(),
            target: "MEMORY.md".to_string(),
            mode: "append".to_string(),
            source_kind: "goal_reflection".to_string(),
            content: "- Error types go in src/error.rs".to_string(),
            fact_keys: vec!["error".to_string(), "src/error.rs".to_string()],
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: 0,
            age_days: 0.0,
            confidence: 1.0,
            status: "active".to_string(),
        };

        assert!(convention_entry_matches(
            &entry,
            &tokenize_convention_query("error")
        ));
        assert!(convention_entry_matches(
            &entry,
            &tokenize_convention_query("src/error.rs")
        ));
        assert!(!convention_entry_matches(
            &entry,
            &tokenize_convention_query("terraform")
        ));
    }

    #[test]
    fn collect_matching_skills_filters_by_target() -> Result<()> {
        let root = make_temp_dir()?;
        fs::create_dir_all(root.join("generated"))?;
        fs::write(
            root.join("generated/error-handling.md"),
            "# Error handling\n",
        )?;
        fs::write(root.join("generated/deploy.md"), "# Deploy\n")?;

        let matches = collect_matching_skills(&root, Some("error"), 5);
        assert_eq!(matches, vec!["generated/error-handling.md".to_string()]);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn render_temporal_summarizes_recent_workspace_history() -> Result<()> {
        let root = make_temp_dir()?;
        let store = HistoryStore::new_test_store(&root).await?;
        store.append_command_log(&amux_protocol::CommandLogEntry {
            id: "cmd-1".to_string(),
            command: "deploy staging".to_string(),
            timestamp: 123,
            path: None,
            cwd: Some(root.display().to_string()),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            exit_code: Some(1),
            duration_ms: Some(50),
        }).await?;

        let rendered = render_temporal(&root, &store, Some("deploy"), 5).await?;
        assert!(rendered.contains("deploy staging"));
        assert!(rendered.contains("1 failure"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn render_temporal_excludes_sibling_paths() -> Result<()> {
        let root = make_temp_dir()?;
        let sibling = root.with_file_name(format!(
            "{}-other",
            root.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("workspace")
        ));
        fs::create_dir_all(&sibling)?;
        let store = HistoryStore::new_test_store(&root).await?;
        store.append_command_log(&amux_protocol::CommandLogEntry {
            id: "cmd-in".to_string(),
            command: "cargo test".to_string(),
            timestamp: 1,
            path: None,
            cwd: Some(root.display().to_string()),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            exit_code: Some(0),
            duration_ms: Some(10),
        }).await?;
        store.append_command_log(&amux_protocol::CommandLogEntry {
            id: "cmd-out".to_string(),
            command: "cargo build".to_string(),
            timestamp: 2,
            path: None,
            cwd: Some(sibling.display().to_string()),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            exit_code: Some(0),
            duration_ms: Some(10),
        }).await?;

        let rendered = render_temporal(&root, &store, None, 10).await?;
        assert!(rendered.contains("cargo test"));
        assert!(!rendered.contains("cargo build"));

        fs::remove_dir_all(root)?;
        fs::remove_dir_all(sibling)?;
        Ok(())
    }
}
