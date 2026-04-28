use super::*;
use crate::agent::collaboration::{Disagreement, Vote};
use crate::agent::{types::TaskStatus, AgentConfig, AgentEngine};
use crate::session_manager::SessionManager;
use tempfile::tempdir;

fn make_framings(count: usize) -> Vec<Framing> {
    let labels = ["analytical-lens", "pragmatic-lens", "creative-lens"];
    let prompts = ["Analyze formally", "Be pragmatic", "Think creatively"];
    (0..count)
        .map(|i| Framing {
            label: labels[i % 3].to_string(),
            system_prompt_override: prompts[i % 3].to_string(),
            task_id: Some(format!("task_{}", i)),
            contribution_id: None,
        })
        .collect()
}

#[test]
fn new_session_with_2_framings_has_spawning_status() {
    let framings = make_framings(2);
    let session = DivergentSession::new("optimize db queries".to_string(), framings)
        .expect("should create session");
    assert_eq!(session.status, DivergentStatus::Spawning);
    assert_eq!(session.framings.len(), 2);
    assert!(session.id.starts_with("divergent_"));
}

#[test]
fn new_session_with_3_framings_succeeds() {
    let framings = make_framings(3);
    let session =
        DivergentSession::new("design auth".to_string(), framings).expect("should create session");
    assert_eq!(session.framings.len(), 3);
}

#[test]
fn new_session_rejects_fewer_than_2_framings() {
    let framings = make_framings(1);
    let result = DivergentSession::new("problem".to_string(), framings);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("at least 2"), "error: {}", err);
}

#[test]
fn new_session_rejects_more_than_3_framings() {
    let labels = ["a", "b", "c", "d"];
    let framings: Vec<Framing> = labels
        .iter()
        .map(|label| Framing {
            label: label.to_string(),
            system_prompt_override: "test".to_string(),
            task_id: None,
            contribution_id: None,
        })
        .collect();
    let result = DivergentSession::new("problem".to_string(), framings);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("at most 3"), "error: {}", err);
}

#[test]
fn status_transitions_spawning_to_running() {
    let mut session = DivergentSession::new("p".to_string(), make_framings(2)).unwrap();
    assert!(session.transition_to(DivergentStatus::Running).is_ok());
    assert_eq!(session.status, DivergentStatus::Running);
}

#[test]
fn status_transitions_running_to_mediating() {
    let mut session = DivergentSession::new("p".to_string(), make_framings(2)).unwrap();
    session.transition_to(DivergentStatus::Running).unwrap();
    assert!(session.transition_to(DivergentStatus::Mediating).is_ok());
    assert_eq!(session.status, DivergentStatus::Mediating);
}

#[test]
fn status_transitions_mediating_to_complete() {
    let mut session = DivergentSession::new("p".to_string(), make_framings(2)).unwrap();
    session.transition_to(DivergentStatus::Running).unwrap();
    session.transition_to(DivergentStatus::Mediating).unwrap();
    assert!(session.transition_to(DivergentStatus::Complete).is_ok());
    assert_eq!(session.status, DivergentStatus::Complete);
}

#[test]
fn status_transitions_full_lifecycle() {
    let mut session = DivergentSession::new("p".to_string(), make_framings(2)).unwrap();
    assert_eq!(session.status, DivergentStatus::Spawning);
    session.transition_to(DivergentStatus::Running).unwrap();
    session.transition_to(DivergentStatus::Mediating).unwrap();
    session.transition_to(DivergentStatus::Complete).unwrap();
    assert_eq!(session.status, DivergentStatus::Complete);
}

#[test]
fn status_any_state_can_transition_to_failed() {
    let mut session = DivergentSession::new("p".to_string(), make_framings(2)).unwrap();
    assert!(session.transition_to(DivergentStatus::Failed).is_ok());
    assert_eq!(session.status, DivergentStatus::Failed);
}

#[test]
fn status_invalid_transition_rejected() {
    let mut session = DivergentSession::new("p".to_string(), make_framings(2)).unwrap();
    assert!(session.transition_to(DivergentStatus::Complete).is_err());
    assert!(session.transition_to(DivergentStatus::Mediating).is_err());
}

#[test]
fn generate_framing_prompts_produces_2_distinct_framings() {
    let framings = generate_framing_prompts("optimize database queries");
    assert_eq!(framings.len(), 2);
    assert_ne!(framings[0].label, framings[1].label);
    assert_ne!(
        framings[0].system_prompt_override,
        framings[1].system_prompt_override
    );
}

#[test]
fn generate_framing_prompts_includes_problem_in_prompts() {
    let framings = generate_framing_prompts("design user auth");
    for framing in &framings {
        assert!(
            framing.system_prompt_override.contains("design user auth"),
            "framing '{}' should contain problem statement",
            framing.label
        );
    }
}

#[test]
fn generate_framing_prompts_has_analytical_and_pragmatic_lenses() {
    let framings = generate_framing_prompts("any problem");
    let labels: Vec<&str> = framings
        .iter()
        .map(|framing| framing.label.as_str())
        .collect();
    assert!(
        labels.contains(&"analytical-lens"),
        "missing analytical-lens"
    );
    assert!(labels.contains(&"pragmatic-lens"), "missing pragmatic-lens");
}

#[test]
fn format_tensions_no_disagreements() {
    let result = format_tensions(&[], &[]);
    assert_eq!(
        result,
        "No significant disagreements detected between framings."
    );
}

#[test]
fn format_tensions_with_disagreements_produces_markdown() {
    let disagreements = vec![
        Disagreement {
            id: "d1".to_string(),
            topic: "caching strategy".to_string(),
            agents: vec!["task_0".to_string(), "task_1".to_string()],
            positions: vec!["recommend".to_string(), "reject".to_string()],
            confidence_gap: 0.3,
            resolution: "pending".to_string(),
            votes: Vec::new(),
            debate_session_id: None,
        },
        Disagreement {
            id: "d2".to_string(),
            topic: "error handling".to_string(),
            agents: vec!["task_0".to_string(), "task_1".to_string()],
            positions: vec!["recommend".to_string(), "reject".to_string()],
            confidence_gap: 0.1,
            resolution: "pending".to_string(),
            votes: vec![Vote {
                task_id: "task_0".to_string(),
                position: "recommend".to_string(),
                weight: 0.9,
            }],
            debate_session_id: None,
        },
    ];
    let framings = make_framings(2);
    let result = format_tensions(&disagreements, &framings);

    assert!(
        result.contains("### caching strategy"),
        "missing topic heading"
    );
    assert!(
        result.contains("### error handling"),
        "missing second topic"
    );
    assert!(result.contains("**"), "missing bold formatting");
    assert!(result.contains("Evidence:"), "missing evidence section");
}

#[test]
fn format_mediator_prompt_includes_problem_statement() {
    let session =
        DivergentSession::new("optimize database queries".to_string(), make_framings(2)).unwrap();
    let prompt = format_mediator_prompt(&session, "some tensions");
    assert!(
        prompt.contains("optimize database queries"),
        "prompt should contain problem statement"
    );
}

#[test]
fn format_mediator_prompt_includes_all_framing_labels() {
    let session = DivergentSession::new("test problem".to_string(), make_framings(2)).unwrap();
    let prompt = format_mediator_prompt(&session, "tensions");
    assert!(
        prompt.contains("analytical-lens"),
        "missing analytical-lens label"
    );
    assert!(
        prompt.contains("pragmatic-lens"),
        "missing pragmatic-lens label"
    );
}

#[test]
fn format_mediator_prompt_includes_tensions() {
    let session = DivergentSession::new("problem".to_string(), make_framings(2)).unwrap();
    let tensions = "### caching\n**analytical-lens:** recommend\n**pragmatic-lens:** reject";
    let prompt = format_mediator_prompt(&session, tensions);
    assert!(
        prompt.contains(tensions),
        "prompt should include tensions verbatim"
    );
}

#[test]
fn format_mediator_prompt_instructs_no_forced_consensus() {
    let session = DivergentSession::new("problem".to_string(), make_framings(2)).unwrap();
    let prompt = format_mediator_prompt(&session, "tensions");
    assert!(
        prompt.contains("do NOT force consensus"),
        "prompt must instruct against forced consensus"
    );
}

#[test]
fn format_mediator_prompt_instructs_acknowledge_tradeoffs() {
    let session = DivergentSession::new("problem".to_string(), make_framings(2)).unwrap();
    let prompt = format_mediator_prompt(&session, "tensions");
    assert!(
        prompt.contains("tradeoffs"),
        "prompt must mention tradeoffs"
    );
    assert!(
        prompt.contains("Do NOT pick a \"winner.\""),
        "prompt must say no winner picking"
    );
}

#[tokio::test]
async fn divergent_completion_hook_records_contribution_for_completed_framing_task() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let session_id = engine
        .start_divergent_session("evaluate cache strategy", None, "thread-div-1", None)
        .await
        .expect("start divergent session");
    let framing_task_id = {
        let sessions = engine.divergent_sessions.read().await;
        sessions
            .get(&session_id)
            .and_then(|session| session.framings.first())
            .and_then(|framing| framing.task_id.clone())
            .expect("first framing task id")
    };
    {
        let mut tasks = engine.tasks.lock().await;
        if let Some(task) = tasks.iter_mut().find(|task| task.id == framing_task_id) {
            task.status = TaskStatus::Completed;
            task.result = Some("cache the hot set with explicit eviction policy".to_string());
        }
    }
    let updated_task = {
        let tasks = engine.tasks.lock().await;
        tasks
            .iter()
            .find(|task| task.id == framing_task_id)
            .cloned()
            .expect("task should exist")
    };
    let handled = engine
        .record_divergent_contribution_on_task_completion(&updated_task)
        .await
        .expect("completion hook should succeed");
    assert!(
        handled,
        "divergent completion hook should run for divergent task"
    );

    let sessions = engine.divergent_sessions.read().await;
    let session = sessions.get(&session_id).expect("session should exist");
    assert!(
        session.framings[0].contribution_id.is_some(),
        "framing should receive contribution_id after completion hook"
    );
}

#[tokio::test]
async fn divergent_completion_hook_final_contribution_synthesizes_tensions_and_mediator_prompt() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let session_id = engine
        .start_divergent_session("choose migration strategy", None, "thread-div-2", None)
        .await
        .expect("start divergent session");
    let task_ids = {
        let sessions = engine.divergent_sessions.read().await;
        sessions
            .get(&session_id)
            .expect("session should exist")
            .framings
            .iter()
            .filter_map(|framing| framing.task_id.clone())
            .collect::<Vec<_>>()
    };
    for (idx, task_id) in task_ids.iter().enumerate() {
        {
            let mut tasks = engine.tasks.lock().await;
            if let Some(task) = tasks.iter_mut().find(|task| &task.id == task_id) {
                task.status = TaskStatus::Completed;
                task.result = Some(if idx == 0 {
                    "Prefer strict correctness checks before rollout".to_string()
                } else {
                    "Prioritize fast staged rollout with lightweight safeguards".to_string()
                });
            }
        }
        let task_snapshot = {
            let tasks = engine.tasks.lock().await;
            tasks
                .iter()
                .find(|task| &task.id == task_id)
                .cloned()
                .expect("task should exist")
        };
        engine
            .record_divergent_contribution_on_task_completion(&task_snapshot)
            .await
            .expect("completion hook should succeed");
    }

    let payload = engine
        .get_divergent_session(&session_id)
        .await
        .expect("session payload should be available");
    assert_eq!(
        payload.get("status").and_then(|value| value.as_str()),
        Some("complete"),
        "final contribution should complete session"
    );
    assert!(
        payload
            .get("tensions_markdown")
            .and_then(|value| value.as_str())
            .is_some_and(|value| !value.is_empty()),
        "completed payload should include tensions markdown"
    );
    assert!(
        payload
            .get("mediator_prompt")
            .and_then(|value| value.as_str())
            .is_some_and(|value| !value.is_empty()),
        "completed payload should include mediator prompt"
    );
}

#[tokio::test]
async fn divergent_completion_hook_ignores_non_divergent_tasks() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let task = engine
        .enqueue_task(
            "Regular task".to_string(),
            "non divergent completion".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "goal_run",
            None,
            None,
            Some("thread-regular".to_string()),
            None,
        )
        .await;
    {
        let mut tasks = engine.tasks.lock().await;
        if let Some(current) = tasks.iter_mut().find(|entry| entry.id == task.id) {
            current.status = TaskStatus::Completed;
            current.result = Some("done".to_string());
        }
    }
    let snapshot = {
        let tasks = engine.tasks.lock().await;
        tasks
            .iter()
            .find(|entry| entry.id == task.id)
            .cloned()
            .expect("task exists")
    };
    let handled = engine
        .record_divergent_contribution_on_task_completion(&snapshot)
        .await
        .expect("hook should not error");
    assert!(!handled, "non-divergent tasks should bypass divergent hook");
}
