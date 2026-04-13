pub(crate) mod advocate;
pub(crate) mod arbiter;
pub(crate) mod critic;
pub(crate) mod types;

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::agent::engine::AgentEngine;
use crate::agent::operator_model::RiskTolerance;

use self::types::{CritiqueSession, Decision, Resolution, SessionStatus};

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl AgentEngine {
    pub(crate) async fn run_critique_preflight(
        &self,
        action_id: &str,
        tool_name: &str,
        action_summary: &str,
        reasons: &[String],
        thread_id: Option<&str>,
        task_id: Option<&str>,
    ) -> Result<CritiqueSession> {
        let risk_tolerance = self
            .operator_model
            .read()
            .await
            .risk_fingerprint
            .risk_tolerance;
        let advocate_argument = advocate::build_argument(tool_name, action_summary, reasons);
        let critic_argument = critic::build_argument(tool_name, action_summary, reasons);
        let resolution = arbiter::resolve(&advocate_argument, &critic_argument, risk_tolerance);
        let created_at_ms = now_millis();
        let resolved_at_ms = Some(created_at_ms);
        let status = if matches!(resolution.decision, Decision::Defer) {
            SessionStatus::Deferred
        } else {
            SessionStatus::Resolved
        };
        let session = CritiqueSession {
            id: format!("critique_{}", Uuid::new_v4()),
            action_id: action_id.to_string(),
            tool_name: tool_name.to_string(),
            proposed_action_summary: action_summary.to_string(),
            thread_id: thread_id.map(str::to_string),
            task_id: task_id.map(str::to_string),
            advocate_id: "advocate".to_string(),
            critic_id: "critic".to_string(),
            arbiter_id: "arbiter".to_string(),
            status,
            advocate_argument,
            critic_argument,
            resolution: Some(resolution),
            created_at_ms,
            resolved_at_ms,
        };
        self.persist_critique_session(&session).await?;
        Ok(session)
    }

    pub(crate) async fn persist_critique_session(&self, session: &CritiqueSession) -> Result<()> {
        let session_json = serde_json::to_string(session)?;
        self.history
            .upsert_critique_session(&session.id, &session_json, session.created_at_ms)
            .await?;

        for point in &session.advocate_argument.points {
            self.history
                .insert_critique_argument(
                    &session.id,
                    "advocate",
                    &point.claim,
                    point.weight,
                    &serde_json::to_string(&point.evidence)?,
                    session.created_at_ms,
                )
                .await?;
        }
        for point in &session.critic_argument.points {
            self.history
                .insert_critique_argument(
                    &session.id,
                    "critic",
                    &point.claim,
                    point.weight,
                    &serde_json::to_string(&point.evidence)?,
                    session.created_at_ms,
                )
                .await?;
        }
        if let Some(resolution) = session.resolution.as_ref() {
            self.history
                .upsert_critique_resolution(
                    &session.id,
                    resolution.decision.as_str(),
                    &serde_json::to_string(resolution)?,
                    resolution.risk_score,
                    resolution.confidence,
                    session.resolved_at_ms.unwrap_or(session.created_at_ms),
                )
                .await?;
        }
        Ok(())
    }

    pub(crate) async fn get_persisted_critique_session(
        &self,
        session_id: &str,
    ) -> Result<Option<CritiqueSession>> {
        let row = self.history.get_critique_session(session_id).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_str(&row.session_json)?))
    }

    pub(crate) async fn get_critique_session_payload(
        &self,
        session_id: &str,
    ) -> Result<serde_json::Value> {
        let session = self
            .get_persisted_critique_session(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("unknown critique session: {session_id}"))?;
        Ok(json!({
            "session_id": session.id,
            "status": session.status,
            "action_id": session.action_id,
            "tool_name": session.tool_name,
            "proposed_action_summary": session.proposed_action_summary,
            "advocate_argument": session.advocate_argument,
            "critic_argument": session.critic_argument,
            "resolution": session.resolution,
            "created_at_ms": session.created_at_ms,
            "resolved_at_ms": session.resolved_at_ms,
        }))
    }

    pub(crate) async fn should_run_critique_preflight(
        &self,
        tool_name: &str,
        classification: &crate::agent::weles_governance::WelesToolClassification,
    ) -> bool {
        let cfg = self.config.read().await;
        if !cfg.critique.enabled {
            return false;
        }
        if matches!(
            cfg.critique.mode,
            crate::agent::types::CritiqueMode::Disabled
        ) {
            return false;
        }
        if cfg.critique.guard_suspicious_tool_calls_only
            && classification.reasons.is_empty()
            && !matches!(
                classification.class,
                crate::agent::weles_governance::WelesGovernanceClass::GuardAlways
            )
        {
            return false;
        }
        matches!(
            tool_name,
            "bash_command"
                | "execute_managed_command"
                | "run_terminal_command"
                | "write_file"
                | "create_file"
                | "append_to_file"
                | "replace_in_file"
                | "apply_file_patch"
                | "apply_patch"
                | "send_slack_message"
                | "send_discord_message"
                | "send_telegram_message"
                | "send_whatsapp_message"
                | "spawn_subagent"
                | "enqueue_task"
        )
    }

    pub(crate) fn critique_requires_blocking_review(
        &self,
        resolution: &Resolution,
        risk_tolerance: RiskTolerance,
    ) -> bool {
        match resolution.decision {
            Decision::Reject => true,
            Decision::Defer => true,
            Decision::ProceedWithModifications => {
                !matches!(risk_tolerance, RiskTolerance::Aggressive)
            }
            Decision::Proceed => false,
        }
    }
}
