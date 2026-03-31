if matches!(
        &msg,
        ClientMessage::AgentValidateProvider{ .. } |
        ClientMessage::AgentSetSubAgent{ .. } |
        ClientMessage::AgentRemoveSubAgent{ .. } |
        ClientMessage::AgentListSubAgents |
        ClientMessage::AgentGetConciergeConfig |
        ClientMessage::AgentSetConciergeConfig{ .. } |
        ClientMessage::AgentRequestConciergeWelcome |
        ClientMessage::AgentDismissConciergeWelcome |
        ClientMessage::AuditQuery{ .. } |
        ClientMessage::AuditDismiss{ .. } |
        ClientMessage::EscalationCancel{ .. } |
        ClientMessage::SkillList{ .. }
    ) {
        match msg {
                ClientMessage::AgentValidateProvider {
                    provider_id,
                    base_url,
                    api_key,
                    auth_source,
                } => {
                    // Resolve credentials: if the client didn't provide them,
                    // look up stored credentials from the agent config.
                    let (resolved_url, resolved_key) = {
                        let config = agent.config.read().await;
                        let url = if base_url.is_empty() {
                            config
                                .providers
                                .get(&provider_id)
                                .map(|pc| pc.base_url.clone())
                                .filter(|u| !u.is_empty())
                                .or_else(|| {
                                    if config.provider == provider_id {
                                        Some(config.base_url.clone())
                                    } else {
                                        crate::agent::types::get_provider_definition(&provider_id)
                                            .map(|d| d.default_base_url.to_string())
                                    }
                                })
                                .unwrap_or_default()
                        } else {
                            base_url
                        };
                        let key = if api_key.is_empty() {
                            config
                                .providers
                                .get(&provider_id)
                                .map(|pc| pc.api_key.clone())
                                .filter(|k| !k.is_empty())
                                .or_else(|| {
                                    if config.provider == provider_id {
                                        Some(config.api_key.clone())
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or_default()
                        } else {
                            api_key
                        };
                        (url, key)
                    };
                    let auth_source = match auth_source.as_str() {
                        "chatgpt_subscription" => {
                            crate::agent::types::AuthSource::ChatgptSubscription
                        }
                        "github_copilot" => crate::agent::types::AuthSource::GithubCopilot,
                        _ => crate::agent::types::AuthSource::ApiKey,
                    };
                    tracing::info!(
                        provider = %provider_id,
                        url = %resolved_url,
                        has_key = !resolved_key.is_empty(),
                        "validating provider connection"
                    );
                    let (valid, error) =
                        match crate::agent::llm_client::validate_provider_connection(
                            &provider_id,
                            &resolved_url,
                            &resolved_key,
                            auth_source,
                        )
                        .await
                        {
                            Ok(_) => (true, None),
                            Err(e) => {
                                tracing::warn!(provider = %provider_id, error = %e, "provider validation failed");
                                (false, Some(e.to_string()))
                            }
                        };
                    framed
                        .send(DaemonMessage::AgentProviderValidation {
                            provider_id,
                            valid,
                            error,
                            models_json: None,
                        })
                        .await?;
                }

                ClientMessage::AgentSetSubAgent { sub_agent_json } => {
                    match serde_json::from_str(&sub_agent_json) {
                        Ok(def) => {
                            agent.set_sub_agent(def).await;
                            framed
                                .send(DaemonMessage::AgentSubAgentUpdated { sub_agent_json })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!("Invalid sub-agent: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AgentRemoveSubAgent { sub_agent_id } => {
                    agent.remove_sub_agent(&sub_agent_id).await;
                    framed
                        .send(DaemonMessage::AgentSubAgentRemoved { sub_agent_id })
                        .await?;
                }

                ClientMessage::AgentListSubAgents => {
                    let list = agent.list_sub_agents().await;
                    let json = serde_json::to_string(&list).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentSubAgentList {
                            sub_agents_json: json,
                        })
                        .await?;
                }

                ClientMessage::AgentGetConciergeConfig => {
                    let concierge = agent.get_concierge_config().await;
                    let json = serde_json::to_string(&concierge).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentConciergeConfig { config_json: json })
                        .await?;
                }

                ClientMessage::AgentSetConciergeConfig { config_json } => {
                    match serde_json::from_str::<crate::agent::types::ConciergeConfig>(&config_json)
                    {
                        Ok(concierge_config) => {
                            agent.set_concierge_config(concierge_config).await;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!("Invalid concierge config: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AgentRequestConciergeWelcome => {
                    tracing::info!("server: received AgentRequestConciergeWelcome");

                    // If first-time user (onboarding not completed), deliver tier-adapted onboarding
                    let (onboarding_done, tier) = {
                        let cfg = agent.config.read().await;
                        let done = cfg.tier.onboarding_completed;
                        let t = cfg
                            .tier
                            .user_self_assessment
                            .unwrap_or(crate::agent::capability_tier::CapabilityTier::Newcomer);
                        (done, t)
                    };
                    let mut onboarding_just_delivered = false;
                    if !onboarding_done {
                        if let Err(e) = agent
                            .concierge
                            .deliver_onboarding(tier, &agent.threads)
                            .await
                        {
                            tracing::warn!(
                                "onboarding delivery failed, falling back to generic welcome: {e}"
                            );
                        } else {
                            onboarding_just_delivered = true;
                            agent
                                .persist_thread_by_id(crate::agent::concierge::CONCIERGE_THREAD_ID)
                                .await;
                        }
                        // Mark onboarding as completed so it doesn't re-trigger on reconnect
                        {
                            let mut cfg = agent.config.write().await;
                            cfg.tier.onboarding_completed = true;
                        }
                    }

                    // Skip welcome generation if onboarding was just delivered —
                    // otherwise we'd emit two concierge messages back to back.
                    if onboarding_just_delivered {
                        continue;
                    }

                    // Generate welcome inline (awaits LLM call for non-Minimal levels).
                    let welcome = agent
                        .concierge
                        .generate_welcome(&agent.threads, &agent.tasks)
                        .await;
                    if let Some((content, detail_level, actions)) = welcome {
                        agent
                            .persist_thread_by_id(crate::agent::concierge::CONCIERGE_THREAD_ID)
                            .await;
                        let event = crate::agent::types::AgentEvent::ConciergeWelcome {
                            thread_id: crate::agent::concierge::CONCIERGE_THREAD_ID.to_string(),
                            content,
                            detail_level,
                            actions,
                        };
                        if let Some(fingerprint) = concierge_welcome_fingerprint(&event) {
                            if last_concierge_welcome_fingerprint.as_deref()
                                == Some(fingerprint.as_str())
                            {
                                tracing::info!(
                                    "server: suppressed duplicate concierge welcome for client"
                                );
                                continue;
                            }
                            last_concierge_welcome_fingerprint = Some(fingerprint);
                        }
                        if let Ok(json) = serde_json::to_string(&event) {
                            framed
                                .send(DaemonMessage::AgentEvent { event_json: json })
                                .await
                                .ok();
                        }
                    }
                }

                ClientMessage::AgentDismissConciergeWelcome => {
                    agent.concierge.prune_welcome_messages(&agent.threads).await;
                    agent
                        .persist_thread_by_id(crate::agent::concierge::CONCIERGE_THREAD_ID)
                        .await;
                    last_concierge_welcome_fingerprint = None;
                    framed
                        .send(DaemonMessage::AgentConciergeWelcomeDismissed)
                        .await?;
                }

                ClientMessage::AuditQuery {
                    action_types,
                    since,
                    limit,
                } => {
                    let action_types_ref = action_types.as_deref();
                    let since_i64 = since.map(|s| s as i64);
                    let limit = limit.unwrap_or(100);
                    match agent
                        .history
                        .list_action_audit(action_types_ref, since_i64, limit)
                        .await
                    {
                        Ok(rows) => {
                            let public_entries: Vec<amux_protocol::AuditEntryPublic> = rows
                                .into_iter()
                                .map(|r| amux_protocol::AuditEntryPublic {
                                    id: r.id,
                                    timestamp: r.timestamp,
                                    action_type: r.action_type,
                                    summary: r.summary,
                                    explanation: r.explanation,
                                    confidence: r.confidence,
                                    confidence_band: r.confidence_band,
                                    causal_trace_id: r.causal_trace_id,
                                    thread_id: r.thread_id,
                                    goal_run_id: r.goal_run_id,
                                    task_id: r.task_id,
                                })
                                .collect();
                            let entries_json =
                                serde_json::to_string(&public_entries).unwrap_or_default();
                            framed
                                .send(DaemonMessage::AuditList { entries_json })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("audit query failed: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AuditDismiss { entry_id } => {
                    tracing::info!(entry_id = %entry_id, "Audit dismiss requested");
                    let result = agent.history.dismiss_audit_entry(&entry_id).await;
                    let msg = match result {
                        Ok(()) => DaemonMessage::AuditDismissResult {
                            success: true,
                            message: format!("Dismissed audit entry {}", entry_id),
                        },
                        Err(e) => DaemonMessage::AuditDismissResult {
                            success: false,
                            message: format!("Failed to dismiss: {}", e),
                        },
                    };
                    framed.send(msg).await?;
                }

                ClientMessage::EscalationCancel { thread_id } => {
                    tracing::info!(thread_id = %thread_id, "escalation cancel requested by user (D-13)");

                    // Create an audit entry for the cancellation.
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as i64;

                    let audit_id = format!("audit-esc-cancel-{}", uuid::Uuid::new_v4());
                    let summary = format!("User cancelled escalation for thread {thread_id}");

                    let audit_entry = crate::history::AuditEntryRow {
                        id: audit_id.clone(),
                        timestamp: now_ms,
                        action_type: "escalation".to_string(),
                        summary: summary.clone(),
                        explanation: Some(summary.clone()),
                        confidence: None,
                        confidence_band: None,
                        causal_trace_id: None,
                        thread_id: Some(thread_id.clone()),
                        goal_run_id: None,
                        task_id: None,
                        raw_data_json: Some(
                            serde_json::json!({
                                "action": "cancel",
                                "thread_id": thread_id,
                                "outcome": "cancelled_by_user",
                            })
                            .to_string(),
                        ),
                    };

                    if let Err(e) = agent.history.insert_action_audit(&audit_entry).await {
                        tracing::warn!("failed to record escalation cancel audit: {e}");
                    }

                    // Broadcast EscalationUpdate event so all clients see the cancel.
                    let _ =
                        agent
                            .event_tx
                            .send(crate::agent::types::AgentEvent::EscalationUpdate {
                                thread_id: thread_id.clone(),
                                from_level: "unknown".to_string(),
                                to_level: "L0".to_string(),
                                reason: "User took over (I'll handle this)".to_string(),
                                attempts: 0,
                                audit_id: Some(audit_id.clone()),
                            });

                    // Broadcast AuditAction event.
                    let _ = agent
                        .event_tx
                        .send(crate::agent::types::AgentEvent::AuditAction {
                            id: audit_id,
                            timestamp: now_ms as u64,
                            action_type: "escalation".to_string(),
                            summary: summary.clone(),
                            explanation: Some(summary.clone()),
                            confidence: None,
                            confidence_band: None,
                            causal_trace_id: None,
                            thread_id: Some(thread_id.clone()),
                        });

                    framed
                        .send(DaemonMessage::EscalationCancelResult {
                            success: true,
                            message: format!("Escalation cancelled for thread {thread_id}. You now have control."),
                        })
                        .await?;
                }

                ClientMessage::SkillList { status, limit } => {
                    let limit = limit.clamp(1, 200);
                    let result = if let Some(ref st) = status {
                        agent.history.list_skill_variants_by_status(st, limit).await
                    } else {
                        agent.history.list_skill_variants(None, limit).await
                    };
                    match result {
                        Ok(records) => {
                            let variants: Vec<amux_protocol::SkillVariantPublic> = records
                                .into_iter()
                                .map(|r| amux_protocol::SkillVariantPublic {
                                    variant_id: r.variant_id,
                                    skill_name: r.skill_name,
                                    variant_name: r.variant_name,
                                    relative_path: r.relative_path,
                                    status: r.status,
                                    use_count: r.use_count,
                                    success_count: r.success_count,
                                    failure_count: r.failure_count,
                                    context_tags: r.context_tags,
                                    created_at: r.created_at,
                                    updated_at: r.updated_at,
                                })
                                .collect();
                            framed
                                .send(DaemonMessage::SkillListResult { variants })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("skill list failed: {e}"),
                                })
                                .await?;
                        }
                    }
                }

            _ => unreachable!("message chunk should be exhaustive"),
        }
        continue;
    }
