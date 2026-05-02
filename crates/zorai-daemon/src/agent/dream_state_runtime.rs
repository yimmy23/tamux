use super::*;

const DREAM_TASK_WINDOW: usize = 20;
const DREAM_SCORE_THRESHOLD: f64 = 0.65;

#[derive(Clone)]
struct DreamCounterfactual {
    variation_type: &'static str,
    description: String,
    estimated_token_saving: Option<f64>,
    estimated_time_saving_ms: Option<u64>,
    estimated_revision_reduction: Option<u64>,
    score: f64,
}

impl AgentEngine {
    pub(super) async fn run_dream_state_cycle_if_idle(&self, idle_duration_ms: u64) {
        let completed_tasks = self.recent_completed_dream_tasks().await;
        if completed_tasks.is_empty() {
            return;
        }

        let started_at_ms = now_millis();
        let Ok(cycle_id) = self
            .history
            .insert_dream_cycle(&crate::history::DreamCycleRow {
                id: None,
                started_at_ms,
                completed_at_ms: None,
                idle_duration_ms,
                tasks_analyzed: 0,
                counterfactuals_generated: 0,
                counterfactuals_successful: 0,
                status: "running".to_string(),
            })
            .await
        else {
            return;
        };

        let mut counterfactuals_generated = 0u64;
        let mut counterfactuals_successful = 0u64;
        let mut dream_hints = Vec::new();
        for task in &completed_tasks {
            for counterfactual in build_dream_counterfactuals(task) {
                counterfactuals_generated = counterfactuals_generated.saturating_add(1);
                let threshold_met = counterfactual.score >= DREAM_SCORE_THRESHOLD;
                if threshold_met {
                    counterfactuals_successful = counterfactuals_successful.saturating_add(1);
                    dream_hints.push(format!(
                        "{} (confidence {:.2})",
                        counterfactual.description, counterfactual.score
                    ));
                }
                let _ = self
                    .history
                    .insert_counterfactual_evaluation(
                        &crate::history::CounterfactualEvaluationRow {
                            id: None,
                            dream_cycle_id: cycle_id,
                            source_task_id: task.id.clone(),
                            variation_type: counterfactual.variation_type.to_string(),
                            counterfactual_description: counterfactual.description.clone(),
                            estimated_token_saving: counterfactual.estimated_token_saving,
                            estimated_time_saving_ms: counterfactual
                                .estimated_time_saving_ms
                                .map(|value| value as i64),
                            estimated_revision_reduction: counterfactual
                                .estimated_revision_reduction,
                            score: counterfactual.score,
                            threshold_met,
                            created_at_ms: started_at_ms,
                        },
                    )
                    .await;
            }
        }

        if let Err(error) = self
            .persist_dream_hints(dream_hints.as_slice(), Some(cycle_id))
            .await
        {
            tracing::warn!(%error, "failed to persist dream-state hints");
        }

        let _ = self
            .history
            .finish_dream_cycle(
                cycle_id,
                now_millis(),
                completed_tasks.len() as u64,
                counterfactuals_generated,
                counterfactuals_successful,
                "completed",
            )
            .await;
    }

    pub(crate) async fn show_dreams_payload(&self, limit: usize) -> Result<serde_json::Value> {
        let limit = limit.max(1);
        let cycles = self.history.list_dream_cycles(limit).await?;
        let mut evaluations = Vec::new();
        for cycle in cycles.iter().take(3) {
            if let Some(cycle_id) = cycle.id {
                evaluations.extend(
                    self.history
                        .list_counterfactual_evaluations(cycle_id)
                        .await?
                        .into_iter()
                        .take(5),
                );
            }
        }

        let scope_id = current_agent_scope_id();
        ensure_memory_files_for_scope(&self.data_dir, &scope_id).await?;
        let memory_path = memory_paths_for_scope(&self.data_dir, &scope_id).memory_path;
        let dream_hints = tokio::fs::read_to_string(&memory_path)
            .await
            .unwrap_or_default()
            .lines()
            .filter(|line| line.contains("[dream]"))
            .take(limit)
            .map(str::to_string)
            .collect::<Vec<_>>();
        let recent_limit = limit.max(5);
        let carryover = self
            .history
            .provenance_report(recent_limit.saturating_mul(4))
            .ok()
            .map(|report| {
                crate::agent::provenance::adaptive_carryover_provenance_summary(
                    &report,
                    recent_limit,
                )
            })
            .filter(|summary| {
                !crate::agent::provenance::adaptive_carryover_is_effectively_empty(summary)
                    || !dream_hints.is_empty()
            })
            .or_else(|| {
                if dream_hints.is_empty() {
                    None
                } else {
                    Some(serde_json::json!({
                        "inspection_tool": "show_dreams",
                        "persisted_event_count": 0,
                        "dream_hint_event_count": 0,
                        "forge_hint_event_count": 0,
                        "recent_event_count": 0,
                        "recent_events": [],
                    }))
                }
            })
            .map(|mut summary| {
                let persisted_event_count = summary
                    .get("persisted_event_count")
                    .and_then(|value| value.as_u64())
                    .unwrap_or_default();
                let recent_event_count = summary
                    .get("recent_event_count")
                    .and_then(|value| value.as_u64())
                    .unwrap_or_default();
                if let Some(object) = summary.as_object_mut() {
                    object.insert("scope_id".to_string(), serde_json::json!(scope_id.clone()));
                    object.insert("hint_count".to_string(), serde_json::json!(dream_hints.len()));
                    object.insert(
                        "summary".to_string(),
                        serde_json::json!(format!(
                            "{persisted_event_count} persisted carryover event(s), {} visible [dream] hint(s), {recent_event_count} recent provenance link(s)",
                            dream_hints.len()
                        )),
                    );
                }
                summary
            });

        Ok(serde_json::json!({
            "cycle_count": cycles.len(),
            "cycles": cycles.into_iter().map(|cycle| serde_json::json!({
                "id": cycle.id,
                "started_at_ms": cycle.started_at_ms,
                "completed_at_ms": cycle.completed_at_ms,
                "idle_duration_ms": cycle.idle_duration_ms,
                "tasks_analyzed": cycle.tasks_analyzed,
                "counterfactuals_generated": cycle.counterfactuals_generated,
                "counterfactuals_successful": cycle.counterfactuals_successful,
                "status": cycle.status,
            })).collect::<Vec<_>>(),
            "evaluations": evaluations.into_iter().map(|evaluation| serde_json::json!({
                "id": evaluation.id,
                "dream_cycle_id": evaluation.dream_cycle_id,
                "source_task_id": evaluation.source_task_id,
                "variation_type": evaluation.variation_type,
                "counterfactual_description": evaluation.counterfactual_description,
                "estimated_token_saving": evaluation.estimated_token_saving,
                "estimated_time_saving_ms": evaluation.estimated_time_saving_ms,
                "estimated_revision_reduction": evaluation.estimated_revision_reduction,
                "score": evaluation.score,
                "threshold_met": evaluation.threshold_met,
                "created_at_ms": evaluation.created_at_ms,
            })).collect::<Vec<_>>(),
            "carryover": carryover,
            "hints": dream_hints,
        }))
    }

    async fn recent_completed_dream_tasks(&self) -> Vec<AgentTask> {
        let mut tasks = self
            .tasks
            .lock()
            .await
            .iter()
            .filter(|task| matches!(task.status, TaskStatus::Completed))
            .cloned()
            .collect::<Vec<_>>();
        tasks.sort_by(|left, right| {
            right
                .completed_at
                .unwrap_or(0)
                .cmp(&left.completed_at.unwrap_or(0))
        });
        tasks.truncate(DREAM_TASK_WINDOW);
        tasks
    }

    async fn persist_dream_hints(
        &self,
        hints: &[String],
        dream_cycle_id: Option<i64>,
    ) -> anyhow::Result<usize> {
        if hints.is_empty() {
            return Ok(0);
        }

        let scope_id = current_agent_scope_id();
        ensure_memory_files_for_scope(&self.data_dir, &scope_id).await?;
        let memory_path = memory_paths_for_scope(&self.data_dir, &scope_id).memory_path;
        let mut existing = tokio::fs::read_to_string(&memory_path)
            .await
            .unwrap_or_default();
        let mut applied = 0usize;
        let mut persisted_hints = Vec::new();

        for hint in hints {
            let normalized_hint = super::forge::strip_forge_markup(hint);
            if normalized_hint.is_empty() || dream_hint_exists(&existing, &normalized_hint) {
                continue;
            }
            let timestamp_ms = now_millis();
            let line = format!(
                "- [dream][{}] {}",
                chrono::DateTime::<chrono::Utc>::from_timestamp_millis(timestamp_ms as i64)
                    .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
                    .unwrap_or_else(|| timestamp_ms.to_string()),
                normalized_hint.trim()
            );
            let next = if existing.trim().is_empty() {
                line.clone()
            } else {
                format!("{}\n\n{}", existing.trim_end(), line)
            };
            if next.chars().count() > 2_500 {
                break;
            }
            existing = next;
            applied = applied.saturating_add(1);
            persisted_hints.push(normalized_hint.trim().to_string());
        }

        if applied > 0 {
            tokio::fs::write(memory_path, existing).await?;
            self.record_dream_hints_persisted(&scope_id, applied, &persisted_hints, dream_cycle_id)
                .await;
        }

        Ok(applied)
    }
}

fn dream_hint_exists(content: &str, hint: &str) -> bool {
    let normalized = hint.trim().to_ascii_lowercase();
    content.lines().any(|line| {
        line.contains("[dream]") && line.to_ascii_lowercase().contains(normalized.as_str())
    })
}

fn build_dream_counterfactuals(task: &AgentTask) -> Vec<DreamCounterfactual> {
    let mut items = Vec::new();
    let base_retry_penalty = task.retry_count as f64 * 0.08;
    let duration_hint_ms = task
        .started_at
        .zip(task.completed_at)
        .map(|(started_at, completed_at)| completed_at.saturating_sub(started_at));

    items.push(DreamCounterfactual {
        variation_type: "order_variation",
        description: format!(
            "Inspect the full task context before executing '{}'.",
            task.title
        ),
        estimated_token_saving: Some(120.0 + (task.retry_count as f64 * 35.0)),
        estimated_time_saving_ms: duration_hint_ms.map(|value| value / 5),
        estimated_revision_reduction: Some((task.retry_count as u64).saturating_add(1)),
        score: (0.62 + base_retry_penalty).clamp(0.0, 0.95),
    });

    if task.description.to_ascii_lowercase().contains("review")
        || task.description.to_ascii_lowercase().contains("compare")
    {
        items.push(DreamCounterfactual {
            variation_type: "subagent_delegation",
            description: format!(
                "Delegate the bounded review portion of '{}' to a subagent earlier.",
                task.title
            ),
            estimated_token_saving: Some(180.0),
            estimated_time_saving_ms: duration_hint_ms.map(|value| value / 4),
            estimated_revision_reduction: Some(1),
            score: 0.74,
        });
    }

    if task.command.as_deref().is_some_and(|command| {
        let command = command.to_ascii_lowercase();
        command.contains("cat ") || command.contains("grep ") || command.contains("find ")
    }) {
        items.push(DreamCounterfactual {
            variation_type: "tool_substitution",
            description: format!(
                "Prefer structured file/context tools before raw shell inspection in '{}'.",
                task.title
            ),
            estimated_token_saving: Some(95.0),
            estimated_time_saving_ms: duration_hint_ms.map(|value| value / 6),
            estimated_revision_reduction: Some(1),
            score: 0.78,
        });
    }

    if task.description.split_whitespace().count() >= 12 {
        items.push(DreamCounterfactual {
            variation_type: "clarification_injection",
            description: format!(
                "Ask one clarifying question before starting '{}'.",
                task.title
            ),
            estimated_token_saving: Some(70.0),
            estimated_time_saving_ms: duration_hint_ms.map(|value| value / 8),
            estimated_revision_reduction: Some(1),
            score: 0.67,
        });
    }

    items
}
