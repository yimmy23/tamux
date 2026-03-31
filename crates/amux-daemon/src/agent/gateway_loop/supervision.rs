use super::*;

impl AgentEngine {
    pub(super) async fn reset_gateway_restart_backoff(&self) {
        let mut control = gateway_runtime_control().lock().await;
        control.restart_attempts = 0;
        control.restart_not_before_ms = None;
    }

    pub(super) async fn schedule_gateway_restart_backoff(&self, reason: &str) {
        let mut control = gateway_runtime_control().lock().await;
        control.restart_attempts = control.restart_attempts.saturating_add(1);
        let delay_secs = crate::agent::liveness::recovery::RecoveryPlanner::default()
            .compute_backoff_secs(control.restart_attempts.saturating_sub(1));
        let next_restart_at = now_millis().saturating_add(delay_secs.saturating_mul(1000));
        control.restart_not_before_ms = Some(next_restart_at);
        tracing::warn!(
            attempts = control.restart_attempts,
            delay_secs,
            next_restart_at,
            %reason,
            "gateway: scheduled restart backoff"
        );
    }

    pub(super) async fn spawn_gateway_process_at(
        &self,
        gateway_path: &std::path::Path,
    ) -> Result<()> {
        let mut proc = self.gateway_process.lock().await;
        if let Some(ref mut child) = *proc {
            let _ = child.kill().await;
        }
        *proc = None;

        tracing::info!(?gateway_path, "spawning gateway process");
        let mut cmd = tokio::process::Command::new(gateway_path);
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        match cmd.spawn() {
            Ok(child) => {
                tracing::info!(pid = ?child.id(), "gateway process started");
                *proc = Some(child);
                drop(proc);
                self.reset_gateway_restart_backoff().await;
                self.clear_gateway_ipc_sender().await;
                Ok(())
            }
            Err(error) => {
                tracing::error!(error = %error, "failed to spawn gateway process");
                drop(proc);
                self.schedule_gateway_restart_backoff("gateway spawn failed")
                    .await;
                Err(error.into())
            }
        }
    }

    pub async fn maybe_spawn_gateway(&self) {
        let config = self.config.read().await.clone();
        let gw = &config.gateway;
        let slack_token = gw.slack_token.clone();
        let telegram_token = gw.telegram_token.clone();
        let discord_token = gw.discord_token.clone();

        self.init_gateway().await;

        if slack_token.is_empty() && telegram_token.is_empty() && discord_token.is_empty() {
            tracing::info!("gateway: no platform tokens configured, skipping");
            return;
        }

        let gateway_path_opt = std::env::current_exe().ok().and_then(|p| {
            let dir = p.parent()?;
            let name = if cfg!(windows) {
                "tamux-gateway.exe"
            } else {
                "tamux-gateway"
            };
            let path = dir.join(name);
            if path.exists() {
                Some(path)
            } else {
                None
            }
        });

        let gateway_path = match gateway_path_opt {
            Some(p) => p,
            None => {
                tracing::warn!("gateway binary not found next to daemon executable");
                return;
            }
        };

        if let Err(error) = self.spawn_gateway_process_at(&gateway_path).await {
            tracing::error!(error = %error, "failed to spawn gateway process");
        }
    }

    pub async fn stop_gateway(&self) {
        if let Some(sender) = self.gateway_ipc_sender.lock().await.clone() {
            let _ = sender.send(amux_protocol::DaemonMessage::GatewayShutdownCommand {
                command: amux_protocol::GatewayShutdownCommand {
                    correlation_id: format!("gateway-shutdown-{}", uuid::Uuid::new_v4()),
                    reason: Some("daemon shutdown".to_string()),
                    requested_at_ms: now_millis(),
                },
            });
        }

        let mut proc = self.gateway_process.lock().await;
        if let Some(ref mut child) = *proc {
            tracing::info!("stopping gateway process");
            let _ = child.kill().await;
        }
        *proc = None;
        drop(proc);
        self.clear_gateway_ipc_sender().await;
        self.reset_gateway_restart_backoff().await;
    }

    pub(crate) async fn request_gateway_reload(&self, reason: Option<String>) -> Result<bool> {
        let sender = self.gateway_ipc_sender.lock().await.clone();
        let Some(sender) = sender else {
            return Ok(false);
        };
        sender
            .send(amux_protocol::DaemonMessage::GatewayReloadCommand {
                command: amux_protocol::GatewayReloadCommand {
                    correlation_id: format!("gateway-reload-{}", uuid::Uuid::new_v4()),
                    reason,
                    requested_at_ms: now_millis(),
                },
            })
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        Ok(true)
    }

    pub(crate) async fn record_gateway_ipc_loss(&self, reason: &str) {
        tracing::warn!(reason, "gateway: ipc connection lost");
        let mut proc = self.gateway_process.lock().await;
        if let Some(ref mut child) = *proc {
            let _ = child.kill().await;
        }
        *proc = None;
        drop(proc);
        self.clear_gateway_ipc_sender().await;
        self.schedule_gateway_restart_backoff(reason).await;
    }

    #[cfg(test)]
    pub(crate) async fn maybe_spawn_gateway_with_path(
        &self,
        gateway_path: &std::path::Path,
    ) -> Result<()> {
        self.init_gateway().await;
        self.spawn_gateway_process_at(gateway_path).await
    }

    #[cfg(test)]
    pub(crate) async fn reinit_gateway_with_path(
        &self,
        gateway_path: &std::path::Path,
    ) -> Result<()> {
        let config = self.config.read().await.clone();
        let gw = &config.gateway;

        if !gw.enabled {
            *self.gateway_state.lock().await = None;
            *self.gateway_discord_channels.write().await = Vec::new();
            *self.gateway_slack_channels.write().await = Vec::new();
            self.stop_gateway().await;
            return Ok(());
        }

        *self.gateway_state.lock().await = None;
        *self.gateway_discord_channels.write().await = Vec::new();
        *self.gateway_slack_channels.write().await = Vec::new();
        let _ = self
            .request_gateway_reload(Some("config reloaded".to_string()))
            .await?;
        self.maybe_spawn_gateway_with_path(gateway_path).await
    }
}
