//! Trusted execution provenance helpers built on the local WORM telemetry ledger.

use super::*;

impl AgentEngine {
    pub(super) fn compliance_mode_label(&self) -> String {
        let config = self.config.blocking_read();
        match config.compliance.mode {
            ComplianceMode::Standard => "standard",
            ComplianceMode::Soc2 => "soc2",
            ComplianceMode::Hipaa => "hipaa",
            ComplianceMode::Fedramp => "fedramp",
        }
        .to_string()
    }

    pub(crate) async fn record_provenance_event(
        &self,
        event_type: &str,
        summary: &str,
        details: serde_json::Value,
        goal_run_id: Option<&str>,
        task_id: Option<&str>,
        thread_id: Option<&str>,
        approval_id: Option<&str>,
        causal_trace_id: Option<&str>,
    ) {
        let config = self.config.read().await.clone();
        if !config.enabled {
            return;
        }
        if let Err(error) =
            self.history
                .record_provenance_event(&crate::history::ProvenanceEventRecord {
                    event_type,
                    summary,
                    details: &details,
                    agent_id: "tamux-daemon",
                    goal_run_id,
                    task_id,
                    thread_id,
                    approval_id,
                    causal_trace_id,
                    compliance_mode: match config.compliance.mode {
                        ComplianceMode::Standard => "standard",
                        ComplianceMode::Soc2 => "soc2",
                        ComplianceMode::Hipaa => "hipaa",
                        ComplianceMode::Fedramp => "fedramp",
                    },
                    sign: config.compliance.sign_all_events,
                    created_at: now_millis(),
                }).await
        {
            tracing::warn!(event_type, error = %error, "failed to record provenance event");
        }
    }

    pub async fn provenance_report_json(&self, limit: usize) -> Result<String> {
        Ok(serde_json::to_string_pretty(
            &self.history.provenance_report(limit)?,
        )?)
    }

    pub async fn generate_soc2_artifact(&self, period_days: u32) -> Result<String> {
        Ok(self
            .history
            .generate_soc2_artifact(period_days)?
            .display()
            .to_string())
    }
}
