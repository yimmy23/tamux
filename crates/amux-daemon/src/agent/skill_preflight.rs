use super::*;
use crate::agent::types::SkillRecommendationConfig;
use amux_protocol::SessionId;

const MAX_SKILL_PREFLIGHT_MATCHES: usize = 3;

pub(crate) struct SkillPreflightContext {
    pub prompt_context: String,
    pub state: LatestSkillDiscoveryState,
    pub workflow_message: String,
    pub workflow_details: Option<String>,
}

impl AgentEngine {
    pub(crate) async fn discover_skill_recommendations_public(
        &self,
        query: &str,
        session_id: Option<SessionId>,
        limit: usize,
    ) -> Result<amux_protocol::SkillDiscoveryResultPublic> {
        let context = self.run_skill_discovery(query, session_id, limit).await?;

        Ok(translate_skill_discovery_result(
            query,
            &context.context_tags,
            &context.result,
            &context.cfg,
        ))
    }

    pub(super) async fn build_skill_preflight_context(
        &self,
        content: &str,
        session_id: Option<SessionId>,
    ) -> Result<Option<SkillPreflightContext>> {
        if !should_run_skill_preflight(content) {
            return Ok(None);
        }

        let context = self
            .run_skill_discovery(content, session_id, MAX_SKILL_PREFLIGHT_MATCHES)
            .await?;
        let state = build_latest_skill_discovery_state(content, &context.result);

        Ok(Some(SkillPreflightContext {
            prompt_context: build_skill_preflight_prompt(content, &context.result, &state),
            workflow_message: format!(
                "Skill discovery confidence: {}. Next action: {}.",
                state.confidence_tier, state.recommended_action
            ),
            workflow_details: serde_json::to_string(&state).ok(),
            state,
        }))
    }

    pub(crate) async fn refresh_thread_skill_discovery_state(
        &self,
        thread_id: &str,
        query: &str,
        session_id: Option<SessionId>,
        limit: usize,
    ) -> Result<LatestSkillDiscoveryState> {
        let context = self.run_skill_discovery(query, session_id, limit).await?;
        let state = build_latest_skill_discovery_state(query, &context.result);
        self.set_thread_skill_discovery_state(thread_id, state.clone())
            .await;
        Ok(state)
    }

    pub(crate) async fn record_thread_skill_skip_rationale(
        &self,
        thread_id: &str,
        rationale: &str,
    ) -> Result<LatestSkillDiscoveryState> {
        let trimmed = rationale.trim();
        if trimmed.is_empty() {
            anyhow::bail!("skip rationale must not be empty");
        }

        let mut state = self
            .get_thread_skill_discovery_state(thread_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("no active skill discovery state for this thread"))?;
        state.skip_rationale = Some(trimmed.to_string());
        state.compliant = !state.confidence_tier.eq_ignore_ascii_case("strong");
        state.updated_at = now_millis();
        self.set_thread_skill_discovery_state(thread_id, state.clone())
            .await;
        Ok(state)
    }

    pub(crate) async fn record_thread_skill_read_compliance(
        &self,
        thread_id: &str,
        skill_identifier: &str,
    ) -> Option<LatestSkillDiscoveryState> {
        let mut state = self.get_thread_skill_discovery_state(thread_id).await?;
        if !state.confidence_tier.eq_ignore_ascii_case("strong") {
            return Some(state);
        }
        let expected = state
            .read_skill_identifier
            .as_deref()
            .or(state.recommended_skill.as_deref())?;
        if !skill_identifier_matches(expected, skill_identifier) {
            return Some(state);
        }

        state.compliant = true;
        state.updated_at = now_millis();
        self.set_thread_skill_discovery_state(thread_id, state.clone())
            .await;
        Some(state)
    }

    async fn run_skill_discovery(
        &self,
        query: &str,
        session_id: Option<SessionId>,
        limit: usize,
    ) -> Result<SkillDiscoveryComputation> {
        let skills_root = self.history.data_dir().to_path_buf();
        let context_tags = resolve_skill_context_tags(&self.session_manager, session_id).await;
        let cfg = self.config.read().await.skill_recommendation.clone();
        let result = super::skill_recommendation::discover_local_skills(
            &self.history,
            &skills_root,
            query,
            &context_tags,
            limit,
            &cfg,
        )
        .await?;
        Ok(SkillDiscoveryComputation {
            context_tags,
            cfg,
            result,
        })
    }
}

struct SkillDiscoveryComputation {
    context_tags: Vec<String>,
    cfg: SkillRecommendationConfig,
    result: super::skill_recommendation::SkillDiscoveryResult,
}

fn translate_skill_discovery_result(
    query: &str,
    context_tags: &[String],
    result: &super::skill_recommendation::SkillDiscoveryResult,
    cfg: &SkillRecommendationConfig,
) -> amux_protocol::SkillDiscoveryResultPublic {
    let top_skill_name = result
        .recommendations
        .first()
        .map(|recommendation| recommendation.record.skill_name.as_str());

    amux_protocol::SkillDiscoveryResultPublic {
        query: query.to_string(),
        required: !matches!(
            result.recommended_action,
            super::skill_recommendation::SkillRecommendationAction::None
        ),
        confidence_tier: confidence_label(result.confidence).to_string(),
        recommended_action: recommended_action_label(result.recommended_action, top_skill_name),
        explicit_rationale_required: matches!(
            result.recommended_action,
            super::skill_recommendation::SkillRecommendationAction::JustifySkip
        ),
        workspace_tags: context_tags.to_vec(),
        candidates: result
            .recommendations
            .iter()
            .map(
                |recommendation| amux_protocol::SkillDiscoveryCandidatePublic {
                    variant_id: recommendation.record.variant_id.clone(),
                    skill_name: recommendation.record.skill_name.clone(),
                    variant_name: recommendation.record.variant_name.clone(),
                    relative_path: recommendation.record.relative_path.clone(),
                    status: recommendation.record.status.clone(),
                    score: recommendation.score,
                    confidence_tier: candidate_confidence_label(recommendation.score, cfg)
                        .to_string(),
                    reasons: split_reasons(&recommendation.reason),
                    context_tags: recommendation.record.context_tags.clone(),
                    use_count: recommendation.record.use_count,
                    success_count: recommendation.record.success_count,
                    failure_count: recommendation.record.failure_count,
                },
            )
            .collect(),
    }
}

async fn resolve_skill_context_tags(
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
) -> Vec<String> {
    let root = if let Some(session_id) = session_id {
        let sessions = session_manager.list().await;
        sessions
            .iter()
            .find(|session| session.id == session_id)
            .and_then(|session| session.cwd.clone())
            .map(PathBuf::from)
    } else {
        None
    }
    .or_else(|| std::env::current_dir().ok());

    root.filter(|path| path.is_dir())
        .map(|path| super::semantic_env::infer_workspace_context_tags(&path))
        .unwrap_or_default()
}

fn should_run_skill_preflight(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.len() >= 48 || trimmed.lines().count() > 1 {
        return true;
    }

    let normalized = trimmed.to_ascii_lowercase();
    [
        "fix",
        "debug",
        "build",
        "implement",
        "refactor",
        "investigate",
        "review",
        "goal",
        "thread",
        "workspace",
        "terminal",
        "session",
        "tool",
    ]
    .iter()
    .any(|keyword| normalized.contains(keyword))
}

fn build_latest_skill_discovery_state(
    query: &str,
    result: &super::skill_recommendation::SkillDiscoveryResult,
) -> LatestSkillDiscoveryState {
    let recommended_skill = result
        .recommendations
        .first()
        .map(|recommendation| recommendation.record.skill_name.clone());
    let confidence_tier = confidence_label(result.confidence).to_string();
    let recommended_action = if confidence_tier == "strong" {
        recommended_skill
            .as_ref()
            .map(|skill_name| format!("read_skill {skill_name}"))
            .unwrap_or_else(|| "read_skill".to_string())
    } else {
        "justify_skill_skip".to_string()
    };

    LatestSkillDiscoveryState {
        query: query.to_string(),
        confidence_tier,
        recommended_skill: recommended_skill.clone(),
        recommended_action,
        read_skill_identifier: recommended_skill.filter(|_| {
            matches!(
                result.confidence,
                super::skill_recommendation::SkillRecommendationConfidence::Strong
            )
        }),
        skip_rationale: None,
        compliant: false,
        updated_at: now_millis(),
    }
}

fn build_skill_preflight_prompt(
    query: &str,
    result: &super::skill_recommendation::SkillDiscoveryResult,
    state: &LatestSkillDiscoveryState,
) -> String {
    let mut body = format!(
        "Daemon skill discovery already evaluated this request.\n- Query: {query}\n- Confidence: {}\n- Next action: {}\n",
        state.confidence_tier, state.recommended_action
    );

    match state.confidence_tier.as_str() {
        "strong" => {
            body.push_str(
                "- Hard gate: do not call non-discovery tools until you read the recommended skill.\n",
            );
        }
        _ => {
            body.push_str(
                "- Recommendation: if you proceed without a local skill, record the rationale with `justify_skill_skip`; this is guidance, not a hard prerequisite for other tools.\n",
            );
        }
    }

    if result.recommendations.is_empty() {
        body.push_str(
            "- No installed skill cleared the weak threshold. If you continue, explain why no local skill fits; `justify_skill_skip` can record that rationale.\n",
        );
        return body;
    }

    for recommendation in &result.recommendations {
        let tags = if recommendation.record.context_tags.is_empty() {
            "none".to_string()
        } else {
            recommendation.record.context_tags.join(", ")
        };
        let summary = recommendation
            .metadata
            .summary
            .as_deref()
            .unwrap_or("No summary extracted.");
        let reasons = split_reasons(&recommendation.reason).join(", ");
        body.push_str(&format!(
            "\n- {} [{} | status={} | score={:.2} | tags={}]\n  Reasons: {}\n  Summary: {}\n  Path: {}\n",
            recommendation.record.skill_name,
            recommendation.record.variant_name,
            recommendation.record.status,
            recommendation.score,
            tags,
            reasons,
            summary,
            recommendation.record.relative_path,
        ));
    }

    body
}

fn skill_identifier_matches(expected: &str, actual: &str) -> bool {
    let expected = expected.trim();
    let actual = actual.trim();
    if expected.is_empty() || actual.is_empty() {
        return false;
    }

    expected.eq_ignore_ascii_case(actual)
        || actual
            .rsplit('/')
            .next()
            .is_some_and(|segment| expected.eq_ignore_ascii_case(segment))
        || actual
            .trim_end_matches(".md")
            .trim_end_matches("/SKILL")
            .ends_with(expected)
}

fn confidence_label(
    value: super::skill_recommendation::SkillRecommendationConfidence,
) -> &'static str {
    match value {
        super::skill_recommendation::SkillRecommendationConfidence::Strong => "strong",
        super::skill_recommendation::SkillRecommendationConfidence::Weak => "weak",
        super::skill_recommendation::SkillRecommendationConfidence::None => "none",
    }
}

fn action_label(value: super::skill_recommendation::SkillRecommendationAction) -> &'static str {
    match value {
        super::skill_recommendation::SkillRecommendationAction::ReadSkill => "read_skill",
        super::skill_recommendation::SkillRecommendationAction::JustifySkip => "justify_skip",
        super::skill_recommendation::SkillRecommendationAction::None => "none",
    }
}

fn recommended_action_label(
    action: super::skill_recommendation::SkillRecommendationAction,
    top_skill_name: Option<&str>,
) -> String {
    match (action, top_skill_name) {
        (super::skill_recommendation::SkillRecommendationAction::ReadSkill, Some(skill_name)) => {
            format!("read_skill {skill_name}")
        }
        (super::skill_recommendation::SkillRecommendationAction::JustifySkip, Some(skill_name)) => {
            format!("justify_skip {skill_name}")
        }
        _ => action_label(action).to_string(),
    }
}

fn candidate_confidence_label(score: f64, cfg: &SkillRecommendationConfig) -> &'static str {
    if score >= cfg.strong_match_threshold {
        "strong"
    } else if score >= cfg.weak_match_threshold {
        "weak"
    } else {
        "none"
    }
}

fn split_reasons(reason: &str) -> Vec<String> {
    let parts = reason
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            if let Some(rest) = value.strip_prefix("matched request terms ") {
                format!("matched {rest}")
            } else if let Some(rest) = value.strip_prefix("matched workspace tags ") {
                format!("workspace {rest}")
            } else if value.starts_with("historical success ") {
                reason_usage_summary(value)
            } else {
                value.to_string()
            }
        })
        .collect::<Vec<_>>();

    if parts.is_empty() {
        vec![reason.to_string()]
    } else {
        parts
    }
}

fn reason_usage_summary(value: &str) -> String {
    let words = value.split_whitespace().collect::<Vec<_>>();
    let uses = words
        .iter()
        .position(|word| *word == "across")
        .and_then(|index| words.get(index + 1))
        .and_then(|count| count.parse::<u32>().ok());
    let success_percent = words
        .get(2)
        .map(|value| value.trim_end_matches('%'))
        .and_then(|value| value.parse::<u32>().ok());

    match (uses, success_percent) {
        (Some(uses), Some(success_percent)) => {
            let successes = ((uses as f64) * (success_percent as f64 / 100.0)).round() as u32;
            format!("{successes}/{uses} successful uses")
        }
        _ => value.to_string(),
    }
}
