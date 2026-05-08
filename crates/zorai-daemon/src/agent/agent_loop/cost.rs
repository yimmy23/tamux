use super::*;
use crate::agent::cost::CostTracker;

impl AgentEngine {
    pub(super) async fn find_active_goal_run_for_thread(&self, thread_id: &str) -> Option<String> {
        let persisted_latest = match self
            .history
            .latest_goal_run_id_and_updated_at_for_thread_ids_and_statuses(
                &[thread_id.to_string()],
                &[GoalRunStatus::Running, GoalRunStatus::Planning],
            )
            .await
        {
            Ok(goal_run) => goal_run,
            Err(error) => {
                tracing::warn!(
                    thread_id,
                    "failed to query latest persisted active goal run for cost accounting: {error}"
                );
                None
            }
        };

        let live_latest = {
            let live_goal_runs = self.goal_runs.lock().await;
            live_goal_runs
                .iter()
                .filter(|goal_run| {
                    matches!(
                        goal_run.status,
                        GoalRunStatus::Running | GoalRunStatus::Planning
                    ) && (goal_run.thread_id.as_deref() == Some(thread_id)
                        || goal_run.root_thread_id.as_deref() == Some(thread_id)
                        || goal_run.active_thread_id.as_deref() == Some(thread_id)
                        || goal_run
                            .execution_thread_ids
                            .iter()
                            .any(|candidate| candidate == thread_id))
                })
                .max_by_key(|goal_run| goal_run.updated_at)
                .map(|goal_run| (goal_run.id.clone(), goal_run.updated_at))
        };

        [persisted_latest, live_latest]
            .into_iter()
            .flatten()
            .max_by_key(|(_, updated_at)| *updated_at)
            .map(|(goal_run_id, _)| goal_run_id)
    }

    pub(super) async fn accumulate_goal_run_cost(
        &self,
        thread_id: &str,
        input_tokens: u64,
        output_tokens: u64,
        provider: &str,
        model: &str,
        duration_ms: Option<u64>,
    ) {
        let goal_run_id = match self.find_active_goal_run_for_thread(thread_id).await {
            Some(id) => id,
            None => return,
        };

        self.accumulate_goal_run_cost_by_id(
            &goal_run_id,
            input_tokens,
            output_tokens,
            provider,
            model,
            duration_ms,
        )
        .await;
    }

    pub(in crate::agent) async fn accumulate_goal_run_cost_by_id(
        &self,
        goal_run_id: &str,
        input_tokens: u64,
        output_tokens: u64,
        provider: &str,
        model: &str,
        duration_ms: Option<u64>,
    ) {
        if input_tokens == 0 && output_tokens == 0 {
            return;
        }

        let config = self.config.read().await;
        if !config.cost.enabled {
            return;
        }
        let rate_cards = config.cost.rate_cards.clone();
        let threshold = config.cost.budget_alert_threshold_usd;
        drop(config);

        let mut trackers = self.cost_trackers.lock().await;
        let tracker = trackers
            .entry(goal_run_id.to_string())
            .or_insert_with(CostTracker::new);
        tracker.accumulate(
            input_tokens,
            output_tokens,
            provider,
            model,
            &rate_cards,
            duration_ms,
        );
        let summary = tracker.summary().clone();
        let model_usage = tracker.model_usage();

        if tracker.budget_alert_needed(threshold) {
            if let Some(cost) = tracker.summary().estimated_cost_usd {
                let _ = self.event_tx.send(AgentEvent::BudgetAlert {
                    goal_run_id: goal_run_id.to_string(),
                    current_cost_usd: cost,
                    threshold_usd: threshold.unwrap_or(0.0),
                });
            }
        }
        drop(trackers);

        let mut updated = None;
        {
            let mut goal_runs = self.goal_runs.lock().await;
            if let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) {
                goal_run.total_prompt_tokens = summary.total_prompt_tokens;
                goal_run.total_completion_tokens = summary.total_completion_tokens;
                goal_run.estimated_cost_usd = summary.estimated_cost_usd;
                goal_run.model_usage = model_usage;
                goal_run.updated_at = now_millis();
                updated = Some(goal_run.clone());
            }
        }

        if let Some(goal_run) = updated {
            self.persist_goal_runs().await;
            self.emit_goal_run_update(&goal_run, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_manager::SessionManager;
    use tempfile::tempdir;

    #[tokio::test]
    async fn find_active_goal_run_for_thread_uses_persisted_goal_after_live_queue_clear() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let goal_run = engine
            .start_goal_run(
                "Persisted cost goal".to_string(),
                Some("Persisted cost goal".to_string()),
                Some("thread-persisted-cost-goal".to_string()),
                None,
                None,
                None,
                None,
                None,
            )
            .await;
        {
            let mut goal_runs = engine.goal_runs.lock().await;
            let goal = goal_runs
                .iter_mut()
                .find(|goal| goal.id == goal_run.id)
                .expect("goal should be live before persistence");
            goal.status = GoalRunStatus::Running;
        }
        engine.persist_goal_runs().await;
        engine.goal_runs.lock().await.clear();

        let thread_id = goal_run.thread_id.as_deref().expect("goal thread id");
        let active_goal_run_id = engine.find_active_goal_run_for_thread(thread_id).await;

        assert_eq!(active_goal_run_id.as_deref(), Some(goal_run.id.as_str()));
    }
}
