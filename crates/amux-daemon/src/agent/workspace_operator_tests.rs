use super::*;
use amux_protocol::{
    WorkspaceActor, WorkspaceOperator, WorkspaceTaskCreate, WorkspaceTaskStatus, WorkspaceTaskType,
    WorkspaceTaskUpdate, AGENT_ID_SWAROG,
};

async fn test_engine(root: &std::path::Path) -> Result<Arc<AgentEngine>> {
    let manager = SessionManager::new_test(root).await;
    Ok(AgentEngine::new_test(manager, AgentConfig::default(), root).await)
}

async fn test_engine_with_config(
    root: &std::path::Path,
    config: AgentConfig,
) -> Result<Arc<AgentEngine>> {
    let manager = SessionManager::new_test(root).await;
    Ok(AgentEngine::new_test(manager, config, root).await)
}

async fn spawn_hung_workspace_server() -> String {
    use tokio::io::AsyncReadExt;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind hung workspace server");
    let addr = listener.local_addr().expect("hung workspace server addr");
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buffer = [0u8; 1024];
                let _ = socket.read(&mut buffer).await;
                tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            });
        }
    });
    format!("http://{addr}/v1")
}

fn workspace_request(task_type: WorkspaceTaskType) -> WorkspaceTaskCreate {
    WorkspaceTaskCreate {
        workspace_id: "main".to_string(),
        title: "Implement board".to_string(),
        task_type,
        description: "Create the workspace board surface".to_string(),
        definition_of_done: Some("Board state persists".to_string()),
        priority: None,
        assignee: None,
        reviewer: Some(WorkspaceActor::User),
    }
}

#[tokio::test]
async fn deferred_svarog_operator_switch_does_not_wait_for_thread_launch() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.base_url = spawn_hung_workspace_server().await;
    let engine = test_engine_with_config(root.path(), config).await?;
    let mut request = workspace_request(WorkspaceTaskType::Thread);
    request.assignee = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));
    let task = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;

    tokio::time::timeout(
        std::time::Duration::from_millis(150),
        engine.set_workspace_operator_deferred_auto_start("main", WorkspaceOperator::Svarog),
    )
    .await
    .expect("operator switch should return before thread launch finishes")?;

    for _ in 0..20 {
        let loaded = engine
            .history
            .get_workspace_task(&task.id)
            .await?
            .expect("assigned task exists");
        if loaded.status == WorkspaceTaskStatus::InProgress {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    anyhow::bail!("deferred operator start did not mark assigned task in progress");
}

#[tokio::test]
async fn svarog_operator_starts_assigned_todo_tasks() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let mut assigned_request = workspace_request(WorkspaceTaskType::Goal);
    assigned_request.title = "Assigned".to_string();
    assigned_request.assignee = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));
    let assigned = engine
        .create_workspace_task(assigned_request, WorkspaceActor::User)
        .await?;
    let mut unassigned_request = workspace_request(WorkspaceTaskType::Goal);
    unassigned_request.title = "Unassigned".to_string();
    let unassigned = engine
        .create_workspace_task(unassigned_request, WorkspaceActor::User)
        .await?;

    engine
        .set_workspace_operator("main", WorkspaceOperator::Svarog)
        .await?;

    let assigned = engine
        .get_workspace_task(&assigned.id)
        .await?
        .expect("assigned task exists");
    let unassigned = engine
        .get_workspace_task(&unassigned.id)
        .await?
        .expect("unassigned task exists");
    assert_eq!(assigned.status, WorkspaceTaskStatus::InProgress);
    assert_eq!(unassigned.status, WorkspaceTaskStatus::Todo);
    let notices = engine
        .list_workspace_notices("main", Some(&assigned.id))
        .await?;
    assert!(notices
        .iter()
        .any(|notice| notice.notice_type == "run_started"));
    Ok(())
}

#[tokio::test]
async fn svarog_operator_starts_todo_task_when_assigned() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let task = engine
        .create_workspace_task(
            workspace_request(WorkspaceTaskType::Goal),
            WorkspaceActor::User,
        )
        .await?;
    engine
        .set_workspace_operator("main", WorkspaceOperator::Svarog)
        .await?;

    let updated = engine
        .update_workspace_task(
            &task.id,
            WorkspaceTaskUpdate {
                title: None,
                description: None,
                definition_of_done: None,
                priority: None,
                assignee: Some(Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()))),
                reviewer: None,
            },
        )
        .await?
        .expect("task should update");

    assert_eq!(updated.status, WorkspaceTaskStatus::InProgress);
    let notices = engine
        .list_workspace_notices("main", Some(&task.id))
        .await?;
    assert!(notices
        .iter()
        .any(|notice| notice.notice_type == "run_started"));
    Ok(())
}

#[tokio::test]
async fn svarog_operator_starts_assigned_task_on_create() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    engine
        .set_workspace_operator("main", WorkspaceOperator::Svarog)
        .await?;
    let mut request = workspace_request(WorkspaceTaskType::Goal);
    request.assignee = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));

    let task = engine
        .create_workspace_task_deferred_auto_start(request, WorkspaceActor::User)
        .await?;

    assert_eq!(task.status, WorkspaceTaskStatus::Todo);
    let mut started = None;
    for _ in 0..20 {
        let current = engine
            .get_workspace_task(&task.id)
            .await?
            .expect("assigned task exists");
        if current.status == WorkspaceTaskStatus::InProgress {
            started = Some(current);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(
        started.expect("assigned task should auto-start").status,
        WorkspaceTaskStatus::InProgress
    );
    let notices = engine
        .list_workspace_notices("main", Some(&task.id))
        .await?;
    assert!(notices.iter().any(|notice| notice.notice_type == "created"));
    assert!(notices
        .iter()
        .any(|notice| notice.notice_type == "run_started"));
    Ok(())
}

#[tokio::test]
async fn svarog_reconciliation_starts_persisted_assigned_todo_tasks() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let mut request = workspace_request(WorkspaceTaskType::Goal);
    request.assignee = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));
    let assigned = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;
    let unassigned = engine
        .create_workspace_task(
            workspace_request(WorkspaceTaskType::Goal),
            WorkspaceActor::User,
        )
        .await?;
    let mut settings = engine.get_or_create_workspace_settings("main").await?;
    settings.operator = WorkspaceOperator::Svarog;
    engine.history.upsert_workspace_settings(&settings).await?;

    engine.reconcile_svarog_workspace_operator_tasks().await?;

    let assigned = engine
        .get_workspace_task(&assigned.id)
        .await?
        .expect("assigned task exists");
    let unassigned = engine
        .get_workspace_task(&unassigned.id)
        .await?
        .expect("unassigned task exists");
    assert_eq!(assigned.status, WorkspaceTaskStatus::InProgress);
    assert_eq!(unassigned.status, WorkspaceTaskStatus::Todo);
    Ok(())
}

#[tokio::test]
async fn engine_startup_spawns_svarog_workspace_reconciliation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let engine = test_engine(root.path()).await?;
    let mut request = workspace_request(WorkspaceTaskType::Goal);
    request.assignee = Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()));
    let assigned = engine
        .create_workspace_task(request, WorkspaceActor::User)
        .await?;
    let mut settings = engine.get_or_create_workspace_settings("main").await?;
    settings.operator = WorkspaceOperator::Svarog;
    engine.history.upsert_workspace_settings(&settings).await?;

    let restarted = AgentEngine::new_with_shared_history(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        Arc::new(engine.history.clone()),
    );

    for _ in 0..20 {
        let task = restarted
            .get_workspace_task(&assigned.id)
            .await?
            .expect("assigned task exists");
        if task.status == WorkspaceTaskStatus::InProgress {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    anyhow::bail!("startup reconciliation did not start assigned todo task");
}
