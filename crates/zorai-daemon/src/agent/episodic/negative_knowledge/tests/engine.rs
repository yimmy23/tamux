use super::super::*;
use super::common::*;

#[tokio::test]
async fn record_negative_knowledge_from_episode_initializes_failure_derived_constraint_fields(
) -> anyhow::Result<()> {
    let engine = make_test_engine().await;
    let episode = make_failure_episode("Deploy CONFIG rollback failed", Some(0.84));

    engine
        .record_negative_knowledge_from_episode(&episode)
        .await?;

    let constraints =
        select_constraints_for_subject(&engine, "Deploy CONFIG rollback failed").await?;
    assert_eq!(constraints.len(), 1);
    assert_eq!(
        constraints[0].related_subject_tokens,
        vec!["config", "deploy", "failed", "rollback"]
    );
    assert!(constraints[0].direct_observation);
    assert_eq!(constraints[0].state, ConstraintState::Dying);
    assert_eq!(constraints[0].evidence_count, 1);
    assert!(constraints[0].derived_from_constraint_ids.is_empty());
    Ok(())
}

#[tokio::test]
async fn record_negative_knowledge_from_episode_promotes_high_confidence_failure_to_dead(
) -> anyhow::Result<()> {
    let engine = make_test_engine().await;
    let episode = make_failure_episode("Deploy CONFIG rollback failed", Some(0.85));

    engine
        .record_negative_knowledge_from_episode(&episode)
        .await?;

    let constraints =
        select_constraints_for_subject(&engine, "Deploy CONFIG rollback failed").await?;
    assert_eq!(constraints.len(), 1);
    assert_eq!(constraints[0].state, ConstraintState::Dead);
    assert!(constraints[0].direct_observation);
    Ok(())
}

#[tokio::test]
async fn add_negative_constraint_merges_with_matching_row_beyond_twenty_row_fallback(
) -> anyhow::Result<()> {
    let engine = make_test_engine().await;

    insert_constraint_for_engine(
        &engine,
        NegativeConstraint {
            id: "nc-old-match".to_string(),
            state: ConstraintState::Suspicious,
            confidence: 0.4,
            evidence_count: 1,
            direct_observation: false,
            valid_until: None,
            created_at: 10,
            ..make_constraint_with_class("deploy config rollback", Some("deploy-fix"))
        },
    )
    .await?;

    for idx in 0..24 {
        insert_constraint_for_engine(
            &engine,
            NegativeConstraint {
                id: format!("nc-filler-{idx}"),
                created_at: 1_000 + idx,
                subject: format!("filler subject {idx}"),
                valid_until: None,
                ..make_constraint_with_class(&format!("filler subject {idx}"), Some("deploy-fix"))
            },
        )
        .await?;
    }

    engine
        .add_negative_constraint(NegativeConstraint {
            id: "nc-incoming".to_string(),
            confidence: 0.7,
            valid_until: None,
            ..make_constraint_with_class("rollback deploy config", Some("deploy-fix"))
        })
        .await?;

    let matching = select_constraints_for_subject(&engine, "deploy config rollback").await?;
    let row_count = count_negative_knowledge_rows(&engine).await?;

    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].id, "nc-old-match");
    assert_eq!(matching[0].evidence_count, 2);
    assert_eq!(matching[0].state, ConstraintState::Dying);
    assert_eq!(row_count, 25);
    Ok(())
}

#[tokio::test]
async fn add_negative_constraint_persists_source_and_propagated_target_updates_together(
) -> anyhow::Result<()> {
    let engine = make_test_engine().await;

    insert_constraint_for_engine(
        &engine,
        NegativeConstraint {
            id: "nc-source".to_string(),
            state: ConstraintState::Dying,
            evidence_count: 2,
            confidence: 0.72,
            valid_until: None,
            created_at: 100,
            ..make_constraint_with_class("deploy config prod fix", Some("deploy-fix"))
        },
    )
    .await?;

    insert_constraint_for_engine(
        &engine,
        NegativeConstraint {
            id: "nc-target".to_string(),
            state: ConstraintState::Suspicious,
            direct_observation: false,
            valid_until: None,
            created_at: 90,
            ..make_constraint_with_class("deploy config rollback", Some("deploy-fix"))
        },
    )
    .await?;

    engine
        .add_negative_constraint(NegativeConstraint {
            id: "nc-source-incoming".to_string(),
            confidence: 0.8,
            valid_until: None,
            ..make_constraint_with_class("fix deploy config prod", Some("deploy-fix"))
        })
        .await?;

    let source = engine
        .history
        .conn
        .call(|conn| Ok(select_constraint_by_id(conn, "nc-source")?))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let target = engine
        .history
        .conn
        .call(|conn| Ok(select_constraint_by_id(conn, "nc-target")?))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(source.state, ConstraintState::Dead);
    assert_eq!(source.evidence_count, 3);
    assert_eq!(target.state, ConstraintState::Dying);
    assert_eq!(
        target.derived_from_constraint_ids,
        vec!["nc-source".to_string()]
    );
    Ok(())
}

#[tokio::test]
async fn add_negative_constraint_cache_update_preserves_concurrent_entries() -> anyhow::Result<()> {
    let engine = make_test_engine().await;
    let scope_id = crate::agent::agent_identity::current_agent_scope_id();

    {
        let mut stores = engine.episodic_store.write().await;
        let store = stores.entry(scope_id.clone()).or_default();
        store.cached_constraints.push(NegativeConstraint {
            id: "nc-concurrent-only".to_string(),
            created_at: 999,
            valid_until: None,
            ..make_constraint_with_class("concurrent cache only", Some("deploy-fix"))
        });
    }

    engine
        .add_negative_constraint(NegativeConstraint {
            id: "nc-new-cache".to_string(),
            created_at: 1_001,
            valid_until: None,
            ..make_constraint_with_class("fresh runtime insert", Some("deploy-fix"))
        })
        .await?;

    let stores = engine.episodic_store.read().await;
    let store = stores.get(&scope_id).expect("store exists");
    assert!(store
        .cached_constraints
        .iter()
        .any(|constraint| constraint.id == "nc-concurrent-only"));
    assert!(store
        .cached_constraints
        .iter()
        .any(|constraint| constraint.id == "nc-new-cache"));
    Ok(())
}

#[tokio::test]
async fn refresh_constraint_cache_can_load_more_than_twenty_active_rows() -> anyhow::Result<()> {
    let engine = make_test_engine().await;

    for idx in 0..25 {
        insert_constraint_for_engine(
            &engine,
            NegativeConstraint {
                id: format!("nc-refresh-{idx}"),
                created_at: 5_000 + idx,
                subject: format!("refresh subject {idx}"),
                valid_until: None,
                ..make_constraint_with_class(&format!("refresh subject {idx}"), Some("deploy-fix"))
            },
        )
        .await?;
    }

    engine.refresh_constraint_cache().await?;

    let scope_id = crate::agent::agent_identity::current_agent_scope_id();
    let stores = engine.episodic_store.read().await;
    let store = stores.get(&scope_id).expect("store exists");
    assert_eq!(store.cached_constraints.len(), 25);
    Ok(())
}

#[tokio::test]
async fn record_negative_knowledge_from_tool_failure_is_immediately_queryable() -> anyhow::Result<()>
{
    let engine = make_test_engine().await;

    engine
        .record_negative_knowledge_from_tool_failure(
            Some("Inspect persisted state"),
            "read_file",
            "src/main.rs",
            "No such file or directory",
        )
        .await?;

    let constraints = engine
        .query_active_constraints(Some("Inspect persisted state"))
        .await?;

    assert_eq!(constraints.len(), 1);
    assert!(constraints[0].subject.contains("Inspect persisted state"));
    assert!(constraints[0].subject.contains("read_file(src/main.rs)"));
    assert_eq!(constraints[0].description, "No such file or directory");
    assert!(constraints[0].direct_observation);
    Ok(())
}
