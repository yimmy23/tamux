use super::super::super::schema::init_episodic_schema;
use super::super::*;
use crate::agent::{types::AgentConfig, SessionManager};
use rusqlite::{params, Connection};
use tempfile::tempdir;

pub(super) fn make_constraint(subject: &str, valid_until: Option<u64>) -> NegativeConstraint {
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

pub(super) fn make_constraint_with_class(
    subject: &str,
    solution_class: Option<&str>,
) -> NegativeConstraint {
    NegativeConstraint {
        solution_class: solution_class.map(str::to_string),
        ..make_constraint(subject, Some(2_000_000_000))
    }
}

pub(super) fn make_constraint_with_details(
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

pub(super) fn make_failure_episode(summary: &str, confidence: Option<f64>) -> Episode {
    Episode {
        id: "ep-failure".to_string(),
        goal_run_id: None,
        thread_id: None,
        session_id: None,
        goal_text: None,
        goal_type: None,
        episode_type: super::super::super::EpisodeType::GoalFailure,
        summary: summary.to_string(),
        outcome: EpisodeOutcome::Failure,
        root_cause: Some("root cause".to_string()),
        entities: Vec::new(),
        causal_chain: Vec::new(),
        solution_class: Some("test-class".to_string()),
        duration_ms: None,
        tokens_used: None,
        confidence,
        confidence_before: None,
        confidence_after: None,
        created_at: 1_000,
        expires_at: None,
    }
}

pub(super) fn select_constraint_by_id(
    conn: &Connection,
    id: &str,
) -> rusqlite::Result<NegativeConstraint> {
    conn.query_row(
        "SELECT id, agent_id, episode_id, constraint_type, subject, solution_class,
                description, confidence, state, evidence_count, direct_observation,
                derived_from_constraint_ids, related_subject_tokens, valid_until, created_at
         FROM negative_knowledge
         WHERE id = ?1",
        params![id],
        row_to_constraint,
    )
}

pub(super) async fn make_test_engine() -> std::sync::Arc<AgentEngine> {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await
}

pub(super) async fn insert_constraint_for_engine(
    engine: &AgentEngine,
    constraint: NegativeConstraint,
) -> anyhow::Result<()> {
    let agent_id = crate::agent::agent_identity::current_agent_scope_id();
    engine
        .history
        .conn
        .call(move |conn| {
            persist_constraint(conn, &constraint, &agent_id)?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

pub(super) async fn select_constraints_for_subject(
    engine: &AgentEngine,
    subject: &str,
) -> anyhow::Result<Vec<NegativeConstraint>> {
    let subject = subject.to_string();
    engine
        .history
        .conn
        .call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, agent_id, episode_id, constraint_type, subject, solution_class,
                        description, confidence, state, evidence_count, direct_observation,
                        derived_from_constraint_ids, related_subject_tokens, valid_until, created_at
                 FROM negative_knowledge
                 WHERE subject = ?1
                 ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map(params![subject], row_to_constraint)?;
            let mut constraints = Vec::new();
            for row in rows {
                constraints.push(row?);
            }
            Ok(constraints)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

pub(super) async fn count_negative_knowledge_rows(engine: &AgentEngine) -> anyhow::Result<u32> {
    engine
        .history
        .conn
        .call(|conn| {
            let count: u32 =
                conn.query_row("SELECT COUNT(*) FROM negative_knowledge", [], |row| {
                    row.get(0)
                })?;
            Ok(count)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

pub(super) fn init_memory_conn() -> anyhow::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    init_episodic_schema(&conn)?;
    Ok(conn)
}
