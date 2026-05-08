use super::*;
use crate::client::ThreadDetailChunkBuffer;
use crate::client::{ClientEvent, DaemonClient};
use crate::wire::{
    AgentConfigSnapshot, AgentTask, AgentThread, AnticipatoryItem, CheckpointSummary, FetchedModel,
    GoalRun, GoalRunStatus, HeartbeatItem, RestoreOutcome, TaskStatus, ThreadParticipantSuggestion,
    ThreadWorkContext,
};
use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::codec::Framed;
use tracing::{debug, error, info, warn};
use zorai_protocol::{ClientMessage, DaemonMessage, ZoraiCodec};
impl DaemonClient {
    pub(crate) async fn handle_daemon_message(
        message: DaemonMessage,
        event_tx: &mpsc::Sender<ClientEvent>,
        thread_detail_chunks: &mut Option<ThreadDetailChunkBuffer>,
    ) -> bool {
        match message {
            message @ (DaemonMessage::AgentEvent { .. }
            | DaemonMessage::AgentThreadList { .. }
            | DaemonMessage::AgentThreadDetail { .. }
            | DaemonMessage::AgentThreadDetailChunk { .. }
            | DaemonMessage::AgentTaskList { .. }
            | DaemonMessage::AgentGoalRunList { .. }
            | DaemonMessage::AgentGoalRunStarted { .. }
            | DaemonMessage::AgentGoalRunDetail { .. }
            | DaemonMessage::AgentGoalRunControlled { .. }
            | DaemonMessage::AgentCheckpointList { .. }
            | DaemonMessage::AgentCheckpointRestored { .. }
            | DaemonMessage::AgentTodoDetail { .. }
            | DaemonMessage::AgentWorkContextDetail { .. }
            | DaemonMessage::GitDiff { .. }
            | DaemonMessage::FilePreview { .. }
            | DaemonMessage::AgentConfigResponse { .. }
            | DaemonMessage::AgentExternalRuntimeMigrationResult { .. }
            | DaemonMessage::AgentModelsResponse { .. }
            | DaemonMessage::AgentHeartbeatItems { .. }
            | DaemonMessage::AgentEventRows { .. }
            | DaemonMessage::AgentDbMessageAck
            | DaemonMessage::SessionSpawned { .. }
            | DaemonMessage::ApprovalRequired { .. }
            | DaemonMessage::AgentTaskApprovalRules { .. }
            | DaemonMessage::ApprovalResolved { .. }
            | DaemonMessage::AgentWorkspaceSettings { .. }
            | DaemonMessage::AgentWorkspaceSettingsList { .. }
            | DaemonMessage::AgentWorkspaceTaskList { .. }
            | DaemonMessage::AgentWorkspaceTaskUpdated { .. }
            | DaemonMessage::AgentWorkspaceTaskDeleted { .. }
            | DaemonMessage::AgentWorkspaceNoticeList { .. }
            | DaemonMessage::AgentWorkspaceError { .. }) => {
                Self::handle_thread_workspace_daemon_messages(
                    message,
                    event_tx,
                    thread_detail_chunks,
                )
                .await
            }
            message @ (DaemonMessage::AgentProviderAuthStates { .. }
            | DaemonMessage::AgentProviderCatalog { .. }
            | DaemonMessage::AgentOpenAICodexAuthStatus { .. }
            | DaemonMessage::AgentOpenAICodexAuthLoginResult { .. }
            | DaemonMessage::AgentOpenAICodexAuthLogoutResult { .. }
            | DaemonMessage::AgentProviderValidation { .. }
            | DaemonMessage::AgentSubAgentList { .. }
            | DaemonMessage::AgentSubAgentUpdated { .. }
            | DaemonMessage::AgentSubAgentRemoved { .. }
            | DaemonMessage::AgentConciergeConfig { .. }
            | DaemonMessage::PluginListResult { .. }
            | DaemonMessage::PluginGetResult { .. }
            | DaemonMessage::PluginSettingsResult { .. }
            | DaemonMessage::PluginTestConnectionResult { .. }
            | DaemonMessage::PluginActionResult { .. }
            | DaemonMessage::PluginCommandsResult { .. }
            | DaemonMessage::PluginOAuthUrl { .. }
            | DaemonMessage::PluginOAuthComplete { .. }) => {
                Self::handle_provider_plugin_daemon_messages(message, event_tx).await
            }
            message @ (DaemonMessage::AgentWhatsAppLinkStatus { .. }
            | DaemonMessage::AgentThreadMessagePinResult { .. }
            | DaemonMessage::AgentWhatsAppLinkQr { .. }
            | DaemonMessage::AgentWhatsAppLinked { .. }
            | DaemonMessage::AgentWhatsAppLinkError { .. }
            | DaemonMessage::AgentWhatsAppLinkDisconnected { .. }
            | DaemonMessage::AgentExplanation { .. }
            | DaemonMessage::AgentDivergentSessionStarted { .. }
            | DaemonMessage::AgentDivergentSession { .. }
            | DaemonMessage::AgentStatusResponse { .. }
            | DaemonMessage::AgentStatisticsResponse { .. }
            | DaemonMessage::AgentPromptInspection { .. }
            | DaemonMessage::AgentOperatorProfileSessionStarted { .. }
            | DaemonMessage::AgentOperatorProfileQuestion { .. }
            | DaemonMessage::AgentOperatorProfileProgress { .. }
            | DaemonMessage::AgentOperatorProfileSummary { .. }
            | DaemonMessage::AgentOperatorModel { .. }
            | DaemonMessage::AgentOperatorModelReset { .. }
            | DaemonMessage::AgentCollaborationSessions { .. }
            | DaemonMessage::AgentCollaborationVoteResult { .. }
            | DaemonMessage::AgentGeneratedTools { .. }
            | DaemonMessage::AgentSpeechToTextResult { .. }
            | DaemonMessage::AgentTextToSpeechResult { .. }
            | DaemonMessage::AgentGenerateImageResult { .. }
            | DaemonMessage::AgentOperatorProfileSessionCompleted { .. }
            | DaemonMessage::AgentError { .. }
            | DaemonMessage::AgentTierChanged { .. }
            | DaemonMessage::SemanticIndexRepairResult { .. }
            | DaemonMessage::GatewayBootstrap { .. }
            | DaemonMessage::GatewaySendRequest { .. }
            | DaemonMessage::GatewayReloadCommand { .. }
            | DaemonMessage::GatewayShutdownCommand { .. }
            | DaemonMessage::Error { .. }) => {
                Self::handle_activity_profile_gateway_daemon_messages(message, event_tx).await
            }
            other => {
                // Surface unhandled variants loudly in dev builds so future
                // protocol drift gets noticed; stay quiet in release so users
                // aren't spammed if the daemon adds new message types between
                // releases. To handle a variant, add it to one of the explicit
                // arms above.
                #[cfg(debug_assertions)]
                {
                    warn!(
                        target: "zorai_tui::client",
                        message = ?other,
                        "unhandled DaemonMessage variant — dispatcher needs an explicit arm",
                    );
                }
                #[cfg(not(debug_assertions))]
                {
                    debug!(
                        target: "zorai_tui::client",
                        message = ?other,
                        "ignoring unhandled daemon message",
                    );
                }
            }
        }

        true
    }
}
