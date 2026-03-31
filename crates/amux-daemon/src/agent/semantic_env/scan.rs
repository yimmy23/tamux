use super::*;

pub(super) async fn resolve_query_root(
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

pub(super) fn scan_workspace_semantics(root: &Path) -> Result<SemanticGraph> {
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

pub(super) fn parse_cargo_manifest(path: &Path) -> Result<Option<SemanticPackage>> {
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

pub(super) fn parse_package_manifest(path: &Path) -> Result<Option<SemanticPackage>> {
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

pub(super) fn parse_compose_services(path: &Path) -> Result<Vec<SemanticService>> {
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

pub(super) fn parse_script_imports(content: &str) -> Vec<String> {
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

pub(super) fn normalize_package_name(raw: &str) -> String {
    raw.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_ascii_lowercase()
}
