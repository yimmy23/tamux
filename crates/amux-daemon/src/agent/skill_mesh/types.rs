use serde::{Deserialize, Serialize};

const SKILL_MESH_STORAGE_KEY_SEPARATOR: char = '\u{001f}';

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillMeshOutcome {
    Success,
    Failure,
    Dismissed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillMeshConfidenceBand {
    Strong,
    Weak,
    None,
}

impl SkillMeshConfidenceBand {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Strong => "strong",
            Self::Weak => "weak",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillMeshNextStep {
    ReadSkill,
    ChooseOrBypass,
    JustifySkillSkip,
}

impl SkillMeshNextStep {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadSkill => "read_skill",
            Self::ChooseOrBypass => "choose_or_bypass",
            Self::JustifySkillSkip => "justify_skill_skip",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillMeshPolicyDecision {
    pub confidence_band: SkillMeshConfidenceBand,
    pub next_step: SkillMeshNextStep,
    pub recommended_action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_skill: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_skill_identifier: Option<String>,
    #[serde(default)]
    pub requires_approval: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillMeshFeedbackState {
    pub use_count: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub dismiss_count: u64,
    pub negative_feedback_weight: f32,
    pub requires_recompile: bool,
}

impl Default for SkillMeshFeedbackState {
    fn default() -> Self {
        Self {
            use_count: 0,
            success_count: 0,
            failure_count: 0,
            dismiss_count: 0,
            negative_feedback_weight: 0.0,
            requires_recompile: false,
        }
    }
}

impl SkillMeshFeedbackState {
    pub fn apply_outcome(&self, outcome: SkillMeshOutcome) -> Self {
        let mut next = self.clone();
        next.use_count += 1;
        next.requires_recompile = false;
        match outcome {
            SkillMeshOutcome::Success => {
                next.success_count += 1;
                next.negative_feedback_weight = (next.negative_feedback_weight - 0.2).max(0.0);
            }
            SkillMeshOutcome::Failure => {
                next.failure_count += 1;
                next.negative_feedback_weight += 0.6;
            }
            SkillMeshOutcome::Dismissed => {
                next.dismiss_count += 1;
                next.negative_feedback_weight += 0.25;
            }
        }
        next
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillMeshEmbeddingRecord {
    pub embedding_id: String,
    pub skill_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_id: Option<String>,
    pub embedding_kind: String,
    pub text: String,
    pub vector: Vec<f32>,
    pub capability_path: Vec<String>,
    pub trust_tier: String,
    pub risk_level: String,
    pub source_kind: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillMeshDocument {
    pub skill_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_id: Option<String>,
    pub skill_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_name: Option<String>,
    pub source_path: String,
    pub source_kind: String,
    pub content_hash: String,
    pub compile_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub capability_path: Vec<String>,
    pub synthetic_queries: Vec<String>,
    pub explicit_trigger_phrases: Vec<String>,
    pub workspace_affinities: Vec<String>,
    pub required_tools: Vec<String>,
    pub required_platforms: Vec<String>,
    pub required_env_hints: Vec<String>,
    pub security_risk_level: String,
    pub trust_tier: String,
    pub provenance: String,
    pub use_count: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub dismiss_count: u64,
    pub negative_feedback_weight: f32,
    pub embedding_records: Vec<SkillMeshEmbeddingRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SkillMeshDocumentKey {
    pub skill_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillMeshStoredDocument {
    pub skill_id: String,
    pub document_json: String,
}

impl SkillMeshDocument {
    pub fn document_key(&self) -> SkillMeshDocumentKey {
        SkillMeshDocumentKey {
            skill_id: self.skill_id.clone(),
            variant_id: self.variant_id.clone(),
        }
    }

    pub fn to_storage_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    pub fn from_storage_json(value: &str) -> serde_json::Result<Self> {
        serde_json::from_str(value)
    }

    pub fn feedback_state(&self) -> SkillMeshFeedbackState {
        SkillMeshFeedbackState {
            use_count: self.use_count,
            success_count: self.success_count,
            failure_count: self.failure_count,
            dismiss_count: self.dismiss_count,
            negative_feedback_weight: self.negative_feedback_weight,
            requires_recompile: false,
        }
    }

    pub fn apply_feedback(&mut self, outcome: SkillMeshOutcome) -> SkillMeshFeedbackState {
        let next = self.feedback_state().apply_outcome(outcome);
        self.use_count = next.use_count;
        self.success_count = next.success_count;
        self.failure_count = next.failure_count;
        self.dismiss_count = next.dismiss_count;
        self.negative_feedback_weight = next.negative_feedback_weight;
        next
    }
}

impl SkillMeshDocumentKey {
    pub fn from_storage_key(storage_key: &str) -> Self {
        match storage_key.split_once(SKILL_MESH_STORAGE_KEY_SEPARATOR) {
            Some((skill_id, variant_id)) => Self {
                skill_id: skill_id.to_string(),
                variant_id: Some(variant_id.to_string()),
            },
            None => Self {
                skill_id: storage_key.to_string(),
                variant_id: None,
            },
        }
    }

    pub fn storage_key(&self) -> String {
        match self.variant_id.as_deref() {
            Some(variant_id) => {
                format!(
                    "{}{}{}",
                    self.skill_id, SKILL_MESH_STORAGE_KEY_SEPARATOR, variant_id
                )
            }
            None => self.skill_id.clone(),
        }
    }
}

impl SkillMeshStoredDocument {
    pub fn from_document(document: &SkillMeshDocument) -> serde_json::Result<Self> {
        Ok(Self {
            skill_id: document.document_key().storage_key(),
            document_json: document.to_storage_json()?,
        })
    }

    pub fn into_document(self) -> serde_json::Result<SkillMeshDocument> {
        SkillMeshDocument::from_storage_json(&self.document_json)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillMeshIntent {
    pub original_query: String,
    pub normalized_query: String,
    pub workspace_hints: Vec<String>,
    pub capability_hints: Vec<String>,
    pub risk_hints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillMeshCandidate {
    pub document: SkillMeshDocument,
    pub score: f32,
    pub rationale: Vec<String>,
    pub policy_allows_read: bool,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillMeshResult {
    pub intent: SkillMeshIntent,
    pub candidates: Vec<SkillMeshCandidate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chosen_document_key: Option<SkillMeshDocumentKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
    pub rationale: Vec<String>,
}
