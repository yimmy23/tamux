use std::collections::HashMap;

use super::metacognitive::self_assessment::Assessment;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PolicyTriggerInput {
    pub thread_id: String,
    pub goal_run_id: Option<String>,
    pub repeated_approach: bool,
    pub awareness_stuck: bool,
    pub should_pivot: bool,
    pub should_escalate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PolicySelfAssessmentSummary {
    pub should_pivot: bool,
    pub should_escalate: bool,
}

impl PolicySelfAssessmentSummary {
    pub(crate) fn is_actionable(&self) -> bool {
        self.should_pivot || self.should_escalate
    }
}

impl From<&Assessment> for PolicySelfAssessmentSummary {
    fn from(value: &Assessment) -> Self {
        Self {
            should_pivot: value.should_pivot,
            should_escalate: value.should_escalate,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PolicyTriggerContext {
    pub thread_id: String,
    pub goal_run_id: Option<String>,
    pub repeated_approach: bool,
    pub awareness_stuck: bool,
    pub self_assessment: PolicySelfAssessmentSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TriggerOutcome {
    NoIntervention,
    EvaluatePolicy(PolicyTriggerContext),
}

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
    inputs
        .iter()
        .filter_map(|input| match evaluate_triggers(input) {
            TriggerOutcome::NoIntervention => None,
            TriggerOutcome::EvaluatePolicy(context) => Some((context.thread_id.clone(), context)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_no_intervention_when_all_inputs_are_nominal() {
        let input = PolicyTriggerInput {
            thread_id: "thread-1".to_string(),
            goal_run_id: Some("goal-1".to_string()),
            repeated_approach: false,
            awareness_stuck: false,
            should_pivot: false,
            should_escalate: false,
        };

        assert_eq!(evaluate_triggers(&input), TriggerOutcome::NoIntervention);
    }

    #[test]
    fn trigger_intervention_required_for_repeated_approach_signal() {
        let input = PolicyTriggerInput {
            thread_id: "thread-1".to_string(),
            goal_run_id: Some("goal-1".to_string()),
            repeated_approach: true,
            awareness_stuck: false,
            should_pivot: false,
            should_escalate: false,
        };

        assert_eq!(
            evaluate_triggers(&input),
            TriggerOutcome::EvaluatePolicy(PolicyTriggerContext {
                thread_id: "thread-1".to_string(),
                goal_run_id: Some("goal-1".to_string()),
                repeated_approach: true,
                awareness_stuck: false,
                self_assessment: PolicySelfAssessmentSummary {
                    should_pivot: false,
                    should_escalate: false,
                },
            })
        );
    }

    #[test]
    fn trigger_intervention_required_for_awareness_stuckness() {
        let input = PolicyTriggerInput {
            thread_id: "thread-2".to_string(),
            goal_run_id: None,
            repeated_approach: false,
            awareness_stuck: true,
            should_pivot: false,
            should_escalate: false,
        };

        assert_eq!(
            evaluate_triggers(&input),
            TriggerOutcome::EvaluatePolicy(PolicyTriggerContext {
                thread_id: "thread-2".to_string(),
                goal_run_id: None,
                repeated_approach: false,
                awareness_stuck: true,
                self_assessment: PolicySelfAssessmentSummary {
                    should_pivot: false,
                    should_escalate: false,
                },
            })
        );
    }

    #[test]
    fn trigger_intervention_required_for_self_assessment_pivot_or_escalate() {
        let pivot_input = PolicyTriggerInput {
            thread_id: "thread-3".to_string(),
            goal_run_id: Some("goal-3".to_string()),
            repeated_approach: false,
            awareness_stuck: false,
            should_pivot: true,
            should_escalate: false,
        };
        let escalate_input = PolicyTriggerInput {
            thread_id: "thread-4".to_string(),
            goal_run_id: Some("goal-4".to_string()),
            repeated_approach: false,
            awareness_stuck: false,
            should_pivot: false,
            should_escalate: true,
        };

        assert_eq!(
            evaluate_triggers(&pivot_input),
            TriggerOutcome::EvaluatePolicy(PolicyTriggerContext {
                thread_id: "thread-3".to_string(),
                goal_run_id: Some("goal-3".to_string()),
                repeated_approach: false,
                awareness_stuck: false,
                self_assessment: PolicySelfAssessmentSummary {
                    should_pivot: true,
                    should_escalate: false,
                },
            })
        );
        assert_eq!(
            evaluate_triggers(&escalate_input),
            TriggerOutcome::EvaluatePolicy(PolicyTriggerContext {
                thread_id: "thread-4".to_string(),
                goal_run_id: Some("goal-4".to_string()),
                repeated_approach: false,
                awareness_stuck: false,
                self_assessment: PolicySelfAssessmentSummary {
                    should_pivot: false,
                    should_escalate: true,
                },
            })
        );
    }

    #[test]
    fn trigger_aggregation_is_keyed_by_thread_id() {
        let inputs = vec![
            PolicyTriggerInput {
                thread_id: "thread-1".to_string(),
                goal_run_id: Some("goal-1".to_string()),
                repeated_approach: true,
                awareness_stuck: false,
                should_pivot: false,
                should_escalate: false,
            },
            PolicyTriggerInput {
                thread_id: "thread-2".to_string(),
                goal_run_id: Some("goal-2".to_string()),
                repeated_approach: false,
                awareness_stuck: false,
                should_pivot: false,
                should_escalate: false,
            },
            PolicyTriggerInput {
                thread_id: "thread-3".to_string(),
                goal_run_id: None,
                repeated_approach: false,
                awareness_stuck: false,
                should_pivot: false,
                should_escalate: true,
            },
        ];

        let contexts = aggregate_trigger_contexts(&inputs);

        assert_eq!(contexts.len(), 2);
        assert_eq!(
            contexts.get("thread-1"),
            Some(&PolicyTriggerContext {
                thread_id: "thread-1".to_string(),
                goal_run_id: Some("goal-1".to_string()),
                repeated_approach: true,
                awareness_stuck: false,
                self_assessment: PolicySelfAssessmentSummary {
                    should_pivot: false,
                    should_escalate: false,
                },
            })
        );
        assert_eq!(
            contexts.get("thread-3"),
            Some(&PolicyTriggerContext {
                thread_id: "thread-3".to_string(),
                goal_run_id: None,
                repeated_approach: false,
                awareness_stuck: false,
                self_assessment: PolicySelfAssessmentSummary {
                    should_pivot: false,
                    should_escalate: true,
                },
            })
        );
        assert!(!contexts.contains_key("thread-2"));
    }
}
