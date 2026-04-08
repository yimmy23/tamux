use crate::history::SkillVariantRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SkillRecommendationConfidence {
    Strong,
    Weak,
    #[default]
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SkillRecommendationAction {
    ReadSkill,
    JustifySkip,
    #[default]
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct SkillDocumentMetadata {
    pub summary: Option<String>,
    pub headings: Vec<String>,
    pub keywords: Vec<String>,
    pub triggers: Vec<String>,
    pub search_text: String,
    pub built_in: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct SkillRecommendation {
    pub record: SkillVariantRecord,
    pub metadata: SkillDocumentMetadata,
    pub reason: String,
    pub excerpt: String,
    pub score: f64,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SkillDiscoveryResult {
    pub recommendations: Vec<SkillRecommendation>,
    pub confidence: SkillRecommendationConfidence,
    pub recommended_action: SkillRecommendationAction,
}

#[derive(Debug, Clone)]
pub(super) struct SkillCandidateInput {
    pub record: SkillVariantRecord,
    pub metadata: SkillDocumentMetadata,
    pub excerpt: String,
}

#[derive(Debug, Clone)]
pub(super) struct CandidateScore {
    pub recommendation: SkillRecommendation,
}
