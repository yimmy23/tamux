impl DaemonClient {
    async fn handle_daemon_message_part3(
        message: DaemonMessage,
        event_tx: &mpsc::Sender<ClientEvent>,
    ) {
        match message {
            DaemonMessage::AgentThreadMessagePinResult { result_json } => {
                if let Ok(result) =
                    serde_json::from_str::<crate::client::ThreadMessagePinResultVm>(&result_json)
                {
                    let _ = event_tx
                        .send(ClientEvent::ThreadMessagePinResult(result))
                        .await;
                } else {
                    let _ = event_tx
                        .send(ClientEvent::Error(
                            "failed to parse thread pin mutation result".to_string(),
                        ))
                        .await;
                }
            }
            DaemonMessage::AgentWhatsAppLinkStatus {
                state,
                phone,
                last_error,
            } => {
                let _ = event_tx
                    .send(ClientEvent::WhatsAppLinkStatus {
                        state,
                        phone,
                        last_error,
                    })
                    .await;
            }
            DaemonMessage::AgentWhatsAppLinkQr {
                ascii_qr,
                expires_at_ms,
            } => {
                let _ = event_tx
                    .send(ClientEvent::WhatsAppLinkQr {
                        ascii_qr,
                        expires_at_ms,
                    })
                    .await;
            }
            DaemonMessage::AgentWhatsAppLinked { phone } => {
                let _ = event_tx.send(ClientEvent::WhatsAppLinked { phone }).await;
            }
            DaemonMessage::AgentWhatsAppLinkError {
                message,
                recoverable,
            } => {
                let _ = event_tx
                    .send(ClientEvent::WhatsAppLinkError {
                        message,
                        recoverable,
                    })
                    .await;
            }
            DaemonMessage::AgentWhatsAppLinkDisconnected { reason } => {
                let _ = event_tx
                    .send(ClientEvent::WhatsAppLinkDisconnected { reason })
                    .await;
            }
            DaemonMessage::AgentExplanation {
                operation_id: _,
                explanation_json,
            } => {
                let payload = serde_json::from_str::<serde_json::Value>(&explanation_json)
                    .unwrap_or_else(|_| serde_json::json!({}));
                let _ = event_tx.send(ClientEvent::AgentExplanation(payload)).await;
            }
            DaemonMessage::AgentDivergentSessionStarted {
                operation_id: _,
                session_json,
            } => {
                let payload = serde_json::from_str::<serde_json::Value>(&session_json)
                    .unwrap_or_else(|_| serde_json::json!({}));
                let _ = event_tx
                    .send(ClientEvent::DivergentSessionStarted(payload))
                    .await;
            }
            DaemonMessage::AgentDivergentSession { session_json } => {
                let payload = serde_json::from_str::<serde_json::Value>(&session_json)
                    .unwrap_or_else(|_| serde_json::json!({}));
                let _ = event_tx.send(ClientEvent::DivergentSession(payload)).await;
            }
            DaemonMessage::AgentStatusResponse {
                tier,
                activity,
                active_thread_id,
                active_goal_run_id,
                active_goal_run_title,
                provider_health_json,
                gateway_statuses_json,
                recent_actions_json,
                diagnostics_json,
                ..
            } => {
                let _ = event_tx
                    .send(ClientEvent::StatusSnapshot(AgentStatusSnapshotVm {
                        tier,
                        activity,
                        active_thread_id,
                        active_goal_run_id,
                        active_goal_run_title,
                        provider_health_json,
                        gateway_statuses_json,
                        recent_actions_json,
                    }))
                    .await;
                if let Ok(diagnostics) =
                    serde_json::from_str::<serde_json::Value>(&diagnostics_json)
                {
                    let _ = event_tx
                        .send(ClientEvent::StatusDiagnostics {
                            operator_profile_sync_state: diagnostics
                                .get("operator_profile_sync_state")
                                .and_then(Value::as_str)
                                .unwrap_or("unknown")
                                .to_string(),
                            operator_profile_sync_dirty: diagnostics
                                .get("operator_profile_sync_dirty")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            operator_profile_scheduler_fallback: diagnostics
                                .get("operator_profile_scheduler_fallback")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            diagnostics_json,
                        })
                        .await;
                    }
            }
            DaemonMessage::AgentStatisticsResponse { statistics_json } => {
                if let Ok(snapshot) = serde_json::from_str::<amux_protocol::AgentStatisticsSnapshot>(
                    &statistics_json,
                ) {
                    let _ = event_tx
                        .send(ClientEvent::StatisticsSnapshot(snapshot))
                        .await;
                }
            }
            DaemonMessage::AgentPromptInspection { prompt_json } => {
                if let Ok(prompt) = serde_json::from_str::<AgentPromptInspectionVm>(&prompt_json) {
                    let _ = event_tx.send(ClientEvent::PromptInspection(prompt)).await;
                }
            }
            DaemonMessage::AgentOperatorProfileSessionStarted { session_id, kind } => {
                let _ = event_tx
                    .send(ClientEvent::OperatorProfileSessionStarted { session_id, kind })
                    .await;
            }
            DaemonMessage::AgentOperatorProfileQuestion {
                session_id,
                question_id,
                field_key,
                prompt,
                input_kind,
                optional,
            } => {
                let _ = event_tx
                    .send(ClientEvent::OperatorProfileQuestion {
                        session_id,
                        question_id,
                        field_key,
                        prompt,
                        input_kind,
                        optional,
                    })
                    .await;
            }
            DaemonMessage::AgentOperatorProfileProgress {
                session_id,
                answered,
                remaining,
                completion_ratio,
            } => {
                let _ = event_tx
                    .send(ClientEvent::OperatorProfileProgress {
                        session_id,
                        answered,
                        remaining,
                        completion_ratio,
                    })
                    .await;
            }
            DaemonMessage::AgentOperatorProfileSummary { summary_json } => {
                let _ = event_tx
                    .send(ClientEvent::OperatorProfileSummary { summary_json })
                    .await;
            }
            DaemonMessage::AgentOperatorModel { model_json } => {
                let _ = event_tx
                    .send(ClientEvent::OperatorModelSummary { model_json })
                    .await;
            }
            DaemonMessage::AgentOperatorModelReset { ok } => {
                let _ = event_tx.send(ClientEvent::OperatorModelReset { ok }).await;
            }
            DaemonMessage::AgentCollaborationSessions { sessions_json } => {
                let _ = event_tx
                    .send(ClientEvent::CollaborationSessions { sessions_json })
                    .await;
            }
            DaemonMessage::AgentCollaborationVoteResult { report_json } => {
                let _ = event_tx
                    .send(ClientEvent::CollaborationVoteResult { report_json })
                    .await;
            }
            DaemonMessage::AgentGeneratedTools { tools_json } => {
                let _ = event_tx
                    .send(ClientEvent::GeneratedTools { tools_json })
                    .await;
            }
            DaemonMessage::AgentOperatorProfileSessionCompleted {
                session_id,
                updated_fields,
            } => {
                let _ = event_tx
                    .send(ClientEvent::OperatorProfileSessionCompleted {
                        session_id,
                        updated_fields,
                    })
                    .await;
            }
            DaemonMessage::GatewayBootstrap { .. }
            | DaemonMessage::GatewaySendRequest { .. }
            | DaemonMessage::GatewayReloadCommand { .. }
            | DaemonMessage::GatewayShutdownCommand { .. } => {
                debug!("Ignoring gateway runtime daemon message in TUI client");
            }
            DaemonMessage::Error { message } => {
                let _ = event_tx.send(ClientEvent::Error(message)).await;
            }
            DaemonMessage::AgentError { message } => {
                let _ = event_tx.send(ClientEvent::Error(message)).await;
            }
            _ => unreachable!("daemon message part3 should be exhaustive"),
        }
    }

}
