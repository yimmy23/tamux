use super::*;
use crate::agent::episodic::{CounterWhoState, NegativeConstraint};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RuntimeContinuityContext {
    pub continuity_summary: Option<String>,
    pub negative_constraints_context: Option<String>,
}

pub(crate) fn select_runtime_context_query(
    task_scope: Option<&str>,
    goal_scope: Option<&str>,
    operator_text: Option<&str>,
) -> Option<String> {
    [task_scope, goal_scope, operator_text]
        .into_iter()
        .flatten()
        .map(|value| value.split_whitespace().collect::<Vec<_>>().join(" "))
        .find(|value| !value.is_empty())
}

pub(crate) fn format_runtime_work_scope_label(
    goal_title: Option<&str>,
    step_title: Option<&str>,
    task_title: Option<&str>,
) -> Option<String> {
    let mut parts = Vec::new();

    if let Some(goal_title) = goal_title.map(str::trim).filter(|value| !value.is_empty()) {
        parts.push(format!("goal \"{goal_title}\""));
    }
    if let Some(step_title) = step_title.map(str::trim).filter(|value| !value.is_empty()) {
        parts.push(format!("step \"{step_title}\""));
    }
    if let Some(task_title) = task_title.map(str::trim).filter(|value| !value.is_empty()) {
        parts.push(format!("task \"{task_title}\""));
    }

    (!parts.is_empty()).then(|| parts.join(" / "))
}

pub(crate) fn format_runtime_continuity_summary(
    agent_name: Option<&str>,
    agent_guidance: Option<&str>,
    work_scope_label: Option<&str>,
    counter_who: &CounterWhoState,
    constraints: &[NegativeConstraint],
    now_ms: u64,
) -> String {
    let mut bullets = Vec::new();

    if let Some(agent_name) = agent_name.map(str::trim).filter(|value| !value.is_empty()) {
        bullets.push(format!("- I am carrying this forward as {agent_name}."));
    }
    if let Some(agent_guidance) = agent_guidance
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        bullets.push(format!(
            "- I should lean into this persona's role: {agent_guidance}"
        ));
    }

    if let Some(work_scope) = work_scope_label
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        bullets.push(format!(
            "- I am continuing the same workstream: {work_scope}"
        ));
    }

    if let Some(current_focus) = counter_who
        .current_focus
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        bullets.push(format!("- I am still focused on: {current_focus}"));
    }

    if let Some(pattern) =
        super::episodic::counter_who::detect_repeated_approaches(&counter_who.tried_approaches, 3)
    {
        bullets.push(format!("- {pattern}"));
    }

    let repeated_corrections = counter_who
        .correction_patterns
        .iter()
        .filter(|pattern| pattern.correction_count >= 2)
        .take(2)
        .map(|pattern| format!("{} ({}x)", pattern.pattern, pattern.correction_count))
        .collect::<Vec<_>>();
    if !repeated_corrections.is_empty() {
        bullets.push(format!(
            "- I should keep these operator corrections active: {}",
            repeated_corrections.join("; ")
        ));
    }

    let active_constraint_descriptions = constraints
        .iter()
        .filter(|constraint| {
            constraint
                .valid_until
                .map(|valid_until| valid_until > now_ms)
                .unwrap_or(true)
        })
        .take(2)
        .map(|constraint| constraint.description.trim().to_string())
        .filter(|description| !description.is_empty())
        .collect::<Vec<_>>();
    if !active_constraint_descriptions.is_empty() {
        bullets.push(format!(
            "- I already ruled out: {}",
            active_constraint_descriptions.join("; ")
        ));
    }

    if bullets.is_empty() {
        return String::new();
    }

    let mut summary = String::from(
        "## Working Continuity\nCarry this forward from the last attempts before choosing the next move:\n",
    );
    summary.push_str(&bullets.join("\n"));

    if summary.chars().count() > 1200 {
        let truncated = summary.chars().take(1197).collect::<String>();
        return format!("{truncated}...");
    }

    summary
}

pub(crate) async fn build_runtime_continuity_context(
    engine: &AgentEngine,
    work_scope_label: Option<&str>,
    query_text: Option<&str>,
) -> RuntimeContinuityContext {
    let now_ms = super::now_millis();
    let scope_id = crate::agent::agent_identity::current_agent_scope_id();
    let agent_name = canonical_agent_name(&scope_id);
    let agent_guidance = canonical_agent_guidance(&scope_id);

    let should_restore_counter_who = {
        let stores = engine.episodic_store.read().await;
        stores
            .get(&scope_id)
            .map(|store| counter_who_state_is_empty(&store.counter_who))
            .unwrap_or(true)
    };
    if should_restore_counter_who {
        if let Err(error) = engine.restore_counter_who(None).await {
            tracing::warn!(scope_id = %scope_id, error = %error, "failed to restore counter-who state for runtime continuity");
        }
    }

    let counter_who = {
        let stores = engine.episodic_store.read().await;
        stores
            .get(&scope_id)
            .map(|store| store.counter_who.clone())
            .unwrap_or_default()
    };

    let constraints = match query_text.filter(|value| !value.trim().is_empty()) {
        Some(query_text) => engine.query_active_constraints(Some(query_text)).await,
        None => Ok(Vec::new()),
    }
    .unwrap_or_default();

    let continuity_summary = format_runtime_continuity_summary(
        Some(agent_name),
        agent_guidance,
        work_scope_label,
        &counter_who,
        &constraints,
        now_ms,
    );
    let negative_constraints_context =
        super::episodic::negative_knowledge::format_negative_constraints(&constraints, now_ms);

    RuntimeContinuityContext {
        continuity_summary: (!continuity_summary.trim().is_empty()).then_some(continuity_summary),
        negative_constraints_context: (!negative_constraints_context.trim().is_empty())
            .then_some(negative_constraints_context),
    }
}

fn counter_who_state_is_empty(state: &CounterWhoState) -> bool {
    state.goal_run_id.is_none()
        && state.thread_id.is_none()
        && state.current_focus.is_none()
        && state.recent_changes.is_empty()
        && state.tried_approaches.is_empty()
        && state.correction_patterns.is_empty()
        && state.active_constraint_ids.is_empty()
        && state.updated_at == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::agent_identity::{MAIN_AGENT_ID, MAIN_AGENT_NAME};
    use crate::agent::episodic::{
        ConstraintState, ConstraintType, CorrectionPattern, EpisodeOutcome, TriedApproach,
    };
    use crate::agent::{types::AgentConfig, SessionManager};
    use tempfile::tempdir;

    fn sample_constraint(description: &str) -> NegativeConstraint {
        NegativeConstraint {
            id: "nc-1".to_string(),
            episode_id: None,
            constraint_type: ConstraintType::RuledOut,
            subject: "bash sync path".to_string(),
            solution_class: Some("sync".to_string()),
            description: description.to_string(),
            confidence: 0.9,
            state: ConstraintState::Dead,
            evidence_count: 2,
            direct_observation: true,
            derived_from_constraint_ids: Vec::new(),
            related_subject_tokens: vec!["bash".to_string(), "sync".to_string()],
            valid_until: Some(2_000_000_000),
            created_at: 1_000_000_000,
        }
    }

    #[test]
    fn select_runtime_context_query_prefers_task_scope_then_falls_back() {
        assert_eq!(
            select_runtime_context_query(
                Some("  Fix the bash sync lane  "),
                Some("Goal text"),
                Some("Operator text"),
            )
            .as_deref(),
            Some("Fix the bash sync lane")
        );
        assert_eq!(
            select_runtime_context_query(None, Some("  Goal text  "), Some("Operator text"))
                .as_deref(),
            Some("Goal text")
        );
        assert_eq!(
            select_runtime_context_query(None, None, Some("  Operator text  ")).as_deref(),
            Some("Operator text")
        );
    }

    #[test]
    fn format_runtime_work_scope_label_includes_goal_step_and_task_titles() {
        assert_eq!(
            format_runtime_work_scope_label(
                Some("Test goal"),
                Some("Investigate failure"),
                Some("Inspect the failing path")
            )
            .as_deref(),
            Some(
                "goal \"Test goal\" / step \"Investigate failure\" / task \"Inspect the failing path\""
            )
        );
    }

    #[test]
    fn format_runtime_continuity_summary_surfaces_focus_learning_and_constraints() {
        let mut state = CounterWhoState {
            current_focus: Some("Tool: bash".to_string()),
            correction_patterns: vec![CorrectionPattern {
                pattern: "Inspect state before retrying".to_string(),
                correction_count: 2,
                last_correction_at: 1_000_000_010,
            }],
            ..Default::default()
        };
        state.tried_approaches = vec![
            TriedApproach {
                approach_hash: "same-hash".to_string(),
                description: "bash(test --sync)".to_string(),
                outcome: EpisodeOutcome::Failure,
                timestamp: 1_000_000_000,
            },
            TriedApproach {
                approach_hash: "same-hash".to_string(),
                description: "bash(test --sync)".to_string(),
                outcome: EpisodeOutcome::Failure,
                timestamp: 1_000_000_001,
            },
            TriedApproach {
                approach_hash: "same-hash".to_string(),
                description: "bash(test --sync)".to_string(),
                outcome: EpisodeOutcome::Failure,
                timestamp: 1_000_000_002,
            },
        ];

        let summary = format_runtime_continuity_summary(
            Some(MAIN_AGENT_NAME),
            None,
            Some(
                "goal \"Test goal\" / step \"Investigate failure\" / task \"Inspect the failing path\"",
            ),
            &state,
            &[sample_constraint(
                "Retrying the old sync path keeps failing.",
            )],
            1_000_000_500,
        );

        assert!(summary.contains("## Working Continuity"));
        assert!(summary.contains("Carry this forward from the last attempts"));
        assert!(summary.contains(&format!("I am carrying this forward as {MAIN_AGENT_NAME}.")));
        assert!(summary.contains("I am continuing the same workstream: goal \"Test goal\" / step \"Investigate failure\" / task \"Inspect the failing path\""));
        assert!(summary.contains("I am still focused on: Tool: bash"));
        assert!(summary.contains("Repeated failure detected"));
        assert!(summary.contains("I should keep these operator corrections active"));
        assert!(summary.contains("I already ruled out: Retrying the old sync path keeps failing."));
    }

    #[test]
    fn format_runtime_continuity_summary_includes_spawned_persona_guidance() {
        let summary = format_runtime_continuity_summary(
            Some(crate::agent::agent_identity::RADOGOST_AGENT_NAME),
            Some(
                "You specialize in negotiation between options, comparing tradeoffs, and surfacing the strongest route forward.",
            ),
            Some(
                "goal \"Spawned goal\" / step \"Investigate failure\" / task \"Investigate failure\"",
            ),
            &CounterWhoState::default(),
            &[],
            1_000_000_500,
        );

        assert!(summary.contains("I am carrying this forward as Radogost."));
        assert!(summary.contains("I should lean into this persona's role:"));
        assert!(summary.contains("comparing tradeoffs"));
    }

    #[test]
    fn build_system_prompt_includes_continuity_and_negative_knowledge_sections() {
        let prompt = build_system_prompt(
            &AgentConfig::default(),
            "Base prompt",
            &AgentMemory::default(),
            &MemoryPaths {
                memory_dir: "/tmp/agent".into(),
                memory_path: "/tmp/agent/MEMORY.md".into(),
                soul_path: "/tmp/agent/SOUL.md".into(),
                user_path: "/tmp/agent/USER.md".into(),
            },
            MAIN_AGENT_ID,
            &[],
            None,
            None,
            None,
            None,
            Some("## Episodic Context\n- Past failure on sync path"),
            Some("## Working Continuity\n- Keep the new approach aligned"),
            Some("## Ruled-Out Approaches (Negative Knowledge)\n- Dead: old sync path"),
        );

        assert!(prompt.contains("## Episodic Context"));
        assert!(prompt.contains("## Working Continuity"));
        assert!(prompt.contains("## Ruled-Out Approaches (Negative Knowledge)"));
    }

    #[tokio::test]
    async fn build_runtime_continuity_context_restores_persisted_counter_who_state() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        engine
            .update_counter_who_on_tool_result(
                "thread-runtime-restore",
                "read_file",
                "src/main.rs",
                false,
            )
            .await;

        {
            let mut stores = engine.episodic_store.write().await;
            stores.clear();
        }

        let context = build_runtime_continuity_context(
            &engine,
            Some("goal \"Test goal\" / task \"Inspect persisted state\""),
            None,
        )
        .await;

        let summary = context
            .continuity_summary
            .expect("continuity summary should be present");
        assert!(summary.contains("I am still focused on: Tool: read_file"));
    }
}
