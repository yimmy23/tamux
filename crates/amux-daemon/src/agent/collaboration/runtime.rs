use super::participants::{apply_vote_to_disagreement, normalize_position};
use super::*;
use crate::agent::debate::protocol::create_debate_session;
use crate::agent::debate::types::{Argument, DebateSession, RoleKind};
use crate::agent::handoff::divergent::Framing;
use std::collections::{BTreeSet, HashMap};

#[derive(Clone)]
struct PendingDebateSeed {
    disagreement_id: String,
    debate_session: DebateSession,
    arguments: Vec<Argument>,
}

#[derive(Clone)]
struct PendingDebateLaunch {
    seed: PendingDebateSeed,
    collaboration_snapshot: CollaborationSession,
}

fn build_pending_debate_seed(
    session: &CollaborationSession,
    debate_config: &DebateConfig,
) -> Option<PendingDebateSeed> {
    let thread_id = session.thread_id.as_ref()?.clone();

    for disagreement in &session.disagreements {
        if disagreement.resolution != "pending" || disagreement.debate_session_id.is_some() {
            continue;
        }

        let mut latest_by_task: HashMap<&str, &Contribution> = HashMap::new();
        for contribution in session.contributions.iter().rev() {
            if contribution.topic != disagreement.topic {
                continue;
            }
            latest_by_task
                .entry(contribution.task_id.as_str())
                .or_insert(contribution);
        }

        let mut latest = latest_by_task.into_values().collect::<Vec<_>>();
        latest.sort_by_key(|contribution| contribution.created_at);

        let distinct_positions = latest
            .iter()
            .map(|contribution| normalize_position(&contribution.position))
            .collect::<BTreeSet<_>>();
        if latest.len() < 2 || distinct_positions.len() < 2 {
            continue;
        }

        let mut ordered_positions = Vec::new();
        for contribution in &latest {
            let position = normalize_position(&contribution.position);
            if !ordered_positions
                .iter()
                .any(|existing| existing == &position)
            {
                ordered_positions.push(position);
            }
        }
        let proponent_position = ordered_positions
            .first()
            .cloned()
            .unwrap_or_else(|| "recommend".to_string());
        let skeptic_position = ordered_positions
            .iter()
            .find(|position| **position != proponent_position)
            .cloned()
            .unwrap_or_else(|| "reject".to_string());

        let mut framings = Vec::new();
        for position in ordered_positions.iter().take(2) {
            framings.push(Framing {
                label: format!("{position}-lens"),
                system_prompt_override: format!(
                    "Defend the `{position}` position for {} using the imported collaboration evidence.",
                    disagreement.topic
                ),
                task_id: latest
                    .iter()
                    .find(|contribution| normalize_position(&contribution.position) == *position)
                    .map(|contribution| contribution.task_id.clone()),
                contribution_id: latest
                    .iter()
                    .find(|contribution| normalize_position(&contribution.position) == *position)
                    .map(|contribution| contribution.id.clone()),
            });
        }
        framings.push(Framing {
            label: "synthesis-lens".to_string(),
            system_prompt_override: format!(
                "Synthesize the strongest recommendation for {} without erasing the imported disagreement.",
                disagreement.topic
            ),
            task_id: None,
            contribution_id: None,
        });

        let debate_session = create_debate_session(
            disagreement.topic.clone(),
            framings,
            debate_config.default_max_rounds,
            debate_config.role_rotation,
            Some(thread_id.clone()),
            session.goal_run_id.clone(),
        )
        .ok()?;

        let arguments = latest
            .into_iter()
            .map(|contribution| {
                let position = normalize_position(&contribution.position);
                Argument {
                    id: format!("arg_{}", uuid::Uuid::new_v4()),
                    round: 1,
                    role: if position == proponent_position {
                        RoleKind::Proponent
                    } else if position == skeptic_position {
                        RoleKind::Skeptic
                    } else {
                        RoleKind::Skeptic
                    },
                    agent_id: contribution.task_id.clone(),
                    content: contribution.position.clone(),
                    evidence_refs: if contribution.evidence.is_empty() {
                        vec![format!("collaboration contribution {}", contribution.id)]
                    } else {
                        contribution.evidence.clone()
                    },
                    responds_to: None,
                    timestamp_ms: contribution.created_at,
                }
            })
            .collect::<Vec<_>>();

        return Some(PendingDebateSeed {
            disagreement_id: disagreement.id.clone(),
            debate_session,
            arguments,
        });
    }

    None
}

impl AgentEngine {
    async fn ensure_task_collaboration_session(
        &self,
        task: &AgentTask,
    ) -> Result<CollaborationSession> {
        let mut collaboration = self.collaboration.write().await;
        let session =
            collaboration
                .entry(task.id.clone())
                .or_insert_with(|| CollaborationSession {
                    id: format!("collab_{}", uuid::Uuid::new_v4()),
                    parent_task_id: task.id.clone(),
                    thread_id: task.thread_id.clone().or(task.parent_thread_id.clone()),
                    goal_run_id: task.goal_run_id.clone(),
                    mission: task.description.clone(),
                    agents: Vec::new(),
                    contributions: Vec::new(),
                    disagreements: Vec::new(),
                    consensus: None,
                    updated_at: now_millis(),
                });

        if !session.agents.iter().any(|agent| agent.task_id == task.id) {
            session.agents.push(CollaborativeAgent {
                task_id: task.id.clone(),
                title: task.title.clone(),
                role: infer_collaboration_role(task),
                confidence: 0.5,
                status: format!("{:?}", task.status).to_lowercase(),
            });
        }
        session.thread_id = session
            .thread_id
            .clone()
            .or(task.thread_id.clone())
            .or(task.parent_thread_id.clone());
        session.goal_run_id = session.goal_run_id.clone().or(task.goal_run_id.clone());
        if session.mission.trim().is_empty() {
            session.mission = task.description.clone();
        }
        session.updated_at = now_millis();

        let snapshot = session.clone();
        drop(collaboration);
        self.persist_collaboration_session(&snapshot).await?;
        Ok(snapshot)
    }

    async fn persisted_collaboration_session(
        &self,
        parent_task_id: &str,
    ) -> Result<Option<CollaborationSession>> {
        let row = self
            .history
            .list_collaboration_sessions()
            .await?
            .into_iter()
            .find(|row| row.parent_task_id == parent_task_id);
        let Some(row) = row else {
            return Ok(None);
        };
        serde_json::from_str(&row.session_json)
            .map(Some)
            .map_err(|error| anyhow::anyhow!(error))
    }

    async fn merged_persisted_collaboration_sessions(&self) -> Result<Vec<CollaborationSession>> {
        let persisted = self.history.list_collaboration_sessions().await?;
        let mut sessions = std::collections::BTreeMap::new();
        for row in persisted {
            let session: CollaborationSession =
                serde_json::from_str(&row.session_json).map_err(|error| anyhow::anyhow!(error))?;
            sessions.insert(session.parent_task_id.clone(), session);
        }

        let collaboration = self.collaboration.read().await;
        for session in collaboration.values() {
            sessions.insert(session.parent_task_id.clone(), session.clone());
        }

        Ok(sessions.into_values().collect())
    }

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

        self.maybe_auto_escalate_collaboration_debate(parent_task_id)
            .await?;
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
                message: format!("Unresolved subagent disagreement on {}", disagreement.topic),
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
        if let Some(parent_task_id) = parent_task_id {
            let collaboration = self.collaboration.read().await;
            if let Some(session) = collaboration.get(parent_task_id) {
                return Ok(serde_json::to_value(session).unwrap_or_else(|_| serde_json::json!({})));
            }
            drop(collaboration);
            if let Some(session) = self.persisted_collaboration_session(parent_task_id).await? {
                return Ok(serde_json::to_value(session).unwrap_or_else(|_| serde_json::json!({})));
            }
            return Ok(serde_json::json!([]));
        }
        Ok(
            serde_json::to_value(self.merged_persisted_collaboration_sessions().await?)
                .unwrap_or_else(|_| serde_json::json!([])),
        )
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
        if let Err(error) = self.ensure_task_collaboration_session(task).await {
            tracing::warn!(
                task_id = %task.id,
                "failed to initialize solo collaboration session: {error}"
            );
            return;
        }
        if let Err(error) = self
            .record_collaboration_contribution(
                &task.id,
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
                parent_task_id = %task.id,
                "failed to record collaboration outcome: {error}"
            );
        }

        let parent_session_exists = {
            let collaboration = self.collaboration.read().await;
            collaboration.contains_key(parent_task_id)
        };
        if parent_session_exists {
            let _ = self
                .record_collaboration_contribution(
                    parent_task_id,
                    &task.id,
                    &task.title,
                    position,
                    vec![crate::agent::summarize_text(summary, 220)],
                    if outcome == "success" { 0.8 } else { 0.6 },
                )
                .await;
        }
    }

    async fn maybe_auto_escalate_collaboration_debate(
        &self,
        parent_task_id: &str,
    ) -> Result<Option<String>> {
        let debate_config = {
            let config = self.config.read().await;
            if !config.collaboration.enabled || !config.debate.enabled {
                return Ok(None);
            }
            config.debate.clone()
        };

        let launch = {
            let mut collaboration = self.collaboration.write().await;
            let Some(session) = collaboration.get_mut(parent_task_id) else {
                return Ok(None);
            };
            let Some(seed) = build_pending_debate_seed(session, &debate_config) else {
                return Ok(None);
            };
            let Some(disagreement) = session
                .disagreements
                .iter_mut()
                .find(|item| item.id == seed.disagreement_id)
            else {
                return Ok(None);
            };
            disagreement.debate_session_id = Some(seed.debate_session.id.clone());
            session.updated_at = now_millis();

            PendingDebateLaunch {
                seed,
                collaboration_snapshot: session.clone(),
            }
        };

        self.persist_collaboration_session(&launch.collaboration_snapshot)
            .await?;

        match self
            .persist_seeded_debate_session(
                launch.seed.debate_session.clone(),
                launch.seed.arguments.clone(),
            )
            .await
        {
            Ok(session) => Ok(Some(session.id)),
            Err(error) => {
                let rollback_snapshot = {
                    let mut collaboration = self.collaboration.write().await;
                    let Some(session) = collaboration.get_mut(parent_task_id) else {
                        return Err(error);
                    };
                    if let Some(disagreement) = session
                        .disagreements
                        .iter_mut()
                        .find(|item| item.id == launch.seed.disagreement_id)
                    {
                        if disagreement.debate_session_id.as_deref()
                            == Some(launch.seed.debate_session.id.as_str())
                        {
                            disagreement.debate_session_id = None;
                        }
                    }
                    session.updated_at = now_millis();
                    session.clone()
                };
                self.persist_collaboration_session(&rollback_snapshot)
                    .await?;
                Err(error)
            }
        }
    }
}
