use super::scan::normalize_package_name;
use super::*;

pub(super) fn render_summary(root: &Path, graph: &SemanticGraph) -> String {
    if graph.packages.is_empty()
        && graph.services.is_empty()
        && graph.infra_resources.is_empty()
        && graph.import_files.is_empty()
    {
        return format!(
            "No semantic manifests, compose services, infra resources, or import edges found under {}.",
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
    let infra_edges = graph
        .infra_resources
        .iter()
        .map(|resource| resource.dependencies.len())
        .sum::<usize>();
    let import_edges = graph
        .import_files
        .iter()
        .map(|file| file.imports.len())
        .sum::<usize>();

    format!(
        "Semantic workspace summary for {}:\n- packages: {}\n- services: {}\n- infra resources: {}\n- import files: {}\n- ecosystems: {}\n- local dependency edges: {}\n- service dependency edges: {}\n- infra dependency edges: {}\n- code import edges: {}",
        root.display(),
        graph.packages.len(),
        graph.services.len(),
        graph.infra_resources.len(),
        graph.import_files.len(),
        ecosystems,
        local_edges,
        service_edges,
        infra_edges,
        import_edges
    )
}

pub(super) fn render_packages(root: &Path, graph: &SemanticGraph, limit: usize) -> String {
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

pub(super) fn render_dependencies(
    root: &Path,
    graph: &SemanticGraph,
    target: Option<&str>,
) -> Result<String> {
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

pub(super) fn render_dependents(
    root: &Path,
    graph: &SemanticGraph,
    target: Option<&str>,
) -> Result<String> {
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

pub(super) fn render_services(root: &Path, graph: &SemanticGraph, limit: usize) -> String {
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

pub(super) fn render_infra(root: &Path, graph: &SemanticGraph, limit: usize) -> String {
    if graph.infra_resources.is_empty() {
        return format!("No Terraform or Kubernetes resources found under {}.", root.display());
    }

    let mut lines = vec![format!("Infra resources under {}:", root.display())];
    for resource in graph.infra_resources.iter().take(limit) {
        let namespace = resource
            .namespace
            .as_deref()
            .map(|value| format!(" ns={value}"))
            .unwrap_or_default();
        let deps = if resource.dependencies.is_empty() {
            "0 deps".to_string()
        } else {
            format!("{} deps [{}]", resource.dependencies.len(), resource.dependencies.join(", "))
        };
        lines.push(format!(
            "- [{}] {} {}{} {} ({})",
            resource.system,
            resource.kind,
            resource.name,
            namespace,
            resource.source_path,
            deps,
        ));
    }
    if graph.infra_resources.len() > limit {
        lines.push(format!(
            "- ... {} more infra resource(s) omitted",
            graph.infra_resources.len() - limit
        ));
    }
    lines.join("\n")
}

pub(super) fn render_service_dependencies(
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

pub(super) fn render_service_dependents(
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

pub(super) fn render_imports(
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

pub(super) fn render_imported_by(
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
