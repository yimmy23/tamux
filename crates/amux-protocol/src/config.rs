use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

pub const DEFAULT_TCP_HOST: &str = "127.0.0.1";
pub const DEFAULT_TCP_PORT: u16 = 17563;
const TAMUX_DATA_DIR_ENV: &str = "TAMUX_DATA_DIR";

pub fn default_tcp_addr() -> String {
    format!("{DEFAULT_TCP_HOST}:{DEFAULT_TCP_PORT}")
}

fn tamux_data_dir_override() -> Option<PathBuf> {
    std::env::var_os(TAMUX_DATA_DIR_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn legacy_amux_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("amux")
    }

    #[cfg(not(windows))]
    {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".amux")
    }
}

pub fn tamux_data_dir() -> PathBuf {
    if let Some(path) = tamux_data_dir_override() {
        return path;
    }

    #[cfg(windows)]
    {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("tamux")
    }

    #[cfg(not(windows))]
    {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".tamux")
    }
}

pub fn amux_data_dir() -> PathBuf {
    tamux_data_dir()
}

fn migrate_legacy_data_dir() -> std::io::Result<()> {
    if tamux_data_dir_override().is_some() {
        return Ok(());
    }

    let target = tamux_data_dir();
    let legacy = legacy_amux_data_dir();
    if target.exists() || !legacy.exists() {
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let _ = std::fs::rename(&legacy, &target);
    Ok(())
}

fn legacy_config_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("amux").join("config.json")
}

fn tamux_config_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("tamux").join("config.json")
}

pub fn ensure_tamux_data_dir() -> std::io::Result<PathBuf> {
    migrate_legacy_data_dir()?;
    let dir = tamux_data_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn ensure_amux_data_dir() -> std::io::Result<PathBuf> {
    ensure_tamux_data_dir()
}

pub fn log_file_path(file_name: &str) -> PathBuf {
    tamux_data_dir().join(file_name)
}

pub fn parse_whatsapp_allowed_contacts(raw: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut contacts = Vec::new();

    for entry in raw.split([',', '\n']) {
        let Some(normalized) = normalize_whatsapp_phone_like_identifier(entry) else {
            continue;
        };

        if seen.insert(normalized.clone()) {
            contacts.push(normalized);
        }
    }

    contacts
}

pub fn has_whatsapp_allowed_contacts(raw: &str) -> bool {
    raw.split([',', '\n'])
        .any(|entry| normalize_whatsapp_phone_like_identifier(entry).is_some())
}

pub fn normalize_whatsapp_phone_like_identifier(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = if let Some(phone) = trimmed.strip_suffix("@s.whatsapp.net") {
        phone
    } else if let Some(phone) = trimmed.strip_suffix("@c.us") {
        phone
    } else if trimmed.contains('@') {
        return None;
    } else {
        trimmed
    };

    let mut chars = candidate.chars().peekable();
    let mut saw_group = false;

    if matches!(chars.peek(), Some('+')) {
        chars.next();
    }

    while chars.peek().is_some() {
        let mut digit_count = 0;

        if matches!(chars.peek(), Some('(')) {
            chars.next();
            while matches!(chars.peek(), Some(ch) if ch.is_ascii_digit()) {
                chars.next();
                digit_count += 1;
            }

            if digit_count == 0 || !matches!(chars.next(), Some(')')) {
                return None;
            }
        } else {
            while matches!(chars.peek(), Some(ch) if ch.is_ascii_digit()) {
                chars.next();
                digit_count += 1;
            }

            if digit_count == 0 {
                return None;
            }
        }

        saw_group = true;

        match chars.peek() {
            None => break,
            Some(' ') | Some('-') => {
                chars.next();
                if matches!(chars.peek(), None | Some(' ') | Some('-')) {
                    return None;
                }
            }
            _ => return None,
        }
    }

    if !saw_group {
        return None;
    }

    let digits: String = candidate.chars().filter(|ch| ch.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        Some(digits)
    }
}

/// User-configurable settings for tamux.
///
/// Config file locations:
/// - Linux:   ~/.config/tamux/config.json
/// - macOS:   ~/Library/Application Support/tamux/config.json
/// - Windows: %APPDATA%\tamux\config.json
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AmuxConfig {
    /// Default shell to spawn (None = system default).
    pub default_shell: Option<String>,

    /// Default terminal dimensions.
    pub default_cols: u16,
    pub default_rows: u16,

    /// Scrollback buffer size in bytes.
    pub scrollback_bytes: usize,

    /// IPC mode: "socket" on Unix platforms or "tcp" on Windows.
    pub ipc_mode: String,

    /// TCP port for IPC when TCP mode is active.
    pub tcp_port: u16,

    /// Font settings (used by the frontend).
    pub font_family: String,
    pub font_size: u16,

    /// Theme name (loaded by frontend).
    pub theme: String,

    /// Whether to auto-start the daemon when the UI launches.
    pub auto_start_daemon: bool,

    /// Whether workspace sandboxing is enabled for managed commands.
    pub sandbox_enabled: bool,

    /// Snapshot backend: "auto", "tar", "zfs", "btrfs". Default is "auto".
    pub snapshot_backend: Option<String>,

    /// Maximum number of snapshots to keep.
    pub snapshot_max_count: usize,

    /// Maximum snapshot archive size in megabytes.
    ///
    /// Retention also uses this as the total budget when auto-cleanup is enabled.
    /// Setting `snapshot_max_count = 0` disables snapshots entirely.
    pub snapshot_max_total_size_mb: u64,

    /// Whether snapshot retention is enforced automatically after create().
    pub snapshot_auto_cleanup: bool,

    /// Cerbos PDP endpoint for external policy evaluation (e.g. "http://localhost:3592").
    pub cerbos_endpoint: Option<String>,
}

impl Default for AmuxConfig {
    fn default() -> Self {
        Self {
            default_shell: None,
            default_cols: 80,
            default_rows: 24,
            scrollback_bytes: 1024 * 1024, // 1 MiB
            ipc_mode: if cfg!(unix) {
                "socket".into()
            } else {
                "tcp".into()
            },
            tcp_port: DEFAULT_TCP_PORT,
            font_family: "Cascadia Code".into(),
            font_size: 14,
            theme: "catppuccin-mocha".into(),
            auto_start_daemon: true,
            sandbox_enabled: true,
            snapshot_backend: None,
            snapshot_max_count: 0,
            snapshot_max_total_size_mb: 10_240,
            snapshot_auto_cleanup: false,
            cerbos_endpoint: None,
        }
    }
}

impl AmuxConfig {
    /// Load config from the default location, or return defaults.
    pub fn load() -> Self {
        for path in [Self::config_path(), legacy_config_path()] {
            if !path.exists() {
                continue;
            }

            match std::fs::read_to_string(&path) {
                Ok(data) => match serde_json::from_str(&data) {
                    Ok(cfg) => return cfg,
                    Err(e) => {
                        eprintln!("warning: invalid config file: {e}");
                    }
                },
                Err(e) => {
                    eprintln!("warning: cannot read config file: {e}");
                }
            }
        }
        Self::default()
    }

    /// Save the current config to disk.
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&path, data)
    }

    /// Platform-specific config file path.
    pub fn config_path() -> PathBuf {
        tamux_config_path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvGuard {
        key: &'static str,
        original: Option<OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
            let original = std::env::var_os(key);
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, original }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.original.take() {
                Some(value) => unsafe {
                    std::env::set_var(self.key, value);
                },
                None => unsafe {
                    std::env::remove_var(self.key);
                },
            }
        }
    }

    #[test]
    fn whatsapp_allowlist_normalize_phone_like_identifiers_conservatively() {
        assert_eq!(
            normalize_whatsapp_phone_like_identifier(" +1 (206) 555-0123 "),
            Some("12065550123".to_string())
        );
        assert_eq!(
            normalize_whatsapp_phone_like_identifier("12065550123@s.whatsapp.net"),
            Some("12065550123".to_string())
        );
        assert_eq!(
            normalize_whatsapp_phone_like_identifier("12065550123@c.us"),
            Some("12065550123".to_string())
        );
        assert_eq!(normalize_whatsapp_phone_like_identifier(""), None);
        assert_eq!(normalize_whatsapp_phone_like_identifier("device"), None);
        assert_eq!(normalize_whatsapp_phone_like_identifier("1+2"), None);
        assert_eq!(normalize_whatsapp_phone_like_identifier("++123"), None);
        assert_eq!(normalize_whatsapp_phone_like_identifier(")(123"), None);
        assert_eq!(normalize_whatsapp_phone_like_identifier("123)"), None);
        assert_eq!(normalize_whatsapp_phone_like_identifier("(123"), None);
        assert_eq!(
            normalize_whatsapp_phone_like_identifier("+1-206-555-ABCD"),
            None
        );
        assert_eq!(
            normalize_whatsapp_phone_like_identifier("12065550123@example.com"),
            None
        );
    }

    #[test]
    fn whatsapp_allowlist_parse_splits_trims_normalizes_and_deduplicates() {
        let parsed = parse_whatsapp_allowed_contacts(
            " +1 (206) 555-0123,\n12065550123@s.whatsapp.net\n\n+49 30 123456,+49-30-123456,invalid ",
        );

        assert_eq!(
            parsed,
            vec!["12065550123".to_string(), "4930123456".to_string()]
        );
    }

    #[test]
    fn whatsapp_allowlist_parse_ignores_empty_and_invalid_entries() {
        let parsed = parse_whatsapp_allowed_contacts(
            "\n, ,\ninvalid\nexample@example.com\n1+2\n++123\n)(123\n",
        );

        assert!(parsed.is_empty());
    }

    #[test]
    fn whatsapp_allowlist_has_contacts_only_when_normalized_entries_exist() {
        assert!(has_whatsapp_allowed_contacts("+1 206 555 0123"));
        assert!(has_whatsapp_allowed_contacts(
            "invalid,\n12065550123@s.whatsapp.net"
        ));
        assert!(!has_whatsapp_allowed_contacts("\n , invalid , device "));
    }

    #[test]
    fn tamux_data_dir_honors_env_override() {
        let _lock = env_lock().lock().expect("env lock");
        let test_name = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect::<String>();
        let temp_dir = std::env::temp_dir().join(format!(
            "tamux-protocol-data-dir-{}-{}",
            std::process::id(),
            test_name
        ));
        let _guard = EnvGuard::set("TAMUX_DATA_DIR", &temp_dir);

        assert_eq!(tamux_data_dir(), temp_dir);
        assert_eq!(
            ensure_tamux_data_dir().expect("override data dir should be created"),
            temp_dir
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
