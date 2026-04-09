use super::*;
use crate::agent::types::SkillRecommendationConfig;
use amux_protocol::SessionId;

const MAX_SKILL_PREFLIGHT_MATCHES: usize = 3;
const SKILL_DISCOVERY_NORMALIZER_MARKER: &str = "[[skill_discovery_query_normalizer]]";
const MAX_NORMALIZED_SKILL_QUERY_CHARS: usize = 160;
const LOCAL_SKILL_DISCOVERY_NORMALIZER_ID: &str = "local-skill-discovery-heuristic";

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
        cursor: Option<&str>,
    ) -> Result<amux_protocol::SkillDiscoveryResultPublic> {
        let context = self.run_skill_discovery(query, session_id, 512).await?;

        super::skill_recommendation::page_public_discovery_result(
            query,
            &context.context_tags,
            &context.result,
            &context.cfg,
            cursor,
            limit,
        )
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
        let result = if should_attempt_query_normalization(query, &result) {
            match self
                .normalize_skill_discovery_query(query, &context_tags, session_id)
                .await
            {
                Some((normalized_query, normalizer_agent_id)) => {
                    let mut normalized_result = super::skill_recommendation::discover_local_skills(
                        &self.history,
                        &skills_root,
                        &normalized_query,
                        &context_tags,
                        limit,
                        &cfg,
                    )
                    .await?;
                    if fallback_result_is_better(&result, &normalized_result) {
                        annotate_fallback_result(
                            &mut normalized_result,
                            &normalized_query,
                            normalizer_agent_id,
                        );
                        normalized_result
                    } else {
                        result
                    }
                }
                None => result,
            }
        } else {
            result
        };
        Ok(SkillDiscoveryComputation {
            context_tags,
            cfg,
            result,
        })
    }

    async fn normalize_skill_discovery_query(
        &self,
        query: &str,
        context_tags: &[String],
        session_id: Option<SessionId>,
    ) -> Option<(String, &'static str)> {
        if let Some(normalized_query) = heuristic_skill_discovery_query_rewrite(query, context_tags)
        {
            return Some((normalized_query, LOCAL_SKILL_DISCOVERY_NORMALIZER_ID));
        }

        let normalizer_agent_id = normalization_agent_id_for_query(query);
        let prompt = build_skill_discovery_normalization_prompt(query, context_tags);
        let preferred_session_hint = session_id.as_ref().map(ToString::to_string);

        let response = match self
            .send_internal_agent_message(
                MAIN_AGENT_ID,
                normalizer_agent_id,
                &prompt,
                preferred_session_hint.as_deref(),
            )
            .await
        {
            Ok(result) => result.response,
            Err(error) => {
                tracing::warn!(
                    %error,
                    normalizer = %normalizer_agent_id,
                    "skill discovery normalization fallback failed"
                );
                return None;
            }
        };

        let normalized_query = parse_normalized_skill_query(&response)?;
        if normalized_query.eq_ignore_ascii_case(query.trim()) {
            return None;
        }

        Some((normalized_query, normalizer_agent_id))
    }
}

struct SkillDiscoveryComputation {
    context_tags: Vec<String>,
    cfg: SkillRecommendationConfig,
    result: super::skill_recommendation::SkillDiscoveryResult,
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

fn should_attempt_query_normalization(
    query: &str,
    result: &super::skill_recommendation::SkillDiscoveryResult,
) -> bool {
    !query.trim().is_empty()
        && !query.contains(SKILL_DISCOVERY_NORMALIZER_MARKER)
        && (result.recommendations.is_empty()
            || matches!(
                result.confidence,
                super::skill_recommendation::SkillRecommendationConfidence::None
            ))
}

fn normalization_agent_id_for_query(query: &str) -> &'static str {
    let lower = query.to_ascii_lowercase();
    let governance_terms = [
        "approval",
        "audit",
        "compliance",
        "control-plane",
        "control plane",
        "governance",
        "guardrail",
        "policy",
        "risk",
        "safety",
    ];

    if governance_terms.iter().any(|term| lower.contains(term)) {
        WELES_AGENT_ID
    } else {
        CONCIERGE_AGENT_ID
    }
}

fn build_skill_discovery_normalization_prompt(query: &str, context_tags: &[String]) -> String {
    let workspace_tags = if context_tags.is_empty() {
        "none".to_string()
    } else {
        context_tags.join(", ")
    };

    format!(
        "{SKILL_DISCOVERY_NORMALIZER_MARKER}\nRewrite the operator request into a short local skill-discovery search query for tamux.\nReturn JSON only in the form {{\"query\":\"...\"}}.\n\nRules:\n- Focus on workflow intent, not the full task.\n- Use 3 to 8 concise keywords or trigger phrases.\n- Prefer local process terms over repository-specific prose.\n- Do not answer the task itself.\n- If the request is about governance, policy, audit, or safety, preserve that intent in compact workflow terms.\n- If no useful rewrite is possible, return {{\"query\":\"\"}}.\n\nWorkspace tags: {workspace_tags}\nOriginal request: {query}"
    )
}

fn parse_normalized_skill_query(response: &str) -> Option<String> {
    let trimmed = strip_code_fences(response.trim());
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(query) = value.get("query").and_then(|value| value.as_str()) {
            return sanitize_normalized_skill_query(query);
        }
    }

    trimmed
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .and_then(sanitize_normalized_skill_query)
}

fn strip_code_fences(input: &str) -> &str {
    let trimmed = input.trim();
    let stripped = trimmed.strip_prefix("```json").unwrap_or(trimmed);
    let stripped = stripped.strip_prefix("```").unwrap_or(stripped);
    stripped.strip_suffix("```").unwrap_or(stripped).trim()
}

fn sanitize_normalized_skill_query(query: &str) -> Option<String> {
    let collapsed = query
        .trim()
        .trim_matches('`')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if collapsed.is_empty() || !collapsed.chars().any(|ch| ch.is_ascii_alphanumeric()) {
        return None;
    }

    let mut normalized = collapsed;
    if normalized.chars().count() > MAX_NORMALIZED_SKILL_QUERY_CHARS {
        normalized = normalized
            .chars()
            .take(MAX_NORMALIZED_SKILL_QUERY_CHARS)
            .collect::<String>()
            .trim()
            .to_string();
    }

    (!normalized.is_empty()).then_some(normalized)
}

fn heuristic_skill_discovery_query_rewrite(query: &str, context_tags: &[String]) -> Option<String> {
    let lower = query.to_ascii_lowercase();
    let token_count = lower
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '-')
        .filter(|token| token.len() >= 3)
        .count();
    if token_count < 10 {
        return None;
    }

    let has_audit_intent =
        lower.contains("audit") || lower.contains("inspect") || lower.contains("review");
    let has_diff_scope = lower.contains("git")
        || lower.contains("worktree")
        || lower.contains("diff")
        || lower.contains("patch");
    let has_governance_scope = lower.contains("governance")
        || lower.contains("policy")
        || lower.contains("compliance")
        || lower.contains("guardrail")
        || lower.contains("safety")
        || lower.contains("risk")
        || lower.contains("orchestration");

    if !(has_audit_intent && has_diff_scope && has_governance_scope) {
        return None;
    }

    let mut terms = Vec::new();
    push_rewrite_term(&mut terms, "audit");
    if lower.contains("git") {
        push_rewrite_term(&mut terms, "git");
    }
    if lower.contains("worktree") {
        push_rewrite_term(&mut terms, "worktree");
    }
    if lower.contains("diff") {
        push_rewrite_term(&mut terms, "diff");
    }

    for tag in context_tags {
        let normalized = tag.trim().to_ascii_lowercase();
        if normalized.is_empty() || !lower.contains(&normalized) {
            continue;
        }
        push_rewrite_term(&mut terms, &normalized);
        if terms.len() >= 5 {
            break;
        }
    }

    if lower.contains("orchestration") {
        push_rewrite_term(&mut terms, "orchestration");
    }
    if lower.contains("safety") {
        push_rewrite_term(&mut terms, "safety");
    }
    if lower.contains("governance") {
        push_rewrite_term(&mut terms, "governance");
    } else if lower.contains("policy") {
        push_rewrite_term(&mut terms, "policy");
    } else if lower.contains("compliance") {
        push_rewrite_term(&mut terms, "compliance");
    }

    sanitize_normalized_skill_query(&terms.join(" "))
}

fn push_rewrite_term(terms: &mut Vec<String>, term: &str) {
    if term.trim().is_empty() || terms.iter().any(|value| value == term) {
        return;
    }
    terms.push(term.to_string());
}

fn fallback_result_is_better(
    current: &super::skill_recommendation::SkillDiscoveryResult,
    fallback: &super::skill_recommendation::SkillDiscoveryResult,
) -> bool {
    let current_rank = recommendation_confidence_rank(current.confidence);
    let fallback_rank = recommendation_confidence_rank(fallback.confidence);
    if fallback_rank != current_rank {
        return fallback_rank > current_rank;
    }

    if fallback.recommendations.len() != current.recommendations.len() {
        return fallback.recommendations.len() > current.recommendations.len();
    }

    fallback
        .recommendations
        .first()
        .map(|item| item.score)
        .unwrap_or_default()
        > current
            .recommendations
            .first()
            .map(|item| item.score)
            .unwrap_or_default()
}

fn recommendation_confidence_rank(
    value: super::skill_recommendation::SkillRecommendationConfidence,
) -> u8 {
    match value {
        super::skill_recommendation::SkillRecommendationConfidence::None => 0,
        super::skill_recommendation::SkillRecommendationConfidence::Weak => 1,
        super::skill_recommendation::SkillRecommendationConfidence::Strong => 2,
    }
}

fn annotate_fallback_result(
    result: &mut super::skill_recommendation::SkillDiscoveryResult,
    normalized_query: &str,
    normalizer_agent_id: &str,
) {
    let agent_name = if normalizer_agent_id == LOCAL_SKILL_DISCOVERY_NORMALIZER_ID {
        "local heuristic"
    } else {
        canonical_agent_name(normalizer_agent_id)
    };
    let annotation = format!("normalized via {agent_name} as `{normalized_query}`");
    for recommendation in &mut result.recommendations {
        if recommendation.reason.trim().is_empty() {
            recommendation.reason = annotation.clone();
        } else {
            recommendation.reason = format!("{annotation}; {}", recommendation.reason);
        }
    }
}

fn should_run_skill_preflight(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.contains(SKILL_DISCOVERY_NORMALIZER_MARKER) {
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
    let recommended_action =
        recommended_action_label(result.recommended_action, recommended_skill.as_deref());

    LatestSkillDiscoveryState {
        query: query.to_string(),
        confidence_tier,
        recommended_skill: recommended_skill.clone(),
        recommended_action,
        read_skill_identifier: recommended_skill.filter(|_| {
            matches!(
                result.recommended_action,
                super::skill_recommendation::SkillRecommendationAction::ReadSkill
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
        _ if state.recommended_action.starts_with("read_skill") => {
            body.push_str(&format!(
                "- Recommendation: prefer `{}` before other substantial tools. If you intentionally bypass the suggested local workflow, record why with `justify_skill_skip`.\n",
                state.recommended_action
            ));
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
        super::skill_recommendation::SkillRecommendationAction::JustifySkip => "justify_skill_skip",
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::fs;
    use std::sync::{Arc, Mutex as StdMutex};
    use tempfile::tempdir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn write_skill(root: &std::path::Path, skill_dir: &str, content: &str) {
        let path = root.join("skills").join(skill_dir).join("SKILL.md");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create skill parent");
        }
        fs::write(path, content).expect("write skill");
    }

    async fn read_http_request_body(socket: &mut tokio::net::TcpStream) -> std::io::Result<String> {
        let mut buffer = Vec::with_capacity(65536);
        let mut temp = [0u8; 4096];
        let headers_end = loop {
            let read = socket.read(&mut temp).await?;
            if read == 0 {
                return Ok(String::new());
            }
            buffer.extend_from_slice(&temp[..read]);
            if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                break index + 4;
            }
        };

        let headers = String::from_utf8_lossy(&buffer[..headers_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let mut parts = line.splitn(2, ':');
                let name = parts.next()?.trim();
                let value = parts.next()?.trim();
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);

        while buffer.len().saturating_sub(headers_end) < content_length {
            let read = socket.read(&mut temp).await?;
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&temp[..read]);
        }

        let available = buffer.len().saturating_sub(headers_end).min(content_length);
        Ok(String::from_utf8_lossy(&buffer[headers_end..headers_end + available]).to_string())
    }

    async fn spawn_normalization_server(
        recorded_bodies: Arc<StdMutex<VecDeque<String>>>,
        assistant_content: &str,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind normalization server");
        let addr = listener.local_addr().expect("normalization server addr");
        let response_json =
            serde_json::to_string(assistant_content).expect("assistant response should serialize");

        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let recorded_bodies = recorded_bodies.clone();
                let response_json = response_json.clone();
                tokio::spawn(async move {
                    let body = read_http_request_body(&mut socket)
                        .await
                        .expect("read normalization request");
                    recorded_bodies
                        .lock()
                        .expect("lock normalization bodies")
                        .push_back(body);

                    let response = format!(
                        concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {{\"choices\":[{{\"delta\":{{\"content\":{}}}}}]}}\n\n",
                            "data: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":7,\"completion_tokens\":3}}}}\n\n",
                            "data: [DONE]\n\n"
                        ),
                        response_json
                    );
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write normalization response");
                });
            }
        });

        format!("http://{addr}/v1")
    }

    #[test]
    fn skill_discovery_normalizer_routes_governance_queries_to_weles() {
        assert_eq!(
            normalization_agent_id_for_query(
                "audit control-plane guardrails against governance policy gaps"
            ),
            WELES_AGENT_ID
        );
    }

    #[test]
    fn skill_discovery_normalizer_parses_json_and_fenced_json() {
        assert_eq!(
            parse_normalized_skill_query("{\"query\":\"debug root cause workflow\"}"),
            Some("debug root cause workflow".to_string())
        );
        assert_eq!(
            parse_normalized_skill_query(
                "```json\n{\"query\":\"design planning architecture workflow\"}\n```"
            ),
            Some("design planning architecture workflow".to_string())
        );
    }

    #[test]
    fn local_skill_discovery_query_rewrite_extracts_compact_governance_terms() {
        assert_eq!(
            heuristic_skill_discovery_query_rewrite(
                "Audit modified git worktree files, inspect diffs, and map changed Rust files to orchestration safety governance RFC concepts to identify related vs unrelated changes",
                &["rust".to_string(), "async".to_string()],
            ),
            Some("audit git worktree diff rust orchestration safety governance".to_string())
        );
    }

    #[tokio::test]
    async fn discover_skill_recommendations_public_uses_rarog_fallback_for_natural_language_queries(
    ) {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
        let base_url = spawn_normalization_server(
            recorded_bodies.clone(),
            "{\"query\":\"debug root cause workflow\"}",
        )
        .await;

        write_skill(
            root.path(),
            "development/superpowers/systematic-debugging",
            r#"---
name: systematic-debugging
description: Debug failures by tracing root cause before patching.
keywords: [debug, root, cause, workflow]
triggers: [bug fix, failure investigation]
---

# Systematic Debugging
"#,
        );

        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.base_url = base_url;
        config.model = "gpt-5.4-mini".to_string();
        config.api_key = "test-key".to_string();
        config.auth_source = AuthSource::ApiKey;
        config.api_transport = ApiTransport::ChatCompletions;
        config.auto_retry = false;
        config.max_retries = 0;
        config.max_tool_loops = 1;

        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        let result = engine
            .discover_skill_recommendations_public(
                "untangle what keeps exploding and prove where it starts before changing code",
                None,
                3,
                None,
            )
            .await
            .expect("skill discovery should succeed");

        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].skill_name, "systematic-debugging");
        assert_ne!(result.confidence_tier, "none");
        assert!(result.candidates[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("normalized")));

        let request_body = recorded_bodies
            .lock()
            .expect("lock recorded bodies")
            .pop_front()
            .expect("fallback normalization should call the model");
        assert!(request_body.contains("tamux concierge"));
    }

    #[tokio::test]
    async fn discover_skill_recommendations_public_uses_weles_fallback_for_governance_queries() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
        let base_url = spawn_normalization_server(
            recorded_bodies.clone(),
            "{\"query\":\"design planning architecture workflow\"}",
        )
        .await;

        write_skill(
            root.path(),
            "development/superpowers/brainstorming",
            r#"---
name: brainstorming
description: Guide design and planning before implementation.
keywords: [design, planning, architecture, workflow]
triggers: [feature work, implementation planning]
---

# Brainstorming
"#,
        );

        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.base_url = base_url;
        config.model = "gpt-5.4-mini".to_string();
        config.api_key = "test-key".to_string();
        config.auth_source = AuthSource::ApiKey;
        config.api_transport = ApiTransport::ChatCompletions;
        config.auto_retry = false;
        config.max_retries = 0;
        config.max_tool_loops = 1;

        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        let result = engine
            .discover_skill_recommendations_public(
                "check whether control-plane guardrails line up with policy rules and close the gaps",
                None,
                3,
                None,
            )
            .await
            .expect("skill discovery should succeed");

        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].skill_name, "brainstorming");
        assert_ne!(result.confidence_tier, "none");

        let request_body = recorded_bodies
            .lock()
            .expect("lock recorded bodies")
            .pop_front()
            .expect("fallback normalization should call the model");
        assert!(request_body
            .contains("You are Weles (weles) operating as the daemon-owned WELES subagent."));
    }
}
