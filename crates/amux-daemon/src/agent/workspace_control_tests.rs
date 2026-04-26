use super::*;
use amux_protocol::{
    WorkspaceActor, WorkspaceTaskCreate, WorkspaceTaskStatus, WorkspaceTaskType, AGENT_ID_SWAROG,
};

async fn test_engine(root: &std::path::Path) -> Result<Arc<AgentEngine>> {
    let manager = SessionManager::new_test(root).await;
    Ok(AgentEngine::new_test(manager, AgentConfig::default(), root).await)
}

fn workspace_request() -> WorkspaceTaskCreate {
    WorkspaceTaskCreate {
        workspace_id: "main".to_string(),
        title: "Pause thread".to_string(),
        task_type: WorkspaceTaskType::Thread,
        description: "Check pause feedback".to_string(),
        definition_of_done: None,
        priority: None,
        assignee: Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string())),
        reviewer: Some(WorkspaceActor::User),
    }
}

#[tokio::test]
async fn pause_thread_workspace_task_records_unsupported_notice() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let mut task = engine
        .create_workspace_task(workspace_request(), WorkspaceActor::User)
        .await?;
    task.status = WorkspaceTaskStatus::InProgress;
    engine.history.upsert_workspace_task(&task).await?;

    engine.pause_workspace_task(&task.id).await?;

    let notices = engine
        .list_workspace_notices("main", Some(&task.id))
        .await?;
    assert!(notices
        .iter()
        .any(|notice| notice.notice_type == "pause_unsupported"
            && notice.message.contains("cannot pause thread tasks")));
    Ok(())
}
