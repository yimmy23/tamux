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
