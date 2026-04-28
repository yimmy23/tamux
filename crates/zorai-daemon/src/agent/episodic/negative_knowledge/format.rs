use super::*;

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
        let noun = if count == 1 {
            "constraint"
        } else {
            "constraints"
        };
        format!("Source: {source} from {count} related dead {noun}")
    }
}

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
        out.push_str(&format!(
            "  State: {}\n",
            constraint_state_str(constraint.state)
        ));
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
                let days_remaining = expiry.saturating_sub(now_ms) / (86400 * 1000);
                out.push_str(&format!("  Expires: in {days_remaining} days\n"));
            }
            None => out.push_str("  Expires: never\n"),
        }

        out.push('\n');
    }

    if active.len() > 10 {
        let remaining = active.len() - 10;
        out.push_str(&format!("({remaining} more constraints not shown)\n"));
    }

    out
}
