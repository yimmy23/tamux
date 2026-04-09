use super::*;

impl SessionManager {
    pub async fn spawn(
        &self,
        shell: Option<String>,
        cwd: Option<String>,
        workspace_id: Option<String>,
        env: Option<Vec<(String, String)>>,
        cols: u16,
        rows: u16,
    ) -> Result<(SessionId, broadcast::Receiver<DaemonMessage>)> {
        let id = Uuid::new_v4();
        let session = PtySession::spawn(
            id,
            shell,
            cwd,
            workspace_id,
            env,
            cols,
            rows,
            (*self.history).clone(),
            self.pty_channel_capacity,
        )?;
        let rx = session.subscribe();

        self.sessions
            .write()
            .await
            .insert(id, Arc::new(Mutex::new(session)));

        self.persist_state().await;

        tracing::info!(%id, "session spawned");
        Ok((id, rx))
    }

    pub async fn clone_session(
        &self,
        source_id: SessionId,
        workspace_id: Option<String>,
        cols: Option<u16>,
        rows: Option<u16>,
        replay_scrollback: bool,
        cwd_override: Option<String>,
    ) -> Result<(
        SessionId,
        broadcast::Receiver<DaemonMessage>,
        Option<String>,
    )> {
        let source = self
            .sessions
            .read()
            .await
            .get(&source_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("session not found: {source_id}"))?;

        let (
            shell,
            cwd,
            source_workspace_id,
            source_cols,
            source_rows,
            replay_bytes,
            src_active_cmd,
        ) = {
            let source = source.lock().await;
            (
                source.shell().map(ToOwned::to_owned),
                source.resolved_cwd().or(cwd_override),
                source.workspace_id().map(ToOwned::to_owned),
                source.cols(),
                source.rows(),
                if replay_scrollback {
                    source.scrollback(None)
                } else {
                    Vec::new()
                },
                source.active_command(),
            )
        };

        let target_workspace_id = workspace_id.or(source_workspace_id);
        let (id, rx) = self
            .spawn(
                shell,
                cwd,
                target_workspace_id,
                None,
                cols.unwrap_or(source_cols),
                rows.unwrap_or(source_rows),
            )
            .await?;

        if replay_scrollback && !replay_bytes.is_empty() {
            let sanitized = crate::pty_session::sanitize_scrollback_for_replay(&replay_bytes);
            if let Some(cloned_session) = self.sessions.read().await.get(&id).cloned() {
                cloned_session.lock().await.preload_output(&sanitized);
            }
        }

        tracing::info!(%source_id, %id, active_command = ?src_active_cmd, "session cloned");
        Ok((id, rx, src_active_cmd))
    }

    pub async fn write_input(&self, id: SessionId, data: &[u8]) -> Result<()> {
        let session = self
            .sessions
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("session not found: {id}"))?;
        session.lock().await.write(data)?;
        Ok(())
    }

    pub async fn resize(&self, id: SessionId, cols: u16, rows: u16) -> Result<()> {
        let session = self
            .sessions
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("session not found: {id}"))?;
        session.lock().await.resize(cols, rows)?;
        Ok(())
    }

    pub async fn kill(&self, id: SessionId) -> Result<()> {
        let session = {
            let mut sessions = self.sessions.write().await;
            sessions.remove(&id)
        };

        if let Some(session) = session {
            session.lock().await.kill()?;
            tracing::info!(%id, "session killed");
        }

        self.persist_state().await;
        Ok(())
    }

    pub async fn subscribe(
        &self,
        id: SessionId,
    ) -> Result<(broadcast::Receiver<DaemonMessage>, bool)> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {id}"))?
            .clone();
        let s = session.lock().await;
        let rx = s.subscribe();
        let alive = !s.is_dead();
        Ok((rx, alive))
    }

    pub fn read_workspace_topology(&self) -> Option<WorkspaceTopology> {
        let path = match amux_protocol::ensure_amux_data_dir() {
            Ok(dir) => dir.join("workspace-topology.json"),
            Err(_) => return None,
        };
        let data = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub async fn list(&self) -> Vec<SessionInfo> {
        self.list_filtered(None).await
    }

    pub async fn list_workspace(&self, workspace_id: &str) -> Vec<SessionInfo> {
        self.list_filtered(Some(workspace_id)).await
    }

    async fn list_filtered(&self, workspace_filter: Option<&str>) -> Vec<SessionInfo> {
        let sessions: Vec<(SessionId, Arc<Mutex<PtySession>>)> = self
            .sessions
            .read()
            .await
            .iter()
            .map(|(id, session)| (*id, session.clone()))
            .collect();

        let mut infos = Vec::with_capacity(sessions.len());
        for (id, session) in sessions {
            let s = session.lock().await;
            if s.is_dead() {
                continue;
            }
            let workspace_id = s.workspace_id().map(ToOwned::to_owned);

            if let Some(filter) = workspace_filter {
                if workspace_id.as_deref() != Some(filter) {
                    continue;
                }
            }

            infos.push(SessionInfo {
                id,
                title: s.title().map(|t| t.to_owned()),
                cwd: s.resolved_cwd().or_else(|| s.cwd().map(|c| c.to_owned())),
                cols: s.cols(),
                rows: s.rows(),
                created_at: s.created_at(),
                workspace_id,
                exit_code: None,
                is_alive: !s.is_dead(),
                active_command: s.active_command(),
            });
        }
        infos
    }

    pub async fn get_scrollback(&self, id: SessionId, max_lines: Option<usize>) -> Result<Vec<u8>> {
        let session = self
            .sessions
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("session not found: {id}"))?;
        let data = session.lock().await.scrollback(max_lines);
        Ok(data)
    }

    pub async fn get_analysis_text(
        &self,
        id: SessionId,
        max_lines: Option<usize>,
    ) -> Result<String> {
        let raw = self.get_scrollback(id, max_lines).await?;
        let stripped = strip_ansi_escapes::strip(&raw);
        Ok(String::from_utf8_lossy(&stripped).into_owned())
    }

    pub async fn get_background_task_status(
        &self,
        background_task_id: &str,
    ) -> Result<Option<BackgroundTaskStatus>> {
        let sessions: Vec<(SessionId, Arc<Mutex<PtySession>>)> = self
            .sessions
            .read()
            .await
            .iter()
            .map(|(id, session)| (*id, session.clone()))
            .collect();

        for (session_id, session) in sessions {
            let live_status = session
                .lock()
                .await
                .managed_command_status(background_task_id);
            if let Some(live_status) = live_status {
                let state = match live_status.state {
                    crate::pty_session::ManagedCommandLiveState::Queued => {
                        BackgroundTaskState::Queued
                    }
                    crate::pty_session::ManagedCommandLiveState::Running => {
                        BackgroundTaskState::Running
                    }
                };
                return Ok(Some(BackgroundTaskStatus {
                    background_task_id: background_task_id.to_string(),
                    kind: "managed_command".to_string(),
                    state,
                    session_id: Some(session_id.to_string()),
                    position: Some(live_status.position),
                    command: Some(live_status.command),
                    exit_code: None,
                    duration_ms: None,
                    snapshot_path: live_status.snapshot_path,
                }));
            }
        }

        if let Some(finished) = self.history.get_managed_finish(background_task_id).await? {
            let state = if finished.exit_code == Some(0) {
                BackgroundTaskState::Completed
            } else {
                BackgroundTaskState::Failed
            };
            return Ok(Some(BackgroundTaskStatus {
                background_task_id: background_task_id.to_string(),
                kind: "managed_command".to_string(),
                state,
                session_id: None,
                position: None,
                command: Some(finished.command),
                exit_code: finished.exit_code,
                duration_ms: finished.duration_ms,
                snapshot_path: finished.snapshot_path,
            }));
        }

        Ok(None)
    }

    pub async fn reap_dead(&self) {
        let mut sessions = self.sessions.write().await;
        let mut dead: Vec<SessionId> = Vec::new();
        for (id, session) in sessions.iter() {
            if session.lock().await.is_dead() {
                dead.push(*id);
            }
        }
        for id in &dead {
            sessions.remove(id);
            tracing::info!(%id, "reaped dead session");
        }

        drop(sessions);
        self.persist_state().await;
    }

    pub async fn execute_managed_command(
        &self,
        id: SessionId,
        request: ManagedCommandRequest,
    ) -> Result<DaemonMessage> {
        validate_command(&request.command, request.language_hint.as_deref())?;

        let session = self
            .sessions
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("session not found: {id}"))?;
        if session.lock().await.is_dead() {
            anyhow::bail!("session is not alive: {id}");
        }

        let workspace_id = session.lock().await.workspace_id().map(ToOwned::to_owned);
        let execution_id = format!("exec_{}", Uuid::new_v4());

        match evaluate_command(execution_id.clone(), &request, workspace_id.clone()) {
            PolicyDecision::Allow => {
                let snapshot = {
                    let session = session.lock().await;
                    self.snapshots
                        .create_snapshot(
                            workspace_id.clone(),
                            Some(id),
                            request.cwd.as_deref().or_else(|| session.cwd()),
                            Some(&request.command),
                            "pre-execution checkpoint",
                        )
                        .await?
                };
                let position = session.lock().await.queue_managed_command(
                    execution_id.clone(),
                    request,
                    snapshot.clone(),
                )?;
                Ok(DaemonMessage::ManagedCommandQueued {
                    id,
                    execution_id,
                    position,
                    snapshot,
                })
            }
            PolicyDecision::RequireApproval(approval) => {
                self.pending_approvals.write().await.insert(
                    approval.approval_id.clone(),
                    PendingApproval {
                        session_id: id,
                        workspace_id,
                        execution_id,
                        request,
                    },
                );
                Ok(DaemonMessage::ApprovalRequired { id, approval })
            }
        }
    }

    pub async fn resolve_approval(
        &self,
        id: SessionId,
        approval_id: &str,
        decision: ApprovalDecision,
    ) -> Result<Vec<DaemonMessage>> {
        let pending = self
            .pending_approvals
            .write()
            .await
            .remove(approval_id)
            .ok_or_else(|| anyhow::anyhow!("approval not found: {approval_id}"))?;

        let mut responses = vec![DaemonMessage::ApprovalResolved {
            id,
            approval_id: approval_id.to_string(),
            decision,
        }];

        if matches!(decision, ApprovalDecision::Deny) {
            responses.push(DaemonMessage::ManagedCommandRejected {
                id,
                execution_id: Some(pending.execution_id),
                message: "execution denied by operator".to_string(),
            });
            return Ok(responses);
        }

        let session = self
            .sessions
            .read()
            .await
            .get(&pending.session_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("session not found: {}", pending.session_id))?;
        let snapshot = {
            let session = session.lock().await;
            self.snapshots
                .create_snapshot(
                    pending.workspace_id.clone(),
                    Some(pending.session_id),
                    pending.request.cwd.as_deref().or_else(|| session.cwd()),
                    Some(&pending.request.command),
                    "approved pre-execution checkpoint",
                )
                .await?
        };
        let position = session.lock().await.queue_managed_command(
            pending.execution_id.clone(),
            pending.request,
            snapshot.clone(),
        )?;
        responses.push(DaemonMessage::ManagedCommandQueued {
            id,
            execution_id: pending.execution_id,
            position,
            snapshot,
        });
        Ok(responses)
    }

    pub fn verify_telemetry_integrity(&self) -> Result<Vec<TelemetryLedgerStatus>> {
        let results = self.history.verify_worm_integrity()?;
        Ok(results
            .into_iter()
            .map(|r| TelemetryLedgerStatus {
                kind: r.kind,
                total_entries: r.total_entries,
                valid: r.valid,
                first_invalid_seq: r.first_invalid_seq,
                message: r.message,
            })
            .collect())
    }

    async fn snapshot_saved_sessions(&self) -> Vec<SavedSession> {
        let sessions: Vec<Arc<Mutex<PtySession>>> =
            self.sessions.read().await.values().cloned().collect();

        let mut saved = Vec::with_capacity(sessions.len());
        for session in sessions {
            let session = session.lock().await;
            saved.push(SavedSession {
                id: session.id().to_string(),
                shell: session.shell().map(ToOwned::to_owned),
                cwd: session.cwd().map(ToOwned::to_owned),
                workspace_id: session.workspace_id().map(ToOwned::to_owned),
                cols: session.cols(),
                rows: session.rows(),
            });
        }

        saved
    }

    async fn persist_state(&self) {
        let previous_sessions = self.snapshot_saved_sessions().await;
        let state = DaemonState { previous_sessions };
        if let Err(error) = save_state(&self.state_path, &state) {
            tracing::error!(error = %error, path = %self.state_path.display(), "failed to persist daemon state");
        }
    }
}
