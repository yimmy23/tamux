#![allow(dead_code)]

use super::*;

fn constraint_state_to_str(state: &ConstraintState) -> &'static str {
    match state {
        ConstraintState::Dead => "dead",
        ConstraintState::Dying => "dying",
        ConstraintState::Suspicious => "suspicious",
    }
}

fn str_to_constraint_state(s: &str) -> ConstraintState {
    match s {
        "dead" => ConstraintState::Dead,
        "suspicious" => ConstraintState::Suspicious,
        _ => ConstraintState::Dying,
    }
}

fn parse_json_string_vec(value: String) -> Vec<String> {
    if value.trim().is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(&value).unwrap_or_default()
    }
}

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

pub(crate) async fn load_all_active_constraints<E: db::DbExecutor + ?Sized>(
    exec: &mut E,
    agent_id: &str,
    include_legacy: i64,
    now_ms: i64,
) -> Result<Vec<NegativeConstraint>> {
    let rows = exec
        .query(
            "SELECT id, agent_id, episode_id, constraint_type, subject, solution_class,
                    description, confidence, state, evidence_count, direct_observation,
                    derived_from_constraint_ids, related_subject_tokens, valid_until, created_at
             FROM negative_knowledge
             WHERE (agent_id = ?1 OR (?2 = 1 AND agent_id IS NULL))
               AND (valid_until IS NULL OR valid_until > ?3)
               AND deleted_at IS NULL
             ORDER BY created_at DESC",
            db::db_params![agent_id, include_legacy, now_ms],
        )
        .await?;
    rows.iter().map(row_to_constraint).collect()
}

pub(crate) async fn persist_constraint<E: db::DbExecutor + ?Sized>(
    exec: &mut E,
    constraint: &NegativeConstraint,
    agent_id: &str,
) -> Result<()> {
    let derived_from_constraint_ids =
        serde_json::to_string(&constraint.derived_from_constraint_ids)?;
    let related_subject_tokens = serde_json::to_string(&constraint.related_subject_tokens)?;

    exec.execute(
        "INSERT OR REPLACE INTO negative_knowledge
         (id, agent_id, episode_id, constraint_type, subject, solution_class,
          description, confidence, state, evidence_count, direct_observation,
          derived_from_constraint_ids, related_subject_tokens, valid_until, created_at, deleted_at)
          VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, NULL)",
        db::db_params![
            constraint.id.clone(),
            agent_id,
            constraint.episode_id.clone(),
            constraint_type_to_str(&constraint.constraint_type),
            constraint.subject.clone(),
            constraint.solution_class.clone(),
            constraint.description.clone(),
            constraint.confidence,
            constraint_state_to_str(&constraint.state),
            constraint.evidence_count,
            if constraint.direct_observation { 1 } else { 0 },
            derived_from_constraint_ids,
            related_subject_tokens,
            constraint.valid_until.map(|v| v as i64),
            constraint.created_at as i64,
        ],
    )
    .await?;

    Ok(())
}

pub(crate) fn row_to_constraint(row: &db::Row) -> Result<NegativeConstraint> {
    let ct_str: String = row.get(3)?;
    let state_str: String = row.get(8)?;
    let direct_observation: i64 = row.get(10)?;
    let derived_from_constraint_ids = parse_json_string_vec(row.get(11)?);
    let related_subject_tokens = parse_json_string_vec(row.get(12)?);

    Ok(NegativeConstraint {
        id: row.get(0)?,
        episode_id: row.get(2)?,
        constraint_type: str_to_constraint_type(&ct_str),
        subject: row.get(4)?,
        solution_class: row.get(5)?,
        description: row.get(6)?,
        confidence: row.get(7)?,
        state: str_to_constraint_state(&state_str),
        evidence_count: row.get(9)?,
        direct_observation: direct_observation != 0,
        derived_from_constraint_ids,
        related_subject_tokens,
        valid_until: row.get::<Option<i64>>(13)?.map(|v| v as u64),
        created_at: row.get::<i64>(14)? as u64,
    })
}
