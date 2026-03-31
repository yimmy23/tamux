use super::participants::apply_vote_to_disagreement;
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
