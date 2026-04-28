use super::*;

pub(super) fn tier_disclosure_features(tier: CapabilityTier) -> Vec<FeatureDisclosure> {
    match tier {
        CapabilityTier::Newcomer => vec![],
        CapabilityTier::Familiar => vec![
            FeatureDisclosure {
                feature_id: "goal_runs".into(),
                tier: CapabilityTier::Familiar,
                title: "Goal Runs".into(),
                description:
                    "You can now set multi-step goals and I'll plan and execute them autonomously."
                        .into(),
            },
            FeatureDisclosure {
                feature_id: "task_queue".into(),
                tier: CapabilityTier::Familiar,
                title: "Task Queue".into(),
                description: "Schedule tasks to run later or in the background.".into(),
            },
            FeatureDisclosure {
                feature_id: "gateway_config".into(),
                tier: CapabilityTier::Familiar,
                title: "Chat Gateways".into(),
                description:
                    "Connect me to Slack, Discord, or Telegram so I can work where you communicate."
                        .into(),
            },
        ],
        CapabilityTier::PowerUser => vec![
            FeatureDisclosure {
                feature_id: "subagents".into(),
                tier: CapabilityTier::PowerUser,
                title: "Sub-Agents".into(),
                description:
                    "I can spawn specialized sub-agents for complex tasks - like having a team."
                        .into(),
            },
            FeatureDisclosure {
                feature_id: "advanced_settings".into(),
                tier: CapabilityTier::PowerUser,
                title: "Advanced Settings".into(),
                description:
                    "Fine-tune my behavior: model selection, tool policies, and execution preferences."
                        .into(),
            },
        ],
        CapabilityTier::Expert => vec![FeatureDisclosure {
            feature_id: "memory_controls".into(),
            tier: CapabilityTier::Expert,
            title: "Memory Controls".into(),
            description:
                "Full control over my memory: consolidation settings, decay rates, and manual fact management."
                    .into(),
        }],
    }
}
