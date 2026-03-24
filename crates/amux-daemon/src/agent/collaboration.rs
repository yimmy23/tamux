//! Lightweight multi-agent collaboration state built on top of real subagent tasks.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(super) struct CollaborationSession {
    pub id: String,
    pub parent_task_id: String,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub mission: String,
    pub agents: Vec<CollaborativeAgent>,
    pub contributions: Vec<Contribution>,
    pub disagreements: Vec<Disagreement>,
    pub consensus: Option<Consensus>,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct CollaborativeAgent {
    pub task_id: String,
    pub title: String,
    pub role: String,
    pub confidence: f64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Contribution {
    pub id: String,
    pub task_id: String,
    pub topic: String,
    pub position: String,
    pub evidence: Vec<String>,
    pub confidence: f64,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Disagreement {
    pub id: String,
    pub topic: String,
    pub agents: Vec<String>,
    pub positions: Vec<String>,
    pub confidence_gap: f64,
    pub resolution: String,
    #[serde(default)]
    pub votes: Vec<Vote>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Vote {
    pub task_id: String,
    pub position: String,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Consensus {
    pub topic: String,
    pub winner: String,
    pub rationale: String,
    pub votes: Vec<Vote>,
}

impl AgentEngine {
    async fn persist_collaboration_session(&self, session: &CollaborationSession) -> Result<()> {
        let session_json = serde_json::to_string(session)?;
        self.history.upsert_collaboration_session(
            &session.parent_task_id,
            &session_json,
            session.updated_at,
        ).await
    }

    pub(super) async fn register_subagent_collaboration(
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

    pub(super) async fn record_collaboration_contribution(
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

    pub(super) async fn collaboration_peer_memory_json(
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

    pub(super) async fn vote_on_collaboration_disagreement(
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
        session.updated_at = now_millis();
        let snapshot = session.clone();
        let report = serde_json::json!({
            "session_id": session_id,
            "resolution": resolution,
            "consensus": consensus,
        });
        drop(collaboration);
        self.persist_collaboration_session(&snapshot).await?;

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

    pub(super) async fn record_collaboration_outcome(&self, task: &AgentTask, outcome: &str) {
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

fn infer_collaboration_role(task: &AgentTask) -> String {
    let lowered = format!(
        "{} {}",
        task.title.to_ascii_lowercase(),
        task.description.to_ascii_lowercase()
    );
    if lowered.contains("review") || lowered.contains("audit") {
        "review".to_string()
    } else if lowered.contains("research") || lowered.contains("analyze") {
        "research".to_string()
    } else if lowered.contains("plan") {
        "planning".to_string()
    } else {
        "execution".to_string()
    }
}

fn normalize_topic(topic: &str) -> String {
    topic.trim().to_ascii_lowercase()
}

fn detect_disagreements(session: &mut CollaborationSession) {
    let previous = session
        .disagreements
        .iter()
        .cloned()
        .map(|item| (item.topic.clone(), item))
        .collect::<HashMap<_, _>>();
    let mut latest_by_topic: HashMap<String, Vec<&Contribution>> = HashMap::new();
    for contribution in session.contributions.iter().rev() {
        latest_by_topic
            .entry(contribution.topic.clone())
            .or_default()
            .push(contribution);
    }

    let mut next = Vec::new();
    for (topic, entries) in latest_by_topic {
        if entries.len() < 2 {
            continue;
        }
        let unique_positions = entries
            .iter()
            .map(|entry| normalize_position(&entry.position))
            .collect::<Vec<_>>();
        let unique_labels = unique_positions
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        if unique_labels.len() < 2 {
            continue;
        }
        let max_confidence = entries
            .iter()
            .map(|entry| entry.confidence)
            .fold(0.0, f64::max);
        let min_confidence = entries
            .iter()
            .map(|entry| entry.confidence)
            .fold(1.0, f64::min);
        let preserved = previous.get(&topic);
        let participating_agents = entries
            .iter()
            .map(|entry| entry.task_id.clone())
            .collect::<Vec<_>>();
        next.push(Disagreement {
            id: preserved
                .map(|item| item.id.clone())
                .unwrap_or_else(|| format!("disagree_{}", uuid::Uuid::new_v4())),
            topic,
            agents: participating_agents.clone(),
            positions: unique_labels.into_iter().collect(),
            confidence_gap: (max_confidence - min_confidence).abs(),
            resolution: preserved
                .map(|item| item.resolution.clone())
                .unwrap_or_else(|| "pending".to_string()),
            votes: preserved
                .map(|item| {
                    item.votes
                        .iter()
                        .filter(|vote| {
                            participating_agents
                                .iter()
                                .any(|agent| agent == &vote.task_id)
                        })
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        });
    }
    session.disagreements = next;
}

fn normalize_position(position: &str) -> String {
    let lowered = position.trim().to_ascii_lowercase();
    if lowered.contains("reject") || lowered.contains("avoid") || lowered.contains("no") {
        "reject".to_string()
    } else if lowered.contains("use") || lowered.contains("yes") || lowered.contains("prefer") {
        "recommend".to_string()
    } else {
        lowered
    }
}

fn role_weight(role: &str, topic: &str) -> f64 {
    match role {
        "review" if topic.contains("risk") || topic.contains("security") => 1.3,
        "research" => 1.2,
        "execution" => 1.1,
        "planning" => 1.0,
        _ => 1.0,
    }
}

fn apply_vote_to_disagreement(
    disagreement: &mut Disagreement,
    agents: &[CollaborativeAgent],
    task_id: &str,
    position: &str,
    confidence: Option<f64>,
) -> Option<Consensus> {
    let weight = confidence.unwrap_or_else(|| {
        agents
            .iter()
            .find(|agent| agent.task_id == task_id)
            .map(|agent| agent.confidence * role_weight(&agent.role, &disagreement.topic))
            .unwrap_or(0.5)
    });
    let normalized_vote = normalize_position(position);
    if let Some(existing_vote) = disagreement
        .votes
        .iter_mut()
        .find(|vote| vote.task_id == task_id)
    {
        existing_vote.position = normalized_vote.clone();
        existing_vote.weight = weight;
    } else {
        disagreement.votes.push(Vote {
            task_id: task_id.to_string(),
            position: normalized_vote.clone(),
            weight,
        });
    }

    let mut by_position: HashMap<String, f64> = HashMap::new();
    for vote in &disagreement.votes {
        *by_position.entry(vote.position.clone()).or_insert(0.0) += vote.weight;
    }
    let mut weighted_positions = by_position
        .iter()
        .map(|(label, weight)| (label.clone(), *weight))
        .collect::<Vec<_>>();
    weighted_positions.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let winner = weighted_positions
        .first()
        .map(|(label, _)| label.clone())
        .unwrap_or(normalized_vote);
    let vote_margin = if weighted_positions.len() >= 2 {
        (weighted_positions[0].1 - weighted_positions[1].1).abs()
    } else {
        weighted_positions.first().map(|item| item.1).unwrap_or(0.0)
    };
    let distinct_voters = disagreement
        .votes
        .iter()
        .map(|vote| vote.task_id.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    if distinct_voters < 2 {
        disagreement.resolution = "pending".to_string();
        return None;
    }

    disagreement.resolution = if vote_margin < 0.15 {
        "escalated".to_string()
    } else {
        "resolved".to_string()
    };
    Some(Consensus {
        topic: disagreement.topic.clone(),
        winner: winner.clone(),
        rationale: if disagreement.resolution == "escalated" {
            "weighted vote was too close; escalate to operator".to_string()
        } else {
            format!("weighted vote favored `{winner}`")
        },
        votes: disagreement.votes.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_vote_to_disagreement_accumulates_votes_before_resolving() {
        let mut disagreement = Disagreement {
            id: "d1".to_string(),
            topic: "auth library".to_string(),
            agents: vec!["a".to_string(), "b".to_string()],
            positions: vec!["recommend".to_string(), "reject".to_string()],
            confidence_gap: 0.4,
            resolution: "pending".to_string(),
            votes: Vec::new(),
        };
        let agents = vec![
            CollaborativeAgent {
                task_id: "a".to_string(),
                title: "Research".to_string(),
                role: "research".to_string(),
                confidence: 0.8,
                status: "running".to_string(),
            },
            CollaborativeAgent {
                task_id: "b".to_string(),
                title: "Review".to_string(),
                role: "review".to_string(),
                confidence: 0.9,
                status: "running".to_string(),
            },
        ];

        let first = apply_vote_to_disagreement(&mut disagreement, &agents, "a", "recommend", None);
        assert!(first.is_none());
        assert_eq!(disagreement.resolution, "pending");

        let second = apply_vote_to_disagreement(&mut disagreement, &agents, "b", "recommend", None)
            .expect("second vote should resolve");
        assert_eq!(disagreement.resolution, "resolved");
        assert_eq!(second.winner, "recommend");
        assert_eq!(second.votes.len(), 2);
    }

    #[test]
    fn detect_disagreements_preserves_existing_votes() {
        let mut session = CollaborationSession {
            id: "s1".to_string(),
            parent_task_id: "p1".to_string(),
            thread_id: None,
            goal_run_id: None,
            mission: "test".to_string(),
            agents: Vec::new(),
            contributions: vec![
                Contribution {
                    id: "c1".to_string(),
                    task_id: "a".to_string(),
                    topic: "auth".to_string(),
                    position: "recommend".to_string(),
                    evidence: Vec::new(),
                    confidence: 0.9,
                    created_at: 1,
                },
                Contribution {
                    id: "c2".to_string(),
                    task_id: "b".to_string(),
                    topic: "auth".to_string(),
                    position: "reject".to_string(),
                    evidence: Vec::new(),
                    confidence: 0.7,
                    created_at: 2,
                },
            ],
            disagreements: vec![Disagreement {
                id: "existing".to_string(),
                topic: "auth".to_string(),
                agents: vec!["a".to_string(), "b".to_string()],
                positions: vec!["recommend".to_string(), "reject".to_string()],
                confidence_gap: 0.2,
                resolution: "pending".to_string(),
                votes: vec![Vote {
                    task_id: "a".to_string(),
                    position: "recommend".to_string(),
                    weight: 1.0,
                }],
            }],
            consensus: None,
            updated_at: 0,
        };

        detect_disagreements(&mut session);

        assert_eq!(session.disagreements.len(), 1);
        assert_eq!(session.disagreements[0].id, "existing");
        assert_eq!(session.disagreements[0].votes.len(), 1);
        assert_eq!(session.disagreements[0].votes[0].task_id, "a");
    }
}
