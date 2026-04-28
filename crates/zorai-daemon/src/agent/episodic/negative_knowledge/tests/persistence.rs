use super::super::*;
use super::common::*;
use rusqlite::params;

#[test]
fn row_to_constraint_reads_richer_persisted_fields() -> anyhow::Result<()> {
    let conn = init_memory_conn()?;

    conn.execute(
        "INSERT INTO negative_knowledge
         (id, agent_id, episode_id, constraint_type, subject, solution_class,
          description, confidence, state, evidence_count, direct_observation,
          derived_from_constraint_ids, related_subject_tokens, valid_until, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            "nc-rich",
            "agent-1",
            "ep-123",
            "known_limitation",
            "rich subject",
            "solver",
            "cannot proceed",
            0.92,
            "dead",
            4,
            0,
            "[\"nc-a\",\"nc-b\"]",
            "[\"alpha\",\"beta\"]",
            1_234_567i64,
            7_654_321i64,
        ],
    )?;

    let constraint = select_constraint_by_id(&conn, "nc-rich")?;

    assert_eq!(constraint.state, ConstraintState::Dead);
    assert_eq!(constraint.evidence_count, 4);
    assert!(!constraint.direct_observation);
    assert_eq!(
        constraint.derived_from_constraint_ids,
        vec!["nc-a".to_string(), "nc-b".to_string()]
    );
    assert_eq!(
        constraint.related_subject_tokens,
        vec!["alpha".to_string(), "beta".to_string()]
    );

    Ok(())
}

#[test]
fn row_to_constraint_defaults_new_fields_for_legacy_rows() -> anyhow::Result<()> {
    let conn = init_memory_conn()?;

    conn.execute(
        "INSERT INTO negative_knowledge
         (id, agent_id, episode_id, constraint_type, subject, solution_class,
          description, confidence, valid_until, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            "nc-legacy",
            "agent-1",
            "ep-legacy",
            "ruled_out",
            "legacy subject",
            "solver",
            "legacy description",
            0.61,
            2_222_222i64,
            3_333_333i64,
        ],
    )?;

    let constraint = select_constraint_by_id(&conn, "nc-legacy")?;

    assert_eq!(constraint.state, ConstraintState::Dying);
    assert_eq!(constraint.evidence_count, 1);
    assert!(constraint.direct_observation);
    assert!(constraint.derived_from_constraint_ids.is_empty());
    assert!(constraint.related_subject_tokens.is_empty());

    Ok(())
}
