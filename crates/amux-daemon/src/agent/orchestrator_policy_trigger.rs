#![allow(dead_code)]

use std::collections::HashMap;

use super::*;

pub(crate) fn evaluate_triggers(input: &PolicyTriggerInput) -> TriggerOutcome {
    let self_assessment = PolicySelfAssessmentSummary {
        should_pivot: input.should_pivot,
        should_escalate: input.should_escalate,
    };

    if !input.repeated_approach && !input.awareness_stuck && !self_assessment.is_actionable() {
        return TriggerOutcome::NoIntervention;
    }

    TriggerOutcome::EvaluatePolicy(PolicyTriggerContext {
        thread_id: input.thread_id.clone(),
        goal_run_id: input.goal_run_id.clone(),
        repeated_approach: input.repeated_approach,
        awareness_stuck: input.awareness_stuck,
        self_assessment,
    })
}

pub(crate) fn aggregate_trigger_contexts(
    inputs: &[PolicyTriggerInput],
) -> HashMap<String, PolicyTriggerContext> {
    let mut contexts = HashMap::new();

    for context in inputs
        .iter()
        .filter_map(|input| match evaluate_triggers(input) {
            TriggerOutcome::NoIntervention => None,
            TriggerOutcome::EvaluatePolicy(context) => Some(context),
        })
    {
        contexts
            .entry(context.thread_id.clone())
            .and_modify(|existing: &mut PolicyTriggerContext| {
                existing.goal_run_id = match (&existing.goal_run_id, &context.goal_run_id) {
                    (Some(existing_id), _) => Some(existing_id.clone()),
                    (None, Some(incoming_id)) => Some(incoming_id.clone()),
                    (None, None) => None,
                };
                existing.repeated_approach |= context.repeated_approach;
                existing.awareness_stuck |= context.awareness_stuck;
                existing.self_assessment.should_pivot |= context.self_assessment.should_pivot;
                existing.self_assessment.should_escalate |= context.self_assessment.should_escalate;
            })
            .or_insert(context);
    }

    contexts
}

pub(crate) fn trigger_requires_intervention(trigger: &PolicyTriggerContext) -> bool {
    trigger.repeated_approach
        || trigger.awareness_stuck
        || trigger.self_assessment.should_pivot
        || trigger.self_assessment.should_escalate
}

pub(crate) fn infer_stuck_reason(
    trigger: &PolicyTriggerContext,
) -> Option<crate::agent::types::StuckReason> {
    if trigger.repeated_approach {
        Some(crate::agent::types::StuckReason::ErrorLoop)
    } else if trigger.awareness_stuck {
        Some(crate::agent::types::StuckReason::NoProgress)
    } else {
        None
    }
}
