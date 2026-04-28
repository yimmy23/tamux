use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TriggerPattern {
    #[serde(default)]
    pub tool_sequence: Vec<String>,
    #[serde(default)]
    pub max_revisions: u32,
    #[serde(default)]
    pub context_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CognitiveBias {
    pub name: String,
    pub trigger_pattern: TriggerPattern,
    pub severity: f64,
    pub mitigation_prompt: String,
    #[serde(default)]
    pub occurrence_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowProfile {
    pub name: String,
    pub avg_success_rate: f64,
    pub avg_steps: u32,
    #[serde(default)]
    pub typical_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelfModel {
    pub agent_id: String,
    pub calibration_offset: f64,
    #[serde(default)]
    pub biases: Vec<CognitiveBias>,
    #[serde(default)]
    pub workflow_profiles: Vec<WorkflowProfile>,
    pub last_updated_ms: u64,
}

impl Default for SelfModel {
    fn default() -> Self {
        Self {
            agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
            calibration_offset: 0.0,
            biases: vec![
                CognitiveBias {
                    name: "sunk_cost".to_string(),
                    trigger_pattern: TriggerPattern {
                        tool_sequence: vec!["bash_command".to_string()],
                        max_revisions: 3,
                        context_tags: vec!["retry_loop".to_string()],
                    },
                    severity: 0.72,
                    mitigation_prompt:
                        "You may be persisting with a failing approach. Re-evaluate and switch strategy."
                            .to_string(),
                    occurrence_count: 0,
                },
                CognitiveBias {
                    name: "overconfidence".to_string(),
                    trigger_pattern: TriggerPattern {
                        tool_sequence: Vec::new(),
                        max_revisions: 0,
                        context_tags: vec!["high_confidence_low_accuracy".to_string()],
                    },
                    severity: 0.64,
                    mitigation_prompt:
                        "Your historical accuracy is lower than your current confidence suggests. Lower confidence and verify."
                            .to_string(),
                    occurrence_count: 0,
                },
                CognitiveBias {
                    name: "confirmation".to_string(),
                    trigger_pattern: TriggerPattern {
                        tool_sequence: vec![
                            "read_file".to_string(),
                            "search_files".to_string(),
                            "list_files".to_string(),
                        ],
                        max_revisions: 3,
                        context_tags: vec!["selective_validation".to_string()],
                    },
                    severity: 0.51,
                    mitigation_prompt:
                        "You may be gathering confirming evidence without testing a disconfirming path. Run a check that could prove you wrong."
                            .to_string(),
                    occurrence_count: 0,
                },
            ],
            workflow_profiles: vec![
                WorkflowProfile {
                    name: "debug_loop".to_string(),
                    avg_success_rate: 0.58,
                    avg_steps: 6,
                    typical_tools: vec![
                        "read_file".to_string(),
                        "search_files".to_string(),
                        "bash_command".to_string(),
                    ],
                },
                WorkflowProfile {
                    name: "refactor".to_string(),
                    avg_success_rate: 0.74,
                    avg_steps: 5,
                    typical_tools: vec![
                        "read_file".to_string(),
                        "replace_in_file".to_string(),
                        "apply_patch".to_string(),
                    ],
                },
            ],
            last_updated_ms: 0,
        }
    }
}
