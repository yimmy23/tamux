use super::*;

pub(super) fn plugins_root() -> Result<PathBuf> {
    Ok(zorai_protocol::ensure_zorai_data_dir()?.join(PLUGINS_DIR))
}

pub(super) fn registry_path() -> Result<PathBuf> {
    Ok(plugins_root()?.join(REGISTRY_FILE))
}

pub(super) fn ensure_plugin_workspace(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root)?;

    let package_json_path = root.join("package.json");
    if !package_json_path.exists() {
        let content = serde_json::json!({
            "name": "zorai-external-plugins",
            "private": true,
            "description": "Runtime-installed zorai plugins"
        });
        std::fs::write(package_json_path, serde_json::to_vec_pretty(&content)?)?;
    }

    Ok(())
}

pub(super) fn npm_command() -> &'static str {
    if cfg!(windows) {
        "npm.cmd"
    } else {
        "npm"
    }
}

pub(super) fn package_name_from_spec(spec: &str) -> Result<String> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        bail!("plugin package spec cannot be empty");
    }

    let candidate_path = Path::new(trimmed);
    if candidate_path.exists() {
        let package_json_path = if candidate_path.is_dir() {
            candidate_path.join("package.json")
        } else {
            bail!("local plugin installation currently supports package directories, not single files");
        };

        let raw = std::fs::read_to_string(&package_json_path)
            .with_context(|| format!("failed to read {}", package_json_path.display()))?;
        let package_json: PackageJson = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse {}", package_json_path.display()))?;
        return Ok(package_json.name);
    }

    if let Some(rest) = trimmed.strip_prefix("npm:") {
        return package_name_from_spec(rest);
    }

    if trimmed.starts_with('@') {
        let slash_idx = trimmed
            .find('/')
            .ok_or_else(|| anyhow!("invalid scoped package spec '{trimmed}'"))?;
        let tail = &trimmed[(slash_idx + 1)..];
        if let Some(version_sep) = tail.rfind('@') {
            if version_sep > 0 {
                return Ok(trimmed[..(slash_idx + 1 + version_sep)].to_string());
            }
        }
        return Ok(trimmed.to_string());
    }

    Ok(trimmed.split('@').next().unwrap_or(trimmed).to_string())
}

pub(super) fn package_dir(root: &Path, package_name: &str) -> PathBuf {
    let mut dir = root.join("node_modules");
    for part in package_name.split('/') {
        dir = dir.join(part);
    }
    dir
}

pub(super) fn load_registry() -> Result<PluginRegistry> {
    let path = registry_path()?;
    if !path.exists() {
        return Ok(PluginRegistry::default());
    }

    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let registry = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(registry)
}

pub(super) fn save_registry(registry: &PluginRegistry) -> Result<()> {
    let path = registry_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_vec_pretty(registry)?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub(super) fn validate_plugin_package(package_dir: &Path) -> Result<InstalledPluginRecord> {
    let package_json_path = package_dir.join("package.json");
    let raw = std::fs::read_to_string(&package_json_path)
        .with_context(|| format!("failed to read {}", package_json_path.display()))?;
    let package_json: PackageJson = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse {}", package_json_path.display()))?;

    let manifest = package_json.zorai_plugin.ok_or_else(|| {
        anyhow!(
            "package '{}' is missing the required 'zoraiPlugin' field",
            package_json.name
        )
    })?;

    let format = manifest.format().trim().to_lowercase();
    if format != "script" {
        bail!(
            "package '{}' declares unsupported zorai plugin format '{}'; only 'script' is currently supported",
            package_json.name,
            format
        );
    }

    let package_root = package_dir
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", package_dir.display()))?;
    let entry_path = package_dir.join(manifest.entry());
    if !entry_path.is_file() {
        bail!(
            "package '{}' declares zoraiPlugin.entry='{}' but the file does not exist",
            package_json.name,
            manifest.entry()
        );
    }

    let canonical_entry_path = entry_path
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", entry_path.display()))?;
    if canonical_entry_path != package_root && !canonical_entry_path.starts_with(&package_root) {
        bail!(
            "package '{}' declares zoraiPlugin.entry='{}' outside the installed package directory",
            package_json.name,
            manifest.entry()
        );
    }

    let installed_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Ok(InstalledPluginRecord {
        package_name: package_json.name.clone(),
        package_version: package_json
            .version
            .unwrap_or_else(|| "unknown".to_string()),
        plugin_name: package_json.name,
        entry_path: canonical_entry_path.to_string_lossy().to_string(),
        format,
        installed_at,
    })
}

pub(super) fn parse_github_url(url: &str) -> Option<(String, String)> {
    if let Some(rest) = url.strip_prefix("github:") {
        let parts: Vec<&str> = rest.splitn(2, '/').collect();
        if parts.len() == 2 {
            let repo = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
            return Some((parts[0].to_string(), repo.to_string()));
        }
    }

    if url.starts_with("git@github.com:") {
        let rest = url.strip_prefix("git@github.com:")?;
        let parts: Vec<&str> = rest.splitn(2, '/').collect();
        if parts.len() == 2 {
            let repo = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
            return Some((parts[0].to_string(), repo.to_string()));
        }
    }

    if url.contains("github.com/") {
        let after = url.split("github.com/").nth(1)?;
        let parts: Vec<&str> = after.splitn(3, '/').collect();
        if parts.len() >= 2 {
            let repo = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
            let repo = repo.split('/').next().unwrap_or(repo);
            let repo = repo.split('?').next().unwrap_or(repo);
            let repo = repo.split('#').next().unwrap_or(repo);
            return Some((parts[0].to_string(), repo.to_string()));
        }
    }

    None
}

pub(super) fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

pub(super) fn detect_nested_plugins(dir: &Path) -> Result<Vec<(PathBuf, String)>> {
    let mut nested: Vec<(PathBuf, String)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let subdir = entry.path();
            if subdir.is_dir() {
                let sub_manifest = subdir.join("plugin.json");
                if sub_manifest.exists() {
                    let bytes = std::fs::read(&sub_manifest)
                        .with_context(|| format!("failed to read {}", sub_manifest.display()))?;
                    let val: serde_json::Value = serde_json::from_slice(&bytes)
                        .with_context(|| format!("invalid JSON in {}", sub_manifest.display()))?;
                    let name = val
                        .get("name")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow!("{} missing 'name' field", sub_manifest.display()))?
                        .to_string();
                    nested.push((subdir, name));
                }
            }
        }
    }
    Ok(nested)
}

pub fn remove_plugin_files(name: &str) -> Result<()> {
    let root = plugins_root()?;
    let plugin_dir = root.join(name);
    if plugin_dir.exists() {
        std::fs::remove_dir_all(&plugin_dir).with_context(|| {
            format!("failed to remove plugin directory {}", plugin_dir.display())
        })?;
    }

    let skills_dir = zorai_protocol::ensure_zorai_data_dir()?
        .join("skills")
        .join("plugins")
        .join(name);
    if skills_dir.exists() {
        std::fs::remove_dir_all(&skills_dir).with_context(|| {
            format!("failed to remove plugin skills at {}", skills_dir.display())
        })?;
    }

    Ok(())
}

pub fn plugin_commands(commands: &[zorai_protocol::PluginCommandInfo]) {
    if commands.is_empty() {
        println!("No plugin commands registered.");
        return;
    }
    println!("{:<30} {:<20} {}", "COMMAND", "PLUGIN", "DESCRIPTION");
    for command in commands {
        println!(
            "{:<30} {:<20} {}",
            truncate(command.command.as_str(), 30),
            truncate(command.plugin_name.as_str(), 20),
            command.description,
        );
    }
    println!("\n{} command(s) registered.", commands.len());
}

fn truncate(value: &str, max: usize) -> String {
    let chars: String = value.chars().take(max).collect();
    if value.chars().count() > max {
        format!("{}…", &chars[..chars.len().saturating_sub(1)])
    } else {
        chars
    }
}
