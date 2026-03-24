use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const PLUGINS_DIR: &str = "plugins";
const REGISTRY_FILE: &str = "registry.json";

// ---------------------------------------------------------------------------
// Legacy plugin types (kept for backward compat with `tamux install plugin`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPluginRecord {
    pub package_name: String,
    pub package_version: String,
    pub plugin_name: String,
    pub entry_path: String,
    pub format: String,
    pub installed_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PluginRegistry {
    #[serde(default)]
    plugins: Vec<InstalledPluginRecord>,
}

#[derive(Debug, Deserialize)]
struct PackageJson {
    name: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default, rename = "tamuxPlugin", alias = "amuxPlugin")]
    tamux_plugin: Option<AmuxPluginManifest>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AmuxPluginManifest {
    EntryPath(String),
    Detailed {
        entry: String,
        #[serde(default)]
        format: Option<String>,
    },
}

impl AmuxPluginManifest {
    fn entry(&self) -> &str {
        match self {
            Self::EntryPath(entry) => entry,
            Self::Detailed { entry, .. } => entry,
        }
    }

    fn format(&self) -> &str {
        match self {
            Self::EntryPath(_) => "script",
            Self::Detailed { format, .. } => format.as_deref().unwrap_or("script"),
        }
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn plugins_root() -> Result<PathBuf> {
    Ok(amux_protocol::ensure_amux_data_dir()?.join(PLUGINS_DIR))
}

fn registry_path() -> Result<PathBuf> {
    Ok(plugins_root()?.join(REGISTRY_FILE))
}

fn ensure_plugin_workspace(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root)?;

    let package_json_path = root.join("package.json");
    if !package_json_path.exists() {
        let content = serde_json::json!({
            "name": "tamux-external-plugins",
            "private": true,
            "description": "Runtime-installed tamux plugins"
        });
        std::fs::write(package_json_path, serde_json::to_vec_pretty(&content)?)?;
    }

    Ok(())
}

fn npm_command() -> &'static str {
    if cfg!(windows) {
        "npm.cmd"
    } else {
        "npm"
    }
}

fn package_name_from_spec(spec: &str) -> Result<String> {
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

fn package_dir(root: &Path, package_name: &str) -> PathBuf {
    let mut dir = root.join("node_modules");
    for part in package_name.split('/') {
        dir = dir.join(part);
    }
    dir
}

// ---------------------------------------------------------------------------
// Legacy registry (for `tamux install plugin`)
// ---------------------------------------------------------------------------

fn load_registry() -> Result<PluginRegistry> {
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

fn save_registry(registry: &PluginRegistry) -> Result<()> {
    let path = registry_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_vec_pretty(registry)?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn validate_plugin_package(package_dir: &Path) -> Result<InstalledPluginRecord> {
    let package_json_path = package_dir.join("package.json");
    let raw = std::fs::read_to_string(&package_json_path)
        .with_context(|| format!("failed to read {}", package_json_path.display()))?;
    let package_json: PackageJson = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse {}", package_json_path.display()))?;

    let manifest = package_json
        .tamux_plugin
        .ok_or_else(|| anyhow!("package '{}' is missing the required 'tamuxPlugin' field (legacy 'amuxPlugin' is also accepted)", package_json.name))?;

    let format = manifest.format().trim().to_lowercase();
    if format != "script" {
        bail!(
            "package '{}' declares unsupported tamux plugin format '{}'; only 'script' is currently supported",
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
            "package '{}' declares tamuxPlugin.entry='{}' but the file does not exist",
            package_json.name,
            manifest.entry()
        );
    }

    let canonical_entry_path = entry_path
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", entry_path.display()))?;
    if canonical_entry_path != package_root && !canonical_entry_path.starts_with(&package_root) {
        bail!(
            "package '{}' declares tamuxPlugin.entry='{}' outside the installed package directory",
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
        package_version: package_json.version.unwrap_or_else(|| "0.0.0".to_string()),
        plugin_name: package_json.name,
        entry_path: canonical_entry_path.to_string_lossy().to_string(),
        format,
        installed_at,
    })
}

/// Legacy plugin install (npm package with tamuxPlugin field in package.json).
/// Used by `tamux install plugin <package>`.
pub fn install_plugin(package_spec: &str) -> Result<InstalledPluginRecord> {
    let root = plugins_root()?;
    ensure_plugin_workspace(&root)?;

    let package_name = package_name_from_spec(package_spec)?;

    let status = Command::new(npm_command())
        .arg("install")
        .arg("--ignore-scripts")
        .arg("--prefix")
        .arg(&root)
        .arg(package_spec)
        .status()
        .with_context(|| {
            "failed to launch npm; ensure Node.js and npm are installed and on PATH"
        })?;

    if !status.success() {
        bail!("npm install failed for plugin spec '{package_spec}'");
    }

    let installed_dir = package_dir(&root, &package_name);
    if !installed_dir.exists() {
        bail!(
            "npm reported success, but the installed package directory was not found at {}",
            installed_dir.display()
        );
    }

    let installed = validate_plugin_package(&installed_dir)?;
    let mut registry = load_registry()?;
    registry
        .plugins
        .retain(|existing| existing.package_name != installed.package_name);
    registry.plugins.push(installed.clone());
    registry
        .plugins
        .sort_by(|left, right| left.package_name.cmp(&right.package_name));
    save_registry(&registry)?;

    Ok(installed)
}

// ===========================================================================
// V2 Plugin Install/Uninstall (plugin.json manifest format)
// ===========================================================================

/// Detected install source type.
#[derive(Debug, Clone, PartialEq)]
pub enum PluginSource {
    /// npm registry package (default fallback).
    Npm(String),
    /// GitHub repository URL (https or git@ or github: shorthand).
    GitHub {
        owner: String,
        repo: String,
        url: String,
    },
    /// Local directory path.
    Local(PathBuf),
}

/// Auto-detect source type from user argument. Per D-01.
/// - Contains "github.com" or ends in ".git" or starts with "github:" -> GitHub
/// - Is an existing local path -> Local
/// - Otherwise -> npm
pub fn detect_source_type(spec: &str) -> PluginSource {
    let trimmed = spec.trim();

    // GitHub detection
    if trimmed.contains("github.com") || trimmed.ends_with(".git") || trimmed.starts_with("github:")
    {
        if let Some((owner, repo)) = parse_github_url(trimmed) {
            return PluginSource::GitHub {
                owner,
                repo,
                url: trimmed.to_string(),
            };
        }
    }

    // Local path detection
    let candidate = Path::new(trimmed);
    if candidate.exists() && candidate.is_dir() {
        return PluginSource::Local(candidate.to_path_buf());
    }

    // Default: npm
    PluginSource::Npm(trimmed.to_string())
}

/// Parse GitHub URL patterns into (owner, repo).
/// Supports: https://github.com/user/repo, https://github.com/user/repo.git,
/// git@github.com:user/repo.git, github:user/repo
fn parse_github_url(url: &str) -> Option<(String, String)> {
    // github:user/repo shorthand
    if let Some(rest) = url.strip_prefix("github:") {
        let parts: Vec<&str> = rest.splitn(2, '/').collect();
        if parts.len() == 2 {
            let repo = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
            return Some((parts[0].to_string(), repo.to_string()));
        }
    }

    // git@github.com:user/repo.git (SSH)
    if url.starts_with("git@github.com:") {
        let rest = url.strip_prefix("git@github.com:")?;
        let parts: Vec<&str> = rest.splitn(2, '/').collect();
        if parts.len() == 2 {
            let repo = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
            return Some((parts[0].to_string(), repo.to_string()));
        }
    }

    // https://github.com/user/repo[.git]
    if url.contains("github.com/") {
        let after = url.split("github.com/").nth(1)?;
        let parts: Vec<&str> = after.splitn(3, '/').collect();
        if parts.len() >= 2 {
            let repo = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
            // Remove any trailing path after repo name
            let repo = repo.split('/').next().unwrap_or(repo);
            let repo = repo.split('?').next().unwrap_or(repo);
            let repo = repo.split('#').next().unwrap_or(repo);
            return Some((parts[0].to_string(), repo.to_string()));
        }
    }

    None
}

/// Recursively copy a directory tree.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
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

/// Install plugin from npm. Uses --ignore-scripts per INST-08/D-03.
/// Copies from node_modules into ~/.tamux/plugins/{name}/ directory.
/// Returns (dir_name, plugin_name) on success.
pub fn install_from_npm(package_spec: &str) -> Result<(String, String)> {
    let root = plugins_root()?;
    ensure_plugin_workspace(&root)?;

    // Install to a temp node_modules area using npm
    let status = Command::new(npm_command())
        .arg("install")
        .arg("--ignore-scripts")
        .arg("--prefix")
        .arg(&root)
        .arg(package_spec)
        .status()
        .with_context(|| "failed to launch npm; ensure Node.js and npm are installed and on PATH")?;

    if !status.success() {
        bail!("npm install failed for '{}'", package_spec);
    }

    // Find the installed package name
    let package_name = package_name_from_spec(package_spec)?;
    let installed_dir = package_dir(&root, &package_name);
    if !installed_dir.exists() {
        bail!(
            "npm reported success but package directory not found at {}",
            installed_dir.display()
        );
    }

    // Look for plugin.json (new v2 format) first, fall back to tamuxPlugin in package.json (legacy)
    let plugin_json = installed_dir.join("plugin.json");
    if !plugin_json.exists() {
        // Check for legacy format
        let pkg_json = installed_dir.join("package.json");
        if pkg_json.exists() {
            let raw = std::fs::read_to_string(&pkg_json)?;
            let pkg: serde_json::Value = serde_json::from_str(&raw)?;
            if pkg.get("tamuxPlugin").is_some() || pkg.get("amuxPlugin").is_some() {
                bail!(
                    "Package '{}' uses legacy tamuxPlugin format. Use 'tamux install plugin {}' for legacy plugins.",
                    package_name, package_spec
                );
            }
        }
        bail!(
            "Package '{}' does not contain a plugin.json manifest",
            package_name
        );
    }

    // Read plugin name from manifest
    let manifest_bytes = std::fs::read(&plugin_json)?;
    let manifest_value: serde_json::Value =
        serde_json::from_slice(&manifest_bytes).with_context(|| "invalid JSON in plugin.json")?;
    let plugin_name = manifest_value
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("plugin.json missing 'name' field"))?
        .to_string();

    // Copy plugin dir to ~/.tamux/plugins/{plugin_name}/
    let target_dir = root.join(&plugin_name);
    if target_dir.exists() && target_dir != installed_dir {
        std::fs::remove_dir_all(&target_dir)
            .with_context(|| format!("failed to remove existing plugin at {}", target_dir.display()))?;
    }

    // If npm installed to node_modules/{package}, copy to plugins/{plugin_name}
    if installed_dir != target_dir {
        copy_dir_recursive(&installed_dir, &target_dir)?;
    }

    Ok((plugin_name.clone(), plugin_name))
}

/// Install plugin from GitHub. Tries git clone first, falls back to tarball download.
/// Per D-02: supports private repos via SSH when using git clone.
pub fn install_from_github(owner: &str, repo: &str, url: &str) -> Result<(String, String)> {
    let root = plugins_root()?;
    std::fs::create_dir_all(&root)?;

    // Try git clone first
    let git_available = Command::new("git")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let temp_dir = tempfile::TempDir::new_in(&root)?;
    let clone_target = temp_dir.path().join(repo);

    if git_available {
        let clone_url = if url.starts_with("github:") {
            format!("https://github.com/{}/{}.git", owner, repo)
        } else {
            url.to_string()
        };

        let status = Command::new("git")
            .arg("clone")
            .arg("--depth")
            .arg("1")
            .arg(&clone_url)
            .arg(&clone_target)
            .status()
            .with_context(|| "failed to run git clone")?;

        if !status.success() {
            // Fall back to tarball
            return install_github_tarball(owner, repo, &root, temp_dir);
        }
    } else {
        // No git available, try tarball
        return install_github_tarball(owner, repo, &root, temp_dir);
    }

    // Validate plugin.json exists in cloned repo
    let plugin_json = clone_target.join("plugin.json");
    if !plugin_json.exists() {
        bail!(
            "GitHub repo {}/{} does not contain a plugin.json manifest",
            owner,
            repo
        );
    }

    let manifest_bytes = std::fs::read(&plugin_json)?;
    let manifest_value: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
    let plugin_name = manifest_value
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("plugin.json missing 'name' field"))?
        .to_string();

    // Remove .git directory to save space
    let git_dir = clone_target.join(".git");
    if git_dir.exists() {
        let _ = std::fs::remove_dir_all(&git_dir);
    }

    // Move to final location
    let target_dir = root.join(&plugin_name);
    if target_dir.exists() {
        std::fs::remove_dir_all(&target_dir)?;
    }
    // Use copy+remove instead of rename (may be cross-filesystem)
    copy_dir_recursive(&clone_target, &target_dir)?;

    Ok((plugin_name.clone(), plugin_name))
}

/// Fallback: download GitHub tarball (for when git is not installed). Per D-02.
/// IMPORTANT: Uses reqwest::blocking::Client (synchronous HTTP) instead of async reqwest.
/// The CLI runs inside #[tokio::main], and calling block_on() from within an existing
/// tokio runtime panics with "Cannot start a runtime from within a runtime."
/// reqwest::blocking::Client spawns its own internal thread and is safe to call from
/// any context. Requires `features = ["blocking"]` on the reqwest dependency.
fn install_github_tarball(
    owner: &str,
    repo: &str,
    plugins_root: &Path,
    _temp_dir: tempfile::TempDir,
) -> Result<(String, String)> {
    let tarball_url = format!(
        "https://api.github.com/repos/{}/{}/tarball/HEAD",
        owner, repo
    );

    let response = reqwest::blocking::Client::new()
        .get(&tarball_url)
        .header("User-Agent", "tamux-cli")
        .header("Accept", "application/vnd.github+json")
        .send()
        .with_context(|| format!("failed to download tarball from {}", tarball_url))?;

    if !response.status().is_success() {
        bail!(
            "GitHub API returned {} for {}/{}. Repository may be private or not found.",
            response.status(),
            owner,
            repo
        );
    }

    let bytes = response
        .bytes()
        .with_context(|| "failed to read tarball response")?;

    // Extract tarball to temp dir using tar command
    let temp_extract = tempfile::TempDir::new_in(plugins_root)?;
    let tarball_path = temp_extract.path().join("download.tar.gz");
    std::fs::write(&tarball_path, &bytes)?;

    let status = Command::new("tar")
        .arg("xzf")
        .arg(&tarball_path)
        .arg("-C")
        .arg(temp_extract.path())
        .status()
        .with_context(|| "failed to extract tarball (is 'tar' installed?)")?;

    if !status.success() {
        bail!("tar extraction failed");
    }

    // GitHub tarballs extract to owner-repo-sha/ directory
    let extracted_dirs: Vec<_> = std::fs::read_dir(temp_extract.path())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();

    let extracted_dir = extracted_dirs
        .first()
        .ok_or_else(|| anyhow!("no directory found in extracted tarball"))?
        .path();

    let plugin_json = extracted_dir.join("plugin.json");
    if !plugin_json.exists() {
        bail!(
            "GitHub repo {}/{} does not contain a plugin.json manifest",
            owner,
            repo
        );
    }

    let manifest_bytes = std::fs::read(&plugin_json)?;
    let manifest_value: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
    let plugin_name = manifest_value
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("plugin.json missing 'name' field"))?
        .to_string();

    let target_dir = plugins_root.join(&plugin_name);
    if target_dir.exists() {
        std::fs::remove_dir_all(&target_dir)?;
    }
    copy_dir_recursive(&extracted_dir, &target_dir)?;

    Ok((plugin_name.clone(), plugin_name))
}

/// Install plugin from local directory. Copies files to ~/.tamux/plugins/{name}/.
pub fn install_from_local(local_path: &Path) -> Result<(String, String)> {
    let root = plugins_root()?;
    std::fs::create_dir_all(&root)?;

    let plugin_json = local_path.join("plugin.json");
    if !plugin_json.exists() {
        bail!(
            "Directory '{}' does not contain a plugin.json manifest",
            local_path.display()
        );
    }

    let manifest_bytes = std::fs::read(&plugin_json)?;
    let manifest_value: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
    let plugin_name = manifest_value
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("plugin.json missing 'name' field"))?
        .to_string();

    let target_dir = root.join(&plugin_name);

    // If source == target (already in plugins dir), skip copy
    let source_canonical = local_path
        .canonicalize()
        .unwrap_or_else(|_| local_path.to_path_buf());
    let target_canonical_check = if target_dir.exists() {
        target_dir
            .canonicalize()
            .unwrap_or_else(|_| target_dir.clone())
    } else {
        target_dir.clone()
    };

    if source_canonical != target_canonical_check {
        if target_dir.exists() {
            std::fs::remove_dir_all(&target_dir)?;
        }
        copy_dir_recursive(local_path, &target_dir)?;
    }

    Ok((plugin_name.clone(), plugin_name))
}

/// Unified v2 plugin install. Auto-detects source, installs files, returns (dir_name, source_label).
/// Does NOT register with daemon -- caller handles IPC separately.
pub fn install_plugin_v2(spec: &str) -> Result<(String, String)> {
    let source = detect_source_type(spec);
    match source {
        PluginSource::Npm(package) => {
            let (dir_name, _name) = install_from_npm(&package)?;
            Ok((dir_name, format!("npm:{}", package)))
        }
        PluginSource::GitHub { owner, repo, url } => {
            let (dir_name, _name) = install_from_github(&owner, &repo, &url)?;
            Ok((dir_name, format!("github:{}/{}", owner, repo)))
        }
        PluginSource::Local(path) => {
            let (dir_name, _name) = install_from_local(&path)?;
            Ok((dir_name, format!("local:{}", path.display())))
        }
    }
}

/// Remove plugin files from disk. Per D-06.
/// Also removes bundled skills from ~/.tamux/skills/plugins/{name}/ if it exists.
/// Uses amux_protocol::ensure_amux_data_dir() (exported from amux_protocol::config).
pub fn remove_plugin_files(name: &str) -> Result<()> {
    let root = plugins_root()?;
    let plugin_dir = root.join(name);
    if plugin_dir.exists() {
        std::fs::remove_dir_all(&plugin_dir)
            .with_context(|| format!("failed to remove plugin directory {}", plugin_dir.display()))?;
    }

    // Also clean up bundled skills (Phase 19 will populate these)
    let skills_dir = amux_protocol::ensure_amux_data_dir()?
        .join("skills")
        .join("plugins")
        .join(name);
    if skills_dir.exists() {
        std::fs::remove_dir_all(&skills_dir)
            .with_context(|| format!("failed to remove plugin skills at {}", skills_dir.display()))?;
    }

    Ok(())
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_npm_package() {
        assert_eq!(
            detect_source_type("tamux-plugin-gmail"),
            PluginSource::Npm("tamux-plugin-gmail".to_string())
        );
        assert_eq!(
            detect_source_type("@tamux/plugin-gmail"),
            PluginSource::Npm("@tamux/plugin-gmail".to_string())
        );
    }

    #[test]
    fn detect_github_https() {
        match detect_source_type("https://github.com/user/tamux-plugin-test") {
            PluginSource::GitHub { owner, repo, .. } => {
                assert_eq!(owner, "user");
                assert_eq!(repo, "tamux-plugin-test");
            }
            other => panic!("expected GitHub, got {:?}", other),
        }
    }

    #[test]
    fn detect_github_ssh() {
        match detect_source_type("git@github.com:user/plugin.git") {
            PluginSource::GitHub { owner, repo, .. } => {
                assert_eq!(owner, "user");
                assert_eq!(repo, "plugin");
            }
            other => panic!("expected GitHub, got {:?}", other),
        }
    }

    #[test]
    fn detect_github_shorthand() {
        match detect_source_type("github:user/my-plugin") {
            PluginSource::GitHub { owner, repo, .. } => {
                assert_eq!(owner, "user");
                assert_eq!(repo, "my-plugin");
            }
            other => panic!("expected GitHub, got {:?}", other),
        }
    }

    #[test]
    fn parse_github_urls() {
        assert_eq!(
            parse_github_url("https://github.com/foo/bar"),
            Some(("foo".into(), "bar".into()))
        );
        assert_eq!(
            parse_github_url("https://github.com/foo/bar.git"),
            Some(("foo".into(), "bar".into()))
        );
        assert_eq!(
            parse_github_url("git@github.com:foo/bar.git"),
            Some(("foo".into(), "bar".into()))
        );
        assert_eq!(
            parse_github_url("github:foo/bar"),
            Some(("foo".into(), "bar".into()))
        );
    }
}
