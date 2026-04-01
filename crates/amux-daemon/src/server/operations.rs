use std::sync::OnceLock;

use amux_protocol::{OperationLifecycleState, OperationStatusSnapshot};

pub(super) const OPERATION_KIND_CONCIERGE_WELCOME: &str = "concierge_welcome";
pub(super) const OPERATION_KIND_PROVIDER_VALIDATION: &str = "provider_validation";
pub(super) const OPERATION_KIND_FETCH_MODELS: &str = "fetch_models";
pub(super) const OPERATION_KIND_EXPLAIN_ACTION: &str = "explain_action";

#[derive(Debug, Clone)]
pub(super) struct OperationRecord {
    pub(super) operation_id: String,
    pub(super) kind: String,
    pub(super) dedup: Option<String>,
    pub(super) state: OperationLifecycleState,
    pub(super) revision: u64,
}

impl OperationRecord {
    pub(super) fn snapshot(&self) -> OperationStatusSnapshot {
        OperationStatusSnapshot {
            operation_id: self.operation_id.clone(),
            kind: self.kind.clone(),
            dedup: self.dedup.clone(),
            state: self.state,
            revision: self.revision,
        }
    }
}

pub(super) fn concierge_welcome_dedup_key(agent: &Arc<crate::agent::AgentEngine>) -> String {
    format!("{OPERATION_KIND_CONCIERGE_WELCOME}:{:p}", Arc::as_ptr(agent))
}

pub(super) fn provider_validation_dedup_key(
    agent: &Arc<crate::agent::AgentEngine>,
    provider_id: &str,
) -> String {
    format!(
        "{OPERATION_KIND_PROVIDER_VALIDATION}:{provider_id}:{:p}",
        Arc::as_ptr(agent)
    )
}

pub(super) fn fetch_models_dedup_key(
    agent: &Arc<crate::agent::AgentEngine>,
    provider_id: &str,
) -> String {
    format!(
        "{OPERATION_KIND_FETCH_MODELS}:{provider_id}:{:p}",
        Arc::as_ptr(agent)
    )
}

pub(super) fn explain_action_dedup_key(
    agent: &Arc<crate::agent::AgentEngine>,
    action_id: &str,
    step_index: Option<usize>,
) -> String {
    format!(
        "{OPERATION_KIND_EXPLAIN_ACTION}:{action_id}:{:?}:{:p}",
        step_index,
        Arc::as_ptr(agent)
    )
}

pub(super) fn operation_registry() -> &'static OperationRegistry {
    static REGISTRY: OnceLock<OperationRegistry> = OnceLock::new();
    REGISTRY.get_or_init(OperationRegistry::default)
}
