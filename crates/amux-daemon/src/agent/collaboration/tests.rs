use super::participants::apply_vote_to_disagreement;
use super::*;
use crate::session_manager::SessionManager;
use tempfile::tempdir;
use tokio::time::{timeout, Duration};

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
        debate_session_id: None,
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
        call_metadata: None,
        bids: Vec::new(),
        role_assignment: None,
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
            debate_session_id: None,
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

#[tokio::test]
async fn vote_on_tight_margin_notifies_operator_escalation() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.subscribe();

    engine.collaboration.write().await.insert(
        "parent-task".to_string(),
        CollaborationSession {
            id: "session-1".to_string(),
            parent_task_id: "parent-task".to_string(),
            thread_id: Some("thread-collab".to_string()),
            goal_run_id: None,
            mission: "decide deployment strategy".to_string(),
            agents: vec![
                CollaborativeAgent {
                    task_id: "a".to_string(),
                    title: "Research".to_string(),
                    role: "planning".to_string(),
                    confidence: 1.0,
                    status: "running".to_string(),
                },
                CollaborativeAgent {
                    task_id: "b".to_string(),
                    title: "Review".to_string(),
                    role: "planning".to_string(),
                    confidence: 0.9,
                    status: "running".to_string(),
                },
            ],
            call_metadata: None,
            bids: Vec::new(),
            role_assignment: None,
            contributions: Vec::new(),
            disagreements: vec![Disagreement {
                id: "disagree-1".to_string(),
                topic: "deployment strategy".to_string(),
                agents: vec!["a".to_string(), "b".to_string()],
                positions: vec!["recommend".to_string(), "reject".to_string()],
                confidence_gap: 0.1,
                resolution: "pending".to_string(),
                votes: vec![Vote {
                    task_id: "a".to_string(),
                    position: "recommend".to_string(),
                    weight: 1.0,
                }],
                debate_session_id: None,
            }],
            consensus: None,
            updated_at: now_millis(),
        },
    );

    let report = engine
        .vote_on_collaboration_disagreement("parent-task", "disagree-1", "b", "reject", Some(0.9))
        .await
        .expect("vote should succeed");

    assert_eq!(report["resolution"], "escalated");
    let event = timeout(Duration::from_millis(200), events.recv())
        .await
        .expect("expected workflow notice")
        .expect("broadcast receive should succeed");

    match event {
        AgentEvent::WorkflowNotice {
            thread_id,
            kind,
            message,
            details,
        } => {
            assert_eq!(thread_id, "thread-collab");
            assert_eq!(kind, "collaboration");
            assert!(message.contains("Unresolved subagent disagreement"));
            assert!(details.unwrap_or_default().contains("recommend"));
        }
        other => panic!("expected workflow notice, got {other:?}"),
    }
}

#[tokio::test]
async fn collaboration_sessions_json_reads_persisted_session_when_memory_is_empty() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let session = CollaborationSession {
        id: "session-1".to_string(),
        parent_task_id: "parent-task".to_string(),
        thread_id: Some("thread-1".to_string()),
        goal_run_id: None,
        mission: "recover persisted collaboration".to_string(),
        agents: vec![CollaborativeAgent {
            task_id: "child-1".to_string(),
            title: "Research".to_string(),
            role: "research".to_string(),
            confidence: 0.8,
            status: "completed".to_string(),
        }],
        call_metadata: None,
        bids: Vec::new(),
        role_assignment: None,
        contributions: vec![Contribution {
            id: "contrib-1".to_string(),
            task_id: "child-1".to_string(),
            topic: "retrieval".to_string(),
            position: "persist session".to_string(),
            evidence: vec!["session was saved to sqlite".to_string()],
            confidence: 0.8,
            created_at: now_millis(),
        }],
        disagreements: Vec::new(),
        consensus: None,
        updated_at: now_millis(),
    };

    engine
        .history
        .upsert_collaboration_session(
            &session.parent_task_id,
            &serde_json::to_string(&session).expect("session should serialize"),
            session.updated_at,
        )
        .await
        .expect("persisted session should save");

    let report = engine
        .collaboration_sessions_json(Some("parent-task"))
        .await
        .expect("persisted session should be readable");

    assert_eq!(report["id"], "session-1");
    assert_eq!(report["parent_task_id"], "parent-task");
    assert_eq!(report["mission"], "recover persisted collaboration");
    assert_eq!(report["agents"][0]["task_id"], "child-1");
}

#[tokio::test]
async fn collaboration_bid_protocol_assigns_primary_and_reviewer_and_persists_outcome() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Choose the best owner for the next workstream".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_a = engine
        .enqueue_task(
            "Research child".to_string(),
            "Prepare a bid for implementation ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_b = engine
        .enqueue_task(
            "Review child".to_string(),
            "Prepare a bid for review ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine
        .register_subagent_collaboration(&parent.id, &child_a)
        .await;
    engine
        .register_subagent_collaboration(&parent.id, &child_b)
        .await;

    let eligible = vec![child_a.id.clone(), child_b.id.clone()];
    let call = engine
        .call_for_bids(&parent.id, &eligible)
        .await
        .expect("call_for_bids should succeed");
    assert_eq!(
        call["eligible_agents"].as_array().map(|items| items.len()),
        Some(2)
    );

    engine
        .submit_bid(
            &parent.id,
            &child_a.id,
            0.91,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("first bid should succeed");
    engine
        .submit_bid(
            &parent.id,
            &child_b.id,
            0.66,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("second bid should succeed");

    let resolution = engine
        .resolve_bids(&parent.id)
        .await
        .expect("resolve_bids should succeed");

    assert_eq!(resolution["primary_task_id"], child_a.id);
    assert_eq!(resolution["reviewer_task_id"], child_b.id);
    assert_eq!(
        resolution["bids"].as_array().map(|items| items.len()),
        Some(2)
    );

    let persisted = engine
        .collaboration_sessions_json(Some(&parent.id))
        .await
        .expect("persisted collaboration session should be readable");
    assert_eq!(persisted["role_assignment"]["primary_task_id"], child_a.id);
    assert_eq!(persisted["role_assignment"]["reviewer_task_id"], child_b.id);
    assert_eq!(
        persisted["bids"].as_array().map(|items| items.len()),
        Some(2)
    );
}

#[tokio::test]
async fn resolve_bids_rejects_when_no_bid_clears_minimum_confidence_threshold() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Choose the best owner for a risky workstream".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_a = engine
        .enqueue_task(
            "Low confidence child A".to_string(),
            "Prepare a weak bid".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_b = engine
        .enqueue_task(
            "Low confidence child B".to_string(),
            "Prepare another weak bid".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine
        .register_subagent_collaboration(&parent.id, &child_a)
        .await;
    engine
        .register_subagent_collaboration(&parent.id, &child_b)
        .await;

    let eligible = vec![child_a.id.clone(), child_b.id.clone()];
    engine
        .call_for_bids(&parent.id, &eligible)
        .await
        .expect("call_for_bids should succeed");

    engine
        .submit_bid(
            &parent.id,
            &child_a.id,
            0.21,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("first low-confidence bid should succeed");
    engine
        .submit_bid(
            &parent.id,
            &child_b.id,
            0.29,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("second low-confidence bid should succeed");

    let error = engine
        .resolve_bids(&parent.id)
        .await
        .expect_err("resolve_bids should fail safe when no bid clears threshold");
    assert!(
        error.to_string().contains("minimum confidence threshold")
            || error.to_string().contains("fallback"),
        "error should explain the safe fallback threshold breach: {error}"
    );

    let persisted = engine
        .collaboration_sessions_json(Some(&parent.id))
        .await
        .expect("persisted collaboration session should be readable");
    assert!(
        persisted.get("role_assignment").is_none() || persisted["role_assignment"].is_null(),
        "role assignment should remain unset when all bids are below threshold"
    );
}

#[tokio::test]
async fn resolve_bids_prefers_available_over_busy_and_persists_assignment_roles() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Choose the best owner for the next workstream".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_available = engine
        .enqueue_task(
            "Research child".to_string(),
            "Prepare a bid for implementation ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_busy = engine
        .enqueue_task(
            "Review child".to_string(),
            "Prepare a bid for review ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine
        .register_subagent_collaboration(&parent.id, &child_available)
        .await;
    engine
        .register_subagent_collaboration(&parent.id, &child_busy)
        .await;

    let eligible = vec![child_available.id.clone(), child_busy.id.clone()];
    engine
        .call_for_bids(&parent.id, &eligible)
        .await
        .expect("call_for_bids should succeed");

    engine
        .submit_bid(
            &parent.id,
            &child_busy.id,
            0.96,
            crate::agent::collaboration::BidAvailability::Busy,
        )
        .await
        .expect("busy bid should succeed");
    engine
        .submit_bid(
            &parent.id,
            &child_available.id,
            0.71,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("available bid should succeed");

    let resolution = engine
        .resolve_bids(&parent.id)
        .await
        .expect("resolve_bids should succeed");

    assert_eq!(resolution["primary_task_id"], child_available.id);
    assert_eq!(resolution["reviewer_task_id"], child_busy.id);

    let persisted = engine
        .collaboration_sessions_json(Some(&parent.id))
        .await
        .expect("persisted collaboration session should be readable");
    assert_eq!(
        persisted["role_assignment"]["primary_task_id"],
        child_available.id
    );
    assert_eq!(
        persisted["role_assignment"]["reviewer_task_id"],
        child_busy.id
    );
    assert_eq!(persisted["role_assignment"]["primary_role"], "primary");
    assert_eq!(persisted["role_assignment"]["reviewer_role"], "reviewer");
    assert!(persisted["agents"].as_array().is_some_and(|agents| {
        agents
            .iter()
            .any(|agent| agent["task_id"] == child_available.id && agent["role"] == "primary")
    }));
    assert!(persisted["agents"].as_array().is_some_and(|agents| {
        agents
            .iter()
            .any(|agent| agent["task_id"] == child_busy.id && agent["role"] == "reviewer")
    }));
}

#[tokio::test]
async fn resolve_bids_prefers_role_with_stronger_consensus_prior_when_confidence_is_close() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Choose the best owner for the next workstream".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_research = engine
        .enqueue_task(
            "Research child".to_string(),
            "Prepare a bid for implementation ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_execution = engine
        .enqueue_task(
            "Execution child".to_string(),
            "Prepare a bid for implementation ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine
        .register_subagent_collaboration(&parent.id, &child_research)
        .await;
    engine
        .register_subagent_collaboration(&parent.id, &child_execution)
        .await;
    engine
        .call_for_bids(
            &parent.id,
            &[child_research.id.clone(), child_execution.id.clone()],
        )
        .await
        .expect("call_for_bids should succeed");

    for _ in 0..3 {
        engine
            .record_consensus_bid_outcome("research", "success")
            .await
            .expect("record consensus prior");
    }

    engine
        .submit_bid(
            &parent.id,
            &child_research.id,
            0.68,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("research bid should succeed");
    engine
        .submit_bid(
            &parent.id,
            &child_execution.id,
            0.72,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("execution bid should succeed");

    let resolution = engine
        .resolve_bids(&parent.id)
        .await
        .expect("resolve_bids should succeed");

    assert_eq!(resolution["primary_task_id"], child_research.id);
    assert_eq!(resolution["reviewer_task_id"], child_execution.id);
}

#[tokio::test]
async fn repeated_bid_round_reuses_recorded_outcome_learning_after_roles_were_assigned() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Choose the best owner for the next workstream".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let mut child_research = engine
        .enqueue_task(
            "Research child".to_string(),
            "Prepare a bid for implementation ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_execution = engine
        .enqueue_task(
            "Execution child".to_string(),
            "Prepare a bid for implementation ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine
        .register_subagent_collaboration(&parent.id, &child_research)
        .await;
    engine
        .register_subagent_collaboration(&parent.id, &child_execution)
        .await;

    engine
        .call_for_bids(
            &parent.id,
            &[child_research.id.clone(), child_execution.id.clone()],
        )
        .await
        .expect("first call_for_bids should succeed");
    engine
        .submit_bid(
            &parent.id,
            &child_research.id,
            0.83,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("first research bid should succeed");
    engine
        .submit_bid(
            &parent.id,
            &child_execution.id,
            0.71,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("first execution bid should succeed");
    engine
        .resolve_bids(&parent.id)
        .await
        .expect("first resolve_bids should succeed");

    child_research.result = Some("implemented successfully".to_string());
    engine
        .record_collaboration_outcome(&child_research, "success")
        .await;

    engine
        .call_for_bids(
            &parent.id,
            &[child_research.id.clone(), child_execution.id.clone()],
        )
        .await
        .expect("second call_for_bids should succeed");

    let reset_report = engine
        .collaboration_sessions_json(Some(&parent.id))
        .await
        .expect("collaboration session should be readable");
    assert!(reset_report["agents"].as_array().is_some_and(|agents| {
        agents
            .iter()
            .any(|agent| agent["task_id"] == child_research.id && agent["role"] == "research")
    }));
    assert!(reset_report["agents"].as_array().is_some_and(|agents| {
        agents
            .iter()
            .any(|agent| agent["task_id"] == child_execution.id && agent["role"] == "execution")
    }));

    engine
        .submit_bid(
            &parent.id,
            &child_research.id,
            0.69,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("second research bid should succeed");
    engine
        .submit_bid(
            &parent.id,
            &child_execution.id,
            0.71,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("second execution bid should succeed");

    let resolution = engine
        .resolve_bids(&parent.id)
        .await
        .expect("second resolve_bids should succeed");

    assert_eq!(resolution["primary_task_id"], child_research.id);
    assert_eq!(resolution["reviewer_task_id"], child_execution.id);
}

#[tokio::test]
async fn resolve_bids_records_collaboration_resolution_trace_and_audit() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Choose the best owner for the next workstream".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_research = engine
        .enqueue_task(
            "Research child".to_string(),
            "Prepare a bid for implementation ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_execution = engine
        .enqueue_task(
            "Execution child".to_string(),
            "Prepare a bid for implementation ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine
        .register_subagent_collaboration(&parent.id, &child_research)
        .await;
    engine
        .register_subagent_collaboration(&parent.id, &child_execution)
        .await;
    engine
        .call_for_bids(
            &parent.id,
            &[child_research.id.clone(), child_execution.id.clone()],
        )
        .await
        .expect("call_for_bids should succeed");
    engine
        .submit_bid(
            &parent.id,
            &child_research.id,
            0.68,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("research bid should succeed");
    engine
        .submit_bid(
            &parent.id,
            &child_execution.id,
            0.72,
            crate::agent::collaboration::BidAvailability::Busy,
        )
        .await
        .expect("execution bid should succeed");

    let resolution = engine
        .resolve_bids(&parent.id)
        .await
        .expect("resolve_bids should succeed");

    let records = engine
        .history
        .list_recent_causal_trace_records("collaboration_bid_resolution", 1)
        .await
        .expect("list collaboration resolution traces");
    assert_eq!(records.len(), 1);
    let selected: serde_json::Value =
        serde_json::from_str(&records[0].selected_json).expect("deserialize selected option");
    assert_eq!(
        selected["option_type"].as_str(),
        Some("collaboration_bid_resolution")
    );
    assert!(selected["reasoning"].as_str().is_some_and(|text| {
        text.contains(child_research.id.as_str()) || text.contains(child_execution.id.as_str())
    }));

    let factors: Vec<crate::agent::learning::traces::CausalFactor> =
        serde_json::from_str(&records[0].causal_factors_json).expect("deserialize factors");
    assert!(factors.iter().any(|factor| {
        factor
            .description
            .contains("resolved collaboration bid round with 2 candidate")
    }));
    assert!(factors.iter().any(|factor| {
        factor
            .description
            .contains("availability-constrained during ranking")
    }));

    let filters = vec!["collaboration_resolution".to_string()];
    let audits = engine
        .history
        .list_action_audit(Some(filters.as_slice()), None, 5)
        .await
        .expect("list collaboration resolution audit entries");
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].task_id.as_deref(), Some(parent.id.as_str()));
    assert_eq!(audits[0].thread_id.as_deref(), Some("thread-parent"));
    let raw_json: serde_json::Value = audits[0]
        .raw_data_json
        .as_deref()
        .map(|text| serde_json::from_str(text).expect("deserialize audit raw json"))
        .expect("raw data json should be present");
    assert_eq!(
        raw_json["primary_task_id"].as_str(),
        resolution["primary_task_id"].as_str()
    );
    assert_eq!(
        raw_json["reviewer_task_id"].as_str(),
        resolution["reviewer_task_id"].as_str()
    );
    assert_eq!(
        raw_json["ranked_bids"].as_array().map(|items| items.len()),
        Some(2)
    );
}

#[tokio::test]
async fn dispatch_via_bid_protocol_runs_bid_flow_end_to_end_through_collaboration_runtime() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Choose the best owner for the next workstream".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_a = engine
        .enqueue_task(
            "Research child".to_string(),
            "Prepare a bid for implementation ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_b = engine
        .enqueue_task(
            "Review child".to_string(),
            "Prepare a bid for review ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine
        .register_subagent_collaboration(&parent.id, &child_a)
        .await;
    engine
        .register_subagent_collaboration(&parent.id, &child_b)
        .await;

    let resolution = engine
        .dispatch_via_bid_protocol(
            &parent.id,
            &[
                crate::agent::collaboration::DispatchBidRequest {
                    task_id: child_b.id.clone(),
                    confidence: 0.94,
                    availability: crate::agent::collaboration::BidAvailability::Busy,
                },
                crate::agent::collaboration::DispatchBidRequest {
                    task_id: child_a.id.clone(),
                    confidence: 0.73,
                    availability: crate::agent::collaboration::BidAvailability::Available,
                },
            ],
        )
        .await
        .expect("dispatch_via_bid_protocol should succeed");

    assert_eq!(resolution["primary_task_id"], child_a.id);
    assert_eq!(resolution["reviewer_task_id"], child_b.id);
    assert_eq!(
        resolution["bids"].as_array().map(|items| items.len()),
        Some(2)
    );

    let persisted = engine
        .collaboration_sessions_json(Some(&parent.id))
        .await
        .expect("persisted collaboration session should be readable");
    assert_eq!(persisted["role_assignment"]["primary_task_id"], child_a.id);
    assert_eq!(persisted["role_assignment"]["primary_role"], "primary");
    assert_eq!(persisted["role_assignment"]["reviewer_task_id"], child_b.id);
    assert_eq!(persisted["role_assignment"]["reviewer_role"], "reviewer");
}

#[tokio::test]
async fn dispatch_via_bid_protocol_persists_call_metadata_in_collaboration_session_report() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Choose the best owner for the next workstream".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_a = engine
        .enqueue_task(
            "Research child".to_string(),
            "Prepare a bid for implementation ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_b = engine
        .enqueue_task(
            "Review child".to_string(),
            "Prepare a bid for review ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine
        .register_subagent_collaboration(&parent.id, &child_a)
        .await;
    engine
        .register_subagent_collaboration(&parent.id, &child_b)
        .await;

    engine
        .dispatch_via_bid_protocol(
            &parent.id,
            &[
                crate::agent::collaboration::DispatchBidRequest {
                    task_id: child_b.id.clone(),
                    confidence: 0.94,
                    availability: crate::agent::collaboration::BidAvailability::Busy,
                },
                crate::agent::collaboration::DispatchBidRequest {
                    task_id: child_a.id.clone(),
                    confidence: 0.73,
                    availability: crate::agent::collaboration::BidAvailability::Available,
                },
            ],
        )
        .await
        .expect("dispatch_via_bid_protocol should succeed");

    let persisted = engine
        .collaboration_sessions_json(Some(&parent.id))
        .await
        .expect("persisted collaboration session should be readable");

    assert_eq!(persisted["call_metadata"]["caller_task_id"], parent.id);
    assert_eq!(
        persisted["call_metadata"]["eligible_agents"]
            .as_array()
            .map(|items| items.len()),
        Some(2)
    );
    assert!(persisted["call_metadata"]["called_at"].as_u64().is_some());
    assert!(persisted["bids"]
        .as_array()
        .is_some_and(|bids| { bids.iter().all(|bid| bid["created_at"].as_u64().is_some()) }));
}

#[tokio::test]
async fn resolve_bids_tie_prefers_higher_agent_affinity_profile_confidence() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Choose the best owner for the next workstream".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_a = engine
        .enqueue_task(
            "Research child".to_string(),
            "Prepare a bid for implementation ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_b = engine
        .enqueue_task(
            "Review child".to_string(),
            "Prepare a bid for review ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine
        .register_subagent_collaboration(&parent.id, &child_a)
        .await;
    engine
        .register_subagent_collaboration(&parent.id, &child_b)
        .await;

    {
        let mut collaboration = engine.collaboration.write().await;
        let session = collaboration
            .get_mut(&parent.id)
            .expect("collaboration session should exist");
        for agent in &mut session.agents {
            if agent.task_id == child_a.id {
                agent.confidence = 0.22;
            } else if agent.task_id == child_b.id {
                agent.confidence = 0.91;
            }
        }
    }

    let eligible = vec![child_a.id.clone(), child_b.id.clone()];
    engine
        .call_for_bids(&parent.id, &eligible)
        .await
        .expect("call_for_bids should succeed");

    engine
        .submit_bid(
            &parent.id,
            &child_a.id,
            0.84,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("first tied bid should succeed");
    engine
        .submit_bid(
            &parent.id,
            &child_b.id,
            0.84,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("second tied bid should succeed");

    let resolution = engine
        .resolve_bids(&parent.id)
        .await
        .expect("resolve_bids should succeed");

    assert_eq!(
        resolution["primary_task_id"], child_b.id,
        "on exact bid ties, higher learned agent confidence profile should win primary routing"
    );
    assert_eq!(resolution["reviewer_task_id"], child_a.id);
}

#[tokio::test]
async fn resolve_bids_tie_seeds_debate_session_with_bid_context() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    config.debate.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Choose the best owner for the next workstream".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_a = engine
        .enqueue_task(
            "Research child".to_string(),
            "Prepare a bid for implementation ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_b = engine
        .enqueue_task(
            "Review child".to_string(),
            "Prepare a bid for review ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine
        .register_subagent_collaboration(&parent.id, &child_a)
        .await;
    engine
        .register_subagent_collaboration(&parent.id, &child_b)
        .await;

    let eligible = vec![child_a.id.clone(), child_b.id.clone()];
    engine
        .call_for_bids(&parent.id, &eligible)
        .await
        .expect("call_for_bids should succeed");

    engine
        .submit_bid(
            &parent.id,
            &child_a.id,
            0.84,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("first tied bid should succeed");
    engine
        .submit_bid(
            &parent.id,
            &child_b.id,
            0.84,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("second tied bid should succeed");

    let resolution = engine
        .resolve_bids(&parent.id)
        .await
        .expect("resolve_bids should succeed");

    assert_eq!(resolution["primary_task_id"], child_a.id);
    assert_eq!(resolution["reviewer_task_id"], child_b.id);

    let persisted = engine
        .collaboration_sessions_json(Some(&parent.id))
        .await
        .expect("persisted collaboration session should be readable");
    let disagreement = persisted["disagreements"]
        .as_array()
        .and_then(|items| items.first())
        .expect("contested bid resolution should create a disagreement");
    let debate_session_id = disagreement["debate_session_id"]
        .as_str()
        .expect("contested bid resolution should link a debate session")
        .to_string();
    assert_eq!(
        disagreement["topic"].as_str(),
        Some("bid resolution for choose the best owner for the next workstream")
    );

    let child_a_bid_created_at = persisted["bids"]
        .as_array()
        .and_then(|items| {
            items.iter().find_map(|bid| {
                (bid["task_id"].as_str() == Some(child_a.id.as_str()))
                    .then(|| bid["created_at"].as_u64())
                    .flatten()
            })
        })
        .expect("child_a bid timestamp should persist");
    let child_b_bid_created_at = persisted["bids"]
        .as_array()
        .and_then(|items| {
            items.iter().find_map(|bid| {
                (bid["task_id"].as_str() == Some(child_b.id.as_str()))
                    .then(|| bid["created_at"].as_u64())
                    .flatten()
            })
        })
        .expect("child_b bid timestamp should persist");

    let debate_payload = engine
        .get_debate_session_payload(&debate_session_id)
        .await
        .expect("debate session should exist");
    assert_eq!(
        debate_payload["topic"].as_str(),
        Some("bid resolution for choose the best owner for the next workstream")
    );
    assert_eq!(debate_payload["current_round"].as_u64(), Some(2));

    let arguments = debate_payload["arguments"]
        .as_array()
        .expect("seeded debate should persist arguments");
    assert_eq!(arguments.len(), 2);
    assert!(arguments.iter().any(|argument| {
        argument["agent_id"].as_str() == Some(child_a.id.as_str())
            && argument["timestamp_ms"].as_u64() == Some(child_a_bid_created_at)
    }));
    assert!(arguments.iter().any(|argument| {
        argument["agent_id"].as_str() == Some(child_b.id.as_str())
            && argument["timestamp_ms"].as_u64() == Some(child_b_bid_created_at)
    }));
    assert!(arguments.iter().all(|argument| {
        argument["evidence_refs"].as_array().is_some_and(|refs| {
            refs.iter().any(|item| {
                item.as_str()
                    .is_some_and(|text| text.contains(parent.id.as_str()))
            }) && refs.iter().any(|item| {
                item.as_str().is_some_and(|text| {
                    text.contains(child_a.id.as_str()) && text.contains(child_b.id.as_str())
                })
            })
        })
    }));
}

#[tokio::test]
async fn seeded_bid_debate_advances_completes_and_persists_winning_assignment() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    config.debate.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Choose the best owner for the next workstream".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_a = engine
        .enqueue_task(
            "Research child".to_string(),
            "Prepare a bid for implementation ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_b = engine
        .enqueue_task(
            "Review child".to_string(),
            "Prepare a bid for review ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine
        .register_subagent_collaboration(&parent.id, &child_a)
        .await;
    engine
        .register_subagent_collaboration(&parent.id, &child_b)
        .await;

    let eligible = vec![child_a.id.clone(), child_b.id.clone()];
    engine
        .call_for_bids(&parent.id, &eligible)
        .await
        .expect("call_for_bids should succeed");
    engine
        .submit_bid(
            &parent.id,
            &child_a.id,
            0.84,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("first tied bid should succeed");
    engine
        .submit_bid(
            &parent.id,
            &child_b.id,
            0.84,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("second tied bid should succeed");

    engine
        .resolve_bids(&parent.id)
        .await
        .expect("resolve_bids should seed a debate");

    let seeded = engine
        .collaboration_sessions_json(Some(&parent.id))
        .await
        .expect("seeded collaboration session should load");
    let debate_session_id = seeded["disagreements"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item["debate_session_id"].as_str())
        .expect("seeded debate session id should persist")
        .to_string();

    let completion = engine
        .resolve_seeded_bid_debate(&parent.id)
        .await
        .expect("seeded bid debate should complete");
    assert_eq!(completion["debate_session_id"], debate_session_id);
    assert_eq!(completion["winner_task_id"], child_a.id);
    assert_eq!(completion["reviewer_task_id"], child_b.id);
    assert_eq!(completion["status"], "completed");
    assert!(completion["verdict"]["recommended_action"]
        .as_str()
        .is_some_and(|text| {
            text.contains(&format!("primary={}", child_a.id))
                && text.contains(&format!("reviewer={}", child_b.id))
        }));

    let debate_payload = engine
        .get_debate_session_payload(&debate_session_id)
        .await
        .expect("completed debate session should exist");
    assert_eq!(debate_payload["status"], "completed");
    assert_eq!(debate_payload["current_round"].as_u64(), Some(2));
    let arguments = debate_payload["arguments"]
        .as_array()
        .expect("completed debate should retain arguments");
    assert_eq!(arguments.len(), 5);
    assert!(arguments.iter().any(|argument| {
        argument["content"].as_str().is_some_and(|text| {
            text.contains("confidence=0.84") && text.contains("availability=available")
        })
    }));
    assert!(debate_payload["verdict"]["recommended_action"]
        .as_str()
        .is_some_and(|text| {
            text.contains(&format!("primary={}", child_a.id))
                && text.contains(&format!("reviewer={}", child_b.id))
        }));

    let persisted = engine
        .collaboration_sessions_json(Some(&parent.id))
        .await
        .expect("collaboration session should reflect the bid debate verdict");
    assert_eq!(persisted["role_assignment"]["primary_task_id"], child_a.id);
    assert_eq!(persisted["role_assignment"]["reviewer_task_id"], child_b.id);
    assert_eq!(persisted["disagreements"][0]["resolution"], "resolved");
    assert_eq!(persisted["consensus"]["winner"], child_a.id);
    assert!(persisted["consensus"]["rationale"]
        .as_str()
        .is_some_and(|text| {
            text == completion["verdict"]["recommended_action"]
                .as_str()
                .unwrap_or_default()
        }));
    assert_eq!(
        persisted["resolution_outcome"]["status"].as_str(),
        Some("resolved")
    );
    assert_eq!(
        persisted["resolution_outcome"]["winner_task_id"].as_str(),
        Some(child_a.id.as_str())
    );
    assert_eq!(
        persisted["resolution_outcome"]["reviewer_task_id"].as_str(),
        Some(child_b.id.as_str())
    );
    assert_eq!(
        persisted["resolution_outcome"]["topic"].as_str(),
        Some("bid resolution for choose the best owner for the next workstream")
    );
    assert!(persisted["resolution_outcome"]["rationale"]
        .as_str()
        .is_some_and(|text| {
            text.contains(&format!("primary={}", child_a.id))
                && text.contains(&format!("reviewer={}", child_b.id))
        }));
}

#[tokio::test]
async fn dispatch_via_bid_protocol_auto_completes_seeded_bid_debate_on_contest() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    config.debate.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Choose the best owner for the next workstream".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_a = engine
        .enqueue_task(
            "Research child".to_string(),
            "Prepare a bid for implementation ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_b = engine
        .enqueue_task(
            "Review child".to_string(),
            "Prepare a bid for review ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine
        .register_subagent_collaboration(&parent.id, &child_a)
        .await;
    engine
        .register_subagent_collaboration(&parent.id, &child_b)
        .await;

    let report = engine
        .dispatch_via_bid_protocol(
            &parent.id,
            &[
                crate::agent::collaboration::DispatchBidRequest {
                    task_id: child_a.id.clone(),
                    confidence: 0.84,
                    availability: crate::agent::collaboration::BidAvailability::Available,
                },
                crate::agent::collaboration::DispatchBidRequest {
                    task_id: child_b.id.clone(),
                    confidence: 0.84,
                    availability: crate::agent::collaboration::BidAvailability::Available,
                },
            ],
        )
        .await
        .expect("dispatch_via_bid_protocol should auto-complete the seeded bid debate");

    assert_eq!(report["primary_task_id"], child_a.id);
    assert_eq!(report["reviewer_task_id"], child_b.id);
    assert_eq!(report["debate"]["winner_task_id"], child_a.id);
    assert_eq!(report["debate"]["status"], "completed");

    let persisted = engine
        .collaboration_sessions_json(Some(&parent.id))
        .await
        .expect("persisted collaboration session should be readable");
    let debate_session_id = persisted["disagreements"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item["debate_session_id"].as_str())
        .expect("debate session id should persist after automatic completion")
        .to_string();

    assert_eq!(persisted["disagreements"][0]["resolution"], "resolved");
    assert_eq!(persisted["consensus"]["winner"], child_a.id);
    assert!(persisted["consensus"]["rationale"]
        .as_str()
        .is_some_and(|text| {
            text == report["debate"]["verdict"]["recommended_action"]
                .as_str()
                .unwrap_or_default()
        }));

    let debate_payload = engine
        .get_debate_session_payload(&debate_session_id)
        .await
        .expect("auto-completed debate session should exist");
    assert_eq!(debate_payload["status"], "completed");
    assert_eq!(debate_payload["current_round"].as_u64(), Some(2));
    assert_eq!(
        debate_payload["arguments"]
            .as_array()
            .map(|items| items.len()),
        Some(5)
    );
}

#[tokio::test]
async fn collaboration_disagreement_auto_escalates_into_seeded_debate_session() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    config.debate.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine.collaboration.write().await.insert(
        "parent-task".to_string(),
        CollaborationSession {
            id: "session-1".to_string(),
            parent_task_id: "parent-task".to_string(),
            thread_id: Some("thread-collab".to_string()),
            goal_run_id: Some("goal-collab".to_string()),
            mission: "decide deployment strategy".to_string(),
            agents: vec![
                CollaborativeAgent {
                    task_id: "a".to_string(),
                    title: "Research".to_string(),
                    role: "planning".to_string(),
                    confidence: 0.82,
                    status: "running".to_string(),
                },
                CollaborativeAgent {
                    task_id: "b".to_string(),
                    title: "Review".to_string(),
                    role: "review".to_string(),
                    confidence: 0.78,
                    status: "running".to_string(),
                },
            ],
            call_metadata: None,
            bids: Vec::new(),
            role_assignment: None,
            contributions: Vec::new(),
            disagreements: Vec::new(),
            consensus: None,
            updated_at: now_millis(),
        },
    );

    engine
        .record_collaboration_contribution(
            "parent-task",
            "a",
            "Deployment Strategy",
            "recommend canary rollout",
            vec!["canary rollout limits blast radius".to_string()],
            0.82,
        )
        .await
        .expect("first contribution should record");
    engine
        .record_collaboration_contribution(
            "parent-task",
            "b",
            "Deployment Strategy",
            "avoid canary rollout",
            vec!["full rollout avoids operational drift".to_string()],
            0.78,
        )
        .await
        .expect("second contribution should record");

    let report = engine
        .collaboration_sessions_json(Some("parent-task"))
        .await
        .expect("session report should load");
    let disagreement = report["disagreements"][0].clone();
    let debate_session_id = disagreement["debate_session_id"]
        .as_str()
        .expect("disagreement should link a debate session")
        .to_string();

    let debate_payload = engine
        .get_debate_session_payload(&debate_session_id)
        .await
        .expect("debate session should exist");
    assert_eq!(
        debate_payload.get("topic").and_then(|value| value.as_str()),
        Some("deployment strategy")
    );
    assert_eq!(
        debate_payload
            .get("current_round")
            .and_then(|value| value.as_u64()),
        Some(2)
    );
    assert_eq!(
        debate_payload
            .get("arguments")
            .and_then(|value| value.as_array())
            .map(|items| items.len()),
        Some(2)
    );
    assert_eq!(
        debate_payload
            .get("arguments")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|value| value.get("content"))
            .and_then(|value| value.as_str()),
        Some("recommend canary rollout")
    );

    engine
        .record_collaboration_contribution(
            "parent-task",
            "b",
            "Deployment Strategy",
            "reject canary rollout",
            vec!["full rollout avoids operational drift".to_string()],
            0.8,
        )
        .await
        .expect("refresh contribution should record");

    let refreshed = engine
        .collaboration_sessions_json(Some("parent-task"))
        .await
        .expect("session report should reload");
    assert_eq!(
        refreshed["disagreements"][0]["debate_session_id"].as_str(),
        Some(debate_session_id.as_str())
    );
}

#[tokio::test]
async fn record_collaboration_outcome_creates_session_for_solo_subagent() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Coordinate the child work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let mut child = engine
        .enqueue_task(
            "Daemon Persistence Advantages".to_string(),
            "Explain why daemon-first persistence survives overnight runs".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-child".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    child.status = TaskStatus::Completed;
    child.result = Some("SQLite-backed state survives process and UI restarts".to_string());

    engine.record_collaboration_outcome(&child, "success").await;

    let report = engine
        .collaboration_sessions_json(Some(&child.id))
        .await
        .expect("solo subagent should now have a collaboration session");

    assert_eq!(report["parent_task_id"], child.id);
    assert_eq!(report["agents"][0]["task_id"], child.id);
    assert_eq!(
        report["contributions"][0]["task_id"],
        serde_json::Value::String(child.id.clone())
    );
    assert!(report["contributions"][0]["position"] == "recommended");
}

#[tokio::test]
async fn record_collaboration_outcome_records_trace_and_audit_for_parent_session() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Choose the best owner for the next workstream".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let mut child = engine
        .enqueue_task(
            "Research child".to_string(),
            "Prepare a bid for implementation ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let reviewer = engine
        .enqueue_task(
            "Review child".to_string(),
            "Prepare a bid for review ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine
        .register_subagent_collaboration(&parent.id, &child)
        .await;
    engine
        .register_subagent_collaboration(&parent.id, &reviewer)
        .await;
    engine
        .call_for_bids(&parent.id, &[child.id.clone(), reviewer.id.clone()])
        .await
        .expect("call_for_bids should succeed");
    engine
        .submit_bid(
            &parent.id,
            &child.id,
            0.83,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("submit primary bid");
    engine
        .submit_bid(
            &parent.id,
            &reviewer.id,
            0.71,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("submit reviewer bid");
    engine
        .resolve_bids(&parent.id)
        .await
        .expect("resolve bids should succeed");

    child.result = Some("implemented successfully".to_string());
    engine.record_collaboration_outcome(&child, "success").await;

    let records = engine
        .history
        .list_recent_causal_trace_records("collaboration_outcome", 1)
        .await
        .expect("list collaboration outcome traces");
    assert_eq!(records.len(), 1);
    let factors: Vec<crate::agent::learning::traces::CausalFactor> =
        serde_json::from_str(&records[0].causal_factors_json).expect("deserialize factors");
    assert!(factors.iter().any(|factor| {
        factor
            .description
            .contains("recorded settled collaboration outcome for role")
    }));
    let outcome: crate::agent::learning::traces::CausalTraceOutcome =
        serde_json::from_str(&records[0].outcome_json).expect("deserialize outcome json");
    assert!(matches!(
        outcome,
        crate::agent::learning::traces::CausalTraceOutcome::Success
    ));

    let filters = vec!["collaboration_outcome".to_string()];
    let audits = engine
        .history
        .list_action_audit(Some(filters.as_slice()), None, 5)
        .await
        .expect("list collaboration outcome audit entries");
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].task_id.as_deref(), Some(child.id.as_str()));
    let raw_json: serde_json::Value = audits[0]
        .raw_data_json
        .as_deref()
        .map(|text| serde_json::from_str(text).expect("deserialize audit raw json"))
        .expect("raw data json should be present");
    assert_eq!(
        raw_json["parent_task_id"].as_str(),
        Some(parent.id.as_str())
    );
    assert_eq!(raw_json["task_id"].as_str(), Some(child.id.as_str()));
    assert_eq!(raw_json["outcome"].as_str(), Some("success"));
}
