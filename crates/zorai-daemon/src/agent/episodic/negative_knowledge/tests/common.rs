use super::super::super::schema::init_episodic_schema;
use super::super::*;
use crate::agent::{types::AgentConfig, SessionManager};
use crate::history::db::{self, sqlite::SqliteWriteConn, DbConn};
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

pub(super) async fn select_constraint_by_id(
    conn: &dyn DbConn,
    id: &str,
) -> anyhow::Result<NegativeConstraint> {
    let row = conn
        .query_opt(
            "SELECT id, agent_id, episode_id, constraint_type, subject, solution_class,
                    description, confidence, state, evidence_count, direct_observation,
                    derived_from_constraint_ids, related_subject_tokens, valid_until, created_at
             FROM negative_knowledge
             WHERE id = ?1",
            db::db_params![id],
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("constraint not found: {id}"))?;
    row_to_constraint(&row)
}

pub(super) async fn make_test_engine() -> std::sync::Arc<AgentEngine> {
    let root = tempdir().expect("tempdir").keep();
    let manager = SessionManager::new_test(&root).await;
    AgentEngine::new_test(manager, AgentConfig::default(), &root).await
}

pub(super) async fn insert_constraint_for_engine(
    engine: &AgentEngine,
    constraint: NegativeConstraint,
) -> anyhow::Result<()> {
    let agent_id = crate::agent::agent_identity::current_agent_scope_id();
    persist_constraint(
        &mut db::ConnExecutor(&*engine.history.conn_db),
        &constraint,
        &agent_id,
    )
    .await?;
    Ok(())
}

pub(super) async fn select_constraints_for_subject(
    engine: &AgentEngine,
    subject: &str,
) -> anyhow::Result<Vec<NegativeConstraint>> {
    let rows = engine
        .history
        .read_db
        .query(
            "SELECT id, agent_id, episode_id, constraint_type, subject, solution_class,
                    description, confidence, state, evidence_count, direct_observation,
                    derived_from_constraint_ids, related_subject_tokens, valid_until, created_at
             FROM negative_knowledge
             WHERE subject = ?1
             ORDER BY created_at DESC",
            db::db_params![subject],
        )
        .await?;
    rows.iter().map(row_to_constraint).collect()
}

pub(super) async fn count_negative_knowledge_rows(engine: &AgentEngine) -> anyhow::Result<u32> {
    let count = engine
        .history
        .read_db
        .query_opt("SELECT COUNT(*) FROM negative_knowledge", db::Params::None)
        .await?
        .map(|row| row.get::<i64>(0))
        .transpose()?
        .unwrap_or(0);
    Ok(count as u32)
}

pub(super) async fn init_memory_conn() -> anyhow::Result<SqliteWriteConn> {
    let raw = tokio_rusqlite::Connection::open_in_memory().await?;
    let conn = SqliteWriteConn::new(raw, std::path::PathBuf::from(":memory:"));
    init_episodic_schema(&mut db::ConnExecutor(&conn)).await?;
    Ok(conn)
}
