use std::collections::{BTreeSet, HashMap};

use super::*;

pub(super) fn infer_collaboration_role(task: &AgentTask) -> String {
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

pub(super) fn normalize_topic(topic: &str) -> String {
    topic.trim().to_ascii_lowercase()
}

pub(in crate::agent) fn detect_disagreements(session: &mut CollaborationSession) {
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
        let unique_labels = unique_positions.iter().cloned().collect::<BTreeSet<_>>();
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
            debate_session_id: preserved.and_then(|item| item.debate_session_id.clone()),
        });
    }
    session.disagreements = next;
}

pub(in crate::agent) fn normalize_position(position: &str) -> String {
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

pub(super) fn apply_vote_to_disagreement(
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
        .collect::<BTreeSet<_>>()
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
