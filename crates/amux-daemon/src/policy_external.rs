#![allow(dead_code)]

//! External policy engine integration.
//! Supports Cerbos PDP for fine-grained ABAC policy evaluation.
//! Falls back to local regex-based policy when external engine is unavailable.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::governance::{
    effective_constraints, evaluate_governance, governance_input_for_managed_command,
    ConstraintKind, GovernanceInput, GovernanceVerdict, RiskClass, TransitionKind, VerdictClass,
};

/// Policy evaluation result.
#[derive(Debug, Clone)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub risk_level: String,
    pub reasons: Vec<String>,
    pub blast_radius: String,
}

/// Governance evaluation result.
#[derive(Debug, Clone)]
pub struct GovernanceDecision {
    pub verdict_class: String,
    pub risk_class: String,
    pub rationale: Vec<String>,
    pub constraints: Vec<String>,
    pub blast_radius: String,
    pub policy_fingerprint: String,
}

/// Trait for policy evaluation providers.
pub trait PolicyProvider: Send + Sync {
    fn evaluate(&self, request: &PolicyRequest) -> Result<PolicyDecision>;
    fn name(&self) -> &'static str;
}

/// Governance evaluation request.
pub type GovernanceRequest = PolicyRequest;

/// Trait for governance evaluation providers.
pub trait GovernanceProvider: Send + Sync {
    fn evaluate_governance(&self, request: &GovernanceRequest) -> Result<GovernanceDecision>;
    fn name(&self) -> &'static str;
}

/// Policy evaluation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRequest {
    pub command: String,
    pub workspace_id: Option<String>,
    pub source: String,
    pub allow_network: bool,
    pub cwd: Option<String>,
}

fn build_managed_request(request: &PolicyRequest) -> amux_protocol::ManagedCommandRequest {
    amux_protocol::ManagedCommandRequest {
        command: request.command.clone(),
        rationale: String::new(),
        allow_network: request.allow_network,
        sandbox_enabled: true,
        security_level: amux_protocol::SecurityLevel::Moderate,
        cwd: request.cwd.clone(),
        language_hint: None,
        source: amux_protocol::ManagedCommandSource::Agent,
    }
}

fn risk_class_str(risk_class: &RiskClass) -> &'static str {
    match risk_class {
        RiskClass::Low => "low",
        RiskClass::Medium => "medium",
        RiskClass::High => "high",
        RiskClass::Critical => "critical",
    }
}

fn verdict_class_str(verdict: &VerdictClass) -> &'static str {
    match verdict {
        VerdictClass::Allow => "allow",
        VerdictClass::AllowWithConstraints => "allow_with_constraints",
        VerdictClass::RequireApproval => "require_approval",
        VerdictClass::Defer => "defer",
        VerdictClass::Deny => "deny",
        VerdictClass::HaltAndIsolate => "halt_and_isolate",
        VerdictClass::AllowOnlyWithCompensationPlan => "allow_only_with_compensation_plan",
    }
}

fn transition_kind_str(kind: &TransitionKind) -> &'static str {
    match kind {
        TransitionKind::RunAdmission => "run_admission",
        TransitionKind::LaneAdmission => "lane_admission",
        TransitionKind::StageAdvance => "stage_advance",
        TransitionKind::LaneRetry => "lane_retry",
        TransitionKind::ResumeFromBlocked => "resume_from_blocked",
        TransitionKind::CompensationEntry => "compensation_entry",
        TransitionKind::FinalDisposition => "final_disposition",
        TransitionKind::ManagedCommandDispatch => "managed_command_dispatch",
        TransitionKind::ApprovalReuseCheck => "approval_reuse_check",
    }
}

fn constraint_kind_str(kind: &ConstraintKind) -> &'static str {
    match kind {
        ConstraintKind::SandboxRequired => "sandbox_required",
        ConstraintKind::NetworkDenied => "network_denied",
        ConstraintKind::NetworkRestricted => "network_restricted",
        ConstraintKind::FilesystemScopeNarrowed => "filesystem_scope_narrowed",
        ConstraintKind::TargetScopeCapped => "target_scope_capped",
        ConstraintKind::SerialOnlyExecution => "serial_only_execution",
        ConstraintKind::RetriesDisabled => "retries_disabled",
        ConstraintKind::RetriesRequireFreshCheckpoint => "retries_require_fresh_checkpoint",
        ConstraintKind::ArtifactRetentionElevated => "artifact_retention_elevated",
        ConstraintKind::ManualResumeRequiredAfterCompletion => {
            "manual_resume_required_after_completion"
        }
    }
}

fn governance_blast_radius(input: &GovernanceInput) -> String {
    format!(
        "{} (lane: {}, stage: {})",
        input.blast_radius.run_scope, input.blast_radius.lane_scope, input.blast_radius.stage_scope
    )
}

fn governance_decision_from_verdict(
    verdict: GovernanceVerdict,
    input: &GovernanceInput,
) -> GovernanceDecision {
    let constraints = effective_constraints(&verdict);
    GovernanceDecision {
        verdict_class: verdict_class_str(&verdict.verdict_class).to_string(),
        risk_class: risk_class_str(&verdict.risk_class).to_string(),
        rationale: if verdict.rationale.is_empty() {
            vec![format!(
                "governance evaluated {} as {}",
                transition_kind_str(&input.transition_kind),
                verdict_class_str(&verdict.verdict_class)
            )]
        } else {
            verdict.rationale.clone()
        },
        constraints: constraints
            .iter()
            .map(|constraint| constraint_kind_str(&constraint.kind).to_string())
            .collect(),
        blast_radius: governance_blast_radius(input),
        policy_fingerprint: verdict.policy_fingerprint,
    }
}

fn governance_verdict_severity(label: &str) -> u8 {
    match label {
        "allow" => 0,
        "allow_with_constraints" => 1,
        "require_approval" => 2,
        "defer" => 3,
        "allow_only_with_compensation_plan" => 4,
        "deny" => 5,
        "halt_and_isolate" => 6,
        _ => 255,
    }
}

/// Local regex-based policy provider (existing implementation wrapper).
pub struct LocalPolicyProvider;

/// Local governance provider built on the shared governance engine.
pub struct LocalGovernanceProvider;

impl GovernanceProvider for LocalGovernanceProvider {
    fn name(&self) -> &'static str {
        "local"
    }

    fn evaluate_governance(&self, request: &GovernanceRequest) -> Result<GovernanceDecision> {
        let managed_request = build_managed_request(request);
        let execution_id = format!("governance_check_{}", uuid::Uuid::new_v4());
        let input = governance_input_for_managed_command(
            &execution_id,
            &managed_request,
            request.workspace_id.clone(),
            None,
        );
        let verdict = evaluate_governance(&input);
        Ok(governance_decision_from_verdict(verdict, &input))
    }
}

impl PolicyProvider for LocalPolicyProvider {
    fn name(&self) -> &'static str {
        "local"
    }

    fn evaluate(&self, request: &PolicyRequest) -> Result<PolicyDecision> {
        let governance = LocalGovernanceProvider.evaluate_governance(request)?;
        Ok(PolicyDecision {
            allowed: matches!(
                governance.verdict_class.as_str(),
                "allow" | "allow_with_constraints"
            ),
            risk_level: governance.risk_class,
            reasons: governance.rationale,
            blast_radius: governance.blast_radius,
        })
    }
}

/// Cerbos PDP policy provider.
/// Connects to a Cerbos instance via HTTP API.
///
/// NOTE: Cerbos integration requires the `ureq` dependency which is not
/// currently included. This provider always falls back to local policy
/// evaluation until the dependency is added.
pub struct CerbosPolicyProvider {
    pub endpoint: String,
}

/// Cerbos-backed governance provider.
pub struct CerbosGovernanceProvider {
    pub endpoint: String,
}

impl CerbosPolicyProvider {
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
        }
    }
}

impl CerbosGovernanceProvider {
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
        }
    }
}

impl PolicyProvider for CerbosPolicyProvider {
    fn name(&self) -> &'static str {
        "cerbos"
    }

    fn evaluate(&self, request: &PolicyRequest) -> Result<PolicyDecision> {
        // Cerbos integration requires the ureq dependency.
        // Falling back to local policy evaluation.
        tracing::info!(
            endpoint = %self.endpoint,
            "Cerbos integration requires the ureq dependency; falling back to local policy"
        );
        LocalPolicyProvider.evaluate(request)
    }
}

impl GovernanceProvider for CerbosGovernanceProvider {
    fn name(&self) -> &'static str {
        "cerbos"
    }

    fn evaluate_governance(&self, request: &GovernanceRequest) -> Result<GovernanceDecision> {
        tracing::info!(
            endpoint = %self.endpoint,
            "Cerbos governance integration requires the ureq dependency; falling back to local governance"
        );
        LocalGovernanceProvider.evaluate_governance(request)
    }
}

/// Composite provider that chains local + external evaluation.
pub struct CompositePolicyProvider {
    providers: Vec<Box<dyn PolicyProvider>>,
}

/// Composite governance provider that chains local + external evaluation.
pub struct CompositeGovernanceProvider {
    providers: Vec<Box<dyn GovernanceProvider>>,
}

impl CompositePolicyProvider {
    pub fn new(providers: Vec<Box<dyn PolicyProvider>>) -> Self {
        Self { providers }
    }
}

impl CompositeGovernanceProvider {
    pub fn new(providers: Vec<Box<dyn GovernanceProvider>>) -> Self {
        Self { providers }
    }
}

impl PolicyProvider for CompositePolicyProvider {
    fn name(&self) -> &'static str {
        "composite"
    }

    fn evaluate(&self, request: &PolicyRequest) -> Result<PolicyDecision> {
        // Run all providers; most restrictive decision wins
        let mut strictest: Option<PolicyDecision> = None;

        for provider in &self.providers {
            match provider.evaluate(request) {
                Ok(decision) => {
                    strictest = Some(match strictest {
                        None => decision,
                        Some(prev) if !decision.allowed => {
                            // Denial overrides previous allows
                            let mut merged = decision;
                            merged.reasons.extend(prev.reasons);
                            merged
                        }
                        Some(prev) => prev,
                    });
                }
                Err(e) => {
                    tracing::warn!(provider = provider.name(), error = %e, "policy evaluation failed");
                }
            }
        }

        strictest.ok_or_else(|| anyhow::anyhow!("no policy providers available"))
    }
}

impl GovernanceProvider for CompositeGovernanceProvider {
    fn name(&self) -> &'static str {
        "composite"
    }

    fn evaluate_governance(&self, request: &GovernanceRequest) -> Result<GovernanceDecision> {
        let mut strictest: Option<GovernanceDecision> = None;

        for provider in &self.providers {
            match provider.evaluate_governance(request) {
                Ok(decision) => {
                    strictest = Some(match strictest {
                        None => decision,
                        Some(prev)
                            if governance_verdict_severity(&decision.verdict_class)
                                >= governance_verdict_severity(&prev.verdict_class) =>
                        {
                            let mut merged = decision;
                            for reason in prev.rationale {
                                if !merged.rationale.contains(&reason) {
                                    merged.rationale.push(reason);
                                }
                            }
                            for constraint in prev.constraints {
                                if !merged.constraints.contains(&constraint) {
                                    merged.constraints.push(constraint);
                                }
                            }
                            merged
                        }
                        Some(prev) => prev,
                    });
                }
                Err(e) => {
                    tracing::warn!(provider = provider.name(), error = %e, "governance evaluation failed");
                }
            }
        }

        strictest.ok_or_else(|| anyhow::anyhow!("no governance providers available"))
    }
}

/// Create the appropriate policy provider based on configuration.
pub fn create_policy_provider(cerbos_endpoint: Option<&str>) -> Box<dyn PolicyProvider> {
    match cerbos_endpoint {
        Some(endpoint) if !endpoint.is_empty() => {
            tracing::info!(endpoint, "using composite policy provider (local + Cerbos)");
            Box::new(CompositePolicyProvider::new(vec![
                Box::new(LocalPolicyProvider),
                Box::new(CerbosPolicyProvider::new(endpoint)),
            ]))
        }
        _ => {
            tracing::info!("using local policy provider");
            Box::new(LocalPolicyProvider)
        }
    }
}

/// Create the appropriate governance provider based on configuration.
pub fn create_governance_provider(cerbos_endpoint: Option<&str>) -> Box<dyn GovernanceProvider> {
    match cerbos_endpoint {
        Some(endpoint) if !endpoint.is_empty() => {
            tracing::info!(
                endpoint,
                "using composite governance provider (local + Cerbos)"
            );
            Box::new(CompositeGovernanceProvider::new(vec![
                Box::new(LocalGovernanceProvider),
                Box::new(CerbosGovernanceProvider::new(endpoint)),
            ]))
        }
        _ => {
            tracing::info!("using local governance provider");
            Box::new(LocalGovernanceProvider)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        create_governance_provider, GovernanceProvider, LocalGovernanceProvider,
        LocalPolicyProvider, PolicyProvider, PolicyRequest,
    };

    fn request(command: &str, allow_network: bool) -> PolicyRequest {
        PolicyRequest {
            command: command.to_string(),
            workspace_id: Some("workspace-a".to_string()),
            source: "test".to_string(),
            allow_network,
            cwd: Some("/tmp".to_string()),
        }
    }

    #[test]
    fn local_policy_provider_remains_backward_compatible() {
        let decision = LocalPolicyProvider
            .evaluate(&request("echo hello", false))
            .expect("policy evaluation should succeed");
        assert!(decision.allowed);
        assert_eq!(decision.risk_level, "low");
    }

    #[test]
    fn local_governance_provider_returns_governance_metadata() {
        let decision = LocalGovernanceProvider
            .evaluate_governance(&request("rm -rf /tmp/demo", false))
            .expect("governance evaluation should succeed");
        assert_eq!(decision.verdict_class, "require_approval");
        assert!(!decision.policy_fingerprint.is_empty());
        assert!(decision.blast_radius.contains("lane:"));
    }

    #[test]
    fn create_governance_provider_returns_callable_provider() {
        let provider = create_governance_provider(None);
        let decision = provider
            .evaluate_governance(&request("echo hello", false))
            .expect("governance provider should evaluate request");
        assert!(!decision.verdict_class.is_empty());
    }
}
