use super::*;

pub(super) fn task_status_to_str(value: TaskStatus) -> &'static str {
    match value {
        TaskStatus::Queued => "queued",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::AwaitingApproval => "awaiting_approval",
        TaskStatus::Blocked => "blocked",
        TaskStatus::FailedAnalyzing => "failed_analyzing",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Cancelled => "cancelled",
    }
}

pub(super) fn parse_task_status(value: &str) -> TaskStatus {
    match value {
        "in_progress" => TaskStatus::InProgress,
        "awaiting_approval" => TaskStatus::AwaitingApproval,
        "blocked" => TaskStatus::Blocked,
        "failed_analyzing" => TaskStatus::FailedAnalyzing,
        "completed" => TaskStatus::Completed,
        "failed" => TaskStatus::Failed,
        "cancelled" => TaskStatus::Cancelled,
        _ => TaskStatus::Queued,
    }
}

pub(super) fn task_priority_to_str(value: TaskPriority) -> &'static str {
    match value {
        TaskPriority::Low => "low",
        TaskPriority::Normal => "normal",
        TaskPriority::High => "high",
        TaskPriority::Urgent => "urgent",
    }
}

pub(super) fn parse_task_priority(value: &str) -> TaskPriority {
    match value {
        "low" => TaskPriority::Low,
        "high" => TaskPriority::High,
        "urgent" => TaskPriority::Urgent,
        _ => TaskPriority::Normal,
    }
}

pub(super) fn task_log_level_to_str(value: TaskLogLevel) -> &'static str {
    match value {
        TaskLogLevel::Info => "info",
        TaskLogLevel::Warn => "warn",
        TaskLogLevel::Error => "error",
    }
}

pub(super) fn parse_task_log_level(value: &str) -> TaskLogLevel {
    match value {
        "warn" => TaskLogLevel::Warn,
        "error" => TaskLogLevel::Error,
        _ => TaskLogLevel::Info,
    }
}

pub(super) fn context_overflow_action_to_str(
    value: crate::agent::types::ContextOverflowAction,
) -> &'static str {
    match value {
        crate::agent::types::ContextOverflowAction::Compress => "compress",
        crate::agent::types::ContextOverflowAction::Truncate => "truncate",
        crate::agent::types::ContextOverflowAction::Error => "error",
    }
}

pub(super) fn parse_context_overflow_action(
    value: &str,
) -> crate::agent::types::ContextOverflowAction {
    match value {
        "truncate" => crate::agent::types::ContextOverflowAction::Truncate,
        "error" => crate::agent::types::ContextOverflowAction::Error,
        _ => crate::agent::types::ContextOverflowAction::Compress,
    }
}

pub(super) fn intervention_level_to_str(
    value: crate::agent::types::InterventionLevel,
) -> &'static str {
    match value {
        crate::agent::types::InterventionLevel::Passive => "passive",
        crate::agent::types::InterventionLevel::Normal => "normal",
        crate::agent::types::InterventionLevel::Aggressive => "aggressive",
    }
}

pub(super) fn parse_intervention_level(value: &str) -> crate::agent::types::InterventionLevel {
    match value {
        "passive" => crate::agent::types::InterventionLevel::Passive,
        "aggressive" => crate::agent::types::InterventionLevel::Aggressive,
        _ => crate::agent::types::InterventionLevel::Normal,
    }
}

pub(super) fn goal_run_status_to_str(value: GoalRunStatus) -> &'static str {
    match value {
        GoalRunStatus::Queued => "queued",
        GoalRunStatus::Planning => "planning",
        GoalRunStatus::Running => "running",
        GoalRunStatus::AwaitingApproval => "awaiting_approval",
        GoalRunStatus::Paused => "paused",
        GoalRunStatus::Completed => "completed",
        GoalRunStatus::Failed => "failed",
        GoalRunStatus::Cancelled => "cancelled",
    }
}

pub(super) fn parse_goal_run_status(value: &str) -> GoalRunStatus {
    match value {
        "planning" => GoalRunStatus::Planning,
        "running" => GoalRunStatus::Running,
        "awaiting_approval" => GoalRunStatus::AwaitingApproval,
        "paused" => GoalRunStatus::Paused,
        "completed" => GoalRunStatus::Completed,
        "failed" => GoalRunStatus::Failed,
        "cancelled" => GoalRunStatus::Cancelled,
        _ => GoalRunStatus::Queued,
    }
}

pub(super) fn goal_run_step_kind_to_str(value: &GoalRunStepKind) -> String {
    match value {
        GoalRunStepKind::Reason => "reason".to_string(),
        GoalRunStepKind::Command => "command".to_string(),
        GoalRunStepKind::Research => "research".to_string(),
        GoalRunStepKind::Memory => "memory".to_string(),
        GoalRunStepKind::Skill => "skill".to_string(),
        GoalRunStepKind::Specialist(role) => format!("specialist:{role}"),
        GoalRunStepKind::Divergent => "divergent".to_string(),
        GoalRunStepKind::Unknown => "reason".to_string(),
    }
}

pub(super) fn parse_goal_run_step_kind(value: &str) -> GoalRunStepKind {
    if let Some(role) = value.strip_prefix("specialist:") {
        return GoalRunStepKind::Specialist(role.to_string());
    }
    match value {
        "reason" => GoalRunStepKind::Reason,
        "command" => GoalRunStepKind::Command,
        "memory" => GoalRunStepKind::Memory,
        "skill" => GoalRunStepKind::Skill,
        "divergent" => GoalRunStepKind::Divergent,
        _ => GoalRunStepKind::Research,
    }
}

pub(super) fn goal_run_step_status_to_str(value: GoalRunStepStatus) -> &'static str {
    match value {
        GoalRunStepStatus::Pending => "pending",
        GoalRunStepStatus::InProgress => "in_progress",
        GoalRunStepStatus::Completed => "completed",
        GoalRunStepStatus::Failed => "failed",
        GoalRunStepStatus::Skipped => "skipped",
    }
}

pub(super) fn parse_goal_run_step_status(value: &str) -> GoalRunStepStatus {
    match value {
        "in_progress" => GoalRunStepStatus::InProgress,
        "completed" => GoalRunStepStatus::Completed,
        "failed" => GoalRunStepStatus::Failed,
        "skipped" => GoalRunStepStatus::Skipped,
        _ => GoalRunStepStatus::Pending,
    }
}

pub(super) fn autonomy_level_to_str(value: crate::agent::AutonomyLevel) -> &'static str {
    match value {
        crate::agent::AutonomyLevel::Autonomous => "autonomous",
        crate::agent::AutonomyLevel::Aware => "aware",
        crate::agent::AutonomyLevel::Supervised => "supervised",
    }
}

pub(super) fn parse_autonomy_level(value: &str) -> crate::agent::AutonomyLevel {
    crate::agent::AutonomyLevel::from_str_or_default(value)
}

pub(super) fn authorship_tag_to_str(value: crate::agent::AuthorshipTag) -> &'static str {
    match value {
        crate::agent::AuthorshipTag::Operator => "operator",
        crate::agent::AuthorshipTag::Agent => "agent",
        crate::agent::AuthorshipTag::Joint => "joint",
    }
}

pub(super) fn parse_authorship_tag(value: &str) -> crate::agent::AuthorshipTag {
    match value {
        "operator" => crate::agent::AuthorshipTag::Operator,
        "agent" => crate::agent::AuthorshipTag::Agent,
        _ => crate::agent::AuthorshipTag::Joint,
    }
}
