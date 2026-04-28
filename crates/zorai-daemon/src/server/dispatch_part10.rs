if matches!(
        &msg,
        ClientMessage::AgentWhatsAppLinkStart |
        ClientMessage::AgentWhatsAppLinkStop |
        ClientMessage::AgentWhatsAppLinkReset |
        ClientMessage::AgentWhatsAppLinkStatus |
        ClientMessage::AgentWhatsAppLinkSubscribe |
        ClientMessage::AgentWhatsAppLinkUnsubscribe
    ) {
        match msg {
                ClientMessage::AgentWhatsAppLinkStart => {
                    tracing::info!("whatsapp link start requested by client");
                    match agent.whatsapp_link.start_if_idle().await {
                        Ok(_started) => {
                            #[cfg(not(test))]
                            {
                                if _started {
                                    if let Err(e) = start_whatsapp_link_backend(agent.clone()).await
                                    {
                                        agent
                                            .whatsapp_link
                                            .broadcast_error(e.to_string(), false)
                                            .await;
                                    }
                                }
                            }
                            let snapshot = agent.whatsapp_link.status_snapshot().await;
                            framed
                                .send(DaemonMessage::AgentWhatsAppLinkStatus {
                                    state: snapshot.state,
                                    phone: snapshot.phone,
                                    last_error: snapshot.last_error,
                                })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentWhatsAppLinkError {
                                    message: e.to_string(),
                                    recoverable: false,
                                })
                                .await?;
                        }
                    }
                }
                ClientMessage::AgentWhatsAppLinkStop => {
                    match agent
                        .whatsapp_link
                        .stop(Some("operator_cancelled".to_string()))
                        .await
                    {
                        Ok(()) => {
                            let snapshot = agent.whatsapp_link.status_snapshot().await;
                            framed
                                .send(DaemonMessage::AgentWhatsAppLinkStatus {
                                    state: snapshot.state,
                                    phone: snapshot.phone,
                                    last_error: snapshot.last_error,
                                })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentWhatsAppLinkError {
                                    message: e.to_string(),
                                    recoverable: false,
                                })
                                .await?;
                        }
                    }
                }
                ClientMessage::AgentWhatsAppLinkReset => {
                    tracing::info!("whatsapp link reset requested by client");
                    match agent.whatsapp_link.reset().await {
                        Ok(()) => {
                            if let Err(e) = crate::agent::clear_persisted_provider_state(
                                &agent.history,
                                crate::agent::WHATSAPP_LINK_PROVIDER_ID,
                            )
                            .await
                            {
                                framed
                                    .send(DaemonMessage::AgentWhatsAppLinkError {
                                        message: format!(
                                            "failed to clear whatsapp provider state: {e}"
                                        ),
                                        recoverable: false,
                                    })
                                    .await?;
                                continue;
                            }
                            let native_store_path =
                                crate::agent::whatsapp_native_store_path(&agent.data_dir);
                            if native_store_path.exists() {
                                tracing::info!(
                                    path = %native_store_path.display(),
                                    "whatsapp link reset removing native store"
                                );
                                if let Err(e) = tokio::fs::remove_file(&native_store_path).await {
                                    framed
                                        .send(DaemonMessage::AgentWhatsAppLinkError {
                                            message: format!(
                                                "failed to remove native whatsapp store {}: {e}",
                                                native_store_path.display()
                                            ),
                                            recoverable: false,
                                        })
                                        .await?;
                                    continue;
                                }
                            }
                            framed
                                .send(DaemonMessage::AgentWhatsAppLinkReset {
                                    ok: true,
                                    message: Some("reset".to_string()),
                                })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentWhatsAppLinkError {
                                    message: e.to_string(),
                                    recoverable: false,
                                })
                                .await?;
                        }
                    }
                }
                ClientMessage::AgentWhatsAppLinkStatus => {
                    let snapshot = agent.whatsapp_link.status_snapshot().await;
                    framed
                        .send(DaemonMessage::AgentWhatsAppLinkStatus {
                            state: snapshot.state,
                            phone: snapshot.phone,
                            last_error: snapshot.last_error,
                        })
                        .await?;
                }
                ClientMessage::AgentWhatsAppLinkSubscribe => {
                    let (subscriber_id, rx) = agent.whatsapp_link.subscribe_with_id().await;
                    whatsapp_link_subscriber_guard.set(subscriber_id).await;
                    whatsapp_link_rx = Some(rx);
                    let snapshot = agent.whatsapp_link.status_snapshot().await;
                    framed
                        .send(DaemonMessage::AgentWhatsAppLinkStatus {
                            state: snapshot.state,
                            phone: snapshot.phone,
                            last_error: snapshot.last_error,
                        })
                        .await?;
                    whatsapp_link_snapshot_replayed = true;
                }
                ClientMessage::AgentWhatsAppLinkUnsubscribe => {
                    whatsapp_link_subscriber_guard.clear().await;
                    whatsapp_link_rx = None;
                    whatsapp_link_snapshot_replayed = false;
                }
            _ => unreachable!("message chunk should be exhaustive"),
        }
        continue;
    }
