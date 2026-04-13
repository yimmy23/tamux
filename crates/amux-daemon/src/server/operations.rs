use std::future::Future;
use std::sync::OnceLock;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use amux_protocol::{OperationLifecycleState, OperationStatusSnapshot};

pub(super) const OPERATION_KIND_CONCIERGE_WELCOME: &str = "concierge_welcome";
pub(super) const OPERATION_KIND_PROVIDER_VALIDATION: &str = "provider_validation";
pub(super) const OPERATION_KIND_FETCH_MODELS: &str = "fetch_models";
pub(super) const OPERATION_KIND_EXPLAIN_ACTION: &str = "explain_action";
pub(super) const OPERATION_KIND_PLUGIN_OAUTH_START: &str = "plugin_oauth_start";
pub(super) const OPERATION_KIND_PLUGIN_API_CALL: &str = "plugin_api_call";
pub(super) const OPERATION_KIND_SKILL_PUBLISH: &str = "skill_publish";
pub(super) const OPERATION_KIND_SKILL_IMPORT: &str = "skill_import";
pub(super) const OPERATION_KIND_SYNTHESIZE_TOOL: &str = "synthesize_tool";
pub(super) const OPERATION_KIND_START_DIVERGENT_SESSION: &str = "start_divergent_session";
pub(super) const OPERATION_KIND_CONFIG_SET_ITEM: &str = "config_set_item";
pub(super) const OPERATION_KIND_SET_PROVIDER_MODEL: &str = "set_provider_model";
pub(super) const OPERATION_KIND_SET_SUB_AGENT: &str = "set_sub_agent";
pub(super) const OPERATION_KIND_REMOVE_SUB_AGENT: &str = "remove_sub_agent";

#[derive(Debug, Clone)]
pub(crate) struct OperationRecord {
    pub(crate) operation_id: String,
    pub(crate) kind: String,
    pub(crate) dedup: Option<String>,
    pub(crate) state: OperationLifecycleState,
    pub(crate) revision: u64,
    pub(crate) terminal_result: Option<serde_json::Value>,
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
    format!(
        "{OPERATION_KIND_CONCIERGE_WELCOME}:{:p}",
        Arc::as_ptr(agent)
    )
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

pub(super) fn plugin_oauth_start_dedup_key(
    agent: &Arc<crate::agent::AgentEngine>,
    plugin_name: &str,
) -> String {
    format!(
        "{OPERATION_KIND_PLUGIN_OAUTH_START}:{plugin_name}:{:p}",
        Arc::as_ptr(agent)
    )
}

pub(super) fn plugin_api_call_dedup_key(
    agent: &Arc<crate::agent::AgentEngine>,
    plugin_name: &str,
    endpoint_name: &str,
    params: &serde_json::Value,
) -> String {
    let mut hasher = DefaultHasher::new();
    params.to_string().hash(&mut hasher);
    let params_hash = hasher.finish();
    format!(
        "{OPERATION_KIND_PLUGIN_API_CALL}:{plugin_name}:{endpoint_name}:{params_hash:x}:{:p}",
        Arc::as_ptr(agent)
    )
}

pub(super) fn skill_publish_dedup_key(
    agent: &Arc<crate::agent::AgentEngine>,
    identifier: &str,
) -> String {
    format!(
        "{OPERATION_KIND_SKILL_PUBLISH}:{identifier}:{:p}",
        Arc::as_ptr(agent)
    )
}

pub(super) fn skill_import_dedup_key(
    agent: &Arc<crate::agent::AgentEngine>,
    source: &str,
    force: bool,
    publisher_verified: bool,
) -> String {
    format!(
        "{OPERATION_KIND_SKILL_IMPORT}:{source}:{force}:{publisher_verified}:{:p}",
        Arc::as_ptr(agent)
    )
}

pub(super) fn synthesize_tool_dedup_key(
    agent: &Arc<crate::agent::AgentEngine>,
    request_json: &str,
) -> String {
    let mut hasher = DefaultHasher::new();
    request_json.hash(&mut hasher);
    let request_hash = hasher.finish();
    format!(
        "{OPERATION_KIND_SYNTHESIZE_TOOL}:{request_hash:x}:{:p}",
        Arc::as_ptr(agent)
    )
}

pub(super) fn start_divergent_session_dedup_key(
    agent: &Arc<crate::agent::AgentEngine>,
    problem_statement: &str,
    thread_id: &str,
    goal_run_id: Option<&str>,
) -> String {
    let mut hasher = DefaultHasher::new();
    problem_statement.hash(&mut hasher);
    thread_id.hash(&mut hasher);
    goal_run_id.hash(&mut hasher);
    let request_hash = hasher.finish();
    format!(
        "{OPERATION_KIND_START_DIVERGENT_SESSION}:{request_hash:x}:{:p}",
        Arc::as_ptr(agent)
    )
}

pub(super) fn config_set_item_dedup_key(
    agent: &Arc<crate::agent::AgentEngine>,
    key_path: &str,
    value_json: &str,
) -> String {
    let mut hasher = DefaultHasher::new();
    key_path.hash(&mut hasher);
    value_json.hash(&mut hasher);
    let request_hash = hasher.finish();
    format!(
        "{OPERATION_KIND_CONFIG_SET_ITEM}:{request_hash:x}:{:p}",
        Arc::as_ptr(agent)
    )
}

pub(super) fn set_provider_model_dedup_key(
    agent: &Arc<crate::agent::AgentEngine>,
    provider_id: &str,
    model: &str,
) -> String {
    format!(
        "{OPERATION_KIND_SET_PROVIDER_MODEL}:{provider_id}:{model}:{:p}",
        Arc::as_ptr(agent)
    )
}

pub(super) fn set_target_agent_provider_model_dedup_key(
    agent: &Arc<crate::agent::AgentEngine>,
    target_agent_id: &str,
    provider_id: &str,
    model: &str,
) -> String {
    format!(
        "{OPERATION_KIND_SET_PROVIDER_MODEL}:{target_agent_id}:{provider_id}:{model}:{:p}",
        Arc::as_ptr(agent)
    )
}

pub(super) fn set_sub_agent_dedup_key(
    agent: &Arc<crate::agent::AgentEngine>,
    sub_agent_id: &str,
    sub_agent_json: &str,
) -> String {
    let mut hasher = DefaultHasher::new();
    sub_agent_id.hash(&mut hasher);
    sub_agent_json.hash(&mut hasher);
    let request_hash = hasher.finish();
    format!(
        "{OPERATION_KIND_SET_SUB_AGENT}:{request_hash:x}:{:p}",
        Arc::as_ptr(agent)
    )
}

pub(super) fn remove_sub_agent_dedup_key(
    agent: &Arc<crate::agent::AgentEngine>,
    sub_agent_id: &str,
) -> String {
    format!(
        "{OPERATION_KIND_REMOVE_SUB_AGENT}:{sub_agent_id}:{:p}",
        Arc::as_ptr(agent)
    )
}

pub(crate) fn operation_registry() -> &'static OperationRegistry {
    static REGISTRY: OnceLock<OperationRegistry> = OnceLock::new();
    REGISTRY.get_or_init(OperationRegistry::default)
}

pub(super) enum BackgroundOperationOutput {
    Completed(DaemonMessage),
    Failed(DaemonMessage),
}

pub(super) enum BackgroundSideEffectOutcome {
    Completed,
    Failed,
}

pub(super) fn spawn_background_operation<Fut>(
    subsystem: BackgroundSubsystem,
    operation_id: Option<String>,
    background_daemon_tx: tokio::sync::mpsc::UnboundedSender<BackgroundSignal>,
    background_daemon_pending: &mut BackgroundPendingCounts,
    future: Fut,
) where
    Fut: Future<Output = BackgroundOperationOutput> + Send + 'static,
{
    background_daemon_pending.increment(subsystem);
    tokio::spawn(async move {
        if let Some(operation_id) = operation_id.as_deref() {
            operation_registry().mark_started(operation_id);
        }

        let (failed, daemon_msg) = match future.await {
            BackgroundOperationOutput::Completed(msg) => (false, msg),
            BackgroundOperationOutput::Failed(msg) => (true, msg),
        };

        if let Some(operation_id) = operation_id.as_deref() {
            if failed {
                operation_registry().mark_failed(operation_id);
            } else {
                operation_registry().mark_completed(operation_id);
            }
        }

        let _ = background_daemon_tx.send(BackgroundSignal::Deliver(daemon_msg));
        let _ = background_daemon_tx.send(BackgroundSignal::Finished);
    });
}

pub(super) fn spawn_background_side_effect<Fut>(
    subsystem: BackgroundSubsystem,
    operation_id: Option<String>,
    background_daemon_tx: tokio::sync::mpsc::UnboundedSender<BackgroundSignal>,
    background_daemon_pending: &mut BackgroundPendingCounts,
    future: Fut,
) where
    Fut: Future<Output = BackgroundSideEffectOutcome> + Send + 'static,
{
    background_daemon_pending.increment(subsystem);
    tokio::spawn(async move {
        if let Some(operation_id) = operation_id.as_deref() {
            operation_registry().mark_started(operation_id);
        }

        let failed = matches!(future.await, BackgroundSideEffectOutcome::Failed);

        if let Some(operation_id) = operation_id.as_deref() {
            if failed {
                operation_registry().mark_failed(operation_id);
            } else {
                operation_registry().mark_completed(operation_id);
            }
        }

        let _ = background_daemon_tx.send(BackgroundSignal::Finished);
    });
}
