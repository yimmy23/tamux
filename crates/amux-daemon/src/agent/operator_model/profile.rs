use super::*;

impl AgentEngine {
    pub(crate) async fn refresh_operator_model(&self) -> Result<()> {
        if !self.config.read().await.operator_model.enabled {
            *self.operator_model.write().await = OperatorModel::default();
            self.active_operator_sessions.write().await.clear();
            self.pending_operator_approvals.write().await.clear();
            return Ok(());
        }
        ensure_operator_model_file(&self.data_dir).await?;
        let raw = tokio::fs::read_to_string(operator_model_path(&self.data_dir)).await?;
        let mut parsed = serde_json::from_str::<OperatorModel>(&raw).unwrap_or_default();
        let samples = self.history.list_recent_implicit_signal_samples(64).await?;
        let persisted_samples = samples
            .into_iter()
            .map(|(weight, timestamp_ms)| PersistedSatisfactionSignalSample {
                weight,
                timestamp_ms,
            })
            .collect::<Vec<_>>();
        let now = now_millis();
        if !apply_persisted_satisfaction_decay(&mut parsed, &persisted_samples, now) {
            refresh_operator_satisfaction(&mut parsed);
        }
        *self.operator_model.write().await = parsed;
        Ok(())
    }

    pub async fn operator_model_json(&self) -> Result<String> {
        if !self.config.read().await.operator_model.enabled {
            return Ok(serde_json::to_string_pretty(
                &*self.operator_model.read().await,
            )?);
        }
        ensure_operator_model_file(&self.data_dir).await?;
        tokio::fs::read_to_string(operator_model_path(&self.data_dir))
            .await
            .map_err(Into::into)
    }

    pub async fn reset_operator_model(&self) -> Result<()> {
        let reset = OperatorModel::default();
        *self.operator_model.write().await = reset.clone();
        self.active_operator_sessions.write().await.clear();
        self.pending_operator_approvals.write().await.clear();
        self.operator_profile_sessions.write().await.clear();
        for row in self.history.list_operator_profile_sessions().await? {
            self.history
                .delete_operator_profile_session(&row.session_id)
                .await?;
        }
        if self.config.read().await.operator_model.enabled {
            persist_operator_model(&self.data_dir, &reset)?;
        } else {
            match tokio::fs::remove_file(operator_model_path(&self.data_dir)).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    fn operator_profile_questions_for_kind(kind: &str) -> Vec<OperatorProfileQuestion> {
        let mut questions = Vec::new();
        let mut push_flag = |id: &str, field_key: &str, prompt: &str| {
            questions.push(OperatorProfileQuestion {
                id: id.to_string(),
                field_key: field_key.to_string(),
                prompt: prompt.to_string(),
                input_kind: OperatorProfileInputKind::Boolean,
                optional: false,
            });
        };

        let _ = kind;
        push_flag("enabled", "enabled", "Enable operator modeling overall?");
        push_flag(
            "allow_message_statistics",
            "allow_message_statistics",
            "Allow learning from message statistics?",
        );
        push_flag(
            "allow_approval_learning",
            "allow_approval_learning",
            "Allow learning from approval decisions?",
        );
        push_flag(
            "allow_attention_tracking",
            "allow_attention_tracking",
            "Allow attention surface tracking?",
        );
        push_flag(
            "allow_implicit_feedback",
            "allow_implicit_feedback",
            "Allow implicit feedback learning (revisions/fallbacks)?",
        );
        questions
    }

    fn parse_bool_answer(answer_json: &str) -> Result<bool> {
        if let Ok(value) = serde_json::from_str::<bool>(answer_json) {
            return Ok(value);
        }
        let value: serde_json::Value = serde_json::from_str(answer_json).map_err(|error| {
            anyhow::anyhow!("invalid answer_json payload for boolean consent: {error}")
        })?;
        value
            .as_bool()
            .ok_or_else(|| anyhow::anyhow!("answer_json must decode to a boolean"))
    }

    fn operator_profile_progress(
        session: &OperatorProfileSessionState,
    ) -> OperatorProfileProgressPayload {
        let now = now_millis();
        let answered = session
            .questions
            .iter()
            .filter(|question| {
                session.answers.get(&question.id).is_some_and(|state| {
                    if state.answer_json.is_some() || state.skipped {
                        return true;
                    }
                    match state.deferred_until_unix_ms {
                        Some(until) => until > now,
                        None => false,
                    }
                })
            })
            .count() as u32;
        let total = session.questions.len() as u32;
        let remaining = total.saturating_sub(answered);
        let completion_ratio = if total == 0 {
            1.0
        } else {
            answered as f64 / total as f64
        };
        OperatorProfileProgressPayload {
            session_id: session.session_id.clone(),
            answered,
            remaining,
            completion_ratio,
        }
    }

    fn next_operator_profile_question(
        session: &OperatorProfileSessionState,
    ) -> Option<OperatorProfileQuestionPayload> {
        let now = now_millis();
        session.questions.iter().find_map(|question| {
            let state = session.answers.get(&question.id);
            let already_done = state.is_some_and(|s| s.answer_json.is_some() || s.skipped);
            if already_done {
                return None;
            }
            if state
                .and_then(|s| s.deferred_until_unix_ms)
                .is_some_and(|until| until > now)
            {
                return None;
            }
            Some(OperatorProfileQuestionPayload {
                session_id: session.session_id.clone(),
                question_id: question.id.clone(),
                field_key: question.field_key.clone(),
                prompt: question.prompt.clone(),
                input_kind: question.input_kind.as_str().to_string(),
                optional: question.optional,
            })
        })
    }

    fn apply_operator_profile_answers(
        config: &mut AgentConfig,
        session: &OperatorProfileSessionState,
    ) -> Result<Vec<String>> {
        let mut updated_fields = Vec::new();
        for question in &session.questions {
            let Some(state) = session.answers.get(&question.id) else {
                continue;
            };
            let Some(answer_json) = state.answer_json.as_deref() else {
                continue;
            };
            let granted = Self::parse_bool_answer(answer_json)?;
            match question.field_key.as_str() {
                "enabled" => {
                    config.operator_model.enabled = granted;
                    updated_fields.push("enabled".to_string());
                }
                "allow_message_statistics" => {
                    config.operator_model.allow_message_statistics = granted;
                    updated_fields.push("allow_message_statistics".to_string());
                }
                "allow_approval_learning" => {
                    config.operator_model.allow_approval_learning = granted;
                    updated_fields.push("allow_approval_learning".to_string());
                }
                "allow_attention_tracking" => {
                    config.operator_model.allow_attention_tracking = granted;
                    updated_fields.push("allow_attention_tracking".to_string());
                }
                "allow_implicit_feedback" => {
                    config.operator_model.allow_implicit_feedback = granted;
                    updated_fields.push("allow_implicit_feedback".to_string());
                }
                _ => {}
            }
        }
        Ok(updated_fields)
    }

    pub(crate) async fn start_operator_profile_session(
        &self,
        kind: &str,
    ) -> Result<OperatorProfileSessionStarted> {
        let session_id = format!("ops_{}", uuid::Uuid::new_v4());
        let now = now_millis();
        let session = OperatorProfileSessionState {
            version: OPERATOR_PROFILE_VERSION.to_string(),
            session_id: session_id.clone(),
            kind: kind.to_string(),
            created_at: now,
            updated_at: now,
            questions: Self::operator_profile_questions_for_kind(kind),
            answers: HashMap::new(),
            completed: false,
        };
        let session_json = serde_json::to_string(&session)?;
        self.history
            .upsert_operator_profile_session(&session_id, kind, &session_json, now)
            .await?;
        self.operator_profile_sessions
            .write()
            .await
            .insert(session_id.clone(), session);
        Ok(OperatorProfileSessionStarted {
            session_id,
            kind: kind.to_string(),
        })
    }

    pub(crate) async fn next_operator_profile_question_for_session(
        &self,
        session_id: &str,
    ) -> Result<(
        Option<OperatorProfileQuestionPayload>,
        OperatorProfileProgressPayload,
    )> {
        let session = {
            let sessions = self.operator_profile_sessions.read().await;
            sessions
                .get(session_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("unknown operator profile session: {session_id}"))?
        };
        let question = Self::next_operator_profile_question(&session);
        let progress = Self::operator_profile_progress(&session);
        Ok((question, progress))
    }

    pub(crate) async fn submit_operator_profile_answer(
        &self,
        session_id: &str,
        question_id: &str,
        answer_json: &str,
    ) -> Result<(
        Option<OperatorProfileQuestionPayload>,
        OperatorProfileProgressPayload,
    )> {
        let mut sessions = self.operator_profile_sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow::anyhow!("unknown operator profile session: {session_id}"))?;
        if !session.questions.iter().any(|q| q.id == question_id) {
            anyhow::bail!("unknown question_id for session {session_id}: {question_id}");
        }
        let state = session.answers.entry(question_id.to_string()).or_default();
        state.answer_json = Some(answer_json.to_string());
        state.skipped = false;
        state.skip_reason = None;
        state.deferred_until_unix_ms = None;
        session.updated_at = now_millis();
        let session_json = serde_json::to_string(session)?;
        self.history
            .upsert_operator_profile_session(
                &session.session_id,
                &session.kind,
                &session_json,
                session.updated_at,
            )
            .await?;
        let question = Self::next_operator_profile_question(session);
        let progress = Self::operator_profile_progress(session);
        Ok((question, progress))
    }

    pub(crate) async fn skip_operator_profile_question(
        &self,
        session_id: &str,
        question_id: &str,
        reason: Option<&str>,
    ) -> Result<(
        Option<OperatorProfileQuestionPayload>,
        OperatorProfileProgressPayload,
    )> {
        let mut sessions = self.operator_profile_sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow::anyhow!("unknown operator profile session: {session_id}"))?;
        if !session.questions.iter().any(|q| q.id == question_id) {
            anyhow::bail!("unknown question_id for session {session_id}: {question_id}");
        }
        let state = session.answers.entry(question_id.to_string()).or_default();
        state.answer_json = None;
        state.skipped = true;
        state.skip_reason = reason.map(str::to_string);
        state.deferred_until_unix_ms = None;
        session.updated_at = now_millis();
        let session_json = serde_json::to_string(session)?;
        self.history
            .upsert_operator_profile_session(
                &session.session_id,
                &session.kind,
                &session_json,
                session.updated_at,
            )
            .await?;
        let question = Self::next_operator_profile_question(session);
        let progress = Self::operator_profile_progress(session);
        Ok((question, progress))
    }

    pub(crate) async fn defer_operator_profile_question(
        &self,
        session_id: &str,
        question_id: &str,
        defer_until_unix_ms: Option<u64>,
    ) -> Result<(
        Option<OperatorProfileQuestionPayload>,
        OperatorProfileProgressPayload,
    )> {
        let mut sessions = self.operator_profile_sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow::anyhow!("unknown operator profile session: {session_id}"))?;
        if !session.questions.iter().any(|q| q.id == question_id) {
            anyhow::bail!("unknown question_id for session {session_id}: {question_id}");
        }
        let now = now_millis();
        let state = session.answers.entry(question_id.to_string()).or_default();
        state.deferred_until_unix_ms = Some(defer_until_unix_ms.unwrap_or(now + 15 * 60_000));
        state.skipped = false;
        session.updated_at = now;
        let session_json = serde_json::to_string(session)?;
        self.history
            .upsert_operator_profile_session(
                &session.session_id,
                &session.kind,
                &session_json,
                session.updated_at,
            )
            .await?;
        let question = Self::next_operator_profile_question(session);
        let progress = Self::operator_profile_progress(session);
        Ok((question, progress))
    }

    pub async fn get_operator_profile_summary_json(&self) -> Result<String> {
        let model = self.operator_model.read().await.clone();
        let config = self.config.read().await.operator_model.clone();
        Ok(serde_json::to_string(&serde_json::json!({
            "model": model,
            "consents": {
                "enabled": config.enabled,
                "allow_message_statistics": config.allow_message_statistics,
                "allow_approval_learning": config.allow_approval_learning,
                "allow_attention_tracking": config.allow_attention_tracking,
                "allow_implicit_feedback": config.allow_implicit_feedback
            }
        }))?)
    }

    pub async fn set_operator_profile_consent(
        &self,
        consent_key: &str,
        granted: bool,
    ) -> Result<Vec<String>> {
        let mut config = self.config.read().await.clone();
        let mut updated_fields = Vec::new();
        match consent_key {
            "enabled" => {
                config.operator_model.enabled = granted;
                updated_fields.push("enabled".to_string());
            }
            "allow_message_statistics" => {
                config.operator_model.allow_message_statistics = granted;
                updated_fields.push("allow_message_statistics".to_string());
            }
            "allow_approval_learning" => {
                config.operator_model.allow_approval_learning = granted;
                updated_fields.push("allow_approval_learning".to_string());
            }
            "allow_attention_tracking" => {
                config.operator_model.allow_attention_tracking = granted;
                updated_fields.push("allow_attention_tracking".to_string());
            }
            "allow_implicit_feedback" => {
                config.operator_model.allow_implicit_feedback = granted;
                updated_fields.push("allow_implicit_feedback".to_string());
            }
            _ => anyhow::bail!("unknown operator profile consent key: {consent_key}"),
        }
        self.set_config(config).await;
        self.refresh_operator_model().await?;
        Ok(updated_fields)
    }

    pub(crate) async fn complete_operator_profile_session(
        &self,
        session_id: &str,
    ) -> Result<OperatorProfileCompletionPayload> {
        let session = {
            let mut sessions = self.operator_profile_sessions.write().await;
            let mut session = sessions
                .remove(session_id)
                .ok_or_else(|| anyhow::anyhow!("unknown operator profile session: {session_id}"))?;
            session.completed = true;
            session.updated_at = now_millis();
            session
        };

        let mut config = self.config.read().await.clone();
        let updated_fields = Self::apply_operator_profile_answers(&mut config, &session)?;
        self.set_config(config).await;
        self.refresh_operator_model().await?;
        self.history
            .delete_operator_profile_session(&session.session_id)
            .await?;
        Ok(OperatorProfileCompletionPayload {
            session_id: session.session_id,
            updated_fields,
        })
    }
}
