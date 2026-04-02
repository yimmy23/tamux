use super::*;
use crate::agent::cost::CostTracker;

impl AgentEngine {
    pub(super) async fn find_active_goal_run_for_thread(&self, thread_id: &str) -> Option<String> {
        let goal_runs = self.goal_runs.lock().await;
        goal_runs
            .iter()
            .find(|gr| {
                matches!(gr.status, GoalRunStatus::Running | GoalRunStatus::Planning)
                    && gr.thread_id.as_deref() == Some(thread_id)
            })
            .map(|gr| gr.id.clone())
    }

    pub(super) async fn accumulate_goal_run_cost(
        &self,
        thread_id: &str,
        input_tokens: u64,
        output_tokens: u64,
        provider: &str,
        model: &str,
    ) {
        let goal_run_id = match self.find_active_goal_run_for_thread(thread_id).await {
            Some(id) => id,
            None => return,
        };

        let config = self.config.read().await;
        if !config.cost.enabled {
            return;
        }
        let rate_cards = config.cost.rate_cards.clone();
        let threshold = config.cost.budget_alert_threshold_usd;
        drop(config);

        let mut trackers = self.cost_trackers.lock().await;
        let tracker = trackers
            .entry(goal_run_id.clone())
            .or_insert_with(CostTracker::new);
        tracker.accumulate(input_tokens, output_tokens, provider, model, &rate_cards);

        if tracker.budget_alert_needed(threshold) {
            if let Some(cost) = tracker.summary().estimated_cost_usd {
                let _ = self.event_tx.send(AgentEvent::BudgetAlert {
                    goal_run_id: goal_run_id.clone(),
                    current_cost_usd: cost,
                    threshold_usd: threshold.unwrap_or(0.0),
                });
            }
        }
    }
}
