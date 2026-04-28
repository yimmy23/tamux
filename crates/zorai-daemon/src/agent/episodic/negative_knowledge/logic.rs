use super::*;

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

pub(crate) fn normalized_subject_key(subject: &str) -> String {
    normalize_subject_tokens(subject).join(" ")
}

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

pub(crate) fn related_for_propagation(
    source: &NegativeConstraint,
    target: &NegativeConstraint,
) -> bool {
    let shared_tokens = shared_normalized_subject_token_count(source, target);

    match (&source.solution_class, &target.solution_class) {
        (Some(source_class), Some(target_class)) => {
            source_class == target_class && shared_tokens >= 2
        }
        (None, None) => shared_tokens >= 3,
        _ => false,
    }
}

pub(crate) fn build_direct_constraint_from_episode(
    episode: &Episode,
    now_ms: u64,
    valid_until: u64,
    id: String,
) -> NegativeConstraint {
    let subject = if episode.summary.len() > 200 {
        format!("{}...", &episode.summary[..197])
    } else {
        episode.summary.clone()
    };
    let confidence = episode.confidence.unwrap_or(0.7);

    NegativeConstraint {
        id,
        episode_id: Some(episode.id.clone()),
        constraint_type: ConstraintType::RuledOut,
        related_subject_tokens: normalize_subject_tokens(&subject),
        subject,
        solution_class: episode.solution_class.clone(),
        description: episode.root_cause.clone().unwrap_or_default(),
        confidence,
        state: next_constraint_state(ConstraintState::Dying, 1, true, confidence),
        evidence_count: 1,
        direct_observation: true,
        derived_from_constraint_ids: Vec::new(),
        valid_until: Some(valid_until),
        created_at: now_ms,
    }
}

fn effective_related_subject_tokens(constraint: &NegativeConstraint) -> Vec<String> {
    if constraint.related_subject_tokens.is_empty() {
        normalize_subject_tokens(&constraint.subject)
    } else {
        constraint.related_subject_tokens.clone()
    }
}

pub(crate) fn merge_constraint_evidence(
    existing: &NegativeConstraint,
    incoming: &NegativeConstraint,
) -> NegativeConstraint {
    let mut related_subject_tokens = effective_related_subject_tokens(existing);
    related_subject_tokens.extend(effective_related_subject_tokens(incoming));
    related_subject_tokens.sort();
    related_subject_tokens.dedup();

    let direct_observation = existing.direct_observation || incoming.direct_observation;
    let confidence = existing.confidence.max(incoming.confidence);
    let evidence_count = existing.evidence_count.saturating_add(1);

    NegativeConstraint {
        id: existing.id.clone(),
        episode_id: incoming
            .episode_id
            .clone()
            .or_else(|| existing.episode_id.clone()),
        constraint_type: existing.constraint_type,
        subject: existing.subject.clone(),
        solution_class: existing.solution_class.clone(),
        description: incoming.description.clone(),
        confidence,
        state: next_constraint_state(
            existing.state,
            evidence_count,
            direct_observation,
            confidence,
        ),
        evidence_count,
        direct_observation,
        derived_from_constraint_ids: existing.derived_from_constraint_ids.clone(),
        related_subject_tokens,
        valid_until: incoming.valid_until.or(existing.valid_until),
        created_at: existing.created_at,
    }
}

pub(crate) fn propagate_dead_constraint(
    source: &NegativeConstraint,
    constraints: &[NegativeConstraint],
) -> Vec<NegativeConstraint> {
    if source.state != ConstraintState::Dead {
        return Vec::new();
    }

    let mut candidates: Vec<NegativeConstraint> = constraints
        .iter()
        .filter(|target| target.id != source.id)
        .filter(|target| target.state != ConstraintState::Dead)
        .filter(|target| related_for_propagation(source, target))
        .cloned()
        .collect();

    candidates.sort_by(|a, b| {
        constraint_state_rank(a.state)
            .cmp(&constraint_state_rank(b.state))
            .then_with(|| b.created_at.cmp(&a.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });

    candidates
        .into_iter()
        .take(10)
        .map(|mut target| {
            if !target
                .derived_from_constraint_ids
                .iter()
                .any(|existing_id| existing_id == &source.id)
            {
                target.derived_from_constraint_ids.push(source.id.clone());
            }

            if target.state == ConstraintState::Suspicious {
                target.state = ConstraintState::Dying;
            }

            if !target.direct_observation {
                target.direct_observation = false;
            }

            target
        })
        .collect()
}

pub(crate) fn is_constraint_active(constraint: &NegativeConstraint, now_ms: u64) -> bool {
    match constraint.valid_until {
        None => true,
        Some(expiry) => expiry > now_ms,
    }
}

pub(crate) fn constraint_state_rank(state: ConstraintState) -> u8 {
    match state {
        ConstraintState::Dead => 3,
        ConstraintState::Dying => 2,
        ConstraintState::Suspicious => 1,
    }
}
