use super::participants::{apply_vote_to_disagreement, normalize_position, normalize_topic};
use crate::agent::consensus::bid_engine::{build_persisted_bid, consensus_round_id};
use crate::agent::consensus::bid_priors::effective_bid_confidence;
use crate::agent::consensus::outcome_feedback::build_quality_metric;
use crate::agent::consensus::role_assigner::build_role_assignment;
use crate::agent::explanation::{confidence_band, generate_explanation, ExplanationResult};
use crate::history::AuditEntryRow;
use rusqlite::OptionalExtension;

const MIN_CONSENSUS_BID_CONFIDENCE: f64 = 0.3;
use super::*;
use crate::agent::debate::protocol::create_debate_session;
use crate::agent::debate::types::{Argument, DebateSession, RoleKind};
use crate::agent::handoff::divergent::Framing;
use std::collections::{BTreeSet, HashMap};

fn bid_sort_key(availability: &BidAvailability) -> u8 {
    match availability {
        BidAvailability::Available => 0,
        BidAvailability::Busy => 1,
        BidAvailability::Unavailable => 2,
    }
}

fn clamp_collaboration_learned_score(success_count: u64, failure_count: u64) -> f64 {
    let attempts = (success_count + failure_count) as f64;
    let successes = success_count as f64;
    ((successes + 1.0) / (attempts + 2.0)).clamp(0.1, 0.95)
}

fn bid_availability_label(availability: &BidAvailability) -> &'static str {
    match availability {
        BidAvailability::Available => "available",
        BidAvailability::Busy => "busy",
        BidAvailability::Unavailable => "unavailable",
    }
}

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

fn collaboration_resolution_outcome(session: &CollaborationSession) -> Option<serde_json::Value> {
    let disagreement = session
        .disagreements
        .iter()
        .find(|item| item.resolution == "resolved")?;
    let winner_task_id = session
        .consensus
        .as_ref()
        .map(|consensus| consensus.winner.clone());
    let reviewer_task_id = session
        .role_assignment
        .as_ref()
        .map(|assignment| assignment.reviewer_task_id.clone());
    let rationale = session
        .consensus
        .as_ref()
        .map(|consensus| consensus.rationale.clone())
        .unwrap_or_default();

    Some(serde_json::json!({
        "status": disagreement.resolution,
        "disagreement_id": disagreement.id,
        "topic": disagreement.topic,
        "winner_task_id": winner_task_id,
        "reviewer_task_id": reviewer_task_id,
        "rationale": rationale,
        "debate_session_id": disagreement.debate_session_id,
    }))
}

fn collaboration_bid_payload(bid: &ConsensusBid) -> serde_json::Value {
    serde_json::json!({
        "task_id": bid.task_id,
        "confidence": bid.confidence,
        "availability": bid_availability_label(&bid.availability),
        "created_at": bid.created_at,
    })
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

        let mut latest_by_task: HashMap<&str, (&Contribution, usize)> = HashMap::new();
        for (source_index, contribution) in session.contributions.iter().enumerate() {
            if contribution.topic != disagreement.topic {
                continue;
            }
            latest_by_task.insert(contribution.task_id.as_str(), (contribution, source_index));
        }

        let mut latest = latest_by_task.into_values().collect::<Vec<_>>();
        latest.sort_by(|(left, left_index), (right, right_index)| {
            left.created_at
                .cmp(&right.created_at)
                .then(left_index.cmp(right_index))
        });
        let latest = latest
            .into_iter()
            .map(|(contribution, _)| contribution)
            .collect::<Vec<_>>();

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

fn seed_debate_from_bid_resolution(
    session: &mut CollaborationSession,
    ranked: &[ConsensusBid],
    debate_config: &DebateConfig,
) -> Option<PendingDebateSeed> {
    let thread_id = session.thread_id.as_ref()?.clone();
    let call_metadata = session.call_metadata.clone()?;
    let primary = ranked.first()?.clone();
    let reviewer = ranked
        .iter()
        .find(|bid| bid.task_id != primary.task_id)
        .cloned()?;

    let tied_confidence = (primary.confidence - reviewer.confidence).abs() <= f64::EPSILON;
    let tied_availability_rank =
        bid_sort_key(&primary.availability) == bid_sort_key(&reviewer.availability);
    if !tied_confidence || !tied_availability_rank {
        return None;
    }

    let topic = normalize_topic(&format!("bid resolution for {}", session.mission));
    if session
        .disagreements
        .iter()
        .any(|disagreement| disagreement.topic == topic && disagreement.debate_session_id.is_some())
    {
        return None;
    }

    let disagreement_id = if let Some(disagreement) = session
        .disagreements
        .iter_mut()
        .find(|disagreement| disagreement.topic == topic)
    {
        disagreement.agents = vec![primary.task_id.clone(), reviewer.task_id.clone()];
        disagreement.positions = vec![
            format!("assign {} as primary", primary.task_id),
            format!("assign {} as primary", reviewer.task_id),
        ];
        disagreement.confidence_gap = (primary.confidence - reviewer.confidence).abs();
        disagreement.resolution = "pending".to_string();
        disagreement.votes.clear();
        disagreement.id.clone()
    } else {
        let disagreement_id = format!("disagree_{}", uuid::Uuid::new_v4());
        session.disagreements.push(Disagreement {
            id: disagreement_id.clone(),
            topic: topic.clone(),
            agents: vec![primary.task_id.clone(), reviewer.task_id.clone()],
            positions: vec![
                format!("assign {} as primary", primary.task_id),
                format!("assign {} as primary", reviewer.task_id),
            ],
            confidence_gap: (primary.confidence - reviewer.confidence).abs(),
            resolution: "pending".to_string(),
            votes: Vec::new(),
            debate_session_id: None,
        });
        disagreement_id
    };

    let mut framings = Vec::new();
    for bid in [&primary, &reviewer] {
        framings.push(Framing {
            label: format!("{}-bid-lens", bid.task_id),
            system_prompt_override: format!(
                "Defend assigning {} as primary for {} using the contested bid evidence.",
                bid.task_id, session.mission
            ),
            task_id: Some(bid.task_id.clone()),
            contribution_id: None,
        });
    }
    framings.push(Framing {
        label: "synthesis-lens".to_string(),
        system_prompt_override: format!(
            "Synthesize the strongest assignment for {} without erasing the contested bid evidence.",
            session.mission
        ),
        task_id: None,
        contribution_id: None,
    });

    let debate_session = create_debate_session(
        topic.clone(),
        framings,
        debate_config.default_max_rounds,
        debate_config.role_rotation,
        Some(thread_id),
        session.goal_run_id.clone(),
    )
    .ok()?;

    if let Some(disagreement) = session
        .disagreements
        .iter_mut()
        .find(|disagreement| disagreement.id == disagreement_id)
    {
        disagreement.debate_session_id = Some(debate_session.id.clone());
    }

    let eligible_agents = if call_metadata.eligible_agents.is_empty() {
        vec![primary.task_id.clone(), reviewer.task_id.clone()]
    } else {
        call_metadata.eligible_agents.clone()
    };
    let shared_evidence = vec![
        format!(
            "bid call parent_task_id={} caller_task_id={} eligible_agents={}",
            session.parent_task_id,
            call_metadata.caller_task_id,
            eligible_agents.join(",")
        ),
        format!(
            "contested bid candidates={},{} confidences={:.2},{:.2} availability={:?},{:?}",
            primary.task_id,
            reviewer.task_id,
            primary.confidence,
            reviewer.confidence,
            primary.availability,
            reviewer.availability
        ),
    ];

    let arguments = vec![
        Argument {
            id: format!("arg_{}", uuid::Uuid::new_v4()),
            round: 1,
            role: RoleKind::Proponent,
            agent_id: primary.task_id.clone(),
            content: format!(
                "Assign {} as primary for {}.",
                primary.task_id, session.mission
            ),
            evidence_refs: shared_evidence.clone(),
            responds_to: None,
            timestamp_ms: primary.created_at,
        },
        Argument {
            id: format!("arg_{}", uuid::Uuid::new_v4()),
            round: 1,
            role: RoleKind::Skeptic,
            agent_id: reviewer.task_id.clone(),
            content: format!(
                "Assign {} as primary for {}.",
                reviewer.task_id, session.mission
            ),
            evidence_refs: shared_evidence,
            responds_to: None,
            timestamp_ms: reviewer.created_at,
        },
    ];

    Some(PendingDebateSeed {
        disagreement_id,
        debate_session,
        arguments,
    })
}

impl AgentEngine {
    async fn collaboration_outcome_scores(
        &self,
        parent_task_id: &str,
        task_ids: &[String],
    ) -> Result<HashMap<String, f64>> {
        let parent_task_id = parent_task_id.to_string();
        let task_ids = task_ids.to_vec();
        self.history
            .read_conn
            .call(move |conn| {
                let mut scores = HashMap::new();
                let mut stmt = conn.prepare(
                    "SELECT task_id, learned_score FROM collaboration_agent_outcomes WHERE parent_task_id = ?1 AND task_id = ?2",
                )?;
                for task_id in task_ids {
                    if let Some(score) = stmt
                        .query_row(rusqlite::params![&parent_task_id, &task_id], |row| row.get::<_, f64>(1))
                        .optional()?
                    {
                        scores.insert(task_id, score);
                    }
                }
                Ok(scores)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn record_collaboration_agent_outcome(
        &self,
        parent_task_id: &str,
        task_id: &str,
        outcome: &str,
    ) -> Result<()> {
        let parent_task_id = parent_task_id.to_string();
        let task_id = task_id.to_string();
        let outcome = outcome.to_string();
        let updated_at_ms = now_millis();
        self.history
            .conn
            .call(move |conn| {
                let existing = conn
                    .query_row(
                        "SELECT success_count, failure_count FROM collaboration_agent_outcomes WHERE parent_task_id = ?1 AND task_id = ?2",
                        rusqlite::params![&parent_task_id, &task_id],
                        |row| Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64)),
                    )
                    .optional()?;
                let (mut success_count, mut failure_count) = existing.unwrap_or((0, 0));
                match outcome.as_str() {
                    "success" | "completed" | "accepted" => success_count += 1,
                    _ => failure_count += 1,
                }
                let learned_score = clamp_collaboration_learned_score(success_count, failure_count);
                conn.execute(
                    "INSERT INTO collaboration_agent_outcomes (
                        parent_task_id, task_id, success_count, failure_count, learned_score, last_outcome, updated_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(parent_task_id, task_id) DO UPDATE SET
                        success_count = excluded.success_count,
                        failure_count = excluded.failure_count,
                        learned_score = excluded.learned_score,
                        last_outcome = excluded.last_outcome,
                        updated_at_ms = excluded.updated_at_ms",
                    rusqlite::params![
                        parent_task_id,
                        task_id,
                        success_count as i64,
                        failure_count as i64,
                        learned_score,
                        outcome,
                        updated_at_ms as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

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
                    call_metadata: None,
                    bids: Vec::new(),
                    role_assignment: None,
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

    async fn bootstrap_bid_dispatch_collaboration(
        &self,
        parent_task_id: &str,
        bid_task_ids: &[String],
    ) -> Result<()> {
        let (parent, eligible_subagents) = {
            let tasks = self.tasks.lock().await;
            let parent = tasks
                .iter()
                .find(|task| task.id == parent_task_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("unknown parent task {parent_task_id}"))?;
            let eligible_subagents = bid_task_ids
                .iter()
                .filter_map(|task_id| {
                    tasks.iter().find(|task| {
                        task.id == *task_id
                            && task.source == "subagent"
                            && task.parent_task_id.as_deref() == Some(parent_task_id)
                    })
                })
                .cloned()
                .collect::<Vec<_>>();
            (parent, eligible_subagents)
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
                call_metadata: None,
                bids: Vec::new(),
                role_assignment: None,
                contributions: Vec::new(),
                disagreements: Vec::new(),
                consensus: None,
                updated_at: now_millis(),
            });

        session.thread_id = session
            .thread_id
            .clone()
            .or(parent.thread_id.clone())
            .or(parent.parent_thread_id.clone());
        session.goal_run_id = session.goal_run_id.clone().or(parent.goal_run_id.clone());
        if session.mission.trim().is_empty() {
            session.mission = parent.description.clone();
        }

        for subagent in eligible_subagents {
            if session
                .agents
                .iter()
                .any(|agent| agent.task_id == subagent.id)
            {
                continue;
            }
            session.agents.push(CollaborativeAgent {
                task_id: subagent.id.clone(),
                title: subagent.title.clone(),
                role: infer_collaboration_role(&subagent),
                confidence: 0.5,
                status: format!("{:?}", subagent.status).to_lowercase(),
            });
        }
        session.updated_at = now_millis();
        let snapshot = session.clone();
        drop(collaboration);

        self.persist_collaboration_session(&snapshot).await?;
        Ok(())
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

    async fn persist_collaboration_resolution_trace(
        &self,
        session: &CollaborationSession,
        assignment: &ConsensusRoleAssignment,
        ranked: &[ConsensusBid],
        role_by_task: &HashMap<String, String>,
        prior_by_role: &HashMap<String, f64>,
    ) {
        let Some(primary_bid) = ranked
            .iter()
            .find(|bid| bid.task_id == assignment.primary_task_id)
        else {
            return;
        };
        let Some(reviewer_bid) = ranked
            .iter()
            .find(|bid| bid.task_id == assignment.reviewer_task_id)
        else {
            return;
        };

        let primary_role = role_by_task
            .get(&assignment.primary_task_id)
            .cloned()
            .unwrap_or_else(|| assignment.primary_role.clone());
        let reviewer_role = role_by_task
            .get(&assignment.reviewer_task_id)
            .cloned()
            .unwrap_or_else(|| assignment.reviewer_role.clone());
        let primary_prior = prior_by_role.get(&primary_role).copied().unwrap_or(0.5);
        let reviewer_prior = prior_by_role.get(&reviewer_role).copied().unwrap_or(0.5);
        let primary_effective = effective_bid_confidence(primary_bid.confidence, primary_prior);
        let reviewer_effective = effective_bid_confidence(reviewer_bid.confidence, reviewer_prior);

        let mut rejected_options = ranked
            .iter()
            .filter(|bid| bid.task_id != assignment.primary_task_id)
            .map(|bid| {
                let role = role_by_task
                    .get(&bid.task_id)
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                let prior = prior_by_role.get(&role).copied().unwrap_or(0.5);
                crate::agent::learning::traces::DecisionOption {
                    option_type: "collaboration_bid".to_string(),
                    reasoning: format!(
                        "Retained {} as a non-primary collaboration bid for role {} with {:.2} confidence and {} availability.",
                        bid.task_id,
                        role,
                        bid.confidence,
                        bid_availability_label(&bid.availability)
                    ),
                    rejection_reason: Some(format!(
                        "ranked behind {} after prior-adjusted confidence {:.2} vs {:.2}",
                        assignment.primary_task_id,
                        effective_bid_confidence(bid.confidence, prior),
                        primary_effective
                    )),
                    estimated_success_prob: Some(effective_bid_confidence(bid.confidence, prior)),
                    arguments_hash: Some(crate::agent::learning::traces::hash_context_blob(
                        &format!(
                            "{}|{}|{:.3}|{}|{}",
                            session.parent_task_id,
                            bid.task_id,
                            bid.confidence,
                            bid_availability_label(&bid.availability),
                            role
                        ),
                    )),
                }
            })
            .collect::<Vec<_>>();
        rejected_options.sort_by(|left, right| {
            right
                .estimated_success_prob
                .unwrap_or(0.0)
                .partial_cmp(&left.estimated_success_prob.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut factors = vec![crate::agent::learning::traces::CausalFactor {
            factor_type: crate::agent::learning::traces::FactorType::PatternMatch,
            description: format!(
                "resolved collaboration bid round with {} candidate(s)",
                ranked.len()
            ),
            weight: 0.7,
        }];
        factors.push(crate::agent::learning::traces::CausalFactor {
            factor_type: crate::agent::learning::traces::FactorType::PastSuccess,
            description: format!(
                "winner role {} carried prior {:.2} and effective confidence {:.2}",
                primary_role, primary_prior, primary_effective
            ),
            weight: 0.65,
        });
        factors.push(crate::agent::learning::traces::CausalFactor {
            factor_type: crate::agent::learning::traces::FactorType::PatternMatch,
            description: format!(
                "reviewer {} remained with role {} and effective confidence {:.2}",
                assignment.reviewer_task_id, reviewer_role, reviewer_effective
            ),
            weight: 0.45,
        });
        if ranked
            .iter()
            .any(|bid| matches!(bid.availability, BidAvailability::Busy))
        {
            factors.push(crate::agent::learning::traces::CausalFactor {
                factor_type: crate::agent::learning::traces::FactorType::ResourceConstraint,
                description: "one or more bids were availability-constrained during ranking"
                    .to_string(),
                weight: 0.35,
            });
        }

        let selected = crate::agent::learning::traces::DecisionOption {
            option_type: "collaboration_bid_resolution".to_string(),
            reasoning: format!(
                "Assigned primary {} ({}) over reviewer {} ({}) after ranking prior-adjusted collaboration bids.",
                assignment.primary_task_id,
                primary_role,
                assignment.reviewer_task_id,
                reviewer_role
            ),
            rejection_reason: None,
            estimated_success_prob: Some(primary_effective),
            arguments_hash: Some(crate::agent::learning::traces::hash_context_blob(&format!(
                "{}|{}|{}|{}|{}",
                session.parent_task_id,
                assignment.primary_task_id,
                assignment.reviewer_task_id,
                assignment.assigned_at,
                ranked
                    .iter()
                    .map(|bid| format!(
                        "{}:{:.3}:{}",
                        bid.task_id,
                        bid.confidence,
                        bid_availability_label(&bid.availability)
                    ))
                    .collect::<Vec<_>>()
                    .join("|")
            ))),
        };
        let trace = crate::agent::learning::traces::CausalTrace {
            trace_id: format!("causal_{}", uuid::Uuid::new_v4()),
            thread_id: session.thread_id.clone(),
            goal_run_id: session.goal_run_id.clone(),
            task_id: Some(session.parent_task_id.clone()),
            decision_type: crate::agent::learning::traces::DecisionType::CollaborationResolution,
            selected,
            rejected_options,
            context_hash: crate::agent::learning::traces::hash_context_blob(&format!(
                "{}|{}|{}|{}|{}",
                session.parent_task_id,
                session.mission,
                assignment.primary_task_id,
                assignment.reviewer_task_id,
                assignment.assigned_at
            )),
            causal_factors: factors,
            outcome: crate::agent::learning::traces::CausalTraceOutcome::Success,
            model_used: Some(self.config.read().await.model.clone()),
            created_at: now_millis(),
        };
        let selected_json = serde_json::to_string(&trace.selected).unwrap_or_default();
        let rejected_json = serde_json::to_string(&trace.rejected_options).unwrap_or_default();
        let factors_json = serde_json::to_string(&trace.causal_factors).unwrap_or_default();
        let outcome_json = serde_json::to_string(&trace.outcome).unwrap_or_default();
        if let Err(error) = self
            .history
            .insert_causal_trace(
                &trace.trace_id,
                trace.thread_id.as_deref(),
                trace.goal_run_id.as_deref(),
                trace.task_id.as_deref(),
                "collaboration_resolution",
                trace.decision_type.family_label(),
                &selected_json,
                &rejected_json,
                &trace.context_hash,
                &factors_json,
                &outcome_json,
                trace.model_used.as_deref(),
                trace.created_at,
            )
            .await
        {
            tracing::warn!(
                parent_task_id = %session.parent_task_id,
                "failed to persist collaboration resolution causal trace: {error}"
            );
            return;
        }

        let confidence_val = trace.selected.estimated_success_prob;
        let data_json = serde_json::json!({
            "task_title": session.mission,
            "primary_task_id": assignment.primary_task_id,
            "reviewer_task_id": assignment.reviewer_task_id,
        });
        let summary = match generate_explanation("subagent_spawn", &data_json) {
            ExplanationResult::Template(summary) => summary,
            ExplanationResult::NeedsLlm => format!(
                "Resolved collaboration bid round for {} with primary {} and reviewer {}",
                session.parent_task_id, assignment.primary_task_id, assignment.reviewer_task_id
            ),
        };
        let audit_entry = AuditEntryRow {
            id: format!("audit-collaboration-resolution-{}", trace.trace_id),
            timestamp: trace.created_at as i64,
            action_type: "collaboration_resolution".to_string(),
            summary: summary.clone(),
            explanation: Some(summary),
            confidence: confidence_val,
            confidence_band: confidence_val.map(|p| confidence_band(p).as_str().to_string()),
            causal_trace_id: Some(trace.trace_id.clone()),
            thread_id: trace.thread_id.clone(),
            goal_run_id: trace.goal_run_id.clone(),
            task_id: trace.task_id.clone(),
            raw_data_json: Some(
                serde_json::json!({
                    "session_id": session.id,
                    "parent_task_id": session.parent_task_id,
                    "mission": session.mission,
                    "primary_task_id": assignment.primary_task_id,
                    "reviewer_task_id": assignment.reviewer_task_id,
                    "primary_role": primary_role,
                    "reviewer_role": reviewer_role,
                    "primary_prior": primary_prior,
                    "reviewer_prior": reviewer_prior,
                    "primary_effective_confidence": primary_effective,
                    "reviewer_effective_confidence": reviewer_effective,
                    "ranked_bids": ranked.iter().map(collaboration_bid_payload).collect::<Vec<_>>(),
                })
                .to_string(),
            ),
        };
        if let Err(error) = self.history.insert_action_audit(&audit_entry).await {
            tracing::warn!(
                parent_task_id = %session.parent_task_id,
                "failed to persist collaboration resolution audit entry: {error}"
            );
        }
    }

    async fn persist_collaboration_outcome_trace(
        &self,
        task: &AgentTask,
        session: &CollaborationSession,
        outcome: &str,
        summary: &str,
    ) {
        let role_assignment = session.role_assignment.as_ref();
        let task_role = session
            .agents
            .iter()
            .find(|agent| agent.task_id == task.id)
            .map(|agent| agent.role.clone())
            .unwrap_or_else(|| super::participants::infer_collaboration_role(task));
        let selected = crate::agent::learning::traces::DecisionOption {
            option_type: "collaboration_outcome".to_string(),
            reasoning: format!(
                "Recorded collaboration outcome {} for {} in role {}.",
                outcome, task.id, task_role
            ),
            rejection_reason: None,
            estimated_success_prob: Some(if outcome == "success" { 0.8 } else { 0.35 }),
            arguments_hash: Some(crate::agent::learning::traces::hash_context_blob(&format!(
                "{}|{}|{}|{}",
                session.parent_task_id, task.id, outcome, summary
            ))),
        };
        let mut factors = vec![crate::agent::learning::traces::CausalFactor {
            factor_type: crate::agent::learning::traces::FactorType::PatternMatch,
            description: format!(
                "recorded settled collaboration outcome for role {}",
                task_role
            ),
            weight: 0.6,
        }];
        if let Some(assignment) = role_assignment {
            factors.push(crate::agent::learning::traces::CausalFactor {
                factor_type: crate::agent::learning::traces::FactorType::PatternMatch,
                description: format!(
                    "role assignment primary={} reviewer={}",
                    assignment.primary_task_id, assignment.reviewer_task_id
                ),
                weight: 0.45,
            });
        }
        factors.push(crate::agent::learning::traces::CausalFactor {
            factor_type: if outcome == "success" {
                crate::agent::learning::traces::FactorType::PastSuccess
            } else {
                crate::agent::learning::traces::FactorType::PastFailure
            },
            description: crate::agent::summarize_text(summary, 180),
            weight: 0.7,
        });

        let outcome_value = match outcome {
            "success" => crate::agent::learning::traces::CausalTraceOutcome::Success,
            "failure" => crate::agent::learning::traces::CausalTraceOutcome::Failure {
                reason: crate::agent::summarize_text(summary, 220),
            },
            "cancelled" => crate::agent::learning::traces::CausalTraceOutcome::Failure {
                reason: "collaboration task was cancelled before conclusion".to_string(),
            },
            _ => crate::agent::learning::traces::CausalTraceOutcome::Unresolved,
        };
        let trace = crate::agent::learning::traces::CausalTrace {
            trace_id: format!("causal_{}", uuid::Uuid::new_v4()),
            thread_id: session.thread_id.clone(),
            goal_run_id: session.goal_run_id.clone(),
            task_id: Some(task.id.clone()),
            decision_type: crate::agent::learning::traces::DecisionType::CollaborationOutcome,
            selected,
            rejected_options: Vec::new(),
            context_hash: crate::agent::learning::traces::hash_context_blob(&format!(
                "{}|{}|{}|{}",
                session.parent_task_id, task.id, outcome, session.updated_at
            )),
            causal_factors: factors,
            outcome: outcome_value,
            model_used: Some(self.config.read().await.model.clone()),
            created_at: now_millis(),
        };
        let selected_json = serde_json::to_string(&trace.selected).unwrap_or_default();
        let rejected_json = serde_json::to_string(&trace.rejected_options).unwrap_or_default();
        let factors_json = serde_json::to_string(&trace.causal_factors).unwrap_or_default();
        let outcome_json = serde_json::to_string(&trace.outcome).unwrap_or_default();
        if let Err(error) = self
            .history
            .insert_causal_trace(
                &trace.trace_id,
                trace.thread_id.as_deref(),
                trace.goal_run_id.as_deref(),
                trace.task_id.as_deref(),
                "collaboration_outcome",
                trace.decision_type.family_label(),
                &selected_json,
                &rejected_json,
                &trace.context_hash,
                &factors_json,
                &outcome_json,
                trace.model_used.as_deref(),
                trace.created_at,
            )
            .await
        {
            tracing::warn!(
                parent_task_id = %session.parent_task_id,
                task_id = %task.id,
                "failed to persist collaboration outcome causal trace: {error}"
            );
            return;
        }

        let confidence_val = trace.selected.estimated_success_prob;
        let audit_entry = AuditEntryRow {
            id: format!("audit-collaboration-outcome-{}", trace.trace_id),
            timestamp: trace.created_at as i64,
            action_type: "collaboration_outcome".to_string(),
            summary: format!(
                "Recorded collaboration outcome {} for {}",
                outcome, task.title
            ),
            explanation: Some(crate::agent::summarize_text(summary, 220)),
            confidence: confidence_val,
            confidence_band: confidence_val.map(|p| confidence_band(p).as_str().to_string()),
            causal_trace_id: Some(trace.trace_id.clone()),
            thread_id: trace.thread_id.clone(),
            goal_run_id: trace.goal_run_id.clone(),
            task_id: trace.task_id.clone(),
            raw_data_json: Some(
                serde_json::json!({
                    "session_id": session.id,
                    "parent_task_id": session.parent_task_id,
                    "task_id": task.id,
                    "task_title": task.title,
                    "role": task_role,
                    "outcome": outcome,
                    "summary": crate::agent::summarize_text(summary, 220),
                    "role_assignment": role_assignment,
                })
                .to_string(),
            ),
        };
        if let Err(error) = self.history.insert_action_audit(&audit_entry).await {
            tracing::warn!(
                parent_task_id = %session.parent_task_id,
                task_id = %task.id,
                "failed to persist collaboration outcome audit entry: {error}"
            );
        }
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
                call_metadata: None,
                bids: Vec::new(),
                role_assignment: None,
                contributions: Vec::new(),
                disagreements: Vec::new(),
                consensus: None,
                updated_at: now_millis(),
            });

        let inferred_role = infer_collaboration_role(subagent);
        let subagent_status = format!("{:?}", subagent.status).to_lowercase();
        if let Some(agent) = session
            .agents
            .iter_mut()
            .find(|agent| agent.task_id == subagent.id)
        {
            agent.title = subagent.title.clone();
            agent.role = inferred_role;
            agent.status = subagent_status;
        } else {
            session.agents.push(CollaborativeAgent {
                task_id: subagent.id.clone(),
                title: subagent.title.clone(),
                role: inferred_role,
                confidence: 0.5,
                status: subagent_status,
            });
        }
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

    pub(in crate::agent) async fn call_for_bids(
        &self,
        parent_task_id: &str,
        eligible_agents: &[String],
    ) -> Result<serde_json::Value> {
        if !self.config.read().await.collaboration.enabled {
            anyhow::bail!("collaboration capability is disabled in agent config");
        }
        let inferred_roles_by_task = {
            let tasks = self.tasks.lock().await;
            eligible_agents
                .iter()
                .filter_map(|task_id| {
                    tasks
                        .iter()
                        .find(|task| task.id == *task_id)
                        .map(|task| (task.id.clone(), infer_collaboration_role(task)))
                })
                .collect::<HashMap<_, _>>()
        };
        let mut collaboration = self.collaboration.write().await;
        let Some(session) = collaboration.get_mut(parent_task_id) else {
            anyhow::bail!("no collaboration session found for parent task {parent_task_id}");
        };
        session.call_metadata = Some(BidCallMetadata {
            caller_task_id: parent_task_id.to_string(),
            eligible_agents: eligible_agents.to_vec(),
            called_at: now_millis(),
        });
        session.bids.retain(|bid| {
            eligible_agents
                .iter()
                .any(|task_id| task_id == &bid.task_id)
        });
        session.role_assignment = None;
        for agent in session.agents.iter_mut() {
            if let Some(inferred_role) = inferred_roles_by_task.get(&agent.task_id) {
                agent.role = inferred_role.clone();
            }
        }
        session.updated_at = now_millis();
        let snapshot = session.clone();
        let report = serde_json::json!({
            "session_id": session.id,
            "eligible_agents": eligible_agents,
            "bid_count": session.bids.len(),
        });
        drop(collaboration);
        self.persist_collaboration_session(&snapshot).await?;
        Ok(report)
    }

    pub(in crate::agent) async fn submit_bid(
        &self,
        parent_task_id: &str,
        task_id: &str,
        confidence: f64,
        availability: BidAvailability,
    ) -> Result<serde_json::Value> {
        if !self.config.read().await.collaboration.enabled {
            anyhow::bail!("collaboration capability is disabled in agent config");
        }
        let mut collaboration = self.collaboration.write().await;
        let Some(session) = collaboration.get_mut(parent_task_id) else {
            anyhow::bail!("no collaboration session found for parent task {parent_task_id}");
        };
        if !session.agents.iter().any(|agent| agent.task_id == task_id) {
            anyhow::bail!("unknown collaborative agent {task_id}");
        }
        let bid = ConsensusBid {
            task_id: task_id.to_string(),
            confidence: confidence.clamp(0.0, 1.0),
            availability,
            created_at: now_millis(),
        };
        if let Some(existing) = session.bids.iter_mut().find(|item| item.task_id == task_id) {
            *existing = bid.clone();
        } else {
            session.bids.push(bid.clone());
        }
        session.updated_at = now_millis();
        let snapshot = session.clone();
        let persisted_bid = build_persisted_bid(
            &snapshot,
            task_id,
            bid.confidence,
            &bid.availability,
            bid.created_at,
        );
        let report = serde_json::json!({
            "session_id": session.id,
            "bid": bid,
        });
        drop(collaboration);
        self.persist_collaboration_session(&snapshot).await?;
        self.persist_consensus_bid(persisted_bid).await?;
        Ok(report)
    }

    pub(in crate::agent) async fn resolve_bids(
        &self,
        parent_task_id: &str,
    ) -> Result<serde_json::Value> {
        let config = self.config.read().await.clone();
        if !config.collaboration.enabled {
            anyhow::bail!("collaboration capability is disabled in agent config");
        }
        let debate_config = config.debate;
        let role_by_task = {
            let collaboration = self.collaboration.read().await;
            let Some(session) = collaboration.get(parent_task_id) else {
                anyhow::bail!("no collaboration session found for parent task {parent_task_id}");
            };
            session
                .agents
                .iter()
                .map(|agent| (agent.task_id.clone(), agent.role.clone()))
                .collect::<HashMap<_, _>>()
        };
        let prior_by_role = self
            .load_consensus_bid_priors(&role_by_task.values().cloned().collect::<Vec<_>>())
            .await?
            .into_iter()
            .map(|prior| (prior.role, prior.prior_score))
            .collect::<HashMap<_, _>>();
        let mut collaboration = self.collaboration.write().await;
        let Some(session) = collaboration.get_mut(parent_task_id) else {
            anyhow::bail!("no collaboration session found for parent task {parent_task_id}");
        };
        if session.bids.len() < 2 {
            anyhow::bail!("at least two bids are required to assign primary and reviewer roles");
        }
        let mut ranked = session.bids.clone();
        let learned_outcomes = self
            .collaboration_outcome_scores(
                parent_task_id,
                &ranked
                    .iter()
                    .map(|bid| bid.task_id.clone())
                    .collect::<Vec<_>>(),
            )
            .await?;
        let agent_confidence_by_task = session
            .agents
            .iter()
            .map(|agent| (agent.task_id.clone(), agent.confidence))
            .collect::<std::collections::HashMap<_, _>>();
        ranked.sort_by(|left, right| {
            let left_profile_confidence = agent_confidence_by_task
                .get(&left.task_id)
                .copied()
                .unwrap_or(0.0);
            let right_profile_confidence = agent_confidence_by_task
                .get(&right.task_id)
                .copied()
                .unwrap_or(0.0);
            let left_prior = role_by_task
                .get(&left.task_id)
                .and_then(|role| prior_by_role.get(role))
                .copied()
                .unwrap_or(0.5);
            let right_prior = role_by_task
                .get(&right.task_id)
                .and_then(|role| prior_by_role.get(role))
                .copied()
                .unwrap_or(0.5);
            let left_effective_confidence = effective_bid_confidence(left.confidence, left_prior);
            let right_effective_confidence =
                effective_bid_confidence(right.confidence, right_prior);
            let left_outcome_score = learned_outcomes.get(&left.task_id).copied().unwrap_or(0.5);
            let right_outcome_score = learned_outcomes.get(&right.task_id).copied().unwrap_or(0.5);
            let left_learned_confidence =
                (left_effective_confidence + ((left_outcome_score - 0.5) * 0.15)).clamp(0.0, 1.0);
            let right_learned_confidence =
                (right_effective_confidence + ((right_outcome_score - 0.5) * 0.15)).clamp(0.0, 1.0);
            bid_sort_key(&left.availability)
                .cmp(&bid_sort_key(&right.availability))
                .then_with(|| {
                    right_learned_confidence
                        .partial_cmp(&left_learned_confidence)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    right_outcome_score
                        .partial_cmp(&left_outcome_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    right_profile_confidence
                        .partial_cmp(&left_profile_confidence)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| left.created_at.cmp(&right.created_at))
        });
        let primary = ranked
            .iter()
            .find(|bid| {
                !matches!(bid.availability, BidAvailability::Unavailable)
                    && bid.confidence >= MIN_CONSENSUS_BID_CONFIDENCE
            })
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "no bid cleared the minimum confidence threshold ({MIN_CONSENSUS_BID_CONFIDENCE}); falling back to safe static routing"
                )
            })?;
        let reviewer = ranked
            .iter()
            .find(|bid| {
                bid.task_id != primary.task_id
                    && !matches!(bid.availability, BidAvailability::Unavailable)
            })
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("a distinct reviewer bid is required"))?;

        let persisted_assignment = build_role_assignment(
            parent_task_id,
            consensus_round_id(session),
            &ranked,
            now_millis(),
        );
        let assignment = ConsensusRoleAssignment {
            primary_task_id: primary.task_id.clone(),
            primary_role: "primary".to_string(),
            reviewer_task_id: reviewer.task_id.clone(),
            reviewer_role: "reviewer".to_string(),
            assigned_at: now_millis(),
        };
        session.role_assignment = Some(assignment.clone());
        for agent in session.agents.iter_mut() {
            if agent.task_id == assignment.primary_task_id {
                agent.role = "primary".to_string();
                agent.confidence = primary.confidence;
            } else if agent.task_id == assignment.reviewer_task_id {
                agent.role = "reviewer".to_string();
                agent.confidence = reviewer.confidence;
            }
        }
        let debate_launch = if debate_config.enabled {
            seed_debate_from_bid_resolution(session, &ranked, &debate_config).map(|seed| {
                PendingDebateLaunch {
                    collaboration_snapshot: session.clone(),
                    seed,
                }
            })
        } else {
            None
        };
        session.updated_at = now_millis();
        let snapshot = session.clone();
        let report = serde_json::json!({
            "session_id": session.id,
            "primary_task_id": assignment.primary_task_id,
            "reviewer_task_id": assignment.reviewer_task_id,
            "bids": ranked,
            "consensus_priors": prior_by_role,
            "collaboration_outcome_learning": learned_outcomes,
        });
        drop(collaboration);
        self.persist_collaboration_session(&snapshot).await?;
        self.persist_role_assignment(persisted_assignment).await?;
        self.persist_collaboration_resolution_trace(
            &snapshot,
            &assignment,
            &ranked,
            &role_by_task,
            &prior_by_role,
        )
        .await;

        if let Some(launch) = debate_launch {
            if let Err(error) = self
                .persist_seeded_debate_session(
                    launch.seed.debate_session.clone(),
                    launch.seed.arguments.clone(),
                )
                .await
            {
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
                return Err(error);
            }
        }

        Ok(report)
    }

    pub(in crate::agent) async fn dispatch_via_bid_protocol(
        &self,
        parent_task_id: &str,
        bids: &[DispatchBidRequest],
    ) -> Result<serde_json::Value> {
        if bids.is_empty() {
            anyhow::bail!("dispatch_via_bid_protocol requires at least one bid request");
        }
        let eligible_agents = bids
            .iter()
            .map(|bid| bid.task_id.clone())
            .collect::<Vec<_>>();
        self.bootstrap_bid_dispatch_collaboration(parent_task_id, &eligible_agents)
            .await?;
        self.call_for_bids(parent_task_id, &eligible_agents).await?;
        for bid in bids {
            self.submit_bid(
                parent_task_id,
                &bid.task_id,
                bid.confidence,
                bid.availability.clone(),
            )
            .await?;
        }
        let mut report = self.resolve_bids(parent_task_id).await?;
        if let Ok(debate_completion) = self.resolve_seeded_bid_debate(parent_task_id).await {
            if let Some(report_map) = report.as_object_mut() {
                report_map.insert("debate".to_string(), debate_completion);
            }
        }
        Ok(report)
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

    pub(in crate::agent) async fn resolve_seeded_bid_debate(
        &self,
        parent_task_id: &str,
    ) -> Result<serde_json::Value> {
        let (
            debate_session_id,
            disagreement_id,
            mission,
            winner_task_id,
            reviewer_task_id,
            winner_bid,
            reviewer_bid,
        ) = {
            if !self.config.read().await.collaboration.enabled {
                anyhow::bail!("collaboration capability is disabled in agent config");
            }
            let collaboration = self.collaboration.read().await;
            let Some(session) = collaboration.get(parent_task_id) else {
                anyhow::bail!("no collaboration session found for parent task {parent_task_id}");
            };
            let assignment = session.role_assignment.clone().ok_or_else(|| {
                anyhow::anyhow!("no bid role assignment found for parent task {parent_task_id}")
            })?;
            let disagreement = session
                .disagreements
                .iter()
                .find(|item| {
                    item.debate_session_id.is_some()
                        && item.topic.starts_with("bid resolution for ")
                        && item.resolution == "pending"
                })
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "no pending seeded bid debate found for parent task {parent_task_id}"
                    )
                })?;
            let winner_bid = session
                .bids
                .iter()
                .find(|bid| bid.task_id == assignment.primary_task_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("missing primary bid for seeded bid debate"))?;
            let reviewer_bid = session
                .bids
                .iter()
                .find(|bid| bid.task_id == assignment.reviewer_task_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("missing reviewer bid for seeded bid debate"))?;
            (
                disagreement
                    .debate_session_id
                    .clone()
                    .expect("seeded bid debate session id should exist"),
                disagreement.id.clone(),
                session.mission.clone(),
                assignment.primary_task_id,
                assignment.reviewer_task_id,
                winner_bid,
                reviewer_bid,
            )
        };

        let debate_session = self
            .get_persisted_debate_session(&debate_session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("unknown debate session: {debate_session_id}"))?;

        let shared_evidence = vec![
            format!(
                "seeded bid debate parent_task_id={} mission={}",
                parent_task_id, mission
            ),
            format!(
                "candidate={} confidence={:.2} availability={}",
                winner_task_id,
                winner_bid.confidence,
                bid_availability_label(&winner_bid.availability)
            ),
            format!(
                "candidate={} confidence={:.2} availability={}",
                reviewer_task_id,
                reviewer_bid.confidence,
                bid_availability_label(&reviewer_bid.availability)
            ),
        ];

        self.append_debate_argument(
            &debate_session_id,
            Argument {
                id: format!("arg_{}", uuid::Uuid::new_v4()),
                round: debate_session.current_round,
                role: RoleKind::Proponent,
                agent_id: winner_task_id.clone(),
                content: format!(
                    "Bid finalist argument: choose primary={} for {} because confidence={:.2} and availability={} remain strongest under the tie-break.",
                    winner_task_id,
                    mission,
                    winner_bid.confidence,
                    bid_availability_label(&winner_bid.availability)
                ),
                evidence_refs: shared_evidence.clone(),
                responds_to: None,
                timestamp_ms: now_millis(),
            },
        )
        .await?;

        self.append_debate_argument(
            &debate_session_id,
            Argument {
                id: format!("arg_{}", uuid::Uuid::new_v4()),
                round: debate_session.current_round,
                role: RoleKind::Skeptic,
                agent_id: reviewer_task_id.clone(),
                content: format!(
                    "Bid finalist counterargument: choose primary={} for {} because confidence={:.2} and availability={} keep the contest open.",
                    reviewer_task_id,
                    mission,
                    reviewer_bid.confidence,
                    bid_availability_label(&reviewer_bid.availability)
                ),
                evidence_refs: shared_evidence.clone(),
                responds_to: None,
                timestamp_ms: now_millis(),
            },
        )
        .await?;

        self.append_debate_argument(
            &debate_session_id,
            Argument {
                id: format!("arg_{}", uuid::Uuid::new_v4()),
                round: debate_session.current_round,
                role: RoleKind::Synthesizer,
                agent_id: "synthesis-lens".to_string(),
                content: format!(
                    "Winning assignment: primary={} reviewer={}. Preserve the deterministic ranking because both bids stayed tied at confidence={:.2} with availability {} vs {}.",
                    winner_task_id,
                    reviewer_task_id,
                    winner_bid.confidence,
                    bid_availability_label(&winner_bid.availability),
                    bid_availability_label(&reviewer_bid.availability)
                ),
                evidence_refs: shared_evidence,
                responds_to: None,
                timestamp_ms: now_millis(),
            },
        )
        .await?;

        let mut completion_ready_session = self
            .get_persisted_debate_session(&debate_session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("unknown debate session: {debate_session_id}"))?;
        completion_ready_session.max_rounds = completion_ready_session.current_round;
        self.persist_debate_session(&completion_ready_session)
            .await?;

        let completion = self.run_debate_to_completion(&debate_session_id).await?;
        let rationale = completion["verdict"]["recommended_action"]
            .as_str()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                format!(
                    "Winning assignment: primary={} reviewer={}",
                    winner_task_id, reviewer_task_id
                )
            });

        let snapshot = {
            let mut collaboration = self.collaboration.write().await;
            let Some(session) = collaboration.get_mut(parent_task_id) else {
                anyhow::bail!("no collaboration session found for parent task {parent_task_id}");
            };

            session.role_assignment = Some(ConsensusRoleAssignment {
                primary_task_id: winner_task_id.clone(),
                primary_role: "primary".to_string(),
                reviewer_task_id: reviewer_task_id.clone(),
                reviewer_role: "reviewer".to_string(),
                assigned_at: now_millis(),
            });
            session.consensus = Some(Consensus {
                topic: format!("bid resolution for {}", normalize_topic(&mission)),
                winner: winner_task_id.clone(),
                rationale: rationale.clone(),
                votes: Vec::new(),
            });
            for agent in session.agents.iter_mut() {
                if agent.task_id == winner_task_id {
                    agent.role = "primary".to_string();
                    agent.confidence = winner_bid.confidence;
                } else if agent.task_id == reviewer_task_id {
                    agent.role = "reviewer".to_string();
                    agent.confidence = reviewer_bid.confidence;
                }
            }
            if let Some(disagreement) = session
                .disagreements
                .iter_mut()
                .find(|item| item.id == disagreement_id)
            {
                disagreement.resolution = "resolved".to_string();
            }
            session.updated_at = now_millis();
            session.clone()
        };
        self.persist_collaboration_session(&snapshot).await?;

        Ok(serde_json::json!({
            "debate_session_id": debate_session_id,
            "winner_task_id": winner_task_id,
            "reviewer_task_id": reviewer_task_id,
            "verdict": completion["verdict"],
            "current_round": completion["current_round"],
            "status": completion["status"],
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
                let mut value =
                    serde_json::to_value(session).unwrap_or_else(|_| serde_json::json!({}));
                if let Some(outcome) = collaboration_resolution_outcome(session) {
                    if let Some(object) = value.as_object_mut() {
                        object.insert("resolution_outcome".to_string(), outcome);
                    }
                }
                return Ok(value);
            }
            drop(collaboration);
            if let Some(session) = self.persisted_collaboration_session(parent_task_id).await? {
                let mut value =
                    serde_json::to_value(&session).unwrap_or_else(|_| serde_json::json!({}));
                if let Some(outcome) = collaboration_resolution_outcome(&session) {
                    if let Some(object) = value.as_object_mut() {
                        object.insert("resolution_outcome".to_string(), outcome);
                    }
                }
                return Ok(value);
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
        self.register_subagent_collaboration(parent_task_id, task)
            .await;
        let inferred_role = super::participants::infer_collaboration_role(task);
        if let Err(error) = self
            .record_consensus_bid_outcome(&inferred_role, outcome)
            .await
        {
            tracing::warn!(
                task_id = %task.id,
                role = %inferred_role,
                "failed to record consensus prior outcome: {error}"
            );
        }
        if let Err(error) = self
            .record_collaboration_agent_outcome(parent_task_id, &task.id, outcome)
            .await
        {
            tracing::warn!(
                task_id = %task.id,
                parent_task_id = %parent_task_id,
                "failed to record collaboration outcome learning: {error}"
            );
        }
        let session_snapshot = {
            let collaboration = self.collaboration.read().await;
            collaboration.get(parent_task_id).cloned()
        };
        if let Some(session) = session_snapshot.as_ref() {
            if let Some(role_assignment) = session.role_assignment.as_ref() {
                let persisted_assignment =
                    crate::agent::consensus::types::PersistedRoleAssignment {
                        task_id: parent_task_id.to_string(),
                        round_id: consensus_round_id(session),
                        primary_agent_id: role_assignment.primary_task_id.clone(),
                        reviewer_agent_id: Some(role_assignment.reviewer_task_id.clone()),
                        observers: session
                            .agents
                            .iter()
                            .filter(|agent| {
                                agent.task_id != role_assignment.primary_task_id
                                    && agent.task_id != role_assignment.reviewer_task_id
                            })
                            .map(|agent| agent.task_id.clone())
                            .collect(),
                        assigned_at_ms: role_assignment.assigned_at,
                        outcome: Some(outcome.to_string()),
                    };
                let metric =
                    build_quality_metric(session, &persisted_assignment, outcome, now_millis());
                if let Err(error) = self
                    .mark_role_assignment_outcome(parent_task_id, outcome)
                    .await
                {
                    tracing::warn!(
                        task_id = %task.id,
                        parent_task_id = %parent_task_id,
                        "failed to mark role assignment outcome: {error}"
                    );
                }
                if let Err(error) = self.persist_consensus_quality_metric(metric).await {
                    tracing::warn!(
                        task_id = %task.id,
                        parent_task_id = %parent_task_id,
                        "failed to persist consensus quality metric: {error}"
                    );
                }
            }
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

        let parent_session = {
            let collaboration = self.collaboration.read().await;
            collaboration.get(parent_task_id).cloned()
        };
        if let Some(session) = parent_session.as_ref() {
            self.persist_collaboration_outcome_trace(task, session, outcome, summary)
                .await;
        }
        let parent_session_exists = parent_session.is_some();
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
