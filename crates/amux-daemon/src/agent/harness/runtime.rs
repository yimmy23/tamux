#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::governance::{evaluate_governance, RiskClass, VerdictClass};
use crate::history::{now_ts, EventLogRow, HistoryStore, ProvenanceEventRecord};

use super::types::*;

fn envelope<T: Serialize>(
    id: &str,
    thread_id: Option<&str>,
    goal_run_id: Option<&str>,
    task_id: Option<&str>,
    kind: HarnessRecordKind,
    status: Option<String>,
    summary: String,
    record: &T,
    created_at_ms: u64,
) -> Result<HarnessRecordEnvelope> {
    Ok(HarnessRecordEnvelope {
        entry_id: format!("hrec_{}", Uuid::new_v4()),
        entity_id: id.to_string(),
        thread_id: thread_id.map(str::to_string),
        goal_run_id: goal_run_id.map(str::to_string),
        task_id: task_id.map(str::to_string),
        kind,
        status,
        summary,
        payload: serde_json::to_value(record)?,
        created_at_ms,
    })
}

fn sort_by_created_at<T, F>(items: HashMap<String, T>, created_at: F) -> Vec<T>
where
    F: Fn(&T) -> u64,
{
    let mut values: Vec<T> = items.into_values().collect();
    values.sort_by_key(created_at);
    values
}

fn next_created_at(counter: &mut u64) -> u64 {
    let current = *counter;
    *counter = counter.saturating_add(1);
    current
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_else(|_| now_ts().saturating_mul(1000))
}

fn latest_projection_created_at(projection: &HarnessStateProjection) -> u64 {
    projection
        .observations
        .iter()
        .map(|record| record.created_at_ms)
        .chain(projection.beliefs.iter().map(|record| record.created_at_ms))
        .chain(projection.goals.iter().map(|record| record.created_at_ms))
        .chain(
            projection
                .world_states
                .iter()
                .map(|record| record.created_at_ms),
        )
        .chain(
            projection
                .tensions
                .iter()
                .map(|record| record.created_at_ms),
        )
        .chain(
            projection
                .commitments
                .iter()
                .map(|record| record.created_at_ms),
        )
        .chain(projection.effects.iter().map(|record| record.created_at_ms))
        .chain(
            projection
                .verifications
                .iter()
                .map(|record| record.created_at_ms),
        )
        .chain(
            projection
                .procedures
                .iter()
                .map(|record| record.created_at_ms),
        )
        .chain(
            projection
                .effect_contracts
                .iter()
                .map(|record| record.created_at_ms),
        )
        .max()
        .unwrap_or(0)
}

fn extract_string_list(details: &Value, key: &str) -> Vec<String> {
    details
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn normalized_signature_component(input: &str) -> String {
    let mut normalized = String::with_capacity(input.len());
    let mut last_was_separator = false;

    for ch in input.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '_'
        };

        if mapped == '_' {
            if !last_was_separator {
                normalized.push(mapped);
            }
            last_was_separator = true;
        } else {
            normalized.push(mapped);
            last_was_separator = false;
        }
    }

    normalized.trim_matches('_').to_string()
}

fn tension_kind_label(kind: &TensionKind) -> &'static str {
    match kind {
        TensionKind::Contradiction => "contradiction",
        TensionKind::InformationGap => "information_gap",
        TensionKind::RiskEscalation => "risk_escalation",
        TensionKind::StaleCommitment => "stale_commitment",
        TensionKind::Drift => "drift",
        TensionKind::Opportunity => "opportunity",
    }
}

fn procedure_status_label(status: &ProcedureStatus) -> &'static str {
    match status {
        ProcedureStatus::Candidate => "candidate",
        ProcedureStatus::Learned => "learned",
    }
}

fn ordered_unique_strings(items: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut ordered = Vec::new();

    for item in items {
        if seen.insert(item.clone()) {
            ordered.push(item);
        }
    }

    ordered
}

fn build_trace_signature(
    input: &HarnessLoopInput,
    execution_kind: &EffectExecutionKind,
    selected_tensions: &[TensionRecord],
) -> String {
    let mut tension_kinds = selected_tensions
        .iter()
        .map(|tension| tension_kind_label(&tension.kind).to_string())
        .collect::<Vec<_>>();
    tension_kinds.sort();
    tension_kinds.dedup();

    let goal_component = normalized_signature_component(&input.goal_summary);
    let goal_component = if goal_component.is_empty() {
        "goal".to_string()
    } else {
        goal_component
    };

    format!(
        "goal:{goal}|effect:{effect}|tensions:{tensions}|network:{network}|desired:{desired}",
        goal = goal_component,
        effect = execution_kind_label(execution_kind),
        tensions = if tension_kinds.is_empty() {
            "none".to_string()
        } else {
            tension_kinds.join("+")
        },
        network = input.allow_network,
        desired = input.desired_state.is_some(),
    )
}

fn procedure_confidence(successes: u32, failures: u32) -> f64 {
    let total = successes.saturating_add(failures);
    if total == 0 {
        return 0.0;
    }

    let success_ratio = successes as f64 / total as f64;
    let repetition_bonus = (successes.min(3) as f64 / 3.0) * 0.15;
    (success_ratio * 0.85 + repetition_bonus).min(1.0)
}

fn update_preferred_effect_order(
    existing: &[String],
    current: &str,
    verified: bool,
) -> Vec<String> {
    let mut order = if existing.is_empty() {
        vec!["read_only".to_string(), "mutating".to_string()]
    } else {
        existing.to_vec()
    };

    for default in ["read_only", "mutating"] {
        if !order.iter().any(|value| value == default) {
            order.push(default.to_string());
        }
    }
    order.retain(|value| value != current);
    if verified {
        order.insert(0, current.to_string());
    } else {
        order.push(current.to_string());
    }
    ordered_unique_strings(order)
}

fn latest_records<T>(items: &[T], limit: usize) -> Vec<&T> {
    items.iter().rev().take(limit).collect::<Vec<_>>()
}

fn compare_json_subset(desired: &Value, actual: &Value) -> bool {
    match (desired, actual) {
        (Value::Object(desired_map), Value::Object(actual_map)) => {
            desired_map.iter().all(|(key, desired_value)| {
                actual_map
                    .get(key)
                    .map(|actual_value| compare_json_subset(desired_value, actual_value))
                    .unwrap_or(false)
            })
        }
        (Value::Array(desired_items), Value::Array(actual_items)) => {
            desired_items.len() <= actual_items.len()
                && desired_items.iter().zip(actual_items.iter()).all(
                    |(desired_item, actual_item)| compare_json_subset(desired_item, actual_item),
                )
        }
        _ => desired == actual,
    }
}

fn commitment_status_label(status: &CommitmentStatus) -> &'static str {
    match status {
        CommitmentStatus::Proposed => "proposed",
        CommitmentStatus::Active => "active",
        CommitmentStatus::Blocked => "blocked",
        CommitmentStatus::Completed => "completed",
    }
}

fn effect_status_label(status: &EffectStatus) -> &'static str {
    match status {
        EffectStatus::Planned => "planned",
        EffectStatus::Dispatched => "dispatched",
        EffectStatus::Succeeded => "succeeded",
        EffectStatus::Blocked => "blocked",
    }
}

fn verification_status_label(status: &VerificationStatus) -> &'static str {
    match status {
        VerificationStatus::Pending => "pending",
        VerificationStatus::Verified => "verified",
        VerificationStatus::Failed => "failed",
    }
}

fn execution_kind_label(kind: &EffectExecutionKind) -> &'static str {
    match kind {
        EffectExecutionKind::ReadOnly => "read_only",
        EffectExecutionKind::Mutating => "mutating",
    }
}

fn risk_label(risk_class: &RiskClass) -> &'static str {
    match risk_class {
        RiskClass::Low => "low",
        RiskClass::Medium => "medium",
        RiskClass::High => "high",
        RiskClass::Critical => "critical",
    }
}

fn world_state_has_drift(world_state: &WorldStateRecord) -> bool {
    world_state
        .desired_state
        .as_ref()
        .map(|desired| !compare_json_subset(desired, &world_state.observed_state))
        .unwrap_or(false)
}

fn determine_execution_kind(
    input: &HarnessLoopInput,
    selected_tensions: &[TensionRecord],
) -> EffectExecutionKind {
    input.preferred_effect_kind.clone().unwrap_or_else(|| {
        if selected_tensions.iter().any(|tension| {
            matches!(
                tension.kind,
                TensionKind::RiskEscalation
                    | TensionKind::InformationGap
                    | TensionKind::Contradiction
            )
        }) {
            EffectExecutionKind::ReadOnly
        } else {
            EffectExecutionKind::ReadOnly
        }
    })
}

fn priority_for_risk_flag(flag: &str) -> u8 {
    if flag.contains("approval") || flag.contains("high_blast_radius") {
        95
    } else if flag.contains("privilege") {
        92
    } else {
        85
    }
}

fn build_world_state(
    projection: &HarnessStateProjection,
    input: &HarnessLoopInput,
    id: String,
    created_at_ms: u64,
) -> WorldStateRecord {
    let mut risk_flags = extract_string_list(&input.observation_details, "risk_flags");
    if input.allow_network && !risk_flags.iter().any(|flag| flag.contains("network")) {
        risk_flags.push("network_surface".to_string());
    }
    if matches!(
        input.preferred_effect_kind,
        Some(EffectExecutionKind::Mutating)
    ) && !risk_flags.iter().any(|flag| flag.contains("mutating"))
    {
        risk_flags.push("mutating_effect_requested".to_string());
    }

    let contradictions = extract_string_list(&input.observation_details, "contradictions");
    let unknowns = extract_string_list(&input.observation_details, "unknowns");
    let opportunities = {
        let extracted = extract_string_list(&input.observation_details, "opportunities");
        if extracted.is_empty() {
            vec![format!(
                "Advance goal with a bounded governed step: {}",
                input.goal_summary
            )]
        } else {
            extracted
        }
    };

    let stale_commitment_ids: Vec<String> = projection
        .commitments
        .iter()
        .filter(|commitment| {
            matches!(
                &commitment.status,
                CommitmentStatus::Proposed | CommitmentStatus::Active
            )
        })
        .map(|commitment| commitment.id.clone())
        .collect();

    let observed_state = input
        .observation_details
        .get("observed_state")
        .or_else(|| input.observation_details.get("state"))
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "observation_summary": input.observation_summary,
                "goal_summary": input.goal_summary,
                "active_commitment_count": stale_commitment_ids.len(),
                "existing_tension_count": projection.tensions.len(),
            })
        });

    WorldStateRecord {
        id,
        thread_id: input.thread_id.clone(),
        goal_run_id: input.goal_run_id.clone(),
        task_id: input.task_id.clone(),
        summary: format!("World state snapshot for {}", input.goal_summary),
        focus: input.goal_summary.clone(),
        observed_state,
        desired_state: input.desired_state.clone(),
        contradictions,
        unknowns,
        risk_flags,
        opportunities,
        stale_commitment_ids,
        active_tension_ids: Vec::new(),
        next_step_hint: None,
        created_at_ms,
    }
}

fn build_tensions(
    world_state: &WorldStateRecord,
    source_belief_id: &str,
    created_at: &mut u64,
) -> Vec<TensionRecord> {
    let mut tensions = Vec::new();

    for flag in &world_state.risk_flags {
        tensions.push(TensionRecord {
            id: format!("tension_{}", Uuid::new_v4()),
            thread_id: world_state.thread_id.clone(),
            goal_run_id: world_state.goal_run_id.clone(),
            task_id: world_state.task_id.clone(),
            kind: TensionKind::RiskEscalation,
            summary: format!("Risk escalation requires governance attention: {flag}"),
            priority: priority_for_risk_flag(flag),
            source_belief_id: Some(source_belief_id.to_string()),
            details: json!({
                "world_state_id": world_state.id,
                "risk_flag": flag,
            }),
            created_at_ms: next_created_at(created_at),
        });
    }

    for contradiction in &world_state.contradictions {
        tensions.push(TensionRecord {
            id: format!("tension_{}", Uuid::new_v4()),
            thread_id: world_state.thread_id.clone(),
            goal_run_id: world_state.goal_run_id.clone(),
            task_id: world_state.task_id.clone(),
            kind: TensionKind::Contradiction,
            summary: format!("World model contradiction detected: {contradiction}"),
            priority: 90,
            source_belief_id: Some(source_belief_id.to_string()),
            details: json!({
                "world_state_id": world_state.id,
                "contradiction": contradiction,
            }),
            created_at_ms: next_created_at(created_at),
        });
    }

    if world_state_has_drift(world_state) {
        tensions.push(TensionRecord {
            id: format!("tension_{}", Uuid::new_v4()),
            thread_id: world_state.thread_id.clone(),
            goal_run_id: world_state.goal_run_id.clone(),
            task_id: world_state.task_id.clone(),
            kind: TensionKind::Drift,
            summary: "Observed state has drifted from desired state".to_string(),
            priority: 75,
            source_belief_id: Some(source_belief_id.to_string()),
            details: json!({
                "world_state_id": world_state.id,
                "desired_state": world_state.desired_state,
                "observed_state": world_state.observed_state,
            }),
            created_at_ms: next_created_at(created_at),
        });
    }

    for unknown in &world_state.unknowns {
        tensions.push(TensionRecord {
            id: format!("tension_{}", Uuid::new_v4()),
            thread_id: world_state.thread_id.clone(),
            goal_run_id: world_state.goal_run_id.clone(),
            task_id: world_state.task_id.clone(),
            kind: TensionKind::InformationGap,
            summary: format!("Unknown information blocks next step: {unknown}"),
            priority: 80,
            source_belief_id: Some(source_belief_id.to_string()),
            details: json!({
                "world_state_id": world_state.id,
                "unknown": unknown,
            }),
            created_at_ms: next_created_at(created_at),
        });
    }

    for stale_commitment_id in &world_state.stale_commitment_ids {
        tensions.push(TensionRecord {
            id: format!("tension_{}", Uuid::new_v4()),
            thread_id: world_state.thread_id.clone(),
            goal_run_id: world_state.goal_run_id.clone(),
            task_id: world_state.task_id.clone(),
            kind: TensionKind::StaleCommitment,
            summary: format!("Prior commitment remains unresolved: {stale_commitment_id}"),
            priority: 70,
            source_belief_id: Some(source_belief_id.to_string()),
            details: json!({
                "world_state_id": world_state.id,
                "stale_commitment_id": stale_commitment_id,
            }),
            created_at_ms: next_created_at(created_at),
        });
    }

    for opportunity in &world_state.opportunities {
        tensions.push(TensionRecord {
            id: format!("tension_{}", Uuid::new_v4()),
            thread_id: world_state.thread_id.clone(),
            goal_run_id: world_state.goal_run_id.clone(),
            task_id: world_state.task_id.clone(),
            kind: TensionKind::Opportunity,
            summary: opportunity.clone(),
            priority: 50,
            source_belief_id: Some(source_belief_id.to_string()),
            details: json!({
                "world_state_id": world_state.id,
                "opportunity": opportunity,
            }),
            created_at_ms: next_created_at(created_at),
        });
    }

    if tensions.is_empty() {
        tensions.push(TensionRecord {
            id: format!("tension_{}", Uuid::new_v4()),
            thread_id: world_state.thread_id.clone(),
            goal_run_id: world_state.goal_run_id.clone(),
            task_id: world_state.task_id.clone(),
            kind: TensionKind::Opportunity,
            summary: format!(
                "A governed next step is available for {}",
                world_state.focus
            ),
            priority: 40,
            source_belief_id: Some(source_belief_id.to_string()),
            details: json!({
                "world_state_id": world_state.id,
            }),
            created_at_ms: next_created_at(created_at),
        });
    }

    tensions
}

fn select_tensions(tensions: &[TensionRecord]) -> Vec<TensionRecord> {
    let mut sorted = tensions.to_vec();
    sorted.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.created_at_ms.cmp(&right.created_at_ms))
    });
    let Some(top_priority) = sorted.first().map(|tension| tension.priority) else {
        return Vec::new();
    };
    sorted
        .into_iter()
        .take_while(|tension| tension.priority == top_priority)
        .collect()
}

fn build_verification_gates(input: &HarnessLoopInput) -> Vec<VerificationGateRecord> {
    let mut gates = vec![
        VerificationGateRecord {
            name: "governance_verdict".to_string(),
            kind: VerificationGateKind::GovernanceVerdict,
            description: "governance must allow or constrain the transition without blocking it"
                .to_string(),
            required: true,
            evidence_key: Some("governance_verdict".to_string()),
        },
        VerificationGateRecord {
            name: "effect_persisted".to_string(),
            kind: VerificationGateKind::EffectPersisted,
            description: "effect record must be durably persisted".to_string(),
            required: true,
            evidence_key: Some("effect_id".to_string()),
        },
        VerificationGateRecord {
            name: "effect_output".to_string(),
            kind: VerificationGateKind::EffectOutput,
            description: "effect output must contain the declared harness evidence".to_string(),
            required: true,
            evidence_key: Some("effect_output".to_string()),
        },
    ];

    if input.desired_state.is_some() {
        gates.push(VerificationGateRecord {
            name: "desired_state_match".to_string(),
            kind: VerificationGateKind::DesiredStateMatch,
            description: "actual post-effect state must satisfy the declared desired state subset"
                .to_string(),
            required: true,
            evidence_key: Some("desired_state".to_string()),
        });
    }

    gates
}

fn build_role_assessments(
    world_state: &WorldStateRecord,
    selected_tensions: &[TensionRecord],
    execution_kind: &EffectExecutionKind,
    verification_plan: &str,
) -> Vec<RoleAssessment> {
    let selected_summaries: Vec<String> = selected_tensions
        .iter()
        .map(|tension| format!("{} (priority {})", tension.summary, tension.priority))
        .collect();
    let selected_summary = if selected_summaries.is_empty() {
        "no active tensions".to_string()
    } else {
        selected_summaries.join("; ")
    };
    let skeptical_concerns = if matches!(execution_kind, EffectExecutionKind::Mutating) {
        vec![
            "mutating execution must remain blocked unless governance explicitly allows it"
                .to_string(),
        ]
    } else if !world_state.unknowns.is_empty() {
        vec!["remaining unknowns limit confidence in any broad corrective action".to_string()]
    } else {
        vec!["keep the first implementation bounded and auditable".to_string()]
    };

    vec![
        RoleAssessment {
            role: CommitmentRole::InterpreterCartographer,
            stance: "map_state".to_string(),
            summary: format!(
                "World state focus is '{}' with tensions: {}",
                world_state.focus, selected_summary
            ),
            concerns: world_state.unknowns.clone(),
            recommended_action:
                "preserve the world-state snapshot and carry selected tensions forward".to_string(),
            confidence: 0.78,
        },
        RoleAssessment {
            role: CommitmentRole::DeliberatorStrategist,
            stance: "prioritize_tensions".to_string(),
            summary: format!("Select the highest-priority tension set: {selected_summary}"),
            concerns: world_state.contradictions.clone(),
            recommended_action:
                "commit to the smallest governed step that addresses the top tension".to_string(),
            confidence: 0.75,
        },
        RoleAssessment {
            role: CommitmentRole::Executor,
            stance: format!("{}_execution", execution_kind_label(execution_kind)),
            summary: format!(
                "Execution should stay {} for the first effect contract",
                execution_kind_label(execution_kind)
            ),
            concerns: vec![format!(
                "risk flags in play: {}",
                if world_state.risk_flags.is_empty() {
                    "none".to_string()
                } else {
                    world_state.risk_flags.join(", ")
                }
            )],
            recommended_action: "dispatch only through the declared effect contract".to_string(),
            confidence: 0.73,
        },
        RoleAssessment {
            role: CommitmentRole::VerifierAuditor,
            stance: "gate_verification".to_string(),
            summary: format!("Verification plan: {verification_plan}"),
            concerns: vec!["tool success is not sufficient without gate results".to_string()],
            recommended_action: "persist verification evidence for every required gate".to_string(),
            confidence: 0.82,
        },
        RoleAssessment {
            role: CommitmentRole::SkepticCritic,
            stance: "challenge_scope".to_string(),
            summary: if matches!(execution_kind, EffectExecutionKind::Mutating) {
                "A mutating commitment must justify itself against governance and reversibility"
                    .to_string()
            } else {
                "A read-only commitment is acceptable only if it sharpens the next governed move"
                    .to_string()
            },
            concerns: skeptical_concerns,
            recommended_action: if matches!(execution_kind, EffectExecutionKind::Mutating) {
                "proceed only when governance verdict passes and verification gates remain explicit"
                    .to_string()
            } else {
                "proceed with bounded inspection and preserve evidence for follow-up selection"
                    .to_string()
            },
            confidence: 0.8,
        },
    ]
}

fn build_actual_state_snapshot(
    _input: &HarnessLoopInput,
    world_state: &WorldStateRecord,
    effect: &EffectRecord,
    execution_kind: &EffectExecutionKind,
    selected_tensions: &[TensionRecord],
) -> Value {
    let resulting_state = world_state.observed_state.clone();

    if let Value::Object(mut actual_map) = resulting_state {
        actual_map.insert(
            "effect_mode".to_string(),
            Value::String(execution_kind_label(execution_kind).to_string()),
        );
        actual_map.insert(
            "dispatch_success".to_string(),
            Value::Bool(effect.dispatch_success),
        );
        actual_map.insert(
            "governance_verdict".to_string(),
            serde_json::to_value(effect.governance_verdict.verdict_class.clone())
                .unwrap_or(Value::Null),
        );
        actual_map.insert(
            "world_state_id".to_string(),
            Value::String(world_state.id.clone()),
        );
        actual_map.insert(
            "selected_tension_ids".to_string(),
            Value::Array(
                selected_tensions
                    .iter()
                    .map(|tension| Value::String(tension.id.clone()))
                    .collect(),
            ),
        );
        actual_map.insert(
            "selected_tension_kinds".to_string(),
            Value::Array(
                selected_tensions
                    .iter()
                    .map(|tension| {
                        serde_json::to_value(tension.kind.clone()).unwrap_or(Value::Null)
                    })
                    .collect(),
            ),
        );
        Value::Object(actual_map)
    } else {
        json!({
            "effect_mode": execution_kind_label(execution_kind),
            "dispatch_success": effect.dispatch_success,
            "governance_verdict": effect.governance_verdict.verdict_class.clone(),
            "world_state_id": world_state.id.clone(),
            "observed_state": resulting_state,
            "selected_tension_ids": selected_tensions.iter().map(|tension| tension.id.clone()).collect::<Vec<_>>(),
            "selected_tension_kinds": selected_tensions.iter().map(|tension| tension.kind.clone()).collect::<Vec<_>>(),
        })
    }
}

fn evaluate_gate(
    gate: &VerificationGateRecord,
    input: &HarnessLoopInput,
    world_state: &WorldStateRecord,
    effect: &EffectRecord,
    execution_kind: &EffectExecutionKind,
    selected_tensions: &[TensionRecord],
) -> VerificationGateResult {
    match gate.kind {
        VerificationGateKind::GovernanceVerdict => {
            let passed = matches!(
                effect.governance_verdict.verdict_class,
                VerdictClass::Allow | VerdictClass::AllowWithConstraints
            );
            VerificationGateResult {
                gate_name: gate.name.clone(),
                passed,
                details: json!({
                    "verdict_class": effect.governance_verdict.verdict_class,
                    "constraints": effect.governance_verdict.constraints,
                    "rationale": effect.governance_verdict.rationale,
                }),
            }
        }
        VerificationGateKind::EffectPersisted => VerificationGateResult {
            gate_name: gate.name.clone(),
            passed: !effect.id.is_empty(),
            details: json!({
                "effect_id": effect.id,
                "status": effect.status,
            }),
        },
        VerificationGateKind::DesiredStateMatch => {
            let actual_state = build_actual_state_snapshot(
                input,
                world_state,
                effect,
                execution_kind,
                selected_tensions,
            );
            let passed = input
                .desired_state
                .as_ref()
                .map(|desired| compare_json_subset(desired, &actual_state))
                .unwrap_or(true);
            VerificationGateResult {
                gate_name: gate.name.clone(),
                passed,
                details: json!({
                    "desired_state": input.desired_state,
                    "actual_state": actual_state,
                }),
            }
        }
        VerificationGateKind::EffectOutput => {
            let selected_ids = effect
                .output
                .get("selected_tension_ids")
                .and_then(Value::as_array)
                .map(|items| !items.is_empty())
                .unwrap_or(false);
            let passed = effect
                .output
                .get("effect_mode")
                .and_then(Value::as_str)
                .is_some()
                && selected_ids;
            VerificationGateResult {
                gate_name: gate.name.clone(),
                passed,
                details: json!({
                    "effect_output": effect.output,
                }),
            }
        }
    }
}

pub(crate) fn project_harness_state(
    records: &[HarnessRecordEnvelope],
) -> Result<HarnessStateProjection> {
    let mut observations = HashMap::new();
    let mut beliefs = HashMap::new();
    let mut goals = HashMap::new();
    let mut world_states = HashMap::new();
    let mut tensions = HashMap::new();
    let mut commitments = HashMap::new();
    let mut effects = HashMap::new();
    let mut verifications = HashMap::new();
    let mut procedures = HashMap::new();
    let mut effect_contracts = HashMap::new();

    for record in records {
        match record.kind {
            HarnessRecordKind::Observation => {
                let value: ObservationRecord = serde_json::from_value(record.payload.clone())?;
                observations.insert(value.id.clone(), value);
            }
            HarnessRecordKind::Belief => {
                let value: BeliefRecord = serde_json::from_value(record.payload.clone())?;
                beliefs.insert(value.id.clone(), value);
            }
            HarnessRecordKind::Goal => {
                let value: GoalRecord = serde_json::from_value(record.payload.clone())?;
                goals.insert(value.id.clone(), value);
            }
            HarnessRecordKind::WorldState => {
                let value: WorldStateRecord = serde_json::from_value(record.payload.clone())?;
                world_states.insert(value.id.clone(), value);
            }
            HarnessRecordKind::Tension => {
                let value: TensionRecord = serde_json::from_value(record.payload.clone())?;
                tensions.insert(value.id.clone(), value);
            }
            HarnessRecordKind::Commitment => {
                let value: CommitmentRecord = serde_json::from_value(record.payload.clone())?;
                commitments.insert(value.id.clone(), value);
            }
            HarnessRecordKind::Effect => {
                let value: EffectRecord = serde_json::from_value(record.payload.clone())?;
                effects.insert(value.id.clone(), value);
            }
            HarnessRecordKind::Verification => {
                let value: VerificationRecord = serde_json::from_value(record.payload.clone())?;
                verifications.insert(value.id.clone(), value);
            }
            HarnessRecordKind::Procedure => {
                let value: ProcedureRecord = serde_json::from_value(record.payload.clone())?;
                procedures.insert(value.id.clone(), value);
            }
            HarnessRecordKind::EffectContract => {
                let value: EffectContractRecord = serde_json::from_value(record.payload.clone())?;
                effect_contracts.insert(value.id.clone(), value);
            }
        }
    }

    Ok(HarnessStateProjection {
        observations: sort_by_created_at(observations, |value| value.created_at_ms),
        beliefs: sort_by_created_at(beliefs, |value| value.created_at_ms),
        goals: sort_by_created_at(goals, |value| value.created_at_ms),
        world_states: sort_by_created_at(world_states, |value| value.created_at_ms),
        tensions: sort_by_created_at(tensions, |value| value.created_at_ms),
        commitments: sort_by_created_at(commitments, |value| value.created_at_ms),
        effects: sort_by_created_at(effects, |value| value.created_at_ms),
        verifications: sort_by_created_at(verifications, |value| value.created_at_ms),
        procedures: sort_by_created_at(procedures, |value| value.created_at_ms),
        effect_contracts: sort_by_created_at(effect_contracts, |value| value.created_at_ms),
    })
}

pub(crate) async fn load_harness_state_projection(
    store: &HistoryStore,
    thread_id: Option<&str>,
    goal_run_id: Option<&str>,
    task_id: Option<&str>,
) -> Result<HarnessStateProjection> {
    let records = store
        .list_harness_state_records(thread_id, goal_run_id, task_id)
        .await?;
    project_harness_state(&records)
}

fn summarize_belief_record(record: &BeliefRecord) -> Value {
    json!({
        "id": record.id,
        "kind": record.kind,
        "summary": record.summary,
        "confidence": record.confidence,
        "source_observation_id": record.source_observation_id,
        "created_at_ms": record.created_at_ms,
    })
}

fn summarize_world_state_record(record: &WorldStateRecord) -> Value {
    json!({
        "id": record.id,
        "summary": record.summary,
        "focus": record.focus,
        "observed_state": record.observed_state,
        "desired_state": record.desired_state,
        "contradictions": record.contradictions,
        "unknowns": record.unknowns,
        "risk_flags": record.risk_flags,
        "opportunities": record.opportunities,
        "active_tension_ids": record.active_tension_ids,
        "next_step_hint": record.next_step_hint,
        "created_at_ms": record.created_at_ms,
    })
}

fn summarize_tension_record(record: &TensionRecord) -> Value {
    json!({
        "id": record.id,
        "kind": record.kind,
        "summary": record.summary,
        "priority": record.priority,
        "source_belief_id": record.source_belief_id,
        "details": record.details,
        "created_at_ms": record.created_at_ms,
    })
}

fn summarize_commitment_record(record: &CommitmentRecord) -> Value {
    json!({
        "id": record.id,
        "summary": record.summary,
        "status": record.status,
        "rationale": record.rationale,
        "source_tension_id": record.source_tension_id,
        "source_world_state_id": record.source_world_state_id,
        "effect_contract_id": record.effect_contract_id,
        "expected_effects": record.expected_effects,
        "verification_plan": record.verification_plan,
        "critique_summary": record.critique_summary,
        "role_assessments": record.role_assessments,
        "created_at_ms": record.created_at_ms,
    })
}

fn summarize_effect_record(record: &EffectRecord) -> Value {
    json!({
        "id": record.id,
        "summary": record.summary,
        "status": record.status,
        "dispatch_success": record.dispatch_success,
        "commitment_id": record.commitment_id,
        "effect_contract_id": record.effect_contract_id,
        "governance_verdict": record.governance_verdict.verdict_class,
        "output": record.output,
        "created_at_ms": record.created_at_ms,
    })
}

fn summarize_verification_record(record: &VerificationRecord) -> Value {
    json!({
        "id": record.id,
        "summary": record.summary,
        "status": record.status,
        "verified": record.verified,
        "effect_id": record.effect_id,
        "gate_results": record.details.get("gate_results").cloned().unwrap_or(Value::Null),
        "created_at_ms": record.created_at_ms,
    })
}

fn summarize_procedure_record(record: &ProcedureRecord) -> Value {
    json!({
        "id": record.id,
        "summary": record.summary,
        "status": record.status,
        "trace_signature": record.trace_signature,
        "applicability": record.applicability,
        "outcome_summary": record.outcome_summary,
        "verified_outcome": record.verified_outcome,
        "successful_trace_count": record.successful_trace_count,
        "failed_trace_count": record.failed_trace_count,
        "confidence": record.confidence,
        "preferred_effect_order": record.preferred_effect_order,
        "source_verification_id": record.source_verification_id,
        "details": record.details,
        "created_at_ms": record.created_at_ms,
    })
}

fn summarize_effect_contract_record(record: &EffectContractRecord) -> Value {
    json!({
        "id": record.id,
        "summary": record.summary,
        "execution_kind": record.execution_kind,
        "reversible": record.reversible,
        "risk_hint": record.risk_hint,
        "blast_radius_hint": record.blast_radius_hint,
        "preconditions": record.preconditions,
        "expected_effects": record.expected_effects,
        "verification_strategy": record.verification_strategy,
        "verification_gates": record.verification_gates,
        "created_at_ms": record.created_at_ms,
    })
}

pub(crate) fn build_harness_state_payload(
    projection: &HarnessStateProjection,
    thread_id: Option<&str>,
    goal_run_id: Option<&str>,
    task_id: Option<&str>,
    limit: usize,
) -> Value {
    let limit = limit.max(1);
    let active_tension_ids = projection
        .world_states
        .last()
        .map(|state| state.active_tension_ids.clone())
        .unwrap_or_default();

    let active_tensions = projection
        .tensions
        .iter()
        .filter(|tension| active_tension_ids.iter().any(|id| id == &tension.id))
        .map(summarize_tension_record)
        .collect::<Vec<_>>();
    let recent_tensions = latest_records(&projection.tensions, limit)
        .into_iter()
        .map(summarize_tension_record)
        .collect::<Vec<_>>();
    let recent_beliefs = latest_records(&projection.beliefs, limit)
        .into_iter()
        .map(summarize_belief_record)
        .collect::<Vec<_>>();
    let recent_commitments = latest_records(&projection.commitments, limit)
        .into_iter()
        .map(summarize_commitment_record)
        .collect::<Vec<_>>();
    let recent_effects = latest_records(&projection.effects, limit)
        .into_iter()
        .map(summarize_effect_record)
        .collect::<Vec<_>>();
    let recent_verifications = latest_records(&projection.verifications, limit)
        .into_iter()
        .map(summarize_verification_record)
        .collect::<Vec<_>>();
    let recent_procedures = latest_records(&projection.procedures, limit)
        .into_iter()
        .map(summarize_procedure_record)
        .collect::<Vec<_>>();
    let learned_procedures = projection
        .procedures
        .iter()
        .filter(|procedure| procedure.status == ProcedureStatus::Learned)
        .rev()
        .take(limit)
        .map(summarize_procedure_record)
        .collect::<Vec<_>>();

    json!({
        "scope": {
            "thread_id": thread_id,
            "goal_run_id": goal_run_id,
            "task_id": task_id,
        },
        "counts": {
            "observations": projection.observations.len(),
            "beliefs": projection.beliefs.len(),
            "goals": projection.goals.len(),
            "world_states": projection.world_states.len(),
            "tensions": projection.tensions.len(),
            "commitments": projection.commitments.len(),
            "effects": projection.effects.len(),
            "verifications": projection.verifications.len(),
            "procedures": projection.procedures.len(),
            "learned_procedures": projection
                .procedures
                .iter()
                .filter(|procedure| procedure.status == ProcedureStatus::Learned)
                .count(),
            "verified_effects": projection
                .verifications
                .iter()
                .filter(|verification| verification.verified)
                .count(),
        },
        "world_state": {
            "latest": projection
                .world_states
                .last()
                .map(summarize_world_state_record)
                .unwrap_or(Value::Null),
        },
        "beliefs": {
            "recent": recent_beliefs,
        },
        "tensions": {
            "active": active_tensions,
            "recent": recent_tensions,
        },
        "commitments": {
            "latest": projection
                .commitments
                .last()
                .map(summarize_commitment_record)
                .unwrap_or(Value::Null),
            "recent": recent_commitments,
        },
        "effects": {
            "latest": projection
                .effects
                .last()
                .map(summarize_effect_record)
                .unwrap_or(Value::Null),
            "recent": recent_effects,
        },
        "verifications": {
            "latest": projection
                .verifications
                .last()
                .map(summarize_verification_record)
                .unwrap_or(Value::Null),
            "recent": recent_verifications,
        },
        "procedures": {
            "latest": projection
                .procedures
                .last()
                .map(summarize_procedure_record)
                .unwrap_or(Value::Null),
            "recent": recent_procedures,
            "learned": learned_procedures,
        },
        "effect_contracts": {
            "latest": projection
                .effect_contracts
                .last()
                .map(summarize_effect_contract_record)
                .unwrap_or(Value::Null),
        },
    })
}

pub(crate) async fn append_record(
    store: &HistoryStore,
    record: &HarnessRecordEnvelope,
) -> Result<()> {
    store.append_harness_state_record(record).await
}

pub(crate) async fn run_minimal_closed_loop(
    store: &HistoryStore,
    input: HarnessLoopInput,
) -> Result<HarnessLoopResult> {
    let thread_id = input.thread_id.as_deref();
    let goal_run_id = input.goal_run_id.as_deref();
    let task_id = input.task_id.as_deref();

    let mut projection =
        load_harness_state_projection(store, thread_id, goal_run_id, task_id).await?;
    let mut timeline =
        current_time_ms().max(latest_projection_created_at(&projection).saturating_add(1));

    let observation = ObservationRecord {
        id: format!("obs_{}", Uuid::new_v4()),
        thread_id: input.thread_id.clone(),
        goal_run_id: input.goal_run_id.clone(),
        task_id: input.task_id.clone(),
        kind: ObservationKind::OperatorRequest,
        summary: input.observation_summary.clone(),
        details: input.observation_details.clone(),
        created_at_ms: next_created_at(&mut timeline),
    };
    append_record(
        store,
        &envelope(
            &observation.id,
            thread_id,
            goal_run_id,
            task_id,
            HarnessRecordKind::Observation,
            None,
            observation.summary.clone(),
            &observation,
            observation.created_at_ms,
        )?,
    )
    .await?;

    let goal = projection
        .goals
        .iter()
        .find(|goal| goal.summary == input.goal_summary)
        .cloned()
        .unwrap_or(GoalRecord {
            id: format!("goal_{}", Uuid::new_v4()),
            thread_id: input.thread_id.clone(),
            goal_run_id: input.goal_run_id.clone(),
            task_id: input.task_id.clone(),
            summary: input.goal_summary.clone(),
            details: json!({"source": "minimal_closed_loop"}),
            created_at_ms: next_created_at(&mut timeline),
        });
    if !projection
        .goals
        .iter()
        .any(|existing| existing.id == goal.id)
    {
        append_record(
            store,
            &envelope(
                &goal.id,
                thread_id,
                goal_run_id,
                task_id,
                HarnessRecordKind::Goal,
                None,
                goal.summary.clone(),
                &goal,
                goal.created_at_ms,
            )?,
        )
        .await?;
    }

    let belief = BeliefRecord {
        id: format!("belief_{}", Uuid::new_v4()),
        thread_id: input.thread_id.clone(),
        goal_run_id: input.goal_run_id.clone(),
        task_id: input.task_id.clone(),
        kind: BeliefKind::Working,
        summary: format!(
            "Latest observation implies bounded follow-up is needed: {}",
            input.observation_summary
        ),
        confidence: 0.65,
        source_observation_id: Some(observation.id.clone()),
        details: json!({
            "goal_summary": input.goal_summary,
            "observation_id": observation.id,
        }),
        created_at_ms: next_created_at(&mut timeline),
    };
    append_record(
        store,
        &envelope(
            &belief.id,
            thread_id,
            goal_run_id,
            task_id,
            HarnessRecordKind::Belief,
            None,
            belief.summary.clone(),
            &belief,
            belief.created_at_ms,
        )?,
    )
    .await?;

    let world_state_id = format!("world_{}", Uuid::new_v4());
    let mut world_state = build_world_state(
        &projection,
        &input,
        world_state_id.clone(),
        next_created_at(&mut timeline),
    );

    let world_model_belief = BeliefRecord {
        id: format!("belief_{}", Uuid::new_v4()),
        thread_id: input.thread_id.clone(),
        goal_run_id: input.goal_run_id.clone(),
        task_id: input.task_id.clone(),
        kind: BeliefKind::WorldModel,
        summary: format!(
            "World model captured {} contradictions, {} unknowns, and {} risk flags",
            world_state.contradictions.len(),
            world_state.unknowns.len(),
            world_state.risk_flags.len()
        ),
        confidence: 0.72,
        source_observation_id: Some(observation.id.clone()),
        details: json!({
            "world_state_id": world_state.id,
            "observed_state": world_state.observed_state,
            "desired_state": world_state.desired_state,
        }),
        created_at_ms: next_created_at(&mut timeline),
    };
    append_record(
        store,
        &envelope(
            &world_model_belief.id,
            thread_id,
            goal_run_id,
            task_id,
            HarnessRecordKind::Belief,
            None,
            world_model_belief.summary.clone(),
            &world_model_belief,
            world_model_belief.created_at_ms,
        )?,
    )
    .await?;

    let tensions = build_tensions(&world_state, &world_model_belief.id, &mut timeline);
    let selected_tensions = select_tensions(&tensions);
    let selected_tension_ids: Vec<String> = selected_tensions
        .iter()
        .map(|tension| tension.id.clone())
        .collect();
    world_state.active_tension_ids = selected_tension_ids.clone();
    world_state.next_step_hint = selected_tensions
        .first()
        .map(|tension| tension.summary.clone());

    append_record(
        store,
        &envelope(
            &world_state.id,
            thread_id,
            goal_run_id,
            task_id,
            HarnessRecordKind::WorldState,
            None,
            world_state.summary.clone(),
            &world_state,
            world_state.created_at_ms,
        )?,
    )
    .await?;

    for tension in &tensions {
        append_record(
            store,
            &envelope(
                &tension.id,
                thread_id,
                goal_run_id,
                task_id,
                HarnessRecordKind::Tension,
                None,
                tension.summary.clone(),
                tension,
                tension.created_at_ms,
            )?,
        )
        .await?;
    }

    let execution_kind = determine_execution_kind(&input, &selected_tensions);
    let verification_gates = build_verification_gates(&input);
    let verification_plan = format!(
        "Evaluate gates: {}",
        verification_gates
            .iter()
            .map(|gate| gate.name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    );

    let effect_contract = EffectContractRecord {
        id: format!("contract_{}", Uuid::new_v4()),
        thread_id: input.thread_id.clone(),
        goal_run_id: input.goal_run_id.clone(),
        task_id: input.task_id.clone(),
        summary: format!(
            "governed {} harness effect for {}",
            execution_kind_label(&execution_kind),
            input.goal_summary
        ),
        execution_kind: execution_kind.clone(),
        reversible: execution_kind == EffectExecutionKind::ReadOnly,
        risk_hint: if world_state.risk_flags.is_empty() {
            format!(
                "{}-risk bounded harness effect",
                execution_kind_label(&execution_kind)
            )
        } else {
            format!(
                "{} effect under risk flags: {}",
                execution_kind_label(&execution_kind),
                world_state.risk_flags.join(", ")
            )
        },
        blast_radius_hint: if input.allow_network {
            "goal-local plus external surface".to_string()
        } else {
            "goal-local only".to_string()
        },
        preconditions: vec![
            "fresh projected state available".to_string(),
            "world-state snapshot persisted".to_string(),
            "selected tensions identified".to_string(),
        ],
        expected_effects: vec![
            "effect record persisted".to_string(),
            "verification record persisted".to_string(),
            "selected tensions carried into commitment state".to_string(),
        ],
        verification_strategy: format!(
            "run named verification gates after governed {} dispatch",
            execution_kind_label(&execution_kind)
        ),
        verification_gates: verification_gates.clone(),
        governance_input: governance_input_for_harness_effect(
            thread_id,
            goal_run_id,
            task_id,
            "persist governed harness effect",
            execution_kind.clone(),
            input.allow_network,
            input.sandbox_enabled,
            &world_state.risk_flags,
        ),
        created_at_ms: next_created_at(&mut timeline),
    };
    append_record(
        store,
        &envelope(
            &effect_contract.id,
            thread_id,
            goal_run_id,
            task_id,
            HarnessRecordKind::EffectContract,
            None,
            effect_contract.summary.clone(),
            &effect_contract,
            effect_contract.created_at_ms,
        )?,
    )
    .await?;

    let role_assessments = build_role_assessments(
        &world_state,
        &selected_tensions,
        &execution_kind,
        &verification_plan,
    );
    let critique_summary = role_assessments
        .iter()
        .find(|assessment| assessment.role == CommitmentRole::SkepticCritic)
        .map(|assessment| assessment.summary.clone())
        .unwrap_or_else(|| "No skeptic critique recorded".to_string());

    let selected_commitment = CommitmentRecord {
        id: format!("commitment_{}", Uuid::new_v4()),
        thread_id: input.thread_id.clone(),
        goal_run_id: input.goal_run_id.clone(),
        task_id: input.task_id.clone(),
        summary: format!(
            "Address top harness tension(s) for {} via {} effect",
            input.goal_summary,
            execution_kind_label(&execution_kind)
        ),
        rationale: selected_tensions
            .iter()
            .map(|tension| tension.summary.clone())
            .collect::<Vec<_>>()
            .join(" | "),
        status: CommitmentStatus::Active,
        source_tension_id: selected_tensions.first().map(|tension| tension.id.clone()),
        source_world_state_id: Some(world_state.id.clone()),
        effect_contract_id: Some(effect_contract.id.clone()),
        expected_effects: effect_contract.expected_effects.clone(),
        verification_plan: verification_plan.clone(),
        critique_summary: Some(critique_summary.clone()),
        role_assessments: role_assessments.clone(),
        details: json!({
            "selection": "highest_priority_tension",
            "selected_tension_ids": selected_tension_ids,
            "world_state_id": world_state.id,
        }),
        created_at_ms: next_created_at(&mut timeline),
    };
    append_record(
        store,
        &envelope(
            &selected_commitment.id,
            thread_id,
            goal_run_id,
            task_id,
            HarnessRecordKind::Commitment,
            Some(commitment_status_label(&selected_commitment.status).to_string()),
            selected_commitment.summary.clone(),
            &selected_commitment,
            selected_commitment.created_at_ms,
        )?,
    )
    .await?;

    let governance_verdict = evaluate_governance(&effect_contract.governance_input);
    let effect_allowed = matches!(
        governance_verdict.verdict_class,
        VerdictClass::Allow | VerdictClass::AllowWithConstraints
    );
    let effect = EffectRecord {
        id: format!("effect_{}", Uuid::new_v4()),
        thread_id: input.thread_id.clone(),
        goal_run_id: input.goal_run_id.clone(),
        task_id: input.task_id.clone(),
        commitment_id: selected_commitment.id.clone(),
        effect_contract_id: Some(effect_contract.id.clone()),
        summary: format!(
            "{} harness effect dispatch",
            execution_kind_label(&execution_kind)
        ),
        status: if effect_allowed {
            EffectStatus::Succeeded
        } else {
            EffectStatus::Blocked
        },
        dispatch_success: effect_allowed,
        output: json!({
            "effect_mode": execution_kind_label(&execution_kind),
            "selected_tension_ids": selected_tensions.iter().map(|tension| tension.id.clone()).collect::<Vec<_>>(),
            "selected_tension_kinds": selected_tensions.iter().map(|tension| tension.kind.clone()).collect::<Vec<_>>(),
            "world_state_id": world_state.id,
            "contract_id": effect_contract.id,
            "governance_verdict": governance_verdict.verdict_class,
            "risk_hint": effect_contract.risk_hint,
        }),
        governance_verdict: governance_verdict.clone(),
        created_at_ms: next_created_at(&mut timeline),
    };
    append_record(
        store,
        &envelope(
            &effect.id,
            thread_id,
            goal_run_id,
            task_id,
            HarnessRecordKind::Effect,
            Some(effect_status_label(&effect.status).to_string()),
            effect.summary.clone(),
            &effect,
            effect.created_at_ms,
        )?,
    )
    .await?;

    let gate_results: Vec<VerificationGateResult> = effect_contract
        .verification_gates
        .iter()
        .map(|gate| {
            evaluate_gate(
                gate,
                &input,
                &world_state,
                &effect,
                &execution_kind,
                &selected_tensions,
            )
        })
        .collect();
    let verified = gate_results.iter().all(|gate| {
        let required = effect_contract
            .verification_gates
            .iter()
            .find(|expected| expected.name == gate.gate_name)
            .map(|expected| expected.required)
            .unwrap_or(false);
        gate.passed || !required
    });

    let verification = VerificationRecord {
        id: format!("verification_{}", Uuid::new_v4()),
        thread_id: input.thread_id.clone(),
        goal_run_id: input.goal_run_id.clone(),
        task_id: input.task_id.clone(),
        effect_id: effect.id.clone(),
        summary: if verified {
            "verification gates satisfied governed harness effect".to_string()
        } else {
            "verification gates detected unresolved governance or state mismatch".to_string()
        },
        status: if verified {
            VerificationStatus::Verified
        } else {
            VerificationStatus::Failed
        },
        verified,
        details: json!({
            "effect_dispatch_success": effect.dispatch_success,
            "checked_expected_effects": effect_contract.expected_effects,
            "verification_strategy": effect_contract.verification_strategy,
            "gate_results": gate_results,
            "selected_tension_ids": selected_tensions.iter().map(|tension| tension.id.clone()).collect::<Vec<_>>(),
            "desired_state": input.desired_state,
        }),
        created_at_ms: next_created_at(&mut timeline),
    };
    append_record(
        store,
        &envelope(
            &verification.id,
            thread_id,
            goal_run_id,
            task_id,
            HarnessRecordKind::Verification,
            Some(verification_status_label(&verification.status).to_string()),
            verification.summary.clone(),
            &verification,
            verification.created_at_ms,
        )?,
    )
    .await?;

    let completed_commitment = CommitmentRecord {
        status: if verification.verified {
            CommitmentStatus::Completed
        } else if !effect_allowed {
            CommitmentStatus::Blocked
        } else {
            CommitmentStatus::Active
        },
        created_at_ms: next_created_at(&mut timeline),
        ..selected_commitment.clone()
    };
    append_record(
        store,
        &envelope(
            &completed_commitment.id,
            thread_id,
            goal_run_id,
            task_id,
            HarnessRecordKind::Commitment,
            Some(commitment_status_label(&completed_commitment.status).to_string()),
            completed_commitment.summary.clone(),
            &completed_commitment,
            completed_commitment.created_at_ms,
        )?,
    )
    .await?;

    let trace_signature = build_trace_signature(&input, &execution_kind, &selected_tensions);
    let prior_matching_procedure = projection
        .procedures
        .iter()
        .rev()
        .find(|procedure| procedure.trace_signature == trace_signature);
    let prior_successes = prior_matching_procedure
        .map(|procedure| procedure.successful_trace_count)
        .unwrap_or(0);
    let prior_failures = prior_matching_procedure
        .map(|procedure| procedure.failed_trace_count)
        .unwrap_or(0);
    let successful_trace_count = prior_successes.saturating_add(u32::from(verification.verified));
    let failed_trace_count = prior_failures.saturating_add(u32::from(!verification.verified));
    let procedure_status = if successful_trace_count >= 2 {
        ProcedureStatus::Learned
    } else {
        ProcedureStatus::Candidate
    };
    let current_effect_mode = execution_kind_label(&execution_kind).to_string();
    let preferred_effect_order = update_preferred_effect_order(
        prior_matching_procedure
            .map(|procedure| procedure.preferred_effect_order.as_slice())
            .unwrap_or(&[]),
        &current_effect_mode,
        verification.verified,
    );
    let confidence = procedure_confidence(successful_trace_count, failed_trace_count);
    let selected_tension_kinds = ordered_unique_strings(
        selected_tensions
            .iter()
            .map(|tension| tension_kind_label(&tension.kind).to_string()),
    );
    let procedure = ProcedureRecord {
        id: format!("procedure_{}", Uuid::new_v4()),
        thread_id: input.thread_id.clone(),
        goal_run_id: input.goal_run_id.clone(),
        task_id: input.task_id.clone(),
        summary: if procedure_status == ProcedureStatus::Learned {
            "learned governed closed-loop procedure".to_string()
        } else {
            "governed closed-loop procedure candidate".to_string()
        },
        status: procedure_status.clone(),
        trace_signature: trace_signature.clone(),
        applicability: ordered_unique_strings(
            std::iter::once(format!("effect:{}", current_effect_mode))
                .chain(
                    selected_tension_kinds
                        .iter()
                        .map(|kind| format!("tension:{kind}")),
                )
                .chain(std::iter::once(format!(
                    "goal:{}",
                    normalized_signature_component(&input.goal_summary)
                )))
                .collect::<Vec<_>>(),
        ),
        outcome_summary: if verification.verified {
            format!(
                "world state, tensions, commitment critique, effect, and verification were persisted across {} successful trace(s)",
                successful_trace_count
            )
        } else {
            format!(
                "verification remained unresolved after governed effect evaluation; {} failure trace(s) recorded",
                failed_trace_count
            )
        },
        verified_outcome: verification.verified,
        successful_trace_count,
        failed_trace_count,
        confidence,
        preferred_effect_order: preferred_effect_order.clone(),
        source_verification_id: Some(verification.id.clone()),
        details: json!({
            "goal_summary": input.goal_summary,
            "observation_summary": input.observation_summary,
            "selected_tension_ids": selected_tensions.iter().map(|tension| tension.id.clone()).collect::<Vec<_>>(),
            "selected_tension_kinds": selected_tension_kinds,
            "world_state_id": world_state.id,
            "effect_contract_id": effect_contract.id,
            "effect_id": effect.id,
            "verification_id": verification.id,
            "latest_outcome_verified": verification.verified,
            "prior_successful_trace_count": prior_successes,
            "prior_failed_trace_count": prior_failures,
            "preferred_effect_order": preferred_effect_order,
            "critique_summary": completed_commitment.critique_summary,
        }),
        created_at_ms: next_created_at(&mut timeline),
    };
    append_record(
        store,
        &envelope(
            &procedure.id,
            thread_id,
            goal_run_id,
            task_id,
            HarnessRecordKind::Procedure,
            Some(procedure_status_label(&procedure.status).to_string()),
            procedure.summary.clone(),
            &procedure,
            procedure.created_at_ms,
        )?,
    )
    .await?;

    store
        .insert_event_log(&EventLogRow {
            id: format!("harness-event-{}", Uuid::new_v4()),
            event_family: "harness".to_string(),
            event_kind: "loop_completed".to_string(),
            state: Some(if verification.verified {
                "verified".to_string()
            } else if !effect_allowed {
                "blocked".to_string()
            } else {
                "verification_failed".to_string()
            }),
            thread_id: input.thread_id.clone(),
            payload_json: json!({
                "observation_id": observation.id,
                "belief_id": belief.id,
                "world_model_belief_id": world_model_belief.id,
                "world_state_id": world_state.id,
                "selected_tension_ids": selected_tensions.iter().map(|tension| tension.id.clone()).collect::<Vec<_>>(),
                "tension_ids": selected_tensions.iter().map(|tension| tension.id.clone()).collect::<Vec<_>>(),
                "selected_tensions": selected_tensions.iter().map(|tension| json!({
                    "id": tension.id,
                    "kind": tension.kind,
                    "priority": tension.priority,
                    "summary": tension.summary,
                })).collect::<Vec<_>>(),
                "commitment_id": completed_commitment.id,
                "effect_contract_id": effect_contract.id,
                "effect_id": effect.id,
                "verification_id": verification.id,
                "procedure_id": procedure.id,
                "procedure_status": procedure.status,
                "trace_signature": procedure.trace_signature,
                "critique_summary": completed_commitment.critique_summary,
            })
            .to_string(),
            risk_label: risk_label(&governance_verdict.risk_class).to_string(),
            handled_at_ms: next_created_at(&mut timeline),
        })
        .await?;

    let provenance_details = json!({
        "world_state_id": world_state.id,
        "selected_tension_ids": selected_tensions.iter().map(|tension| tension.id.clone()).collect::<Vec<_>>(),
        "commitment_id": completed_commitment.id,
        "effect_contract_id": effect_contract.id,
        "effect_id": effect.id,
        "verification_id": verification.id,
        "procedure_id": procedure.id,
        "procedure_status": procedure.status,
        "trace_signature": procedure.trace_signature,
        "procedure_confidence": procedure.confidence,
        "verified": verification.verified,
        "critique_summary": completed_commitment.critique_summary,
    });
    store
        .record_provenance_event(&ProvenanceEventRecord {
            event_type: "harness_loop_completed",
            summary: "harness loop persisted governed effect contracts, world state, tensions, and commitment critique",
            details: &provenance_details,
            agent_id: "svarog",
            goal_run_id,
            task_id,
            thread_id,
            approval_id: None,
            causal_trace_id: None,
            compliance_mode: "governed_transition_harness",
            sign: false,
            created_at: next_created_at(&mut timeline),
        })
        .await?;

    projection = load_harness_state_projection(store, thread_id, goal_run_id, task_id).await?;

    Ok(HarnessLoopResult {
        projection,
        world_state_id: world_state.id,
        selected_tension_ids,
        selected_commitment_id: completed_commitment.id,
        effect_contract_id: effect_contract.id,
        effect_id: effect.id,
        verification_id: verification.id,
        procedure_id: procedure.id,
    })
}
