fn managed_alias_args(args: &serde_json::Value, fallback_rationale: &str) -> serde_json::Value {
    let command = args
        .get("command")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let rationale = args
        .get("rationale")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback_rationale);

    let mut mapped = serde_json::Map::new();
    mapped.insert(
        "command".to_string(),
        serde_json::Value::String(command.to_string()),
    );
    mapped.insert(
        "rationale".to_string(),
        serde_json::Value::String(rationale.to_string()),
    );

    for key in [
        "session",
        "cwd",
        "allow_network",
        "sandbox_enabled",
        "security_level",
        "language_hint",
        "wait_for_completion",
        "timeout_seconds",
    ] {
        if let Some(value) = args.get(key) {
            mapped.insert(key.to_string(), value.clone());
        }
    }
    serde_json::Value::Object(mapped)
}

async fn execute_managed_command(
    args: &serde_json::Value,
    agent: &AgentEngine,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
    thread_id: &str,
    cancel_token: Option<CancellationToken>,
) -> Result<(String, Option<ToolPendingApproval>)> {
    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'command' argument"))?;
    let rationale = args
        .get("rationale")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'rationale' argument"))?;

    let sessions = session_manager.list().await;
    if sessions.is_empty() {
        anyhow::bail!("No active terminal sessions are available for managed execution");
    }

    let resolved_session_id =
        if let Some(session_ref) = args.get("session").and_then(|v| v.as_str()) {
            sessions
                .iter()
                .find(|session| {
                    session.id.to_string() == session_ref
                        || session.id.to_string().contains(session_ref)
                })
                .map(|session| session.id)
                .ok_or_else(|| anyhow::anyhow!("session not found: {session_ref}"))?
        } else {
            session_id.unwrap_or(sessions[0].id)
        };

    let default_managed_execution = agent.config.read().await.managed_execution.clone();
    let security_level = match args
        .get("security_level")
        .and_then(|value| value.as_str())
        .unwrap_or(match default_managed_execution.security_level {
            SecurityLevel::Highest => "highest",
            SecurityLevel::Moderate => "moderate",
            SecurityLevel::Lowest => "lowest",
            SecurityLevel::Yolo => "yolo",
        }) {
        "highest" => SecurityLevel::Highest,
        "lowest" => SecurityLevel::Lowest,
        "yolo" => SecurityLevel::Yolo,
        _ => SecurityLevel::Moderate,
    };
    let requested_timeout = args
        .get("timeout_seconds")
        .and_then(|value| value.as_u64())
        .unwrap_or(30);
    let timeout_secs = requested_timeout.min(600);
    // Auto-background: if requested timeout exceeds max, run in background with monitoring
    let auto_background = requested_timeout > 600;
    let wait_for_completion = if auto_background {
        false
    } else {
        args.get("wait_for_completion")
            .and_then(|value| value.as_bool())
            .unwrap_or(true)
    };
    let mut wait_rx = if wait_for_completion {
        Some(session_manager.subscribe(resolved_session_id).await?.0)
    } else {
        None
    };

    let request = ManagedCommandRequest {
        command: command.to_string(),
        rationale: rationale.to_string(),
        allow_network: args
            .get("allow_network")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        sandbox_enabled: args
            .get("sandbox_enabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(default_managed_execution.sandbox_enabled),
        security_level,
        cwd: args
            .get("cwd")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        language_hint: args
            .get("language_hint")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        source: ManagedCommandSource::Agent,
    };

    let response = match session_manager
        .execute_managed_command(resolved_session_id, request)
        .await?
    {
        DaemonMessage::ApprovalRequired { mut approval, .. } => {
            if let Some(advisory) = agent
                .command_blast_radius_advisory("execute_managed_command", command)
                .await
            {
                approval
                    .reasons
                    .push(format!("causal history: {}", advisory.evidence));
                for reason in advisory.recent_reasons.iter().take(2) {
                    approval.reasons.push(format!(
                        "recent related issue: {}",
                        crate::agent::summarize_text(reason, 120)
                    ));
                }
                if approval.risk_level == "medium" && advisory.risk_level == "high" {
                    approval.risk_level = "high".to_string();
                }
                if !approval.blast_radius.contains("historical") {
                    approval.blast_radius =
                        format!("{} + historical {}", approval.blast_radius, advisory.family);
                }
            }

            let pending_approval = ToolPendingApproval {
                approval_id: approval.approval_id,
                execution_id: approval.execution_id,
                command: approval.command,
                rationale: approval.rationale,
                risk_level: approval.risk_level,
                blast_radius: approval.blast_radius,
                reasons: approval.reasons,
                session_id: Some(resolved_session_id.to_string()),
            };
            let command_category = crate::agent::classify_command_category(
                &pending_approval.command,
                &pending_approval.risk_level,
            )
            .to_string();
            agent
                .remember_pending_approval_command(&pending_approval)
                .await;

            if agent
                .mark_task_approval_rule_used(&pending_approval.command)
                .await
            {
                agent
                    .record_operator_approval_requested(&pending_approval)
                    .await?;
                let responses = session_manager
                    .resolve_approval(
                        resolved_session_id,
                        &pending_approval.approval_id,
                        amux_protocol::ApprovalDecision::ApproveOnce,
                    )
                    .await?;
                agent
                    .record_operator_approval_resolution(
                        &pending_approval.approval_id,
                        amux_protocol::ApprovalDecision::ApproveOnce,
                    )
                    .await?;
                responses
                    .into_iter()
                    .find(|message| matches!(message, DaemonMessage::ManagedCommandQueued { .. }))
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "managed command auto-approved by saved rule but queue response was missing"
                        )
                    })?
            } else {
                match agent
                    .learned_approval_decision(
                        &pending_approval.command,
                        &pending_approval.risk_level,
                    )
                    .await
                {
                    Some(amux_protocol::ApprovalDecision::ApproveOnce)
                    | Some(amux_protocol::ApprovalDecision::ApproveSession) => {
                        agent
                            .record_operator_approval_requested(&pending_approval)
                            .await?;
                        let responses = session_manager
                            .resolve_approval(
                                resolved_session_id,
                                &pending_approval.approval_id,
                                amux_protocol::ApprovalDecision::ApproveOnce,
                            )
                            .await?;
                        agent
                            .record_operator_approval_resolution(
                                &pending_approval.approval_id,
                                amux_protocol::ApprovalDecision::ApproveOnce,
                            )
                            .await?;
                        responses
                            .into_iter()
                            .find(|message| {
                                matches!(message, DaemonMessage::ManagedCommandQueued { .. })
                            })
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "managed command auto-approved but queue response was missing"
                                )
                            })?
                    }
                    Some(amux_protocol::ApprovalDecision::Deny) => {
                        agent
                            .record_operator_approval_requested(&pending_approval)
                            .await?;
                        let responses = session_manager
                            .resolve_approval(
                                resolved_session_id,
                                &pending_approval.approval_id,
                                amux_protocol::ApprovalDecision::Deny,
                            )
                            .await?;
                        agent
                            .record_operator_approval_resolution(
                                &pending_approval.approval_id,
                                amux_protocol::ApprovalDecision::Deny,
                            )
                            .await?;
                        let rejection_message = responses
                            .iter()
                            .find_map(|message| match message {
                                DaemonMessage::ManagedCommandRejected { message, .. } => {
                                    Some(message.clone())
                                }
                                _ => None,
                            })
                            .unwrap_or_else(|| {
                                "execution denied by learned operator policy".to_string()
                            });
                        return Ok((
                        format!(
                            "Managed command auto-denied by learned operator policy for category {}. {}",
                            command_category, rejection_message
                        ),
                        None,
                    ));
                    }
                    None => {
                        return Ok((
                            format!(
                                "Managed command requires approval before execution. Approval ID: {}\nRisk: {}\nBlast radius: {}\nCommand: {}\nReasons:\n- {}",
                                pending_approval.approval_id,
                                pending_approval.risk_level,
                                pending_approval.blast_radius,
                                pending_approval.command,
                                pending_approval.reasons.join("\n- "),
                            ),
                            Some(pending_approval),
                        ));
                    }
                }
            }
        }
        other => other,
    };

    match response {
        DaemonMessage::ManagedCommandQueued {
            execution_id,
            position,
            snapshot,
            ..
        } => {
            let snapshot_suffix = snapshot
                .as_ref()
                .map(|item| format!(" (snapshot: {})", item.snapshot_id))
                .unwrap_or_default();
            let queued_summary = format!(
                "Managed command queued in session {} as {} at lane position {}{}",
                resolved_session_id, execution_id, position, snapshot_suffix
            );

            if !wait_for_completion {
                // Spawn background monitor if auto-backgrounded due to high timeout
                if auto_background {
                    let sm = session_manager.clone();
                    let sid = resolved_session_id.clone();
                    let eid = execution_id.clone();
                    let etx = event_tx.clone();
                    let tid = thread_id.to_string();
                    let monitor_timeout = requested_timeout;
                    tokio::spawn(async move {
                        if let Ok((rx, _)) = sm.subscribe(sid).await {
                            let mut rx = rx;
                            match wait_for_managed_command_outcome(
                                &mut rx,
                                sid,
                                &eid,
                                monitor_timeout,
                                None,
                            )
                            .await
                            {
                                Ok(ManagedCommandWaitOutcome::Finished {
                                    exit_code,
                                    duration_ms,
                                    output_tail,
                                }) => {
                                    let timing = duration_ms
                                        .map(|v| format!(" in {}ms", v))
                                        .unwrap_or_default();
                                    let status = if exit_code == Some(0) {
                                        "completed successfully"
                                    } else {
                                        "failed"
                                    };
                                    let msg = format!(
                                        "Background command {} {}{} (exit_code: {:?})\n\nOutput (tail):\n{}",
                                        eid, status, timing, exit_code, output_tail
                                    );
                                    let _ = etx.send(AgentEvent::Delta {
                                        thread_id: tid.clone(),
                                        content: format!("\n\n[Background monitor] {msg}"),
                                    });
                                    let _ = etx.send(AgentEvent::WorkflowNotice {
                                        thread_id: tid,
                                        kind: "background-command-finished".to_string(),
                                        message: msg,
                                        details: None,
                                    });
                                }
                                Ok(ManagedCommandWaitOutcome::Timeout { output_tail }) => {
                                    let _ = etx.send(AgentEvent::WorkflowNotice {
                                        thread_id: tid,
                                        kind: "background-command-timeout".to_string(),
                                        message: format!(
                                            "Background command {} still running after {}s. Last output:\n{}",
                                            eid, monitor_timeout, output_tail
                                        ),
                                        details: None,
                                    });
                                }
                                _ => {}
                            }
                        }
                    });
                    return Ok((
                        format!(
                            "{queued_summary}\nbackground_task_id: {execution_id}\noperation_id: {execution_id}\nCommand auto-backgrounded (requested timeout {}s > max 600s). \
                             A background monitor will notify this thread when the command completes. Use get_operation_status with this operation_id for explicit polling. `get_background_task_status` remains available as a compatibility alias.",
                            requested_timeout,
                        ),
                        None,
                    ));
                }
                return Ok((
                    format!(
                        "{queued_summary}\nbackground_task_id: {execution_id}\noperation_id: {execution_id}\nNot waiting for completion because wait_for_completion=false. Use get_operation_status with this operation_id for explicit polling. `get_background_task_status` remains available as a compatibility alias."
                    ),
                    None,
                ));
            }

            let Some(ref mut rx) = wait_rx else {
                return Ok((queued_summary, None));
            };

            match wait_for_managed_command_outcome(
                rx,
                resolved_session_id,
                &execution_id,
                timeout_secs,
                cancel_token.as_ref(),
            )
            .await?
            {
                ManagedCommandWaitOutcome::Finished {
                    exit_code,
                    duration_ms,
                    output_tail,
                } => {
                    let timing = duration_ms
                        .map(|value| format!(" in {}ms", value))
                        .unwrap_or_default();
                    if exit_code == Some(0) {
                        let output_section = if output_tail.trim().is_empty() {
                            String::new()
                        } else {
                            format!("\n\nTerminal output (tail):\n{output_tail}")
                        };
                        Ok((
                            format!(
                                "Managed command finished{timing} in session {} (execution_id: {}, exit_code: 0).{}",
                                resolved_session_id, execution_id, output_section
                            ),
                            None,
                        ))
                    } else {
                        let output_section = if output_tail.trim().is_empty() {
                            String::new()
                        } else {
                            format!("\n\nTerminal output (tail):\n{output_tail}")
                        };
                        Err(anyhow::anyhow!(
                            "Managed command failed in session {} (execution_id: {}, exit_code: {:?}).{}",
                            resolved_session_id,
                            execution_id,
                            exit_code,
                            output_section
                        ))
                    }
                }
                ManagedCommandWaitOutcome::Rejected { message } => Err(anyhow::anyhow!(
                    "Managed command rejected after queueing (execution_id: {}): {}",
                    execution_id,
                    message
                )),
                ManagedCommandWaitOutcome::Timeout { output_tail } => {
                    let output_section = if output_tail.trim().is_empty() {
                        String::new()
                    } else {
                        format!("\n\nTerminal output so far (tail):\n{output_tail}")
                    };
                    Err(anyhow::anyhow!(
                        "{queued_summary}\nManaged command is still running after {}s in session {}. Do not reuse this terminal for additional blocking work. Continue monitoring this execution_id or switch to another terminal/session before proceeding. If you need another lane in the same workspace, call allocate_terminal first.{}",
                        timeout_secs,
                        resolved_session_id,
                        output_section
                    ))
                }
            }
        }
        other => Err(anyhow::anyhow!(
            "unexpected managed command response: {}",
            serde_json::to_string(&other).unwrap_or_else(|_| "<unserializable>".to_string())
        )),
    }
}

async fn execute_get_background_task_status(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
) -> Result<String> {
    let background_task_id = args
        .get("background_task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'background_task_id' argument"))?;

    execute_operation_status_lookup(background_task_id, session_manager, true).await
}

async fn execute_get_operation_status(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
) -> Result<String> {
    let operation_id = args
        .get("operation_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'operation_id' argument"))?;

    execute_operation_status_lookup(operation_id, session_manager, false).await
}

async fn execute_operation_status_lookup(
    operation_id: &str,
    session_manager: &Arc<SessionManager>,
    compatibility_alias: bool,
) -> Result<String> {
    if let Some(status) = session_manager
        .get_background_task_status(operation_id)
        .await?
    {
        let mut payload = serde_json::json!({
            "operation_id": status.background_task_id,
            "kind": status.kind,
            "state": status.state,
            "background_task_id": operation_id,
        });

        if let Some(session_id) = status.session_id {
            payload["session_id"] = serde_json::Value::String(session_id);
        }
        if let Some(position) = status.position {
            payload["position"] = serde_json::Value::Number(position.into());
        }
        if let Some(command) = status.command {
            payload["command"] = serde_json::Value::String(command);
        }
        if let Some(exit_code) = status.exit_code {
            payload["exit_code"] = serde_json::Value::Number(exit_code.into());
        }
        if let Some(duration_ms) = status.duration_ms {
            payload["duration_ms"] = serde_json::Value::Number(duration_ms.into());
        }
        if let Some(snapshot_path) = status.snapshot_path {
            payload["snapshot_path"] = serde_json::Value::String(snapshot_path);
        }
        if !compatibility_alias {
            payload
                .as_object_mut()
                .map(|obj| obj.remove("background_task_id"));
        }

        return Ok(payload.to_string());
    }

    if let Some(snapshot) = crate::server::operation_registry().snapshot(operation_id) {
        let mut payload = serde_json::json!({
            "operation_id": snapshot.operation_id,
            "kind": snapshot.kind,
            "state": snapshot.state,
            "revision": snapshot.revision,
        });
        if let Some(dedup) = snapshot.dedup {
            payload["dedup"] = serde_json::Value::String(dedup);
        }
        if let Some(terminal_result) =
            crate::server::operation_registry().terminal_result(operation_id)
        {
            if let Some(exit_code) = terminal_result
                .get("exit_code")
                .and_then(|value| value.as_i64())
            {
                payload["exit_code"] = serde_json::Value::Number(exit_code.into());
            }
            payload["terminal_result"] = terminal_result;
        } else if matches!(
            payload["kind"].as_str(),
            Some("bash_command" | "run_terminal_command")
        ) && matches!(payload["state"].as_str(), Some("accepted" | "started"))
        {
            payload["status_hint"] = serde_json::Value::String(
                "Final terminal payload will appear under `terminal_result` once this background headless command reaches completed or failed. Do not rerun it in foreground just to inspect output.".to_string(),
            );
        }
        if compatibility_alias {
            payload["background_task_id"] = serde_json::Value::String(operation_id.to_string());
        }
        return Ok(payload.to_string());
    }

    Err(anyhow::anyhow!("unknown operation id: {operation_id}"))
}
