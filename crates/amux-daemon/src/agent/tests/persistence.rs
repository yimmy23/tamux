use super::*;
use crate::agent::orchestrator_policy::{
    PolicyAction, PolicyDecision, PolicyDecisionScope, RecentPolicyDecision,
    SHORT_LIVED_POLICY_WINDOW_SECS,
};
use crate::session_manager::SessionManager;
use tempfile::tempdir;

#[tokio::test]
async fn hydrate_migrates_legacy_gateway_threads_json_to_sqlite_and_removes_file() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let existing = engine
        .history
        .list_gateway_thread_bindings()
        .await
        .expect("list initial gateway bindings");
    for (channel_key, _) in existing {
        engine
            .history
            .delete_gateway_thread_binding(&channel_key)
            .await
            .expect("delete initial gateway binding");
    }

    let legacy_path = engine.data_dir.join("gateway-threads.json");
    let legacy_json = serde_json::json!({
        "Slack:C123": "thread_alpha",
        "Discord:456": "thread_beta"
    });
    tokio::fs::write(
        &legacy_path,
        serde_json::to_string_pretty(&legacy_json).expect("serialize legacy json"),
    )
    .await
    .expect("write legacy gateway map");
    assert!(legacy_path.exists());

    engine.hydrate().await.expect("hydrate should succeed");

    assert!(
        !legacy_path.exists(),
        "legacy gateway-threads.json should be removed after migration"
    );

    let bindings = engine
        .history
        .list_gateway_thread_bindings()
        .await
        .expect("list migrated gateway bindings");
    let map: std::collections::HashMap<String, String> = bindings.into_iter().collect();
    assert_eq!(
        map.get("Slack:C123").map(String::as_str),
        Some("thread_alpha")
    );
    assert_eq!(
        map.get("Discord:456").map(String::as_str),
        Some("thread_beta")
    );

    let in_memory = engine.gateway_threads.read().await;
    assert_eq!(
        map.get("Slack:C123").map(String::as_str),
        Some("thread_alpha")
    );
    assert_eq!(
        map.get("Discord:456").map(String::as_str),
        Some("thread_beta")
    );
    assert_eq!(
        in_memory.get("Slack:C123").map(String::as_str),
        Some("thread_alpha")
    );
    assert_eq!(
        in_memory.get("Discord:456").map(String::as_str),
        Some("thread_beta")
    );
}

#[tokio::test]
async fn latest_policy_decision_round_trips_through_agent_engine_memory() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let scope = PolicyDecisionScope {
        thread_id: "thread-1".to_string(),
        goal_run_id: Some("goal-1".to_string()),
    };
    let decision = PolicyDecision {
        action: PolicyAction::Pivot,
        reason: "Try a narrower recovery path.".to_string(),
        strategy_hint: Some("inspect logs first".to_string()),
        retry_guard: Some("approach-hash-1".to_string()),
    };

    engine
        .record_policy_decision(&scope, decision.clone(), 1_000)
        .await;

    assert_eq!(
        engine.latest_policy_decision(&scope, 1_030).await,
        Some(RecentPolicyDecision {
            decision,
            decided_at_epoch_secs: 1_000,
        })
    );
}

#[tokio::test]
async fn retry_guard_expires_from_agent_engine_short_lived_memory() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let scope = PolicyDecisionScope {
        thread_id: "thread-1".to_string(),
        goal_run_id: Some("goal-1".to_string()),
    };

    engine
        .record_retry_guard(&scope, "approach-hash-1", 1_000)
        .await;

    assert!(
        engine
            .is_retry_guard_active(
                &scope,
                "approach-hash-1",
                1_000 + SHORT_LIVED_POLICY_WINDOW_SECS
            )
            .await
    );
    assert!(
        !engine
            .is_retry_guard_active(
                &scope,
                "approach-hash-1",
                1_001 + SHORT_LIVED_POLICY_WINDOW_SECS,
            )
            .await
    );
}

#[tokio::test]
async fn policy_memory_does_not_leak_across_goal_runs_in_same_thread() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_one = PolicyDecisionScope {
        thread_id: "thread-1".to_string(),
        goal_run_id: Some("goal-1".to_string()),
    };
    let goal_two = PolicyDecisionScope {
        thread_id: "thread-1".to_string(),
        goal_run_id: Some("goal-2".to_string()),
    };

    engine
        .record_policy_decision(
            &goal_one,
            PolicyDecision {
                action: PolicyAction::HaltRetries,
                reason: "Stop retrying this failing path.".to_string(),
                strategy_hint: None,
                retry_guard: Some("approach-hash-1".to_string()),
            },
            1_000,
        )
        .await;

    assert_eq!(engine.latest_policy_decision(&goal_two, 1_030).await, None);
    assert!(
        !engine
            .is_retry_guard_active(&goal_two, "approach-hash-1", 1_030)
            .await
    );
}

#[tokio::test]
async fn hydrate_async_syncs_seeded_builtin_skills_into_catalog() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine.hydrate().await.expect("hydrate should succeed");

    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let variants = engine
                .history
                .list_skill_variants(Some("brainstorming"), 10)
                .await
                .expect("list skill variants");
            if variants.iter().any(|variant| {
                variant.relative_path.ends_with("/brainstorming/SKILL.md")
                    || variant.relative_path == "development/superpowers/brainstorming/SKILL.md"
            }) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("built-in skill sync should complete in the background");
}

#[tokio::test]
async fn hydrate_returns_before_background_gateway_init_finishes() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.gateway.telegram_token = "telegram-token".to_string();
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .set_gateway_init_test_delay(std::time::Duration::from_millis(300))
        .await;

    tokio::time::timeout(std::time::Duration::from_millis(100), engine.hydrate())
        .await
        .expect("hydrate should not block on gateway init")
        .expect("hydrate should succeed");

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if engine.gateway_state.lock().await.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("background gateway init should eventually complete");
}

#[tokio::test]
async fn hydrate_does_not_wait_for_non_playground_thread_message_hydration() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let thread_id = "thread-hydrate-lazy-messages";
    let thread_row = amux_protocol::AgentDbThread {
        id: thread_id.to_string(),
        workspace_id: None,
        surface_id: None,
        pane_id: None,
        agent_name: Some("Rarog".to_string()),
        title: "Hydrate lazy thread".to_string(),
        created_at: 1_000,
        updated_at: 2_000,
        message_count: 1,
        total_tokens: 0,
        last_preview: "hello from hydrate".to_string(),
        metadata_json: None,
    };
    let message_row = amux_protocol::AgentDbMessage {
        id: "msg-hydrate-lazy".to_string(),
        thread_id: thread_id.to_string(),
        created_at: 2_000,
        role: "user".to_string(),
        content: "hello from hydrate".to_string(),
        provider: None,
        model: None,
        input_tokens: None,
        output_tokens: None,
        total_tokens: None,
        cost_usd: None,
        reasoning: None,
        tool_calls_json: None,
        metadata_json: None,
    };
    engine
        .history
        .reconcile_thread_snapshot(&thread_row, &[message_row])
        .await
        .expect("persist thread snapshot");
    engine
        .set_thread_message_hydration_test_delay(std::time::Duration::from_millis(300))
        .await;

    tokio::time::timeout(std::time::Duration::from_millis(100), engine.hydrate())
        .await
        .expect("hydrate should not wait on non-playground thread message hydration")
        .expect("hydrate should succeed");

    assert!(
        engine
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id),
        "non-playground thread messages should remain lazily hydrated after startup"
    );

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        engine.ensure_thread_messages_loaded(thread_id).await;
    })
    .await
    .expect("explicit thread hydration should still work on demand");

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    assert_eq!(thread.messages.len(), 1);
    assert_eq!(thread.messages[0].content, "hello from hydrate");
}

#[tokio::test]
async fn hydrate_does_not_wait_for_goal_run_projection_persistence() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-hydrate-background-persist";

    engine.goal_runs.lock().await.push_back(GoalRun {
        id: goal_run_id.to_string(),
        title: "Hydrate goal persistence".to_string(),
        goal: "Ensure hydrate returns before goal projections finish".to_string(),
        client_request_id: None,
        status: GoalRunStatus::Running,
        priority: TaskPriority::Normal,
        created_at: 4_000,
        updated_at: 4_500,
        started_at: Some(4_000),
        completed_at: None,
        thread_id: Some("thread-goal-hydrate".to_string()),
        session_id: None,
        current_step_index: 0,
        current_step_title: Some("Investigate".to_string()),
        current_step_kind: Some(GoalRunStepKind::Reason),
        planner_owner_profile: None,
        current_step_owner_profile: None,
        replan_count: 0,
        max_replans: 1,
        plan_summary: Some("One-step plan".to_string()),
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        stopped_reason: None,
        child_task_ids: Vec::new(),
        child_task_count: 0,
        approval_count: 0,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        active_task_id: None,
        duration_ms: None,
        steps: vec![GoalRunStep {
            id: "step-1".to_string(),
            position: 0,
            title: "Investigate".to_string(),
            instructions: "Inspect startup state".to_string(),
            kind: GoalRunStepKind::Reason,
            success_criteria: "Identify the blocking work".to_string(),
            session_id: None,
            status: GoalRunStepStatus::InProgress,
            task_id: None,
            summary: None,
            error: None,
            started_at: Some(4_010),
            completed_at: None,
        }],
        events: Vec::new(),
        dossier: None,
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        model_usage: Vec::new(),
        autonomy_level: crate::agent::AutonomyLevel::Supervised,
        authorship_tag: None,
        launch_assignment_snapshot: Vec::new(),
        runtime_assignment_list: Vec::new(),
        root_thread_id: None,
        active_thread_id: None,
        execution_thread_ids: Vec::new(),
    });
    engine.persist_goal_runs().await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    let _delay_guard = crate::agent::goal_dossier::set_goal_projection_write_delay_for_tests(
        std::time::Duration::from_millis(250),
    );

    tokio::time::timeout(std::time::Duration::from_millis(100), rehydrated.hydrate())
        .await
        .expect("hydrate should not wait on delayed goal projection persistence")
        .expect("hydrate should still succeed while goal persistence continues");

    let hydrated = rehydrated
        .get_goal_run(goal_run_id)
        .await
        .expect("goal run should be available immediately after hydrate");
    assert_eq!(
        hydrated.status,
        GoalRunStatus::Paused,
        "interrupted goal runs should still be paused during hydrate"
    );

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let persisted = rehydrated
                .history
                .get_goal_run(goal_run_id)
                .await
                .expect("read persisted goal run")
                .expect("goal run should remain persisted");
            if persisted.status == GoalRunStatus::Paused {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("background goal persistence should eventually finish");
}

#[tokio::test]
async fn hydrate_restores_repo_watchers_without_duplicate_root_watchers() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let repo_one = root.path().join("repo-one");
    let repo_two = root.path().join("repo-two");
    std::fs::create_dir_all(&repo_one).expect("create repo one");
    std::fs::create_dir_all(&repo_two).expect("create repo two");

    let contexts = HashMap::from([
        (
            "thread-1".to_string(),
            ThreadWorkContext {
                thread_id: "thread-1".to_string(),
                entries: vec![WorkContextEntry {
                    path: "alpha.rs".to_string(),
                    previous_path: None,
                    kind: WorkContextEntryKind::RepoChange,
                    source: "test".to_string(),
                    change_kind: None,
                    repo_root: Some(repo_one.to_string_lossy().to_string()),
                    goal_run_id: None,
                    step_index: None,
                    session_id: None,
                    is_text: true,
                    updated_at: 1,
                }],
            },
        ),
        (
            "thread-2".to_string(),
            ThreadWorkContext {
                thread_id: "thread-2".to_string(),
                entries: vec![WorkContextEntry {
                    path: "beta.rs".to_string(),
                    previous_path: None,
                    kind: WorkContextEntryKind::RepoChange,
                    source: "test".to_string(),
                    change_kind: None,
                    repo_root: Some(repo_one.to_string_lossy().to_string()),
                    goal_run_id: None,
                    step_index: None,
                    session_id: None,
                    is_text: true,
                    updated_at: 2,
                }],
            },
        ),
        (
            "thread-3".to_string(),
            ThreadWorkContext {
                thread_id: "thread-3".to_string(),
                entries: vec![WorkContextEntry {
                    path: "gamma.rs".to_string(),
                    previous_path: None,
                    kind: WorkContextEntryKind::RepoChange,
                    source: "test".to_string(),
                    change_kind: None,
                    repo_root: Some(repo_two.to_string_lossy().to_string()),
                    goal_run_id: None,
                    step_index: None,
                    session_id: None,
                    is_text: true,
                    updated_at: 3,
                }],
            },
        ),
    ]);
    tokio::fs::write(
        engine.data_dir.join("work-context.json"),
        serde_json::to_string_pretty(&contexts).expect("serialize work contexts"),
    )
    .await
    .expect("write work contexts");

    engine.hydrate().await.expect("hydrate should succeed");

    let immediate_watcher_count = engine.repo_watchers.lock().await.len();
    assert!(
        immediate_watcher_count <= 2,
        "hydrate should not restore more than one watcher per repo root"
    );

    let repo_one_key = std::fs::canonicalize(&repo_one)
        .expect("canonicalize repo one")
        .to_string_lossy()
        .to_string();
    let repo_two_key = std::fs::canonicalize(&repo_two)
        .expect("canonicalize repo two")
        .to_string_lossy()
        .to_string();

    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let membership = {
                let watchers = engine.repo_watchers.lock().await;
                if watchers.len() != 2 {
                    None
                } else {
                    let repo_one_threads = watchers
                        .get(&repo_one_key)
                        .map(|entry| {
                            entry
                                .thread_ids
                                .lock()
                                .expect("repo one watcher membership")
                                .clone()
                        })
                        .unwrap_or_default();
                    let repo_two_threads = watchers
                        .get(&repo_two_key)
                        .map(|entry| {
                            entry
                                .thread_ids
                                .lock()
                                .expect("repo two watcher membership")
                                .clone()
                        })
                        .unwrap_or_default();
                    Some((repo_one_threads, repo_two_threads))
                }
            };

            if let Some((repo_one_threads, repo_two_threads)) = membership {
                if repo_one_threads
                    == HashSet::from(["thread-1".to_string(), "thread-2".to_string()])
                    && repo_two_threads == HashSet::from(["thread-3".to_string()])
                {
                    break;
                }
            }

            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("repo watcher restoration should finish in the background");
}

#[tokio::test]
async fn remove_repo_watcher_keeps_shared_root_watcher_until_last_thread_leaves() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let repo_root = root.path().join("shared-repo");
    std::fs::create_dir_all(&repo_root).expect("create shared repo");
    let repo_key = std::fs::canonicalize(&repo_root)
        .expect("canonicalize shared repo")
        .to_string_lossy()
        .to_string();

    engine
        .ensure_repo_watcher("thread-1", &repo_root.to_string_lossy())
        .await;
    engine
        .ensure_repo_watcher("thread-2", &repo_root.to_string_lossy())
        .await;

    {
        let watchers = engine.repo_watchers.lock().await;
        let entry = watchers
            .get(&repo_key)
            .expect("shared watcher should exist");
        assert_eq!(watchers.len(), 1);
        assert_eq!(
            entry
                .thread_ids
                .lock()
                .expect("shared watcher membership")
                .clone(),
            HashSet::from(["thread-1".to_string(), "thread-2".to_string()])
        );
    }

    engine.remove_repo_watcher("thread-1").await;

    {
        let watchers = engine.repo_watchers.lock().await;
        let entry = watchers
            .get(&repo_key)
            .expect("shared watcher should remain for thread-2");
        assert_eq!(watchers.len(), 1);
        assert_eq!(
            entry
                .thread_ids
                .lock()
                .expect("remaining watcher membership")
                .clone(),
            HashSet::from(["thread-2".to_string()])
        );
    }

    engine.remove_repo_watcher("thread-2").await;
    assert!(engine.repo_watchers.lock().await.is_empty());
}
