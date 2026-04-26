use super::*;
use amux_protocol::{WorkspaceActor, WorkspaceTaskCreate, WorkspaceTaskStatus, WorkspaceTaskType};

async fn test_engine(root: &std::path::Path) -> Result<Arc<AgentEngine>> {
    let manager = SessionManager::new_test(root).await;
    Ok(AgentEngine::new_test(manager, AgentConfig::default(), root).await)
}

fn workspace_request() -> WorkspaceTaskCreate {
    WorkspaceTaskCreate {
        workspace_id: "main".to_string(),
        title: "Implement board".to_string(),
        task_type: WorkspaceTaskType::Thread,
        description: "Create the workspace board surface".to_string(),
        definition_of_done: None,
        priority: None,
        assignee: None,
        reviewer: Some(WorkspaceActor::User),
    }
}

#[tokio::test]
async fn create_workspace_task_broadcasts_task_and_notice_updates() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let mut events = engine.subscribe();

    let created = engine
        .create_workspace_task(workspace_request(), WorkspaceActor::User)
        .await?;

    let mut saw_task = false;
    let mut saw_notice = false;
    for _ in 0..4 {
        let event =
            tokio::time::timeout(std::time::Duration::from_secs(1), events.recv()).await??;
        match event {
            AgentEvent::WorkspaceTaskUpdate { task } if task.id == created.id => {
                assert_eq!(task.status, WorkspaceTaskStatus::Todo);
                assert!(task.last_notice_id.is_some());
                saw_task = true;
            }
            AgentEvent::WorkspaceNoticeUpdate { notice } if notice.task_id == created.id => {
                assert_eq!(notice.notice_type, "created");
                saw_notice = true;
            }
            _ => {}
        }
        if saw_task && saw_notice {
            return Ok(());
        }
    }

    anyhow::bail!("missing workspace task or notice broadcast");
}
