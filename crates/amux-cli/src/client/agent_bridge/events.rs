use amux_protocol::{AmuxCodec, DaemonMessage};
use anyhow::Result;
use futures::StreamExt;
use tokio_util::codec::Framed;

use super::emit_agent_event;

pub(super) async fn handle_message<T>(framed: &mut Framed<T, AmuxCodec>) -> Result<bool>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    match framed.next().await {
        Some(Ok(DaemonMessage::AgentEvent { event_json })) => {
            emit_agent_event(&event_json)?;
        }
        Some(Ok(DaemonMessage::AgentThreadList { threads_json })) => {
            let msg = serde_json::json!({"type":"thread-list","data":serde_json::from_str::<serde_json::Value>(&threads_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentThreadDetail { thread_json })) => {
            let msg = serde_json::json!({"type":"thread-detail","data":serde_json::from_str::<serde_json::Value>(&thread_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentTaskList { tasks_json })) => {
            let msg = serde_json::json!({"type":"task-list","data":serde_json::from_str::<serde_json::Value>(&tasks_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentRunList { runs_json })) => {
            let msg = serde_json::json!({"type":"run-list","data":serde_json::from_str::<serde_json::Value>(&runs_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentRunDetail { run_json })) => {
            let msg = serde_json::json!({"type":"run-detail","data":serde_json::from_str::<serde_json::Value>(&run_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentTaskEnqueued { task_json })) => {
            let msg = serde_json::json!({"type":"task-enqueued","data":serde_json::from_str::<serde_json::Value>(&task_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentTaskCancelled { task_id, cancelled })) => {
            let msg = serde_json::json!({"type":"task-cancelled","task_id":task_id,"cancelled":cancelled});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentGoalRunStarted { goal_run_json })) => {
            let msg = serde_json::json!({"type":"goal-run-started","data":serde_json::from_str::<serde_json::Value>(&goal_run_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentGoalRunList { goal_runs_json })) => {
            let msg = serde_json::json!({"type":"goal-run-list","data":serde_json::from_str::<serde_json::Value>(&goal_runs_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentGoalRunDetail { goal_run_json })) => {
            let msg = serde_json::json!({"type":"goal-run-detail","data":serde_json::from_str::<serde_json::Value>(&goal_run_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentGoalRunControlled { goal_run_id, ok })) => {
            let msg = serde_json::json!({"type":"goal-run-controlled","data":{"goal_run_id":goal_run_id,"ok":ok}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentTodoList { todos_json })) => {
            let msg = serde_json::json!({"type":"todo-list","data":serde_json::from_str::<serde_json::Value>(&todos_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentTodoDetail {
            thread_id,
            todos_json,
        })) => {
            let msg = serde_json::json!({"type":"todo-detail","data":{"thread_id":thread_id,"items":serde_json::from_str::<serde_json::Value>(&todos_json).unwrap_or_default()}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWorkContextDetail {
            thread_id,
            context_json,
        })) => {
            let msg = serde_json::json!({"type":"work-context-detail","data":{"thread_id":thread_id,"context":serde_json::from_str::<serde_json::Value>(&context_json).unwrap_or_default()}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::GitDiff {
            repo_path,
            file_path,
            diff,
        })) => {
            let msg = serde_json::json!({"type":"git-diff","data":{"repo_path":repo_path,"file_path":file_path,"diff":diff}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::FilePreview {
            path,
            content,
            truncated,
            is_text,
        })) => {
            let msg = serde_json::json!({"type":"file-preview","data":{"path":path,"content":content,"truncated":truncated,"is_text":is_text}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentConfigResponse { config_json })) => {
            let msg = serde_json::json!({"type":"config","data":serde_json::from_str::<serde_json::Value>(&config_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentHeartbeatItems { items_json })) => {
            let msg = serde_json::json!({"type":"heartbeat-items","data":serde_json::from_str::<serde_json::Value>(&items_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentProviderValidation {
            operation_id: _,
            provider_id,
            valid,
            error,
            models_json,
        })) => {
            let msg = serde_json::json!({
                "type": "provider-validation",
                "data": {
                    "provider_id": provider_id,
                    "valid": valid,
                    "error": error,
                    "models": models_json.and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok()),
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentProviderAuthStates { states_json })) => {
            let msg = serde_json::json!({"type":"provider-auth-states","data":serde_json::from_str::<serde_json::Value>(&states_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentSubAgentList { sub_agents_json })) => {
            let msg = serde_json::json!({"type":"sub-agent-list","data":serde_json::from_str::<serde_json::Value>(&sub_agents_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentSubAgentUpdated { sub_agent_json })) => {
            let msg = serde_json::json!({"type":"sub-agent-updated","data":serde_json::from_str::<serde_json::Value>(&sub_agent_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentSubAgentRemoved { sub_agent_id })) => {
            let msg = serde_json::json!({"type":"sub-agent-removed","data":{"sub_agent_id":sub_agent_id}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentConciergeConfig { config_json })) => {
            let msg = serde_json::json!({"type":"concierge-config","data":serde_json::from_str::<serde_json::Value>(&config_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentConciergeWelcomeDismissed)) => {
            let msg = serde_json::json!({"type":"concierge-welcome-dismissed"});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentTierChanged {
            previous_tier,
            new_tier,
            reason,
        })) => {
            let msg = serde_json::json!({
                "type": "tier-changed",
                "data": {
                    "previous_tier": previous_tier,
                    "new_tier": new_tier,
                    "reason": reason,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentStatusResponse {
            tier,
            feature_flags_json,
            activity,
            active_thread_id,
            active_goal_run_id,
            active_goal_run_title,
            provider_health_json,
            gateway_statuses_json,
            recent_actions_json,
            diagnostics_json,
        })) => {
            let msg = serde_json::json!({
                "type": "status-response",
                "data": {
                    "tier": tier,
                    "feature_flags": serde_json::from_str::<serde_json::Value>(&feature_flags_json).unwrap_or_default(),
                    "activity": activity,
                    "active_thread_id": active_thread_id,
                    "active_goal_run_id": active_goal_run_id,
                    "active_goal_run_title": active_goal_run_title,
                    "provider_health": serde_json::from_str::<serde_json::Value>(&provider_health_json).unwrap_or_default(),
                    "gateway_statuses": serde_json::from_str::<serde_json::Value>(&gateway_statuses_json).unwrap_or_default(),
                    "recent_actions": serde_json::from_str::<serde_json::Value>(&recent_actions_json).unwrap_or_default(),
                    "diagnostics": serde_json::from_str::<serde_json::Value>(&diagnostics_json).unwrap_or_default(),
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentExplanation {
            operation_id: _,
            explanation_json,
        })) => {
            let msg = serde_json::json!({
                "type": "agent-explanation",
                "data": serde_json::from_str::<serde_json::Value>(&explanation_json).unwrap_or_default(),
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentDivergentSessionStarted {
            operation_id: _,
            session_json,
        })) => {
            let msg = serde_json::json!({
                "type": "agent-divergent-session-started",
                "data": serde_json::from_str::<serde_json::Value>(&session_json).unwrap_or_default(),
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentDivergentSession { session_json })) => {
            let msg = serde_json::json!({
                "type": "agent-divergent-session",
                "data": serde_json::from_str::<serde_json::Value>(&session_json).unwrap_or_default(),
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::PluginListResult { plugins })) => {
            let msg = serde_json::json!({
                "type": "plugin-list-result",
                "plugins": plugins.iter().map(|p| serde_json::json!({
                    "name": p.name, "version": p.version, "description": p.description,
                    "author": p.author, "enabled": p.enabled, "install_source": p.install_source,
                    "has_api": p.has_api, "has_auth": p.has_auth, "has_commands": p.has_commands,
                    "has_skills": p.has_skills, "endpoint_count": p.endpoint_count,
                    "settings_count": p.settings_count, "installed_at": p.installed_at,
                    "updated_at": p.updated_at, "auth_status": p.auth_status,
                })).collect::<Vec<_>>(),
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::PluginGetResult {
            plugin,
            settings_schema,
        })) => {
            let msg = serde_json::json!({
                "type": "plugin-get-result",
                "plugin": plugin.as_ref().map(|p| serde_json::json!({
                    "name": p.name, "version": p.version, "description": p.description,
                    "author": p.author, "enabled": p.enabled, "install_source": p.install_source,
                    "has_api": p.has_api, "has_auth": p.has_auth, "has_commands": p.has_commands,
                    "has_skills": p.has_skills, "endpoint_count": p.endpoint_count,
                    "settings_count": p.settings_count, "installed_at": p.installed_at,
                    "updated_at": p.updated_at, "auth_status": p.auth_status,
                })),
                "settings_schema": settings_schema,
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::PluginActionResult { success, message })) => {
            let msg = serde_json::json!({
                "type": "plugin-action-result",
                "success": success,
                "message": message,
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::PluginSettingsResult {
            plugin_name,
            settings,
        })) => {
            let msg = serde_json::json!({
                "type": "plugin-settings",
                "plugin_name": plugin_name,
                "settings": settings.iter().map(|(k, v, s)| serde_json::json!({"key": k, "value": v, "is_secret": s})).collect::<Vec<_>>(),
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::PluginTestConnectionResult {
            plugin_name,
            success,
            message,
        })) => {
            let msg = serde_json::json!({
                "type": "plugin-test-connection-result",
                "plugin_name": plugin_name,
                "success": success,
                "message": message,
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::PluginOAuthUrl { name, url })) => {
            let msg = serde_json::json!({
                "type": "plugin-oauth-url",
                "name": name,
                "url": url,
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::PluginOAuthComplete {
            operation_id: _,
            name,
            success,
            error,
        })) => {
            let msg = serde_json::json!({
                "type": "plugin-oauth-complete",
                "name": name,
                "success": success,
                "error": error,
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWhatsAppLinkStatus {
            state,
            phone,
            last_error,
        })) => {
            let msg = serde_json::json!({
                "type": "whatsapp-link-status",
                "data": {
                    "status": state,
                    "phone": phone,
                    "last_error": last_error,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWhatsAppLinkQr {
            ascii_qr,
            expires_at_ms,
        })) => {
            let msg = serde_json::json!({
                "type": "whatsapp-link-qr",
                "data": {
                    "ascii_qr": ascii_qr,
                    "expires_at_ms": expires_at_ms,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWhatsAppLinked { phone })) => {
            let msg = serde_json::json!({
                "type": "whatsapp-link-linked",
                "data": {
                    "phone": phone,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWhatsAppLinkError {
            message,
            recoverable,
        })) => {
            let msg = serde_json::json!({
                "type": "whatsapp-link-error",
                "data": {
                    "message": message,
                    "recoverable": recoverable,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWhatsAppLinkDisconnected { reason })) => {
            let msg = serde_json::json!({
                "type": "whatsapp-link-disconnected",
                "data": {
                    "reason": reason,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentOperatorProfileSessionStarted { session_id, kind })) => {
            let msg = serde_json::json!({
                "type": "operator-profile-session-started",
                "data": { "session_id": session_id, "kind": kind }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentOperatorProfileQuestion {
            session_id,
            question_id,
            field_key,
            prompt,
            input_kind,
            optional,
        })) => {
            let msg = serde_json::json!({
                "type": "operator-profile-question",
                "data": {
                    "session_id": session_id,
                    "question_id": question_id,
                    "field_key": field_key,
                    "prompt": prompt,
                    "input_kind": input_kind,
                    "optional": optional,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentOperatorProfileProgress {
            session_id,
            answered,
            remaining,
            completion_ratio,
        })) => {
            let msg = serde_json::json!({
                "type": "operator-profile-progress",
                "data": {
                    "session_id": session_id,
                    "answered": answered,
                    "remaining": remaining,
                    "completion_ratio": completion_ratio,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentOperatorProfileSummary { summary_json })) => {
            let msg = serde_json::json!({
                "type": "operator-profile-summary",
                "data": serde_json::from_str::<serde_json::Value>(&summary_json).unwrap_or_default()
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentOperatorProfileSessionCompleted {
            session_id,
            updated_fields,
        })) => {
            let msg = serde_json::json!({
                "type": "operator-profile-session-completed",
                "data": {
                    "session_id": session_id,
                    "updated_fields": updated_fields,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::Error { message }))
        | Some(Ok(DaemonMessage::AgentError { message })) => {
            let msg = serde_json::json!({"type":"error","message":message});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(_)) => {}
        Some(Err(error)) => return Err(error.into()),
        None => {
            let msg = serde_json::json!({"type":"error","message":"daemon connection closed"});
            emit_agent_event(&msg.to_string())?;
            return Ok(false);
        }
    }

    Ok(true)
}
