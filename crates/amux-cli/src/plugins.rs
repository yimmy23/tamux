use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

mod helpers;
use helpers::{
    copy_dir_recursive, detect_nested_plugins, ensure_plugin_workspace, load_registry, npm_command,
    package_dir, package_name_from_spec, parse_github_url, plugins_root, save_registry,
    validate_plugin_package,
};
pub use helpers::{plugin_commands, remove_plugin_files};

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

    let candidate = Path::new(trimmed);
    if candidate.exists() && candidate.is_dir() {
        return PluginSource::Local(candidate.to_path_buf());
    }

    PluginSource::Npm(trimmed.to_string())
}

/// Install plugin from npm. Uses --ignore-scripts per INST-08/D-03.
/// Copies from node_modules into ~/.tamux/plugins/{name}/ directory.
/// Returns Vec of (dir_name, plugin_name) -- single entry for root plugin.json,
/// multiple entries for nested plugin subdirectories (e.g. gmail + calendar).
pub fn install_from_npm(package_spec: &str) -> Result<Vec<(String, String)>> {
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
        .with_context(|| {
            "failed to launch npm; ensure Node.js and npm are installed and on PATH"
        })?;

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

    // Look for plugin.json (new v2 format) at root first (Pitfall 5: backward compat)
    let plugin_json = installed_dir.join("plugin.json");
    if plugin_json.exists() {
        // Single plugin at root (existing behavior, now returns Vec with one item)
        let manifest_bytes = std::fs::read(&plugin_json)?;
        let manifest_value: serde_json::Value = serde_json::from_slice(&manifest_bytes)
            .with_context(|| "invalid JSON in plugin.json")?;
        let plugin_name = manifest_value
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("plugin.json missing 'name' field"))?
            .to_string();

        let target_dir = root.join(&plugin_name);
        if target_dir.exists() && target_dir != installed_dir {
            std::fs::remove_dir_all(&target_dir).with_context(|| {
                format!(
                    "failed to remove existing plugin at {}",
                    target_dir.display()
                )
            })?;
        }
        if installed_dir != target_dir {
            copy_dir_recursive(&installed_dir, &target_dir)?;
        }

        return Ok(vec![(plugin_name.clone(), plugin_name)]);
    }

    // No root plugin.json -- check for nested plugin subdirectories (D-03)
    let nested_plugins = detect_nested_plugins(&installed_dir)?;

    if nested_plugins.is_empty() {
        // Check for legacy format before bailing
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
            "Package '{}' does not contain a plugin.json manifest (at root or in subdirectories)",
            package_name
        );
    }

    // Copy each nested plugin to ~/.tamux/plugins/{plugin_name}/
    let mut installed = Vec::new();
    for (subdir, plugin_name) in &nested_plugins {
        let target_dir = root.join(plugin_name);
        if target_dir.exists() {
            std::fs::remove_dir_all(&target_dir).with_context(|| {
                format!(
                    "failed to remove existing plugin at {}",
                    target_dir.display()
                )
            })?;
        }
        copy_dir_recursive(subdir, &target_dir)?;
        installed.push((plugin_name.clone(), plugin_name.clone()));
    }

    Ok(installed)
}

/// Install plugin from GitHub. Tries git clone first, falls back to tarball download.
/// Per D-02: supports private repos via SSH when using git clone.
/// Returns Vec of (dir_name, plugin_name) for nested plugin support.
pub fn install_from_github(owner: &str, repo: &str, url: &str) -> Result<Vec<(String, String)>> {
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

    // Remove .git directory to save space before processing
    let git_dir = clone_target.join(".git");
    if git_dir.exists() {
        let _ = std::fs::remove_dir_all(&git_dir);
    }

    // Check root plugin.json first (Pitfall 5: backward compat)
    let plugin_json = clone_target.join("plugin.json");
    if plugin_json.exists() {
        let manifest_bytes = std::fs::read(&plugin_json)?;
        let manifest_value: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
        let plugin_name = manifest_value
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("plugin.json missing 'name' field"))?
            .to_string();

        let target_dir = root.join(&plugin_name);
        if target_dir.exists() {
            std::fs::remove_dir_all(&target_dir)?;
        }
        copy_dir_recursive(&clone_target, &target_dir)?;

        return Ok(vec![(plugin_name.clone(), plugin_name)]);
    }

    // No root plugin.json -- check for nested plugin subdirectories (D-03)
    let nested_plugins = detect_nested_plugins(&clone_target)?;
    if nested_plugins.is_empty() {
        bail!(
            "GitHub repo {}/{} does not contain a plugin.json manifest (at root or in subdirectories)",
            owner,
            repo
        );
    }

    let mut installed = Vec::new();
    for (subdir, plugin_name) in &nested_plugins {
        let target_dir = root.join(plugin_name);
        if target_dir.exists() {
            std::fs::remove_dir_all(&target_dir)?;
        }
        copy_dir_recursive(subdir, &target_dir)?;
        installed.push((plugin_name.clone(), plugin_name.clone()));
    }

    Ok(installed)
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
) -> Result<Vec<(String, String)>> {
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

    // Check root plugin.json first (Pitfall 5: backward compat)
    let plugin_json = extracted_dir.join("plugin.json");
    if plugin_json.exists() {
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

        return Ok(vec![(plugin_name.clone(), plugin_name)]);
    }

    // No root plugin.json -- check for nested plugin subdirectories (D-03)
    let nested_plugins = detect_nested_plugins(&extracted_dir)?;
    if nested_plugins.is_empty() {
        bail!(
            "GitHub repo {}/{} does not contain a plugin.json manifest (at root or in subdirectories)",
            owner,
            repo
        );
    }

    let mut installed = Vec::new();
    for (subdir, plugin_name) in &nested_plugins {
        let target_dir = plugins_root.join(plugin_name);
        if target_dir.exists() {
            std::fs::remove_dir_all(&target_dir)?;
        }
        copy_dir_recursive(subdir, &target_dir)?;
        installed.push((plugin_name.clone(), plugin_name.clone()));
    }

    Ok(installed)
}

/// Install plugin from local directory. Copies files to ~/.tamux/plugins/{name}/.
/// Returns Vec of (dir_name, plugin_name) for nested plugin support.
pub fn install_from_local(local_path: &Path) -> Result<Vec<(String, String)>> {
    let root = plugins_root()?;
    std::fs::create_dir_all(&root)?;

    // Check root plugin.json first (Pitfall 5: backward compat)
    let plugin_json = local_path.join("plugin.json");
    if plugin_json.exists() {
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

        return Ok(vec![(plugin_name.clone(), plugin_name)]);
    }

    // No root plugin.json -- check for nested plugin subdirectories (D-03)
    let nested_plugins = detect_nested_plugins(local_path)?;
    if nested_plugins.is_empty() {
        bail!(
            "Directory '{}' does not contain a plugin.json manifest (at root or in subdirectories)",
            local_path.display()
        );
    }

    let mut installed = Vec::new();
    for (subdir, plugin_name) in &nested_plugins {
        let target_dir = root.join(plugin_name);

        let source_canonical = subdir
            .canonicalize()
            .unwrap_or_else(|_| subdir.to_path_buf());
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
            copy_dir_recursive(subdir, &target_dir)?;
        }
        installed.push((plugin_name.clone(), plugin_name.clone()));
    }

    Ok(installed)
}

/// Unified v2 plugin install. Auto-detects source, installs files,
/// returns Vec of (dir_name, source_label) for multi-plugin packages.
/// Does NOT register with daemon -- caller handles IPC separately.
pub fn install_plugin_v2(spec: &str) -> Result<Vec<(String, String)>> {
    let source = detect_source_type(spec);
    match source {
        PluginSource::Npm(package) => {
            let results = install_from_npm(&package)?;
            Ok(results
                .into_iter()
                .map(|(dir_name, _name)| (dir_name, format!("npm:{}", package)))
                .collect())
        }
        PluginSource::GitHub { owner, repo, url } => {
            let results = install_from_github(&owner, &repo, &url)?;
            Ok(results
                .into_iter()
                .map(|(dir_name, _name)| (dir_name, format!("github:{}/{}", owner, repo)))
                .collect())
        }
        PluginSource::Local(path) => {
            let results = install_from_local(&path)?;
            Ok(results
                .into_iter()
                .map(|(dir_name, _name)| (dir_name, format!("local:{}", path.display())))
                .collect())
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[path = "tests/plugins.rs"]
mod tests;
