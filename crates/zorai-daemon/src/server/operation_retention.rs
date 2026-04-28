#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OperationTerminalVisibility {
    UntilProcessExit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InterruptedVisibility {
    ReconnectOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OperationDurability {
    Ephemeral,
    DesiredStateDurable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShutdownDisposition {
    LostOnDaemonStop,
    DesiredStateRemainsNeedsReconcile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SupersessionPolicy {
    NotEmittedInPhaseTwo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OperationRetentionPolicy {
    pub(super) terminal_visibility: OperationTerminalVisibility,
    pub(super) interrupted_visibility: InterruptedVisibility,
    pub(super) survives_process_restart: bool,
    pub(super) release_dedup_on_terminal: bool,
    pub(super) durability: OperationDurability,
    pub(super) accepted_shutdown: ShutdownDisposition,
    pub(super) started_shutdown: ShutdownDisposition,
    pub(super) supersession: SupersessionPolicy,
}

const IN_MEMORY_RECONNECT_ONLY_POLICY: OperationRetentionPolicy = OperationRetentionPolicy {
    terminal_visibility: OperationTerminalVisibility::UntilProcessExit,
    interrupted_visibility: InterruptedVisibility::ReconnectOnly,
    survives_process_restart: false,
    release_dedup_on_terminal: true,
    durability: OperationDurability::Ephemeral,
    accepted_shutdown: ShutdownDisposition::LostOnDaemonStop,
    started_shutdown: ShutdownDisposition::LostOnDaemonStop,
    supersession: SupersessionPolicy::NotEmittedInPhaseTwo,
};

const DESIRED_STATE_DURABLE_POLICY: OperationRetentionPolicy = OperationRetentionPolicy {
    terminal_visibility: OperationTerminalVisibility::UntilProcessExit,
    interrupted_visibility: InterruptedVisibility::ReconnectOnly,
    survives_process_restart: false,
    release_dedup_on_terminal: true,
    durability: OperationDurability::DesiredStateDurable,
    accepted_shutdown: ShutdownDisposition::DesiredStateRemainsNeedsReconcile,
    started_shutdown: ShutdownDisposition::DesiredStateRemainsNeedsReconcile,
    supersession: SupersessionPolicy::NotEmittedInPhaseTwo,
};

pub(super) fn retention_policy_for_kind(kind: &str) -> OperationRetentionPolicy {
    match kind {
        OPERATION_KIND_CONFIG_SET_ITEM
        | OPERATION_KIND_SET_PROVIDER_MODEL
        | OPERATION_KIND_SET_SUB_AGENT
        | OPERATION_KIND_REMOVE_SUB_AGENT => DESIRED_STATE_DURABLE_POLICY,
        _ => IN_MEMORY_RECONNECT_ONLY_POLICY,
    }
}
