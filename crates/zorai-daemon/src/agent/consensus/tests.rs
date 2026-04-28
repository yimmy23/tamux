use tempfile::tempdir;

use crate::agent::engine::AgentEngine;
use crate::agent::types::AgentConfig;
use crate::session_manager::SessionManager;

#[tokio::test]
async fn bid_resolution_persists_consensus_bids_and_role_assignments() {
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
        .call_for_bids(&parent.id, &[child_a.id.clone(), child_b.id.clone()])
        .await
        .expect("call_for_bids");
    engine
        .submit_bid(
            &parent.id,
            &child_a.id,
            0.81,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("submit first bid");
    engine
        .submit_bid(
            &parent.id,
            &child_b.id,
            0.74,
            crate::agent::collaboration::BidAvailability::Available,
        )
        .await
        .expect("submit second bid");
    engine.resolve_bids(&parent.id).await.expect("resolve bids");

    let bid_count = engine
        .history
        .read_conn
        .call({
            let parent_task_id = parent.id.clone();
            move |conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM consensus_bids WHERE task_id = ?1",
                    rusqlite::params![parent_task_id],
                    |row| row.get::<_, i64>(0),
                )?)
            }
        })
        .await
        .expect("count bids");
    assert_eq!(bid_count, 2);

    let persisted = engine
        .history
        .read_conn
        .call({
            let parent_task_id = parent.id.clone();
            move |conn| {
                Ok(conn.query_row(
                    "SELECT primary_agent_id, reviewer_agent_id FROM role_assignments WHERE task_id = ?1 ORDER BY assigned_at_ms DESC LIMIT 1",
                    rusqlite::params![parent_task_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
                )?)
            }
        })
        .await
        .expect("load role assignment");
    assert_eq!(persisted.0, child_a.id);
    assert_eq!(persisted.1.as_deref(), Some(child_b.id.as_str()));
}

#[tokio::test]
async fn record_collaboration_outcome_persists_consensus_quality_metric() {
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
        .expect("call_for_bids");
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
    engine.resolve_bids(&parent.id).await.expect("resolve bids");

    child.result = Some("implemented successfully".to_string());
    engine.record_collaboration_outcome(&child, "success").await;

    let metric = engine
        .history
        .read_conn
        .call({
            let parent_task_id = parent.id.clone();
            move |conn| {
                Ok(conn.query_row(
                    "SELECT predicted_confidence, actual_outcome_score, prediction_error
                     FROM consensus_quality_metrics
                     WHERE task_id = ?1
                     ORDER BY updated_at_ms DESC, id DESC
                     LIMIT 1",
                    rusqlite::params![parent_task_id],
                    |row| {
                        Ok((
                            row.get::<_, f64>(0)?,
                            row.get::<_, f64>(1)?,
                            row.get::<_, f64>(2)?,
                        ))
                    },
                )?)
            }
        })
        .await
        .expect("load quality metric");

    assert!(metric.0 > 0.0);
    assert_eq!(metric.1, 1.0);
    assert!(metric.2 >= 0.0);
}
