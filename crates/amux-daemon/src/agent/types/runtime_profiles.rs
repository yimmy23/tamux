#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalRuntimeProfile {
    pub runtime: String,
    pub source_config_path: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub cwd: Option<String>,
    pub has_tamux_mcp: bool,
    pub imported_at_ms: u64,
}

