use anyhow::{anyhow, Result};
use regex::Regex;
use std::sync::LazyLock;

pub const MAX_MANIFEST_SIZE: usize = 100 * 1024; // 100KB per D-10
pub const MAX_ENDPOINTS: usize = 50; // per D-10
pub const MAX_SETTINGS: usize = 30; // per D-10

/// Plugin name validation regex: lowercase alphanumeric with dots, hyphens, underscores.
static NAME_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z0-9]([a-z0-9._-]*[a-z0-9])?$").unwrap());

/// Validate plugin name against safe pattern. Returns the short name (plugin ID).
/// Accepts plain names (`my-plugin`) and scoped names (`@scope/my-plugin`).
pub fn validate_plugin_name(name: &str) -> Result<String> {
    if name.is_empty() {
        return Err(anyhow!("plugin name is empty"));
    }
    if name.contains("..") || name.contains('\\') {
        return Err(anyhow!(
            "plugin name contains forbidden characters: '{}'",
            name
        ));
    }

    if let Some(rest) = name.strip_prefix('@') {
        // Scoped name: @scope/short-name
        let parts: Vec<&str> = rest.splitn(2, '/').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Err(anyhow!("invalid scoped plugin name: '{}'", name));
        }
        let scope = parts[0];
        let short = parts[1];
        if !NAME_PATTERN.is_match(scope) {
            return Err(anyhow!("invalid scope in plugin name: '{}'", scope));
        }
        if !NAME_PATTERN.is_match(short) {
            return Err(anyhow!("invalid short name in plugin name: '{}'", short));
        }
        Ok(short.to_string())
    } else {
        if name.contains('/') {
            return Err(anyhow!(
                "plugin name contains forbidden characters: '{}'",
                name
            ));
        }
        if !NAME_PATTERN.is_match(name) {
            return Err(anyhow!("invalid plugin name pattern: '{}'", name));
        }
        Ok(name.to_string())
    }
}

/// Two-phase manifest validation (per D-09/D-10/PLUG-02/PLUG-08):
/// Phase 1: Size gate (100KB raw bytes)
/// Phase 2: JSON Schema validation using compiled validator
/// Phase 3: Structural limits (50 endpoints, 30 settings) + name validation
/// Returns (PluginManifest, raw JSON string) on success.
pub fn validate_manifest(
    raw_bytes: &[u8],
    validator: &jsonschema::Validator,
) -> Result<(super::manifest::PluginManifest, String)> {
    // Phase 1: Size gate
    if raw_bytes.len() > MAX_MANIFEST_SIZE {
        return Err(anyhow!(
            "manifest exceeds 100KB limit ({} bytes)",
            raw_bytes.len()
        ));
    }

    // Phase 2: JSON Schema validation
    let value: serde_json::Value =
        serde_json::from_slice(raw_bytes).map_err(|e| anyhow!("invalid JSON: {e}"))?;
    let errors: Vec<_> = validator.iter_errors(&value).collect();
    if !errors.is_empty() {
        let msgs: Vec<String> = errors
            .iter()
            .map(|e| format!("{}: {}", e.instance_path(), e))
            .collect();
        return Err(anyhow!("manifest validation failed:\n{}", msgs.join("\n")));
    }

    // Deserialize into typed struct
    let manifest: super::manifest::PluginManifest = serde_json::from_value(value)?;

    // Phase 3: Structural limits
    let endpoint_count = manifest
        .api
        .as_ref()
        .map(|a| a.endpoints.len())
        .unwrap_or(0);
    if endpoint_count > MAX_ENDPOINTS {
        return Err(anyhow!(
            "manifest has {} endpoints (max {})",
            endpoint_count,
            MAX_ENDPOINTS
        ));
    }

    let settings_count = manifest.settings.as_ref().map(|s| s.len()).unwrap_or(0);
    if settings_count > MAX_SETTINGS {
        return Err(anyhow!(
            "manifest has {} settings (max {})",
            settings_count,
            MAX_SETTINGS
        ));
    }

    // Validate plugin name
    validate_plugin_name(&manifest.name)?;

    let raw_json = String::from_utf8_lossy(raw_bytes).to_string();
    Ok((manifest, raw_json))
}

/// Result of scanning the plugins directory.
pub struct ScanResult {
    pub loaded: Vec<LoadedPlugin>,
    pub skipped: Vec<(String, String)>, // (dir_name, error_message)
}

/// A plugin successfully loaded from disk.
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub manifest: super::manifest::PluginManifest,
    pub manifest_json: String,
    pub dir_name: String,
}

/// Scan ~/.tamux/plugins/ directory and load all valid manifests.
/// Per D-09: skip and warn on failures, never block daemon startup.
pub fn scan_plugins_dir(
    plugins_dir: &std::path::Path,
    validator: &jsonschema::Validator,
) -> ScanResult {
    let mut result = ScanResult {
        loaded: Vec::new(),
        skipped: Vec::new(),
    };

    let entries = match std::fs::read_dir(plugins_dir) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!(path = %plugins_dir.display(), error = %e, "cannot read plugins directory");
            return result;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, "error reading plugins directory entry");
                continue;
            }
        };

        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };

        let manifest_path = path.join("plugin.json");
        if !manifest_path.exists() {
            result
                .skipped
                .push((dir_name.clone(), "no plugin.json found".to_string()));
            tracing::warn!(plugin = %dir_name, "skipping plugin directory: no plugin.json found");
            continue;
        }

        match std::fs::read(&manifest_path) {
            Ok(raw_bytes) => match validate_manifest(&raw_bytes, validator) {
                Ok((manifest, manifest_json)) => {
                    result.loaded.push(LoadedPlugin {
                        manifest,
                        manifest_json,
                        dir_name,
                    });
                }
                Err(e) => {
                    tracing::warn!(plugin = %dir_name, error = %e, "skipping invalid plugin manifest");
                    result.skipped.push((dir_name, e.to_string()));
                }
            },
            Err(e) => {
                tracing::warn!(plugin = %dir_name, error = %e, "failed to read plugin manifest");
                result.skipped.push((dir_name, e.to_string()));
            }
        }
    }

    result
}

#[cfg(test)]
#[path = "loader/tests.rs"]
mod tests;
