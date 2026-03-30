//! Negative knowledge constraint graph: tracks ruled-out approaches,
//! impossible combinations, and known limitations with TTL expiry.

use super::{ConstraintState, ConstraintType, Episode, EpisodeOutcome, NegativeConstraint};
use crate::agent::engine::AgentEngine;

use anyhow::Result;
use rusqlite::params;

// ---------------------------------------------------------------------------
// Pure functions
// ---------------------------------------------------------------------------

/// Normalize a subject into lowercase alphanumeric tokens.
/// Drops tokens shorter than 3 characters, then sorts and dedupes.
pub(crate) fn normalize_subject_tokens(subject: &str) -> Vec<String> {
    let mut tokens: Vec<String> = subject
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 3)
        .map(|token| token.to_ascii_lowercase())
        .collect();

    tokens.sort();
    tokens.dedup();
    tokens
}

/// Build a stable deduped subject key for exact normalized comparisons.
pub(crate) fn normalized_subject_key(subject: &str) -> String {
    normalize_subject_tokens(subject).join(" ")
}

/// Compute the next monotonic constraint state from accumulated evidence.
pub(crate) fn next_constraint_state(
    current: ConstraintState,
    evidence_count: u32,
    direct_observation: bool,
    confidence: f64,
) -> ConstraintState {
    if current == ConstraintState::Dead {
        return ConstraintState::Dead;
    }

    if (direct_observation && confidence >= 0.85) || evidence_count >= 3 {
        return ConstraintState::Dead;
    }

    if evidence_count >= 2 {
        return ConstraintState::Dying;
    }

    current
}

/// Determine whether two constraints are identical enough to merge.
pub(crate) fn constraints_match_for_merge(a: &NegativeConstraint, b: &NegativeConstraint) -> bool {
    let a_key = normalized_subject_key(&a.subject);
    let b_key = normalized_subject_key(&b.subject);

    !a_key.is_empty() && a_key == b_key && a.solution_class == b.solution_class
}

fn shared_normalized_subject_token_count(a: &NegativeConstraint, b: &NegativeConstraint) -> usize {
    let a_tokens = normalize_subject_tokens(&a.subject);
    let b_tokens = normalize_subject_tokens(&b.subject);

    a_tokens
        .iter()
        .filter(|token| b_tokens.binary_search(token).is_ok())
        .count()
}

/// Determine whether two constraints are related enough for propagation.
pub(crate) fn related_for_propagation(source: &NegativeConstraint, target: &NegativeConstraint) -> bool {
    let shared_tokens = shared_normalized_subject_token_count(source, target);

    match (&source.solution_class, &target.solution_class) {
        (Some(source_class), Some(target_class)) => {
            source_class == target_class && shared_tokens >= 2
        }
        (None, None) => shared_tokens >= 3,
        _ => false,
    }
}

/// Check if a constraint is still active (not expired).
pub fn is_constraint_active(constraint: &NegativeConstraint, now_ms: u64) -> bool {
    match constraint.valid_until {
        None => true,
        Some(expiry) => expiry > now_ms,
    }
}

fn constraint_state_rank(state: ConstraintState) -> u8 {
    match state {
        ConstraintState::Dead => 3,
        ConstraintState::Dying => 2,
        ConstraintState::Suspicious => 1,
    }
}

fn constraint_state_label(state: ConstraintState) -> &'static str {
    match state {
        ConstraintState::Dead => "DO NOT attempt",
        ConstraintState::Dying => "Avoid unless you have new evidence",
        ConstraintState::Suspicious => "Use caution",
    }
}

fn constraint_state_str(state: ConstraintState) -> &'static str {
    match state {
        ConstraintState::Dead => "dead",
        ConstraintState::Dying => "dying",
        ConstraintState::Suspicious => "suspicious",
    }
}

fn constraint_source_line(constraint: &NegativeConstraint) -> String {
    let source = if constraint.direct_observation {
        "direct"
    } else {
        "inferred"
    };

    if constraint.derived_from_constraint_ids.is_empty() {
        format!("Source: {source}")
    } else {
        let count = constraint.derived_from_constraint_ids.len();
        let noun = if count == 1 { "constraint" } else { "constraints" };
        format!("Source: {source} from {count} related dead {noun}")
    }
}

/// Format active negative constraints for system prompt injection.
/// Filters to active only, sorts strongest-first, and caps at 10.
pub fn format_negative_constraints(constraints: &[NegativeConstraint], now_ms: u64) -> String {
    let mut active: Vec<&NegativeConstraint> = constraints
        .iter()
        .filter(|c| is_constraint_active(c, now_ms))
        .collect();

    if active.is_empty() {
        return String::new();
    }

    active.sort_by(|a, b| {
        constraint_state_rank(b.state)
            .cmp(&constraint_state_rank(a.state))
            .then_with(|| b.created_at.cmp(&a.created_at))
    });

    let mut out = String::new();
    out.push_str("## Ruled-Out Approaches (Negative Knowledge)\n");

    let display_count = active.len().min(10);
    for constraint in active.iter().take(display_count) {
        let constraint_type_str = match constraint.constraint_type {
            ConstraintType::RuledOut => "ruled_out",
            ConstraintType::ImpossibleCombination => "impossible_combination",
            ConstraintType::KnownLimitation => "known_limitation",
        };

        out.push_str(&format!(
            "{}: {}\n",
            constraint_state_label(constraint.state),
            constraint.subject
        ));
        out.push_str(&format!("  State: {}\n", constraint_state_str(constraint.state)));
        out.push_str(&format!("  Reason: {}\n", constraint.description));
        out.push_str(&format!(
            "  Type: {} (confidence: {:.0}%)\n",
            constraint_type_str,
            constraint.confidence * 100.0
        ));
        out.push_str(&format!("  {}\n", constraint_source_line(constraint)));

        if let Some(ref sc) = constraint.solution_class {
            out.push_str(&format!("  Solution class: {sc}\n"));
        }

        match constraint.valid_until {
            Some(expiry) => {
                // Format as human-readable date
                let days_remaining = expiry.saturating_sub(now_ms) / (86400 * 1000);
                out.push_str(&format!("  Expires: in {days_remaining} days\n"));
            }
            None => {
                out.push_str("  Expires: never\n");
            }
        }

        out.push('\n');
    }

    if active.len() > 10 {
        let remaining = active.len() - 10;
        out.push_str(&format!("({remaining} more constraints not shown)\n"));
    }

    out
}

// ---------------------------------------------------------------------------
// AgentEngine integration methods
// ---------------------------------------------------------------------------

fn constraint_type_to_str(ct: &ConstraintType) -> &'static str {
    match ct {
        ConstraintType::RuledOut => "ruled_out",
        ConstraintType::ImpossibleCombination => "impossible_combination",
        ConstraintType::KnownLimitation => "known_limitation",
    }
}

fn str_to_constraint_type(s: &str) -> ConstraintType {
    match s {
        "impossible_combination" => ConstraintType::ImpossibleCombination,
        "known_limitation" => ConstraintType::KnownLimitation,
        _ => ConstraintType::RuledOut,
    }
}

impl AgentEngine {
    /// Add a negative knowledge constraint (NKNO-01).
    pub(crate) async fn add_negative_constraint(
        &self,
        constraint: NegativeConstraint,
    ) -> Result<()> {
        let c = constraint.clone();
        let ct_str = constraint_type_to_str(&c.constraint_type).to_string();
        let agent_id = crate::agent::agent_identity::current_agent_scope_id();

        self.history
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO negative_knowledge
                     (id, agent_id, episode_id, constraint_type, subject, solution_class,
                      description, confidence, valid_until, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        c.id,
                        agent_id,
                        c.episode_id,
                        ct_str,
                        c.subject,
                        c.solution_class,
                        c.description,
                        c.confidence,
                        c.valid_until.map(|v| v as i64),
                        c.created_at as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        // Update cached constraints
        let scope_id = crate::agent::agent_identity::current_agent_scope_id();
        let mut stores = self.episodic_store.write().await;
        let store = stores.entry(scope_id).or_default();
        store.cached_constraints.push(constraint.clone());

        tracing::info!(subject = %constraint.subject, "Added negative knowledge constraint");

        Ok(())
    }

    /// Record negative knowledge from a failed episode (NKNO-02).
    /// Only creates a constraint when the episode is a failure with a root cause.
    pub(crate) async fn record_negative_knowledge_from_episode(
        &self,
        episode: &Episode,
    ) -> Result<()> {
        // Only process failures with root causes
        if episode.outcome != EpisodeOutcome::Failure || episode.root_cause.is_none() {
            return Ok(());
        }

        let config = self.config.read().await;
        let constraint_ttl_days = config.episodic.constraint_ttl_days;
        drop(config);

        let now_ms = super::super::now_millis();
        let valid_until = now_ms + constraint_ttl_days * 86400 * 1000;

        let subject = if episode.summary.len() > 200 {
            format!("{}...", &episode.summary[..197])
        } else {
            episode.summary.clone()
        };

        let constraint = NegativeConstraint {
            id: format!("nc_{}", uuid::Uuid::new_v4()),
            episode_id: Some(episode.id.clone()),
            constraint_type: ConstraintType::RuledOut,
            subject,
            solution_class: episode.solution_class.clone(),
            description: episode.root_cause.clone().unwrap_or_default(),
            confidence: episode.confidence.unwrap_or(0.7),
            state: ConstraintState::Dying,
            evidence_count: 1,
            direct_observation: true,
            derived_from_constraint_ids: Vec::new(),
            related_subject_tokens: Vec::new(),
            valid_until: Some(valid_until),
            created_at: now_ms,
        };

        self.add_negative_constraint(constraint).await
    }

    /// Query active (non-expired) constraints, optionally filtered by entity (NKNO-03).
    pub(crate) async fn query_active_constraints(
        &self,
        entity_filter: Option<&str>,
    ) -> Result<Vec<NegativeConstraint>> {
        let now_ms = super::super::now_millis() as i64;
        let filter = entity_filter.map(|s| format!("%{s}%"));
        let agent_id = crate::agent::agent_identity::current_agent_scope_id();
        let include_legacy = crate::agent::is_main_agent_scope(&agent_id) as i64;

        self.history
            .conn
            .call(move |conn| {
                if let Some(ref pattern) = filter {
                    let mut stmt = conn.prepare(
                        "SELECT id, agent_id, episode_id, constraint_type, subject, solution_class,
                                description, confidence, valid_until, created_at
                         FROM negative_knowledge
                         WHERE (agent_id = ?1 OR (?2 = 1 AND agent_id IS NULL))
                         AND (valid_until IS NULL OR valid_until > ?3)
                         AND (subject LIKE ?4 OR solution_class LIKE ?4)
                         ORDER BY created_at DESC
                         LIMIT 20",
                    )?;
                    let rows =
                        stmt.query_map(params![agent_id, include_legacy, now_ms, pattern], row_to_constraint)?;
                    let mut constraints = Vec::new();
                    for row in rows {
                        constraints.push(row?);
                    }
                    Ok(constraints)
                } else {
                    let mut stmt = conn.prepare(
                        "SELECT id, agent_id, episode_id, constraint_type, subject, solution_class,
                                description, confidence, valid_until, created_at
                         FROM negative_knowledge
                         WHERE (agent_id = ?1 OR (?2 = 1 AND agent_id IS NULL))
                           AND (valid_until IS NULL OR valid_until > ?3)
                         ORDER BY created_at DESC
                         LIMIT 20",
                    )?;
                    let rows = stmt.query_map(params![agent_id, include_legacy, now_ms], row_to_constraint)?;
                    let mut constraints = Vec::new();
                    for row in rows {
                        constraints.push(row?);
                    }
                    Ok(constraints)
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Expire (delete) constraints past their TTL (NKNO-04).
    pub(crate) async fn expire_negative_constraints(&self) -> Result<usize> {
        let now_ms = super::super::now_millis() as i64;

        let deleted = self
            .history
            .conn
            .call(move |conn| {
                let count = conn.execute(
                    "DELETE FROM negative_knowledge WHERE valid_until IS NOT NULL AND valid_until <= ?1",
                    params![now_ms],
                )?;
                Ok(count)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        // Update cached constraints: filter out expired
        if deleted > 0 {
            let now_ms = super::super::now_millis();
            let scope_id = crate::agent::agent_identity::current_agent_scope_id();
            let mut stores = self.episodic_store.write().await;
            let store = stores.entry(scope_id).or_default();
            store
                .cached_constraints
                .retain(|c| is_constraint_active(c, now_ms));
        }

        Ok(deleted)
    }

    /// Refresh the in-memory constraint cache from the database.
    pub(crate) async fn refresh_constraint_cache(&self) -> Result<()> {
        let constraints = self.query_active_constraints(None).await?;
        let scope_id = crate::agent::agent_identity::current_agent_scope_id();
        let mut stores = self.episodic_store.write().await;
        let store = stores.entry(scope_id).or_default();
        store.cached_constraints = constraints;
        Ok(())
    }
}

fn row_to_constraint(row: &rusqlite::Row<'_>) -> rusqlite::Result<NegativeConstraint> {
    let ct_str: String = row.get(3)?;
    Ok(NegativeConstraint {
        id: row.get(0)?,
        episode_id: row.get(2)?,
        constraint_type: str_to_constraint_type(&ct_str),
        subject: row.get(4)?,
        solution_class: row.get(5)?,
        description: row.get(6)?,
        confidence: row.get(7)?,
        state: ConstraintState::Dying,
        evidence_count: 1,
        direct_observation: true,
        derived_from_constraint_ids: Vec::new(),
        related_subject_tokens: Vec::new(),
        valid_until: row.get::<_, Option<i64>>(8)?.map(|v| v as u64),
        created_at: row.get::<_, i64>(9)? as u64,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_constraint(subject: &str, valid_until: Option<u64>) -> NegativeConstraint {
        NegativeConstraint {
            id: format!("nc-{subject}"),
            episode_id: Some("ep-001".to_string()),
            constraint_type: ConstraintType::RuledOut,
            subject: subject.to_string(),
            solution_class: Some("test-class".to_string()),
            description: format!("Reason for {subject}"),
            confidence: 0.85,
            state: ConstraintState::Dying,
            evidence_count: 1,
            direct_observation: true,
            derived_from_constraint_ids: Vec::new(),
            related_subject_tokens: Vec::new(),
            valid_until,
            created_at: 1_000_000_000,
        }
    }

    fn make_constraint_with_class(subject: &str, solution_class: Option<&str>) -> NegativeConstraint {
        NegativeConstraint {
            solution_class: solution_class.map(str::to_string),
            ..make_constraint(subject, Some(2_000_000_000))
        }
    }

    fn make_constraint_with_details(
        subject: &str,
        state: ConstraintState,
        created_at: u64,
        direct_observation: bool,
        derived_from_constraint_ids: &[&str],
    ) -> NegativeConstraint {
        NegativeConstraint {
            subject: subject.to_string(),
            state,
            created_at,
            direct_observation,
            derived_from_constraint_ids: derived_from_constraint_ids
                .iter()
                .map(|id| (*id).to_string())
                .collect(),
            valid_until: Some(2_000_000_000),
            ..make_constraint(subject, Some(2_000_000_000))
        }
    }

    #[test]
    fn format_negative_constraints_empty_returns_empty() {
        assert!(format_negative_constraints(&[], 1_000_000_000).is_empty());
    }

    #[test]
    fn format_negative_constraints_groups_and_sorts_by_state_then_created_at() {
        let constraints = vec![
            make_constraint_with_details("suspicious old", ConstraintState::Suspicious, 100, true, &[]),
            make_constraint_with_details("dead newer", ConstraintState::Dead, 300, true, &["nc-1"]),
            make_constraint_with_details("dying newest", ConstraintState::Dying, 400, false, &["nc-2"]),
            make_constraint_with_details("dead oldest", ConstraintState::Dead, 200, true, &[]),
            make_constraint_with_details("suspicious newer", ConstraintState::Suspicious, 500, false, &[]),
        ];

        let result = format_negative_constraints(&constraints, 1_000_000_000);

        let dead_newer = result.find("DO NOT attempt: dead newer").unwrap();
        let dead_oldest = result.find("DO NOT attempt: dead oldest").unwrap();
        let dying_newest = result.find("Avoid unless you have new evidence: dying newest").unwrap();
        let suspicious_newer = result.find("Use caution: suspicious newer").unwrap();
        let suspicious_old = result.find("Use caution: suspicious old").unwrap();

        assert!(dead_newer < dead_oldest);
        assert!(dead_oldest < dying_newest);
        assert!(dying_newest < suspicious_newer);
        assert!(suspicious_newer < suspicious_old);
    }

    #[test]
    fn format_negative_constraints_renders_state_metadata_and_conditional_provenance() {
        let constraints = vec![
            make_constraint_with_details("dead path", ConstraintState::Dead, 300, true, &["nc-1"]),
            make_constraint_with_details("dying path", ConstraintState::Dying, 200, false, &["nc-2"]),
            make_constraint_with_details("suspicious path", ConstraintState::Suspicious, 100, true, &[]),
        ];

        let result = format_negative_constraints(&constraints, 1_000_000_000);

        assert!(result.contains("DO NOT attempt: dead path"));
        assert!(result.contains("Avoid unless you have new evidence: dying path"));
        assert!(result.contains("Use caution: suspicious path"));
        assert!(result.contains("State: dead"));
        assert!(result.contains("State: dying"));
        assert!(result.contains("State: suspicious"));
        assert!(result.contains("Reason: Reason for dead path"));
        assert!(result.contains("Type: ruled_out"));
        assert!(result.contains("confidence: 85%"));
        assert!(result.contains("Source: direct"));
        assert!(result.contains("Source: inferred"));
        assert!(result.contains("Source: direct from 1 related dead constraint"));
        assert!(result.contains("Source: inferred from 1 related dead constraint"));
        assert!(!result.contains("Source: direct from 0 related dead constraints"));
    }

    #[test]
    fn format_negative_constraints_with_two_constraints() {
        let constraints = vec![
            make_constraint("npm install approach", Some(2_000_000_000)),
            make_constraint("yarn install approach", None),
        ];
        let result = format_negative_constraints(&constraints, 1_000_000_000);
        assert!(result.contains("Avoid unless you have new evidence: npm install approach"));
        assert!(result.contains("Avoid unless you have new evidence: yarn install approach"));
        assert!(result.contains("Ruled-Out Approaches"));
        assert!(result.contains("State: dying"));
        assert!(result.contains("Reason: Reason for npm install approach"));
        assert!(result.contains("Type: ruled_out"));
        assert!(result.contains("confidence: 85%"));
        assert!(result.contains("Source: direct"));
    }

    #[test]
    fn format_negative_constraints_includes_solution_class() {
        let constraints = vec![make_constraint("bad approach", Some(2_000_000_000))];
        let result = format_negative_constraints(&constraints, 1_000_000_000);
        assert!(result.contains("Solution class: test-class"));
    }

    #[test]
    fn format_negative_constraints_includes_expiry() {
        let now_ms = 1_000_000_000u64;
        let in_10_days = now_ms + 10 * 86400 * 1000;
        let constraints = vec![make_constraint("approach", Some(in_10_days))];
        let result = format_negative_constraints(&constraints, now_ms);
        assert!(result.contains("Expires: in 10 days"));
    }

    #[test]
    fn is_constraint_active_no_valid_until_returns_true() {
        let c = make_constraint("test", None);
        assert!(is_constraint_active(&c, 9_999_999_999));
    }

    #[test]
    fn is_constraint_active_future_valid_until_returns_true() {
        let c = make_constraint("test", Some(2_000_000_000));
        assert!(is_constraint_active(&c, 1_000_000_000));
    }

    #[test]
    fn is_constraint_active_past_valid_until_returns_false() {
        let c = make_constraint("test", Some(500_000_000));
        assert!(!is_constraint_active(&c, 1_000_000_000));
    }

    #[test]
    fn normalize_subject_tokens_sorts_dedupes_and_filters() {
        assert_eq!(
            normalize_subject_tokens("Fix deploy-config in prod!"),
            vec!["config", "deploy", "fix", "prod"]
        );
    }

    #[test]
    fn normalize_subject_tokens_is_stable_across_case_and_punctuation() {
        assert_eq!(
            normalize_subject_tokens("Deploy, CONFIG; fix fix prod??"),
            vec!["config", "deploy", "fix", "prod"]
        );
    }

    #[test]
    fn normalized_subject_key_returns_stable_deduped_key() {
        assert_eq!(
            normalized_subject_key("Fix deploy-config in prod!"),
            "config deploy fix prod"
        );
        assert_eq!(
            normalized_subject_key("prod deploy fix config fix"),
            "config deploy fix prod"
        );
    }

    #[test]
    fn constraints_match_for_merge_requires_same_normalized_subject_and_solution_class() {
        let a = make_constraint_with_class("Fix deploy-config in prod!", Some("deploy-fix"));
        let b = make_constraint_with_class("prod deploy fix config", Some("deploy-fix"));

        assert!(constraints_match_for_merge(&a, &b));
    }

    #[test]
    fn constraints_match_for_merge_rejects_different_solution_class() {
        let a = make_constraint_with_class("Fix deploy-config in prod!", Some("deploy-fix"));
        let b = make_constraint_with_class("prod deploy fix config", Some("ops-fix"));

        assert!(!constraints_match_for_merge(&a, &b));
    }

    #[test]
    fn constraints_match_for_merge_rejects_missing_solution_class_on_one_side() {
        let a = make_constraint_with_class("Fix deploy-config in prod!", Some("deploy-fix"));
        let b = make_constraint_with_class("prod deploy fix config", None);

        assert!(!constraints_match_for_merge(&a, &b));
    }

    #[test]
    fn constraints_match_for_merge_allows_matching_none_solution_class() {
        let a = make_constraint_with_class("Fix deploy-config in prod!", None);
        let b = make_constraint_with_class("prod deploy fix config", None);

        assert!(constraints_match_for_merge(&a, &b));
    }

    #[test]
    fn constraints_match_for_merge_rejects_empty_normalized_subjects() {
        let a = make_constraint_with_class("CI CD", Some("deploy-fix"));
        let b = make_constraint_with_class("QA DB", Some("deploy-fix"));

        assert!(!constraints_match_for_merge(&a, &b));
    }

    #[test]
    fn constraints_match_for_merge_rejects_different_normalized_subjects_with_same_solution_class() {
        let a = make_constraint_with_class("deploy config rollback", Some("deploy-fix"));
        let b = make_constraint_with_class("cache rebuild timeout", Some("deploy-fix"));

        assert!(!constraints_match_for_merge(&a, &b));
    }

    #[test]
    fn related_for_propagation_requires_two_shared_tokens_with_same_solution_class() {
        let source = make_constraint_with_class("fix deploy config prod", Some("deploy-fix"));
        let target = make_constraint_with_class("deploy config rollback", Some("deploy-fix"));

        assert!(related_for_propagation(&source, &target));
    }

    #[test]
    fn related_for_propagation_rejects_same_class_with_only_one_shared_token() {
        let source = make_constraint_with_class("fix deploy config prod", Some("deploy-fix"));
        let target = make_constraint_with_class("deploy cache rebuild", Some("deploy-fix"));

        assert!(!related_for_propagation(&source, &target));
    }

    #[test]
    fn related_for_propagation_requires_three_shared_tokens_without_solution_class() {
        let source = make_constraint_with_class("deploy config prod fix", None);
        let target = make_constraint_with_class("prod deploy config rollback", None);

        assert!(related_for_propagation(&source, &target));
    }

    #[test]
    fn related_for_propagation_rejects_mixed_solution_class_even_with_shared_tokens() {
        let source = make_constraint_with_class("deploy config prod fix", Some("deploy-fix"));
        let target = make_constraint_with_class("prod deploy config rollback", None);

        assert!(!related_for_propagation(&source, &target));
    }

    #[test]
    fn next_constraint_state_keeps_dead_dead() {
        assert_eq!(
            next_constraint_state(ConstraintState::Dead, 1, false, 0.2),
            ConstraintState::Dead
        );
    }

    #[test]
    fn next_constraint_state_promotes_to_dead_for_direct_high_confidence_observation() {
        assert_eq!(
            next_constraint_state(ConstraintState::Suspicious, 1, true, 0.85),
            ConstraintState::Dead
        );
    }

    #[test]
    fn next_constraint_state_promotes_to_dead_at_three_evidence() {
        assert_eq!(
            next_constraint_state(ConstraintState::Dying, 3, false, 0.4),
            ConstraintState::Dead
        );
    }

    #[test]
    fn next_constraint_state_promotes_to_dying_at_two_evidence() {
        assert_eq!(
            next_constraint_state(ConstraintState::Suspicious, 2, false, 0.4),
            ConstraintState::Dying
        );
    }

    #[test]
    fn next_constraint_state_does_not_promote_for_direct_observation_alone() {
        assert_eq!(
            next_constraint_state(ConstraintState::Suspicious, 1, true, 0.84),
            ConstraintState::Suspicious
        );
    }

    #[test]
    fn next_constraint_state_keeps_existing_non_terminal_state_when_thresholds_not_met() {
        assert_eq!(
            next_constraint_state(ConstraintState::Dying, 1, false, 0.4),
            ConstraintState::Dying
        );
        assert_eq!(
            next_constraint_state(ConstraintState::Suspicious, 1, false, 0.4),
            ConstraintState::Suspicious
        );
    }
}
