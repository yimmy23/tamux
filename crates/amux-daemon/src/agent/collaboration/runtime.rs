use super::participants::apply_vote_to_disagreement;
use super::*;

impl AgentEngine {
    async fn persist_collaboration_session(&self, session: &CollaborationSession) -> Result<()> {
        let session_json = serde_json::to_string(session)?;
        self.history
            .upsert_collaboration_session(
                &session.parent_task_id,
                &session_json,
                session.updated_at,
            )
            .await
    }

    pub(in crate::agent) async fn register_subagent_collaboration(
        &self,
        parent_task_id: &str,
        subagent: &AgentTask,
    ) {
        if !self.config.read().await.collaboration.enabled {
            return;
        }
        let parent = {
            let tasks = self.tasks.lock().await;
            tasks.iter().find(|task| task.id == parent_task_id).cloned()
        };
        let Some(parent) = parent else {
            return;
        };
        let mut collaboration = self.collaboration.write().await;
        let session = collaboration
            .entry(parent_task_id.to_string())
            .or_insert_with(|| CollaborationSession {
                id: format!("collab_{}", uuid::Uuid::new_v4()),
                parent_task_id: parent_task_id.to_string(),
                thread_id: parent.thread_id.clone().or(parent.parent_thread_id.clone()),
                goal_run_id: parent.goal_run_id.clone(),
                mission: parent.description.clone(),
                agents: Vec::new(),
                contributions: Vec::new(),
                disagreements: Vec::new(),
                consensus: None,
                updated_at: now_millis(),
            });

        if session
            .agents
            .iter()
            .any(|agent| agent.task_id == subagent.id)
        {
            return;
        }
        session.agents.push(CollaborativeAgent {
            task_id: subagent.id.clone(),
            title: subagent.title.clone(),
            role: infer_collaboration_role(subagent),
            confidence: 0.5,
            status: format!("{:?}", subagent.status).to_lowercase(),
        });
        session.updated_at = now_millis();
        let snapshot = session.clone();
        drop(collaboration);
        if let Err(error) = self.persist_collaboration_session(&snapshot).await {
            tracing::warn!(
                parent_task_id = %parent_task_id,
                "failed to persist collaboration session: {error}"
            );
        }
    }

    pub(in crate::agent) async fn record_collaboration_contribution(
        &self,
        parent_task_id: &str,
        task_id: &str,
        topic: &str,
        position: &str,
        evidence: Vec<String>,
        confidence: f64,
    ) -> Result<serde_json::Value> {
        if !self.config.read().await.collaboration.enabled {
            anyhow::bail!("collaboration capability is disabled in agent config");
        }
        let mut collaboration = self.collaboration.write().await;
        let Some(session) = collaboration.get_mut(parent_task_id) else {
            anyhow::bail!("no collaboration session found for parent task {parent_task_id}");
        };
        let contribution = Contribution {
            id: format!("contrib_{}", uuid::Uuid::new_v4()),
            task_id: task_id.to_string(),
            topic: normalize_topic(topic),
            position: position.trim().to_string(),
            evidence,
            confidence: confidence.clamp(0.0, 1.0),
            created_at: now_millis(),
        };
        if let Some(agent) = session
            .agents
            .iter_mut()
            .find(|agent| agent.task_id == task_id)
        {
            agent.confidence = contribution.confidence;
        }
        session.contributions.push(contribution.clone());
        detect_disagreements(session);
        session.updated_at = now_millis();
        let snapshot = session.clone();

        let escalation = session
            .disagreements
            .iter()
            .find(|item| item.resolution == "pending" && item.confidence_gap < 0.15)
            .cloned();
        let thread_id = session.thread_id.clone();
        let report = serde_json::json!({
            "session_id": session.id,
            "contribution": contribution,
            "disagreements": session.disagreements,
            "consensus": session.consensus,
        });
        drop(collaboration);
        self.persist_collaboration_session(&snapshot).await?;

        if let Some(disagreement) = escalation {
            if let Some(thread_id) = thread_id {
                let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                    thread_id,
                    kind: "collaboration".to_string(),
                    message: format!("Unresolved subagent disagreement on {}", disagreement.topic),
                    details: Some(disagreement.positions.join(" vs ")),
                });
            }
        }
        Ok(report)
    }

    pub(in crate::agent) async fn collaboration_peer_memory_json(
        &self,
        parent_task_id: &str,
        task_id: &str,
    ) -> Result<serde_json::Value> {
        if !self.config.read().await.collaboration.enabled {
            anyhow::bail!("collaboration capability is disabled in agent config");
        }
        let collaboration = self.collaboration.read().await;
        let Some(session) = collaboration.get(parent_task_id) else {
            anyhow::bail!("no collaboration session found for parent task {parent_task_id}");
        };
        Ok(serde_json::json!({
            "session_id": session.id,
            "mission": session.mission,
            "peers": session.agents.iter().filter(|agent| agent.task_id != task_id).collect::<Vec<_>>(),
            "shared_context": session.contributions.iter().filter(|entry| entry.task_id != task_id).collect::<Vec<_>>(),
            "disagreements": session.disagreements,
            "consensus": session.consensus,
        }))
    }

    pub(crate) async fn vote_on_collaboration_disagreement(
        &self,
        parent_task_id: &str,
        disagreement_id: &str,
        task_id: &str,
        position: &str,
        confidence: Option<f64>,
    ) -> Result<serde_json::Value> {
        if !self.config.read().await.collaboration.enabled {
            anyhow::bail!("collaboration capability is disabled in agent config");
        }
        let mut collaboration = self.collaboration.write().await;
        let Some(session) = collaboration.get_mut(parent_task_id) else {
            anyhow::bail!("no collaboration session found for parent task {parent_task_id}");
        };
        let Some(disagreement) = session
            .disagreements
            .iter_mut()
            .find(|item| item.id == disagreement_id)
        else {
            anyhow::bail!("unknown disagreement {disagreement_id}");
        };
        session.consensus = apply_vote_to_disagreement(
            disagreement,
            &session.agents,
            task_id,
            position,
            confidence,
        );
        let resolution = disagreement.resolution.clone();
        let consensus = session.consensus.clone();
        let session_id = session.id.clone();
        let escalation = (disagreement.resolution == "escalated").then(|| disagreement.clone());
        let thread_id = session.thread_id.clone();
        session.updated_at = now_millis();
        let snapshot = session.clone();
        let report = serde_json::json!({
            "session_id": session_id,
            "resolution": resolution,
            "consensus": consensus,
        });
        drop(collaboration);
        self.persist_collaboration_session(&snapshot).await?;

        if let (Some(disagreement), Some(thread_id)) = (escalation, thread_id) {
            let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                thread_id,
                kind: "collaboration".to_string(),
                message: format!(
                    "Unresolved subagent disagreement on {}",
                    disagreement.topic
                ),
                details: Some(disagreement.positions.join(" vs ")),
            });
        }

        Ok(report)
    }

    pub async fn collaboration_sessions_json(
        &self,
        parent_task_id: Option<&str>,
    ) -> Result<serde_json::Value> {
        if !self.config.read().await.collaboration.enabled {
            anyhow::bail!("collaboration capability is disabled in agent config");
        }
        let collaboration = self.collaboration.read().await;
        if let Some(parent_task_id) = parent_task_id {
            if let Some(session) = collaboration.get(parent_task_id) {
                return Ok(serde_json::to_value(session).unwrap_or_else(|_| serde_json::json!({})));
            }
            return Ok(serde_json::json!([]));
        }
        Ok(serde_json::to_value(
            collaboration
                .values()
                .cloned()
                .collect::<Vec<CollaborationSession>>(),
        )
        .unwrap_or_else(|_| serde_json::json!([])))
    }

    pub(in crate::agent) async fn record_collaboration_outcome(
        &self,
        task: &AgentTask,
        outcome: &str,
    ) {
        if !self.config.read().await.collaboration.enabled {
            return;
        }
        let Some(parent_task_id) = task.parent_task_id.as_deref() else {
            return;
        };
        if task.source != "subagent" {
            return;
        }
        let summary = match outcome {
            "success" => task
                .result
                .as_deref()
                .or(task.logs.last().map(|entry| entry.message.as_str()))
                .unwrap_or("subagent completed successfully"),
            "failure" => task
                .last_error
                .as_deref()
                .or(task.error.as_deref())
                .unwrap_or("subagent failed"),
            "cancelled" => "subagent cancelled before conclusion",
            _ => "subagent updated",
        };
        let position = match outcome {
            "success" => "recommended",
            "failure" => "rejected",
            "cancelled" => "cancelled",
            _ => "reported",
        };
        if let Err(error) = self
            .record_collaboration_contribution(
                parent_task_id,
                &task.id,
                &task.title,
                position,
                vec![crate::agent::summarize_text(summary, 220)],
                if outcome == "success" { 0.8 } else { 0.6 },
            )
            .await
        {
            tracing::warn!(
                task_id = %task.id,
                parent_task_id = %parent_task_id,
                "failed to record collaboration outcome: {error}"
            );
        }
    }
}
