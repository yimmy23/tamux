use super::workspace_create_modal::*;
use crate::state::{modal, DaemonCommand};
use amux_protocol::{
    WorkspaceActor, WorkspaceOperator, WorkspacePriority, WorkspaceTask, WorkspaceTaskCreate,
    WorkspaceTaskStatus, WorkspaceTaskType, AGENT_ID_SWAROG,
};
use tokio::sync::mpsc::unbounded_channel;

#[test]
fn create_form_defaults_to_low_priority_and_svarog_assignee() {
    let form = WorkspaceCreateTaskForm::new(WorkspaceTaskType::Thread);

    assert_eq!(form.task_type, WorkspaceTaskType::Thread);
    assert_eq!(form.priority, WorkspacePriority::Low);
    assert_eq!(
        form.assignee,
        Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()))
    );
    assert_eq!(form.reviewer, Some(WorkspaceActor::User));
    assert_eq!(form.field, WorkspaceCreateTaskField::Title);
}

#[test]
fn create_form_requires_title_and_description() {
    let mut form = WorkspaceCreateTaskForm::new(WorkspaceTaskType::Goal);

    assert_eq!(form.to_request("main").unwrap_err(), "Title is required");

    form.title = "Ship board".to_string();
    assert_eq!(
        form.to_request("main").unwrap_err(),
        "Description is required"
    );
}

#[test]
fn create_form_requires_reviewer() {
    let mut form = WorkspaceCreateTaskForm::new(WorkspaceTaskType::Goal);
    form.title = "Ship board".to_string();
    form.description = "Build the first-class workspace task board".to_string();
    form.reviewer = None;

    assert_eq!(form.to_request("main").unwrap_err(), "Reviewer is required");
}

#[test]
fn create_form_marks_missing_required_fields_until_valid() {
    let mut form = WorkspaceCreateTaskForm::new(WorkspaceTaskType::Goal);
    form.reviewer = None;

    assert_eq!(
        form.required_field_state(WorkspaceCreateTaskField::Title),
        RequiredFieldState::Missing
    );
    assert_eq!(
        form.required_field_state(WorkspaceCreateTaskField::Description),
        RequiredFieldState::Missing
    );
    assert_eq!(
        form.required_field_state(WorkspaceCreateTaskField::Reviewer),
        RequiredFieldState::Missing
    );

    form.title = "Ship board".to_string();
    form.description = "Build the first-class workspace task board".to_string();
    form.reviewer = Some(WorkspaceActor::User);

    assert_eq!(
        form.required_field_state(WorkspaceCreateTaskField::Title),
        RequiredFieldState::Valid
    );
    assert_eq!(
        form.required_field_state(WorkspaceCreateTaskField::Description),
        RequiredFieldState::Valid
    );
    assert_eq!(
        form.required_field_state(WorkspaceCreateTaskField::Reviewer),
        RequiredFieldState::Valid
    );
}

#[test]
fn create_modal_styles_missing_required_fields_with_danger_until_valid() {
    let mut form = WorkspaceCreateTaskForm::new(WorkspaceTaskType::Goal);
    form.reviewer = None;
    let subagents = crate::state::SubAgentsState::new();
    let theme = crate::theme::ThemeTokens::default();

    let lines = workspace_create_modal_lines_with_subagents(&form, &subagents, &theme);
    assert_eq!(lines[0].spans[0].style, theme.accent_danger);
    assert_eq!(lines[2].spans[0].style, theme.accent_danger);
    assert_eq!(lines[6].spans[0].style, theme.accent_danger);

    form.title = "Ship board".to_string();
    form.description = "Build the first-class workspace task board".to_string();
    form.reviewer = Some(WorkspaceActor::User);

    let lines = workspace_create_modal_lines_with_subagents(&form, &subagents, &theme);
    assert_ne!(lines[0].spans[0].style, theme.accent_danger);
    assert_ne!(lines[2].spans[0].style, theme.accent_danger);
    assert_ne!(lines[6].spans[0].style, theme.accent_danger);
}

#[test]
fn create_form_builds_workspace_task_create_payload() {
    let mut form = WorkspaceCreateTaskForm::new(WorkspaceTaskType::Goal);
    form.title = "Ship board".to_string();
    form.description = "Build the first-class workspace task board".to_string();
    form.definition_of_done = "Tests pass".to_string();
    form.priority = WorkspacePriority::High;
    form.assignee = Some(WorkspaceActor::Subagent("qa".to_string()));
    form.reviewer = Some(WorkspaceActor::User);

    let request = form.to_request("main").expect("valid create request");

    assert_eq!(
        request,
        WorkspaceTaskCreate {
            workspace_id: "main".to_string(),
            title: "Ship board".to_string(),
            task_type: WorkspaceTaskType::Goal,
            description: "Build the first-class workspace task board".to_string(),
            definition_of_done: Some("Tests pass".to_string()),
            priority: Some(WorkspacePriority::High),
            assignee: Some(WorkspaceActor::Subagent("qa".to_string())),
            reviewer: Some(WorkspaceActor::User),
        }
    );
}

#[test]
fn create_form_edits_active_text_field_and_navigates() {
    let mut form = WorkspaceCreateTaskForm::new(WorkspaceTaskType::Thread);

    form.insert_char('A');
    form.insert_char('b');
    assert_eq!(form.title, "Ab");
    form.backspace();
    assert_eq!(form.title, "A");

    form.next_field();
    form.next_field();
    form.insert_char('D');
    assert_eq!(form.description, "D");

    form.previous_field();
    assert_eq!(form.field, WorkspaceCreateTaskField::TaskType);
}

#[test]
fn create_form_places_task_type_below_title_and_cycles_it() {
    let mut form = WorkspaceCreateTaskForm::new(WorkspaceTaskType::Thread);

    form.next_field();
    assert_eq!(form.field, WorkspaceCreateTaskField::TaskType);
    form.activate_current_field(&crate::state::SubAgentsState::new());
    assert_eq!(form.task_type, WorkspaceTaskType::Goal);
    form.activate_current_field(&crate::state::SubAgentsState::new());
    assert_eq!(form.task_type, WorkspaceTaskType::Thread);

    let body = workspace_create_modal_body(&form);
    let title_index = body.find("Title").expect("title row");
    let type_index = body.find("Task type").expect("task type row");
    let description_index = body.find("Description").expect("description row");

    assert!(title_index < type_index);
    assert!(type_index < description_index);
}

#[test]
fn create_form_cycles_priority_and_actors() {
    let mut form = WorkspaceCreateTaskForm::new(WorkspaceTaskType::Goal);
    let subagents = crate::state::SubAgentsState::new();

    form.field = WorkspaceCreateTaskField::Priority;
    form.activate_current_field(&subagents);
    assert_eq!(form.priority, WorkspacePriority::Normal);

    form.field = WorkspaceCreateTaskField::Assignee;
    form.activate_current_field(&subagents);
    assert_eq!(
        form.assignee,
        Some(WorkspaceActor::Subagent("weles".to_string()))
    );

    form.field = WorkspaceCreateTaskField::Reviewer;
    form.activate_current_field(&subagents);
    assert_eq!(
        form.reviewer,
        Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string()))
    );
}

#[test]
fn create_modal_displays_subagent_names_and_builtin_persona_labels() {
    let mut form = WorkspaceCreateTaskForm::new(WorkspaceTaskType::Thread);
    form.assignee = Some(WorkspaceActor::Subagent(
        "subagent-1777071832136".to_string(),
    ));
    form.reviewer = Some(WorkspaceActor::Subagent("mokosh".to_string()));
    let mut subagents = crate::state::SubAgentsState::new();
    subagents.entries = vec![crate::state::SubAgentEntry {
        id: "subagent-1777071832136".to_string(),
        name: "Tester".to_string(),
        provider: "openai".to_string(),
        model: "gpt-5.4".to_string(),
        role: None,
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: None,
        raw_json: None,
    }];

    let body = workspace_create_modal_body_with_subagents(&form, &subagents);

    assert!(body.contains("Assignee: Tester"));
    assert!(body.contains("Reviewer: Mokosh"));
    assert!(!body.contains("subagent-1777071832136"));
}

#[test]
fn model_submits_create_modal_as_workspace_create_command() {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, mut daemon_rx) = unbounded_channel();
    let mut model = crate::app::TuiModel::new(event_rx, daemon_tx);

    model.open_workspace_create_modal(WorkspaceTaskType::Goal);
    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::WorkspaceCreateTask)
    );

    let form = model
        .pending_workspace_create_form
        .as_mut()
        .expect("create form");
    form.title = "Ship board".to_string();
    form.description = "Build first-class create modal".to_string();
    form.definition_of_done = "Tests pass".to_string();
    form.priority = WorkspacePriority::Urgent;
    form.assignee = None;

    model.submit_workspace_create_modal();

    match daemon_rx.try_recv().expect("create command") {
        DaemonCommand::CreateWorkspaceTask(request) => {
            assert_eq!(request.workspace_id, "main");
            assert_eq!(request.title, "Ship board");
            assert_eq!(request.task_type, WorkspaceTaskType::Goal);
            assert_eq!(request.priority, Some(WorkspacePriority::Urgent));
            assert_eq!(request.assignee, None);
            assert_eq!(request.definition_of_done, Some("Tests pass".to_string()));
        }
        other => panic!("unexpected command: {other:?}"),
    }
    assert_eq!(model.modal.top(), None);
}

#[test]
fn workspace_run_command_blocks_unassigned_cached_task() {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, mut daemon_rx) = unbounded_channel();
    let mut model = crate::app::TuiModel::new(event_rx, daemon_tx);
    model.workspace.set_tasks(
        "main".to_string(),
        vec![WorkspaceTask {
            id: "wtask_1".to_string(),
            workspace_id: "main".to_string(),
            title: "Unassigned".to_string(),
            task_type: WorkspaceTaskType::Thread,
            description: "Needs an assignee".to_string(),
            definition_of_done: None,
            priority: WorkspacePriority::Low,
            status: WorkspaceTaskStatus::Todo,
            sort_order: 1,
            reporter: WorkspaceActor::User,
            assignee: None,
            reviewer: Some(WorkspaceActor::User),
            thread_id: Some("workspace-thread:wtask_1".to_string()),
            goal_run_id: None,
            runtime_history: Vec::new(),
            created_at: 1,
            updated_at: 1,
            started_at: None,
            completed_at: None,
            deleted_at: None,
            last_notice_id: None,
        }],
    );

    model.run_workspace_task_from_args("wtask_1");

    assert_eq!(model.status_line, "Assign workspace task before running");
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn workspace_operator_ui_switch_refreshes_tasks() {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, mut daemon_rx) = unbounded_channel();
    let mut model = crate::app::TuiModel::new(event_rx, daemon_tx);

    model.switch_workspace_operator_from_ui(WorkspaceOperator::Svarog);

    match daemon_rx.try_recv().expect("operator command") {
        DaemonCommand::SetWorkspaceOperator {
            workspace_id,
            operator,
        } => {
            assert_eq!(workspace_id, "main");
            assert_eq!(operator, WorkspaceOperator::Svarog);
        }
        other => panic!("unexpected command: {other:?}"),
    }
    match daemon_rx.try_recv().expect("task refresh command") {
        DaemonCommand::ListWorkspaceTasks {
            workspace_id,
            include_deleted,
        } => {
            assert_eq!(workspace_id, "main");
            assert!(!include_deleted);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn create_modal_assignee_uses_actor_picker_without_closing_create_modal() {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, mut daemon_rx) = unbounded_channel();
    let mut model = crate::app::TuiModel::new(event_rx, daemon_tx);

    model.open_workspace_create_modal(WorkspaceTaskType::Thread);
    model
        .pending_workspace_create_form
        .as_mut()
        .expect("create form")
        .field = WorkspaceCreateTaskField::Assignee;

    model.handle_workspace_create_modal_key(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    );

    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::WorkspaceActorPicker)
    );
    model.submit_workspace_actor_picker();

    assert_eq!(
        model
            .pending_workspace_create_form
            .as_ref()
            .expect("create form")
            .assignee,
        None
    );
    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::WorkspaceCreateTask)
    );
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn create_modal_builtin_persona_assignee_runs_setup_then_selects_actor() {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, mut daemon_rx) = unbounded_channel();
    let mut model = crate::app::TuiModel::new(event_rx, daemon_tx);
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: amux_shared::providers::PROVIDER_ID_ALIBABA_CODING_PLAN.to_string(),
        provider_name: "Alibaba Coding Plan".to_string(),
        authenticated: true,
        auth_source: "api_key".to_string(),
        model: "qwen3.6-plus".to_string(),
    }];

    model.open_workspace_create_modal(WorkspaceTaskType::Thread);
    model
        .pending_workspace_create_form
        .as_mut()
        .expect("create form")
        .field = WorkspaceCreateTaskField::Assignee;
    model.handle_workspace_create_modal_key(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    );
    let options = crate::app::workspace_actor_picker::workspace_actor_picker_options(
        crate::app::workspace_actor_picker::WorkspaceActorPickerMode::Assignee,
        &model.subagents,
    );
    let mokosh_index = options
        .iter()
        .position(|option| option.label == "Mokosh")
        .expect("mokosh builtin persona option");
    model
        .modal
        .reduce(modal::ModalAction::Navigate(mokosh_index as i32));

    model.submit_workspace_actor_picker();

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ProviderPicker));
    assert!(
        daemon_rx.try_recv().is_err(),
        "builtin setup should happen before actor selection is applied"
    );

    let provider_index = crate::widgets::provider_picker::available_provider_defs(&model.auth)
        .iter()
        .position(|provider| provider.id == amux_shared::providers::PROVIDER_ID_ALIBABA_CODING_PLAN)
        .expect("provider to exist");
    if provider_index > 0 {
        model
            .modal
            .reduce(modal::ModalAction::Navigate(provider_index as i32));
    }
    assert!(!model.handle_key_modal(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
        modal::ModalKind::ProviderPicker,
    ));
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    assert!(!model.handle_key_modal(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    ));

    match daemon_rx
        .try_recv()
        .expect("expected builtin persona config command")
    {
        DaemonCommand::SetTargetAgentProviderModel {
            target_agent_id,
            provider_id,
            model,
        } => {
            assert_eq!(target_agent_id, "mokosh");
            assert_eq!(
                provider_id,
                amux_shared::providers::PROVIDER_ID_ALIBABA_CODING_PLAN
            );
            assert!(!model.trim().is_empty());
        }
        other => panic!("unexpected command: {other:?}"),
    }
    assert_eq!(
        model
            .pending_workspace_create_form
            .as_ref()
            .expect("create form")
            .assignee,
        Some(WorkspaceActor::Subagent("mokosh".to_string()))
    );
    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::WorkspaceCreateTask)
    );
}
