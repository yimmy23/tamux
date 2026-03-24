//! External policy engine integration.
//! Supports Cerbos PDP for fine-grained ABAC policy evaluation.
//! Falls back to local regex-based policy when external engine is unavailable.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Policy evaluation result.
#[derive(Debug, Clone)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub risk_level: String,
    pub reasons: Vec<String>,
    pub blast_radius: String,
}

/// Trait for policy evaluation providers.
pub trait PolicyProvider: Send + Sync {
    fn evaluate(&self, request: &PolicyRequest) -> Result<PolicyDecision>;
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

/// Local regex-based policy provider (existing implementation wrapper).
pub struct LocalPolicyProvider;

impl PolicyProvider for LocalPolicyProvider {
    fn name(&self) -> &'static str {
        "local"
    }

    fn evaluate(&self, request: &PolicyRequest) -> Result<PolicyDecision> {
        let managed_request = amux_protocol::ManagedCommandRequest {
            command: request.command.clone(),
            rationale: String::new(),
            allow_network: request.allow_network,
            sandbox_enabled: true,
            security_level: amux_protocol::SecurityLevel::Moderate,
            cwd: request.cwd.clone(),
            language_hint: None,
            source: amux_protocol::ManagedCommandSource::Agent,
        };
        let execution_id = format!("policy_check_{}", uuid::Uuid::new_v4());
        let result = crate::policy::evaluate_command(
            execution_id,
            &managed_request,
            request.workspace_id.clone(),
        );
        match result {
            crate::policy::PolicyDecision::Allow => Ok(PolicyDecision {
                allowed: true,
                risk_level: "low".to_string(),
                reasons: vec![],
                blast_radius: "current session".to_string(),
            }),
            crate::policy::PolicyDecision::RequireApproval(approval) => Ok(PolicyDecision {
                allowed: false,
                risk_level: approval.risk_level,
                reasons: approval.reasons,
                blast_radius: approval.blast_radius,
            }),
        }
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

impl CerbosPolicyProvider {
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

/// Composite provider that chains local + external evaluation.
pub struct CompositePolicyProvider {
    providers: Vec<Box<dyn PolicyProvider>>,
}

impl CompositePolicyProvider {
    pub fn new(providers: Vec<Box<dyn PolicyProvider>>) -> Self {
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
