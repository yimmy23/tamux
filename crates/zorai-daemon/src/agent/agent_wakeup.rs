use super::*;

#[derive(Debug, Clone)]
pub(in crate::agent) struct AgentWakeup {
    pub(in crate::agent) id: String,
    pub(in crate::agent) thread_id: String,
    pub(in crate::agent) message: String,
    pub(in crate::agent) interval_ms: u64,
    pub(in crate::agent) next_fire_at: u64,
    pub(in crate::agent) repetitions_remaining: Option<u64>,
    pub(in crate::agent) created_at: u64,
}

impl AgentWakeup {
    fn to_row(&self) -> crate::history::AgentWakeupRow {
        crate::history::AgentWakeupRow {
            id: self.id.clone(),
            thread_id: self.thread_id.clone(),
            message: self.message.clone(),
            interval_ms: self.interval_ms,
            next_fire_at: self.next_fire_at,
            repetitions_remaining: self.repetitions_remaining,
            created_at: self.created_at,
        }
    }
}

impl AgentEngine {
    pub(in crate::agent) async fn schedule_wakeup(
        &self,
        thread_id: &str,
        delay_ms: u64,
        repetitions: u64,
        message: &str,
    ) -> AgentWakeup {
        let now = now_millis();
        let interval_ms = delay_ms.max(1);
        let wakeup = AgentWakeup {
            id: format!("wakeup_{}", uuid::Uuid::new_v4()),
            thread_id: thread_id.to_string(),
            message: message.to_string(),
            interval_ms,
            next_fire_at: now.saturating_add(interval_ms),
            repetitions_remaining: (repetitions != 0).then_some(repetitions),
            created_at: now,
        };
        self.timer_wakeups
            .lock()
            .await
            .insert(wakeup.id.clone(), wakeup.clone());
        if let Err(error) = self.history.upsert_agent_wakeup(&wakeup.to_row()).await {
            tracing::warn!(error = %error, "failed to persist scheduled wakeup");
        }
        wakeup
    }

    pub(in crate::agent) async fn cancel_wakeup(&self, wakeup_id: &str) -> bool {
        let wakeup_id = wakeup_id.trim();
        let removed = self.timer_wakeups.lock().await.remove(wakeup_id).is_some();
        if let Err(error) = self.history.delete_agent_wakeup(wakeup_id).await {
            tracing::warn!(error = %error, "failed to delete persisted wakeup");
        }
        removed
    }

    #[cfg(test)]
    pub(in crate::agent) async fn pending_wakeup_count(&self) -> usize {
        self.timer_wakeups.lock().await.len()
    }

    pub(in crate::agent) async fn supervise_timer_wakeups(&self) -> Result<()> {
        let now = now_millis();
        let live_threads: std::collections::HashSet<String> =
            self.threads.read().await.keys().cloned().collect();
        let mut fired_by_thread: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        let mut to_delete: Vec<String> = Vec::new();
        let mut to_upsert: Vec<AgentWakeup> = Vec::new();
        {
            let mut wakeups = self.timer_wakeups.lock().await;
            let due_ids = wakeups
                .values()
                .filter(|wakeup| wakeup.next_fire_at <= now)
                .map(|wakeup| wakeup.id.clone())
                .collect::<Vec<_>>();
            for id in due_ids {
                let Some(wakeup) = wakeups.get_mut(&id) else {
                    continue;
                };
                if !live_threads.contains(&wakeup.thread_id) {
                    wakeups.remove(&id);
                    to_delete.push(id);
                    continue;
                }
                fired_by_thread
                    .entry(wakeup.thread_id.clone())
                    .or_default()
                    .push(wakeup.message.clone());
                let is_last = matches!(wakeup.repetitions_remaining, Some(remaining) if remaining <= 1);
                if is_last {
                    wakeups.remove(&id);
                    to_delete.push(id);
                } else {
                    if let Some(remaining) = wakeup.repetitions_remaining {
                        wakeup.repetitions_remaining = Some(remaining - 1);
                    }
                    wakeup.next_fire_at = now.saturating_add(wakeup.interval_ms);
                    to_upsert.push(wakeup.clone());
                }
            }
        }

        for id in to_delete {
            if let Err(error) = self.history.delete_agent_wakeup(&id).await {
                tracing::warn!(error = %error, "failed to delete fired wakeup");
            }
        }
        for wakeup in to_upsert {
            if let Err(error) = self.history.upsert_agent_wakeup(&wakeup.to_row()).await {
                tracing::warn!(error = %error, "failed to persist rescheduled wakeup");
            }
        }

        for (thread_id, messages) in fired_by_thread {
            let body = if messages.len() == 1 {
                messages.into_iter().next().unwrap_or_default()
            } else {
                messages
                    .iter()
                    .enumerate()
                    .map(|(index, message)| format!("{}. {message}", index + 1))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            let content = format!("[Scheduled wakeup] {body}");
            let agent_id = self
                .active_agent_id_for_thread(&thread_id)
                .await
                .unwrap_or_else(|| MAIN_AGENT_ID.to_string());
            self.enqueue_visible_thread_continuation(
                &thread_id,
                DeferredVisibleThreadContinuation {
                    agent_id,
                    task_id: None,
                    preferred_session_hint: None,
                    llm_user_content: content,
                    queued_at_ms: 0,
                    force_compaction: false,
                    rerun_participant_observers_after_turn: true,
                    internal_delegate_sender: None,
                    internal_delegate_message: None,
                },
            )
            .await;
            if let Err(error) = self
                .flush_deferred_visible_thread_continuations(&thread_id)
                .await
            {
                tracing::warn!(
                    thread_id = %thread_id,
                    error = %error,
                    "scheduled wakeup continuation flush failed"
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    async fn engine_with_thread(thread_id: &str) -> std::sync::Arc<AgentEngine> {
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let now = now_millis();
        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Wakeup".to_string(),
                messages: vec![AgentMessage::user("kick off the job", now)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                created_at: now,
                updated_at: now,
                total_input_tokens: 0,
                total_output_tokens: 0,
            },
        );
        engine
    }

    async fn force_due(engine: &AgentEngine, id: &str) {
        let mut wakeups = engine.timer_wakeups.lock().await;
        wakeups.get_mut(id).expect("wakeup should be present").next_fire_at = 0;
    }

    #[tokio::test]
    async fn single_fire_wakeup_enqueues_continuation_and_clears() {
        let thread_id = "thread-wakeup-single";
        let engine = engine_with_thread(thread_id).await;
        let wakeup = engine
            .schedule_wakeup(thread_id, 5 * 60_000, 1, "check job progress")
            .await;
        force_due(&engine, &wakeup.id).await;
        let (_generation, _, _) = engine.begin_stream_cancellation(thread_id).await;

        engine
            .supervise_timer_wakeups()
            .await
            .expect("wakeup supervision should succeed");

        let continuations = engine
            .deferred_visible_thread_continuations_for(thread_id)
            .await;
        assert_eq!(continuations.len(), 1);
        assert!(continuations[0].llm_user_content.contains("check job progress"));
        assert!(continuations[0]
            .llm_user_content
            .starts_with("[Scheduled wakeup]"));
        assert_eq!(
            engine.pending_wakeup_count().await,
            0,
            "a single-fire wakeup must be cleared after it fires"
        );
    }

    #[tokio::test]
    async fn repeating_wakeup_reschedules_until_exhausted() {
        let thread_id = "thread-wakeup-repeat";
        let engine = engine_with_thread(thread_id).await;
        let wakeup = engine.schedule_wakeup(thread_id, 60_000, 2, "poll").await;
        force_due(&engine, &wakeup.id).await;
        let (_generation, _, _) = engine.begin_stream_cancellation(thread_id).await;

        engine
            .supervise_timer_wakeups()
            .await
            .expect("first supervision tick should succeed");
        assert_eq!(
            engine.pending_wakeup_count().await,
            1,
            "a repeating wakeup must remain scheduled after its first fire"
        );
        {
            let wakeups = engine.timer_wakeups.lock().await;
            let stored = wakeups.get(&wakeup.id).expect("wakeup should still exist");
            assert_eq!(stored.repetitions_remaining, Some(1));
            assert!(
                stored.next_fire_at > now_millis(),
                "the wakeup must be rescheduled into the future"
            );
        }

        force_due(&engine, &wakeup.id).await;
        engine
            .supervise_timer_wakeups()
            .await
            .expect("second supervision tick should succeed");
        assert_eq!(
            engine.pending_wakeup_count().await,
            0,
            "the wakeup must be cleared after its final repetition"
        );
    }

    #[tokio::test]
    async fn cancel_wakeup_removes_pending_wakeup() {
        let thread_id = "thread-wakeup-cancel";
        let engine = engine_with_thread(thread_id).await;
        let wakeup = engine.schedule_wakeup(thread_id, 60_000, 0, "loop forever").await;
        assert_eq!(engine.pending_wakeup_count().await, 1);
        assert!(engine.cancel_wakeup(&wakeup.id).await);
        assert_eq!(engine.pending_wakeup_count().await, 0);
        assert!(
            !engine.cancel_wakeup(&wakeup.id).await,
            "cancelling an unknown wakeup id is a no-op"
        );
    }

    #[tokio::test]
    async fn wakeup_is_persisted_and_removed_on_cancel() {
        let thread_id = "thread-wakeup-persist";
        let engine = engine_with_thread(thread_id).await;
        let wakeup = engine
            .schedule_wakeup(thread_id, 60_000, 1, "persisted check")
            .await;

        let rows = engine
            .history
            .list_agent_wakeups()
            .await
            .expect("listing wakeups should succeed");
        assert!(
            rows.iter()
                .any(|row| row.id == wakeup.id && row.message == "persisted check"),
            "a scheduled wakeup must be persisted to the database"
        );

        assert!(engine.cancel_wakeup(&wakeup.id).await);
        let rows = engine
            .history
            .list_agent_wakeups()
            .await
            .expect("listing wakeups should succeed");
        assert!(
            !rows.iter().any(|row| row.id == wakeup.id),
            "a cancelled wakeup must be deleted from the database"
        );
    }
}
