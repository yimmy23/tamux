use super::*;
use crate::agent::AgentEngine;

pub(super) fn build_recovery_investigation_description(
    thread_id: &str,
    signature: &str,
    failure_class: &str,
    summary: &str,
    diagnostics: &Value,
) -> String {
    format!(
        "Investigate a daemon-side upstream request failure.\n\nThread: {thread_id}\nSignature: {signature}\nFailure class: {failure_class}\nSummary: {summary}\nDiagnostics: {diagnostics}\n\nInspect thread state, request-body synthesis inputs, tool definitions, and transport selection. Do not switch provider or transport automatically. If you find a daemon-internal side-effect-free repair, explain it; otherwise report the likely root cause and operator-safe next step.",
    )
}

#[derive(Debug, PartialEq)]
pub enum WelcomeProfileDecision {
    StandardWelcome,
    EmitProfileQuestion {
        field_key: String,
        prompt: String,
        input_kind: String,
        optional: bool,
    },
}

pub fn profile_interview_decision(
    specs: &[crate::agent::operator_profile::ProfileFieldSpec],
    answered: &std::collections::HashMap<String, crate::agent::operator_profile::ProfileFieldValue>,
    session: &crate::agent::operator_profile::InterviewSession,
    now_ms: u64,
) -> WelcomeProfileDecision {
    if crate::agent::operator_profile::is_complete(specs, answered) {
        return WelcomeProfileDecision::StandardWelcome;
    }
    match crate::agent::operator_profile::next_question(specs, answered, session, now_ms) {
        Some(spec) => WelcomeProfileDecision::EmitProfileQuestion {
            field_key: spec.field_key.clone(),
            prompt: spec.prompt.clone(),
            input_kind: spec.input_kind.clone(),
            optional: !spec.required,
        },
        None => WelcomeProfileDecision::StandardWelcome,
    }
}

impl AgentEngine {
    pub(crate) fn announce_skill_draft(&self, skill_name: &str, description: &str) {
        self.emit_workflow_notice(
            CONCIERGE_THREAD_ID,
            "skill_discovery",
            format!(
                "I noticed a new pattern in your work -- drafted a skill: {}",
                skill_name
            ),
            Some(description.to_string()),
        );
    }

    pub(crate) fn announce_skill_promotion(
        &self,
        skill_name: &str,
        from_status: &str,
        to_status: &str,
        success_count: u32,
    ) {
        let cycle_id = uuid::Uuid::new_v4().to_string();
        let now = super::super::now_millis();
        let is_canonical = to_status == "promoted_to_canonical";

        let _ = self.event_tx.send(AgentEvent::HeartbeatDigest {
            cycle_id,
            actionable: true,
            digest: format!("Skill '{}' promoted to {}", skill_name, to_status),
            items: vec![HeartbeatDigestItem {
                priority: 2,
                check_type: HeartbeatCheckType::SkillLifecycle,
                title: format!("Skill promoted: {}", skill_name),
                suggestion: format!(
                    "Skill '{}' was promoted from {} to {} after {} successful uses.",
                    skill_name, from_status, to_status, success_count
                ),
            }],
            checked_at: now,
            explanation: Some(
                "This skill has been consistently helpful and earned a promotion.".to_string(),
            ),
            confidence: Some(0.9),
        });

        if is_canonical {
            self.emit_workflow_notice(
                CONCIERGE_THREAD_ID,
                "skill_discovery",
                format!(
                    "Skill '{}' has been promoted to canonical after {} successful uses!",
                    skill_name, success_count
                ),
                Some(format!(
                    "Promoted from {} to {} -- this skill is now part of your permanent toolkit.",
                    from_status, to_status
                )),
            );
        }
    }
}
