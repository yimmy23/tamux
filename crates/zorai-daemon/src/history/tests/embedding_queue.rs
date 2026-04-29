use super::*;
use crate::agent::types::{AgentTask, TaskPriority, TaskStatus};
use zorai_protocol::{AgentDbMessage, AgentDbThread};

fn sample_thread() -> AgentDbThread {
    AgentDbThread {
        id: "thread-1".to_string(),
        workspace_id: Some("workspace-1".into()),
        surface_id: None,
        pane_id: None,
        agent_name: None,
        title: "Thread".to_string(),
        created_at: 90,
        updated_at: 100,
        message_count: 0,
        total_tokens: 0,
        last_preview: String::new(),
        metadata_json: None,
    }
}

fn sample_message(id: &str, content: &str) -> AgentDbMessage {
    AgentDbMessage {
        id: id.to_string(),
        thread_id: "thread-1".to_string(),
        created_at: 100,
        role: "user".to_string(),
        content: content.to_string(),
        provider: None,
        model: None,
        input_tokens: None,
        output_tokens: None,
        total_tokens: None,
        cost_usd: None,
        reasoning: None,
        tool_calls_json: None,
        metadata_json: None,
    }
}

fn sample_task(id: &str) -> AgentTask {
    AgentTask {
        id: id.to_string(),
        title: "Implement indexing".to_string(),
        description: "Persist semantic indexing work from daemon events".to_string(),
        status: TaskStatus::Queued,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: 200,
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some("thread-1".to_string()),
        source: "user".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: Some("session-1".to_string()),
        goal_run_id: None,
        goal_run_title: None,
        goal_step_id: None,
        goal_step_title: None,
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 3,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        lane_id: None,
        last_error: None,
        logs: Vec::new(),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_overflow_action: None,
        termination_conditions: None,
        success_criteria: None,
        max_duration_secs: None,
        supervisor_config: None,
        override_provider: None,
        override_model: None,
        override_system_prompt: None,
        sub_agent_def_id: None,
    }
}

#[tokio::test]
async fn adding_message_enqueues_embedding_job() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;

    store
        .add_message(&sample_message("msg-1", "semantic content"))
        .await?;

    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].source_kind, "agent_message");
    assert_eq!(jobs[0].source_id, "msg-1");
    assert_eq!(jobs[0].body, "semantic content");
    assert_eq!(jobs[0].thread_id.as_deref(), Some("thread-1"));
    Ok(())
}

#[tokio::test]
async fn adding_blank_message_does_not_enqueue_embedding_job() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;

    store
        .add_message(&sample_message("msg-blank", "   "))
        .await?;

    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert!(jobs.is_empty());
    Ok(())
}

#[tokio::test]
async fn upserting_task_enqueues_embedding_job() -> Result<()> {
    let (store, _root) = make_test_store().await?;

    store.upsert_agent_task(&sample_task("task-1")).await?;

    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].source_kind, "agent_task");
    assert_eq!(jobs[0].source_id, "task-1");
    assert!(jobs[0].body.contains("Implement indexing"));
    assert_eq!(jobs[0].thread_id.as_deref(), Some("thread-1"));
    Ok(())
}

#[tokio::test]
async fn embedding_completion_is_scoped_to_model_and_dimensions() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;
    store
        .add_message(&sample_message("msg-1", "same source"))
        .await?;

    let first = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert_eq!(first.len(), 1);
    store
        .complete_embedding_job(&first[0], "text-embedding-3-small", 1536)
        .await?;

    let same_model = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert!(same_model.is_empty());

    let switched_model = store
        .claim_embedding_jobs("nomic-embed-text", 768, 10)
        .await?;
    assert_eq!(switched_model.len(), 1);
    assert_eq!(switched_model[0].source_id, "msg-1");
    Ok(())
}

#[tokio::test]
async fn deleting_message_queues_vector_deletion() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;
    store
        .add_message(&sample_message("msg-delete", "remove me"))
        .await?;

    let deleted = store.delete_messages("thread-1", &["msg-delete"]).await?;
    assert_eq!(deleted, 1);

    let deletions = store.claim_embedding_deletions(10).await?;
    assert_eq!(deletions.len(), 1);
    assert_eq!(deletions[0].source_kind, "agent_message");
    assert_eq!(deletions[0].source_id, "msg-delete");
    Ok(())
}

#[tokio::test]
async fn updating_message_content_refreshes_embedding_job() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;
    store
        .add_message(&sample_message("msg-update", "old content"))
        .await?;

    store
        .update_message(
            "msg-update",
            &AgentMessagePatch {
                content: Some("new content".to_string()),
                ..AgentMessagePatch::default()
            },
        )
        .await?;

    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].body, "new content");
    Ok(())
}

#[tokio::test]
async fn queue_semantic_backfill_enqueues_existing_messages_and_tasks() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;
    store
        .add_message(&sample_message("msg-backfill", "index this old message"))
        .await?;
    store
        .upsert_agent_task(&sample_task("task-backfill"))
        .await?;

    let initial = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    for job in &initial {
        store
            .complete_embedding_job(job, "text-embedding-3-small", 1536)
            .await?;
    }

    let result = store.queue_semantic_backfill(None).await?;
    assert_eq!(result.messages_queued, 1);
    assert_eq!(result.tasks_queued, 1);

    let switched_model = store
        .claim_embedding_jobs("nomic-embed-text", 768, 10)
        .await?;
    assert_eq!(switched_model.len(), 2);
    Ok(())
}

#[tokio::test]
async fn semantic_index_status_counts_pending_for_selected_model() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;
    store
        .add_message(&sample_message("msg-status", "status content"))
        .await?;

    let pending = store
        .semantic_index_status("text-embedding-3-small", 1536)
        .await?;
    assert_eq!(pending.pending_for_model, 1);
    assert_eq!(pending.completed_for_model, 0);

    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    store
        .complete_embedding_job(&jobs[0], "text-embedding-3-small", 1536)
        .await?;

    let completed = store
        .semantic_index_status("text-embedding-3-small", 1536)
        .await?;
    assert_eq!(completed.pending_for_model, 0);
    assert_eq!(completed.completed_for_model, 1);
    Ok(())
}
