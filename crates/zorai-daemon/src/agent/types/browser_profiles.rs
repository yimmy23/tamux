#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserProfile {
    pub profile_id: String,
    pub label: String,
    pub profile_dir: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub last_used_at: Option<u64>,
}

