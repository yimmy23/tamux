#[cfg(test)]
use super::*;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::sync::{LazyLock, Mutex};
use tokio::sync::mpsc::unbounded_channel;
use zorai_shared::providers::{PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_OPENAI};

fn make_model() -> TuiModel {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, _daemon_rx) = unbounded_channel();
    TuiModel::new(event_rx, daemon_tx)
}

fn make_model_with_daemon_rx() -> (
    TuiModel,
    tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, daemon_rx) = unbounded_channel();
    (TuiModel::new(event_rx, daemon_tx), daemon_rx)
}

fn next_thread_request(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Option<(String, Option<usize>, Option<usize>)> {
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        } = command
        {
            return Some((thread_id, message_limit, message_offset));
        }
    }
    None
}

fn saw_list_tasks_command(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> bool {
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(command, DaemonCommand::ListTasks) {
            return true;
        }
    }
    false
}

#[cfg(unix)]
fn with_fake_mpv_in_path<F: FnOnce()>(test: F) {
    static PATH_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    let _guard = PATH_LOCK.lock().expect("path lock should not be poisoned");
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let temp_dir =
        std::env::temp_dir().join(format!("zorai-test-mpv-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).expect("fake mpv dir should be created");

    let fake_mpv = temp_dir.join("mpv");
    std::fs::write(&fake_mpv, "#!/bin/sh\nsleep 5\n").expect("fake mpv should be written");
    let mut permissions = std::fs::metadata(&fake_mpv)
        .expect("fake mpv metadata should exist")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_mpv, permissions).expect("fake mpv should be executable");

    let old_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{old_path}", temp_dir.display()));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test));

    std::env::set_var("PATH", old_path);
    let _ = std::fs::remove_file(&fake_mpv);
    let _ = std::fs::remove_dir(&temp_dir);

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

fn next_goal_run_page_request(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Option<(
    String,
    Option<usize>,
    Option<usize>,
    Option<usize>,
    Option<usize>,
)> {
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::RequestGoalRunDetailPage {
            goal_run_id,
            step_offset,
            step_limit,
            event_offset,
            event_limit,
        } = command
        {
            return Some((
                goal_run_id,
                step_offset,
                step_limit,
                event_offset,
                event_limit,
            ));
        }
    }
    None
}

fn next_goal_run_detail_request(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Option<String> {
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::RequestGoalRunDetail(goal_run_id) = command {
            return Some(goal_run_id);
        }
    }
    None
}

fn next_goal_run_checkpoints_request(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Option<String> {
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::RequestGoalRunCheckpoints(goal_run_id) = command {
            return Some(goal_run_id);
        }
    }
    None
}

fn next_goal_hydration_schedule(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Option<String> {
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::ScheduleGoalHydrationRefresh(goal_run_id) = command {
            return Some(goal_run_id);
        }
    }
    None
}

fn active_goal_run_sidebar_model() -> TuiModel {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Goal Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.tasks.reduce(task::TaskAction::TaskListReceived(vec![
        task::AgentTask {
            id: "task-1".to_string(),
            title: "Child Task One".to_string(),
            thread_id: Some("thread-1".to_string()),
            goal_run_id: Some("goal-1".to_string()),
            created_at: 10,
            ..Default::default()
        },
        task::AgentTask {
            id: "task-2".to_string(),
            title: "Child Task Two".to_string(),
            thread_id: Some("thread-2".to_string()),
            goal_run_id: Some("goal-1".to_string()),
            created_at: 20,
            ..Default::default()
        },
    ]));
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Goal Title".to_string(),
            thread_id: Some("thread-1".to_string()),
            goal: "goal definition body".to_string(),
            current_step_title: Some("Implement".to_string()),
            child_task_ids: vec!["task-1".to_string(), "task-2".to_string()],
            steps: vec![
                task::GoalRunStep {
                    id: "step-1".to_string(),
                    title: "Plan".to_string(),
                    order: 0,
                    ..Default::default()
                },
                task::GoalRunStep {
                    id: "step-2".to_string(),
                    title: "Implement".to_string(),
                    order: 1,
                    ..Default::default()
                },
                task::GoalRunStep {
                    id: "step-3".to_string(),
                    title: "Verify".to_string(),
                    order: 2,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }));
    model
        .tasks
        .reduce(task::TaskAction::GoalRunCheckpointsReceived {
            goal_run_id: "goal-1".to_string(),
            checkpoints: vec![
                task::GoalRunCheckpointSummary {
                    id: "checkpoint-1".to_string(),
                    checkpoint_type: "plan".to_string(),
                    step_index: Some(0),
                    context_summary_preview: Some("Checkpoint for Plan".to_string()),
                    ..Default::default()
                },
                task::GoalRunCheckpointSummary {
                    id: "checkpoint-2".to_string(),
                    checkpoint_type: "verify".to_string(),
                    step_index: Some(2),
                    context_summary_preview: Some("Checkpoint for Verify".to_string()),
                    ..Default::default()
                },
            ],
        });
    model.tasks.reduce(task::TaskAction::WorkContextReceived(
        task::ThreadWorkContext {
            thread_id: "thread-1".to_string(),
            entries: vec![
                task::WorkContextEntry {
                    path: "/tmp/plan.md".to_string(),
                    goal_run_id: Some("goal-1".to_string()),
                    is_text: true,
                    ..Default::default()
                },
                task::WorkContextEntry {
                    path: "/tmp/report.md".to_string(),
                    goal_run_id: Some("goal-1".to_string()),
                    is_text: true,
                    ..Default::default()
                },
            ],
        },
    ));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: Some("step-1".to_string()),
    });
    model
}

#[test]
fn connected_event_defers_concierge_welcome_until_config_loads() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_connected_event();

    let mut saw_refresh = false;
    let mut saw_get_config = false;
    let mut saw_refresh_services = false;
    while let Ok(command) = daemon_rx.try_recv() {
        match command {
            DaemonCommand::Refresh => saw_refresh = true,
            DaemonCommand::GetConfig => saw_get_config = true,
            DaemonCommand::RefreshServices => saw_refresh_services = true,
            DaemonCommand::RequestConciergeWelcome => {
                panic!("concierge welcome should wait until config is loaded")
            }
            _ => {}
        }
    }

    assert!(saw_refresh, "connect should still request thread refresh");
    assert!(
        saw_get_config,
        "connect should request config on the startup-critical lane"
    );
    assert!(
        !saw_refresh_services,
        "connect should defer heavy service refresh until config has loaded"
    );
    assert!(
        !model.concierge.loading,
        "concierge loading should not start until welcome is actually requested"
    );
}

#[test]
fn first_raw_config_load_triggers_concierge_welcome_request() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = false;

    model.handle_agent_config_raw_event(serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
        "managed_execution": {
            "sandbox_enabled": false,
            "security_level": "yolo"
        }
    }));

    assert!(
        model.agent_config_loaded,
        "raw config should mark config as loaded"
    );
    assert_eq!(model.config.managed_security_level, "yolo");
    assert!(
        model.concierge.loading,
        "first config load should start concierge welcome"
    );
    let mut saw_welcome = false;
    let mut saw_refresh_services = false;
    let mut saw_provider_auth_states = false;
    while let Ok(command) = daemon_rx.try_recv() {
        match command {
            DaemonCommand::RequestConciergeWelcome => saw_welcome = true,
            DaemonCommand::RefreshServices => saw_refresh_services = true,
            DaemonCommand::GetProviderAuthStates => saw_provider_auth_states = true,
            _ => {}
        }
    }
    assert!(saw_welcome, "expected concierge welcome request");
    assert!(
        saw_refresh_services,
        "config load should trigger the deferred heavy startup refresh after concierge is queued"
    );
    assert!(
        saw_provider_auth_states,
        "config load should release deferred startup follow-up requests"
    );
}

#[test]
fn first_raw_config_load_replaces_goal_pane_with_concierge_loading() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Goal Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: Some("step-1".to_string()),
    });

    model.handle_agent_config_raw_event(serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
    }));

    assert!(
        matches!(model.main_pane_view, MainPaneView::Conversation),
        "welcome request should immediately leave the goal pane so the loading hero can render"
    );
    assert_eq!(
        model.chat.active_thread_id(),
        None,
        "welcome request should clear the active thread until concierge content arrives"
    );
    assert!(
        model.concierge.loading,
        "welcome request should start concierge loading before the daemon responds"
    );
    assert!(
        model.should_show_concierge_hero_loading(),
        "goal panes should not block the concierge loading hero after the request is sent"
    );

    let mut saw_welcome = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(command, DaemonCommand::RequestConciergeWelcome) {
            saw_welcome = true;
            break;
        }
    }
    assert!(saw_welcome, "expected concierge welcome request");
}

#[test]
fn reconnect_config_load_restores_last_thread_instead_of_requesting_concierge() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Recovered Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_reconnecting_event(3);
    model.handle_connected_event();
    while daemon_rx.try_recv().is_ok() {}

    model.handle_agent_config_raw_event(serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
    }));

    assert!(
        matches!(model.main_pane_view, MainPaneView::Conversation),
        "reconnect restore should return to the conversation pane"
    );
    assert_eq!(
        model.chat.active_thread_id(),
        Some("thread-1"),
        "reconnect restore should keep the last visible thread selected"
    );

    let mut saw_welcome = false;
    let mut saw_thread_request = false;
    while let Ok(command) = daemon_rx.try_recv() {
        match command {
            DaemonCommand::RequestConciergeWelcome => saw_welcome = true,
            DaemonCommand::RequestThread { thread_id, .. } if thread_id == "thread-1" => {
                saw_thread_request = true
            }
            _ => {}
        }
    }

    assert!(
        !saw_welcome,
        "reconnect restore should not discard the visible thread for concierge welcome"
    );
    assert!(
        saw_thread_request,
        "reconnect restore should request an authoritative reload for the last visible thread"
    );
}

#[test]
fn reconnect_restore_resumes_thread_only_if_it_was_streaming_before_disconnect() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Recovered Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-1".to_string(),
        content: "partial answer".to_string(),
    });

    model.handle_reconnecting_event(3);
    model.handle_connected_event();
    while daemon_rx.try_recv().is_ok() {}

    model.handle_agent_config_raw_event(serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
    }));
    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Recovered Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::Assistant,
            content: "Recovered".to_string(),
            timestamp: 1,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    let mut saw_continue = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::SendMessage {
            thread_id, content, ..
        } = command
        {
            if thread_id.as_deref() == Some("thread-1") && content == "continue" {
                saw_continue = true;
            }
        }
    }

    assert!(
        saw_continue,
        "reconnect restore should resume the interrupted thread with the existing continue path"
    );
}

#[test]
fn reconnect_restore_does_not_resume_idle_thread() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Recovered Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_reconnecting_event(3);
    model.handle_connected_event();
    while daemon_rx.try_recv().is_ok() {}

    model.handle_agent_config_raw_event(serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
    }));
    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Recovered Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::Assistant,
            content: "Recovered".to_string(),
            timestamp: 1,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(command, DaemonCommand::SendMessage { .. }) {
            panic!("idle reconnect restore should not auto-resume the thread");
        }
    }
}

#[test]
fn pump_daemon_events_budgeted_stops_after_limit() {
    let (daemon_tx, daemon_rx) = std::sync::mpsc::channel();
    let (cmd_tx, _cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);

    daemon_tx
        .send(ClientEvent::Error("first".to_string()))
        .expect("first event should send");
    daemon_tx
        .send(ClientEvent::Error("second".to_string()))
        .expect("second event should send");
    daemon_tx
        .send(ClientEvent::Error("third".to_string()))
        .expect("third event should send");

    let processed = model.pump_daemon_events_budgeted(2);

    assert_eq!(processed, 2);
    assert_eq!(model.last_error.as_deref(), Some("second"));

    let remaining = model.pump_daemon_events_budgeted(usize::MAX);

    assert_eq!(remaining, 1);
    assert_eq!(model.last_error.as_deref(), Some("third"));
}

#[test]
fn concierge_loading_state_is_visible_before_full_startup_burst_is_drained() {
    let (daemon_tx, daemon_rx) = std::sync::mpsc::channel();
    let (cmd_tx, _cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.agent_config_loaded = false;

    daemon_tx
        .send(ClientEvent::AgentConfigRaw(serde_json::json!({
            "provider": PROVIDER_ID_OPENAI,
            "base_url": "https://api.openai.com/v1",
            "model": "gpt-5.4",
        })))
        .expect("config event should send");
    daemon_tx
        .send(ClientEvent::Error("startup follow-up".to_string()))
        .expect("follow-up event should send");

    let processed = model.pump_daemon_events_budgeted(1);

    assert_eq!(processed, 1);
    assert!(
        model.concierge.loading,
        "the first startup frame should keep concierge loading visible"
    );
    assert!(
        matches!(model.main_pane_view, MainPaneView::Conversation),
        "the loading hero should stay on the conversation view until later startup events are processed"
    );

    let remaining = model.pump_daemon_events_budgeted(usize::MAX);
    assert_eq!(remaining, 1);
    assert!(
        !model.concierge.loading,
        "processing the remaining burst should clear the loading state"
    );
}

#[test]
fn operator_profile_completion_starts_concierge_loading_before_response() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Goal Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: Some("step-1".to_string()),
    });
    model.operator_profile.visible = true;
    model.operator_profile.loading = true;

    model.handle_operator_profile_session_completed_event(
        "session-1".to_string(),
        vec!["experience".to_string()],
    );

    assert!(
        matches!(model.main_pane_view, MainPaneView::Conversation),
        "operator profile completion should also surface the concierge loading view immediately"
    );
    assert_eq!(model.chat.active_thread_id(), None);
    assert!(model.concierge.loading);
    assert!(model.should_show_concierge_hero_loading());

    let mut saw_welcome = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(command, DaemonCommand::RequestConciergeWelcome) {
            saw_welcome = true;
            break;
        }
    }
    assert!(saw_welcome, "expected concierge welcome request");
}

#[test]
fn partial_concierge_welcome_keeps_loading_animation_until_final_actions_arrive() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::AgentConfigRaw(serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
    })));
    model.concierge.loading = true;

    model.handle_concierge_welcome_event("Draft welcome".to_string(), vec![]);

    assert!(
        model.concierge.loading,
        "partial concierge content should keep the loading animation active"
    );
    assert!(
        matches!(model.main_pane_view, MainPaneView::Conversation),
        "partial concierge content should stay in the conversation pane"
    );
    assert_eq!(model.chat.active_thread_id(), Some("concierge"));
    assert!(
        model.actions_bar_visible(),
        "loading banner should remain visible while the welcome is still streaming"
    );

    model.handle_concierge_welcome_event(
        "Final welcome".to_string(),
        vec![crate::state::ConciergeActionVm {
            label: "Start new session".to_string(),
            action_type: "start_new".to_string(),
            thread_id: None,
        }],
    );

    assert!(
        !model.concierge.loading,
        "final concierge welcome should clear the loading animation"
    );
}

#[test]
fn whatsapp_qr_event_opens_modal_and_sets_ascii_payload() {
    let mut model = make_model();
    assert!(model.modal.top().is_none());

    model.handle_client_event(ClientEvent::WhatsAppLinkQr {
        ascii_qr: "██\n██".to_string(),
        expires_at_ms: Some(123),
    });

    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::WhatsAppLink)
    );
    assert_eq!(model.modal.whatsapp_link().ascii_qr(), Some("██\n██"));
    assert_eq!(model.modal.whatsapp_link().expires_at_ms(), Some(123));
}

#[test]
fn whatsapp_status_events_update_modal_state() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::WhatsAppLinkStatus {
        state: "connected".to_string(),
        phone: Some("+12065550123".to_string()),
        last_error: None,
    });
    assert_eq!(
        model.modal.whatsapp_link().phase(),
        crate::state::modal::WhatsAppLinkPhase::Connected
    );

    model.handle_client_event(ClientEvent::WhatsAppLinkError {
        message: "scan timeout".to_string(),
        recoverable: true,
    });
    assert_eq!(
        model.modal.whatsapp_link().phase(),
        crate::state::modal::WhatsAppLinkPhase::Error
    );
    assert!(model
        .modal
        .whatsapp_link()
        .status_text()
        .contains("scan timeout"));

    model.handle_client_event(ClientEvent::WhatsAppLinkDisconnected {
        reason: Some("socket closed".to_string()),
    });
    assert_eq!(
        model.modal.whatsapp_link().phase(),
        crate::state::modal::WhatsAppLinkPhase::Disconnected
    );
    assert!(model
        .modal
        .whatsapp_link()
        .status_text()
        .contains("socket closed"));
}

#[test]
fn tts_request_surfaces_pending_footer_activity_until_audio_starts() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "Say this aloud".to_string(),
            timestamp: 1,
            ..Default::default()
        },
    });

    model.speak_latest_assistant_message();

    let command = daemon_rx.try_recv().expect("expected TTS command");
    assert!(matches!(command, DaemonCommand::TextToSpeech { .. }));
    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("preparing speech")
    );

    model.handle_client_event(ClientEvent::TextToSpeechResult {
        content: r#"{"path":"/tmp/speech.mp3"}"#.to_string(),
    });

    assert!(
        model.footer_activity_text().is_none(),
        "pending TTS activity should clear once audio is ready to play"
    );
}

#[test]
fn image_generation_result_refreshes_thread_and_work_context() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Thread".to_string(),
            ..Default::default()
        },
    ]));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::GenerateImageResult {
        content: r#"{"ok":true,"thread_id":"thread-1","path":"/tmp/generated-image.png"}"#
            .to_string(),
    });

    assert_eq!(
        next_thread_request(&mut daemon_rx),
        Some((
            "thread-1".to_string(),
            Some(model.config.tui_chat_history_page_size as usize),
            Some(0),
        ))
    );
    assert!(matches!(
        daemon_rx.try_recv(),
        Ok(DaemonCommand::RequestThreadWorkContext(thread_id)) if thread_id == "thread-1"
    ));
    assert_eq!(
        model.status_line,
        "Image generated: /tmp/generated-image.png"
    );
}

#[test]
fn late_tool_result_after_done_does_not_restore_footer_activity() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::ToolCall {
        thread_id: "thread-1".to_string(),
        call_id: "call-1".to_string(),
        name: "generate_image".to_string(),
        arguments: "{\"prompt\":\"test\"}".to_string(),
        weles_review: None,
    });
    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("⚙  generate_image")
    );

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-1".to_string(),
        input_tokens: 0,
        output_tokens: 0,
        cost: None,
        provider: None,
        model: None,
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: None,
    });
    assert!(
        model.footer_activity_text().is_none(),
        "done should clear the footer activity for the completed turn"
    );

    model.handle_client_event(ClientEvent::ToolResult {
        thread_id: "thread-1".to_string(),
        call_id: "call-1".to_string(),
        name: "generate_image".to_string(),
        content: "{\"path\":\"/tmp/image.png\"}".to_string(),
        is_error: false,
        weles_review: None,
    });

    assert!(
        model.footer_activity_text().is_none(),
        "late tool results after done must not restore the footer activity badge"
    );
}

#[cfg(unix)]
#[test]
fn text_to_speech_tool_result_autoplays_audio_in_chat_threads() {
    with_fake_mpv_in_path(|| {
        let mut model = make_model();
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "thread-1".to_string(),
            title: "Thread".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

        let audio_path =
            std::env::temp_dir().join(format!("zorai-test-speech-{}.mp3", std::process::id()));
        std::fs::write(&audio_path, b"fake mp3 bytes").expect("fake audio file should exist");

        model.handle_client_event(ClientEvent::ToolResult {
            thread_id: "thread-1".to_string(),
            call_id: "call-1".to_string(),
            name: "text_to_speech".to_string(),
            content: serde_json::json!({
                "path": audio_path.display().to_string(),
            })
            .to_string(),
            is_error: false,
            weles_review: None,
        });

        assert_eq!(model.status_line, "Playing synthesized speech...");

        if let Some(mut child) = model.voice_player.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_file(audio_path);
    });
}

#[test]
fn collaboration_sessions_event_populates_workspace_state_without_error_modal() {
    let mut model = make_model();

    model.handle_collaboration_sessions_event(
        serde_json::json!([
            {
                "id": "session-1",
                "parent_task_id": "task-1",
                "agents": [{"role": "research"}, {"role": "testing"}],
                "disagreements": [
                    {
                        "id": "disagreement-1",
                        "topic": "deployment strategy",
                        "positions": ["roll forward", "roll back"],
                        "votes": [{"task_id": "subagent-1"}]
                    }
                ]
            }
        ])
        .to_string(),
    );

    assert!(matches!(model.main_pane_view, MainPaneView::Collaboration));
    assert_eq!(model.modal.top(), None);
    assert!(!model.error_active);
    assert_eq!(model.status_line, "Collaboration sessions loaded");
    assert_eq!(model.collaboration.rows().len(), 2);
    assert_eq!(
        model
            .collaboration
            .selected_row()
            .and_then(crate::state::collaboration::CollaborationRowVm::disagreement_id),
        Some("disagreement-1")
    );
}

#[test]
fn collaboration_vote_result_requests_refresh_and_updates_status() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_client_event(ClientEvent::CollaborationVoteResult {
        report_json: serde_json::json!({
            "session_id": "session-1",
            "resolution": "resolved"
        })
        .to_string(),
    });

    assert_eq!(model.status_line, "Vote recorded: resolved.");
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected collaboration refresh after vote result"),
        DaemonCommand::GetCollaborationSessions
    ));
}

#[test]
fn collaboration_sessions_event_surfaces_escalation_notice() {
    let mut model = make_model();

    model.handle_collaboration_sessions_event(
        serde_json::json!([
            {
                "id": "session-1",
                "parent_task_id": "task-1",
                "disagreements": [
                    {
                        "id": "disagreement-1",
                        "topic": "deployment strategy",
                        "positions": ["roll forward", "roll back"],
                        "resolution": "pending",
                        "confidence_gap": 0.1,
                        "votes": []
                    }
                ]
            }
        ])
        .to_string(),
    );

    let notice = model
        .input_notice_style()
        .expect("escalation should surface a notice");
    assert!(notice.0.contains("Collaboration escalation"));
    assert!(
        model
            .collaboration
            .selected_session()
            .and_then(|session| session.escalation.as_ref())
            .is_some(),
        "session should carry escalation summary for workspace rendering"
    );
}

#[test]
fn operator_question_event_appends_inline_message_and_actions_without_modal() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::OperatorQuestion {
        question_id: "oq-1".to_string(),
        content: "Approve this slice?\nA - proceed\nB - revise".to_string(),
        options: vec!["A".to_string(), "B".to_string()],
        session_id: None,
        thread_id: Some("thread-1".to_string()),
    });

    let thread = model.chat.active_thread().expect("thread should exist");
    assert_eq!(
        thread.messages.len(),
        1,
        "operator question should append an inline transcript message"
    );
    let message = thread
        .messages
        .last()
        .expect("question message should exist");
    assert!(message.is_operator_question);
    assert_eq!(message.operator_question_id.as_deref(), Some("oq-1"));
    assert_eq!(
        message.content,
        "Approve this slice?\nA - proceed\nB - revise"
    );
    assert_eq!(message.actions.len(), 2);
    assert_eq!(message.actions[0].label, "A");
    assert_eq!(message.actions[1].label, "B");
    assert_eq!(model.modal.top(), None);
    assert_ne!(
        model.modal.top(),
        Some(modal::ModalKind::OperatorQuestionOverlay)
    );
}

#[test]
fn thread_deleted_event_removes_thread_from_chat_state() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "thread-1".into(),
            title: "Thread One".into(),
            ..Default::default()
        },
        chat::AgentThread {
            id: "thread-2".into(),
            title: "Thread Two".into(),
            ..Default::default()
        },
    ]));
    model.open_thread_conversation("thread-1".into());

    model.handle_client_event(ClientEvent::ThreadDeleted {
        thread_id: "thread-1".into(),
        deleted: true,
    });

    assert!(model
        .chat
        .threads()
        .iter()
        .all(|thread| thread.id != "thread-1"));
}

#[test]
fn stale_thread_detail_after_delete_does_not_recreate_thread() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "thread-1".into(),
            title: "Thread One".into(),
            ..Default::default()
        },
        chat::AgentThread {
            id: "thread-2".into(),
            title: "Thread Two".into(),
            ..Default::default()
        },
    ]));
    model.open_thread_conversation("thread-1".into());

    model.handle_client_event(ClientEvent::ThreadDeleted {
        thread_id: "thread-1".into(),
        deleted: true,
    });
    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".into(),
        title: "Thread One".into(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::Assistant,
            content: "stale detail".into(),
            timestamp: 1,
            ..Default::default()
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    assert!(model
        .chat
        .threads()
        .iter()
        .all(|thread| thread.id != "thread-1"));
    assert_eq!(model.chat.active_thread_id(), Some("thread-2"));
}

#[test]
fn stale_thread_list_after_delete_does_not_recreate_thread() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::ThreadList(vec![
        crate::wire::AgentThread {
            id: "thread-1".into(),
            title: "Thread One".into(),
            ..Default::default()
        },
        crate::wire::AgentThread {
            id: "thread-2".into(),
            title: "Thread Two".into(),
            ..Default::default()
        },
    ]));
    model.open_thread_conversation("thread-1".into());

    model.handle_client_event(ClientEvent::ThreadDeleted {
        thread_id: "thread-1".into(),
        deleted: true,
    });
    model.handle_client_event(ClientEvent::ThreadList(vec![
        crate::wire::AgentThread {
            id: "thread-1".into(),
            title: "Thread One".into(),
            ..Default::default()
        },
        crate::wire::AgentThread {
            id: "thread-2".into(),
            title: "Thread Two".into(),
            ..Default::default()
        },
    ]));

    assert!(model
        .chat
        .threads()
        .iter()
        .all(|thread| thread.id != "thread-1"));
    assert_eq!(model.chat.active_thread_id(), Some("thread-2"));
}

#[test]
fn thread_deleted_event_reclamps_open_thread_picker_cursor() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "thread-1".into(),
            agent_name: Some("Svarog".into()),
            title: "Thread One".into(),
            ..Default::default()
        },
        chat::AgentThread {
            id: "thread-2".into(),
            agent_name: Some("Svarog".into()),
            title: "Thread Two".into(),
            ..Default::default()
        },
    ]));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model.sync_thread_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(2));

    assert_eq!(model.modal.picker_cursor(), 2);
    assert_eq!(
        model
            .selected_thread_picker_thread()
            .map(|thread| thread.id.as_str()),
        Some("thread-2")
    );

    model.handle_client_event(ClientEvent::ThreadDeleted {
        thread_id: "thread-2".into(),
        deleted: true,
    });

    assert_eq!(model.modal.picker_cursor(), 1);
    assert_eq!(
        model
            .selected_thread_picker_thread()
            .map(|thread| thread.id.as_str()),
        Some("thread-1")
    );
}

#[test]
fn internal_dm_thread_created_refreshes_open_thread_picker() {
    let mut model = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model
        .modal
        .set_thread_picker_tab(modal::ThreadPickerTab::Internal);
    model.sync_thread_picker_item_count();

    assert!(
        model.selected_thread_picker_thread().is_none(),
        "internal picker should start empty in this fixture"
    );

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "dm:svarog:weles".into(),
        title: "Internal DM · Swarog ↔ WELES".into(),
        agent_name: None,
    });

    model.modal.reduce(modal::ModalAction::Navigate(1));

    assert_eq!(
        model
            .selected_thread_picker_thread()
            .map(|thread| thread.id.as_str()),
        Some("dm:svarog:weles")
    );
}

#[test]
fn goal_run_deleted_event_removes_goal_from_task_state() {
    let mut model = make_model();
    model
        .tasks
        .reduce(task::TaskAction::GoalRunListReceived(vec![
            task::GoalRun {
                id: "goal-1".into(),
                title: "Goal One".into(),
                status: Some(task::GoalRunStatus::Cancelled),
                ..Default::default()
            },
            task::GoalRun {
                id: "goal-2".into(),
                title: "Goal Two".into(),
                status: Some(task::GoalRunStatus::Completed),
                ..Default::default()
            },
        ]));

    model.handle_client_event(ClientEvent::GoalRunDeleted {
        goal_run_id: "goal-1".into(),
        deleted: true,
    });

    assert!(model.tasks.goal_run_by_id("goal-1").is_none());
    assert!(model.tasks.goal_run_by_id("goal-2").is_some());
}

#[test]
fn goal_run_deleted_event_reclamps_open_goal_picker_cursor() {
    let mut model = make_model();
    model
        .tasks
        .reduce(task::TaskAction::GoalRunListReceived(vec![
            task::GoalRun {
                id: "goal-1".into(),
                title: "Goal One".into(),
                status: Some(task::GoalRunStatus::Completed),
                ..Default::default()
            },
            task::GoalRun {
                id: "goal-2".into(),
                title: "Goal Two".into(),
                status: Some(task::GoalRunStatus::Cancelled),
                ..Default::default()
            },
        ]));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
    model.sync_goal_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(2));

    assert_eq!(model.modal.picker_cursor(), 2);
    assert_eq!(
        model.selected_goal_picker_run().map(|run| run.id.as_str()),
        Some("goal-2")
    );

    model.handle_client_event(ClientEvent::GoalRunDeleted {
        goal_run_id: "goal-2".into(),
        deleted: true,
    });

    assert_eq!(model.modal.picker_cursor(), 1);
    assert_eq!(
        model.selected_goal_picker_run().map(|run| run.id.as_str()),
        Some("goal-1")
    );
}

#[test]
fn goal_run_list_event_reclamps_open_goal_picker_cursor() {
    let mut model = make_model();
    model
        .tasks
        .reduce(task::TaskAction::GoalRunListReceived(vec![
            task::GoalRun {
                id: "goal-1".into(),
                title: "Goal One".into(),
                status: Some(task::GoalRunStatus::Completed),
                ..Default::default()
            },
            task::GoalRun {
                id: "goal-2".into(),
                title: "Goal Two".into(),
                status: Some(task::GoalRunStatus::Cancelled),
                ..Default::default()
            },
        ]));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
    model.sync_goal_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(2));

    assert_eq!(model.modal.picker_cursor(), 2);
    assert_eq!(
        model.selected_goal_picker_run().map(|run| run.id.as_str()),
        Some("goal-2")
    );

    model.handle_client_event(ClientEvent::GoalRunList(vec![crate::wire::GoalRun {
        id: "goal-1".into(),
        title: "Goal One".into(),
        status: Some(crate::wire::GoalRunStatus::Completed),
        goal: "Goal One".into(),
        ..Default::default()
    }]));

    assert_eq!(model.modal.picker_cursor(), 1);
    assert_eq!(
        model.selected_goal_picker_run().map(|run| run.id.as_str()),
        Some("goal-1")
    );
}

#[test]
fn operator_question_resolved_event_marks_message_answered_and_clears_actions() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "Approve this slice?\nA - proceed\nB - revise".to_string(),
            is_operator_question: true,
            operator_question_id: Some("oq-1".to_string()),
            actions: vec![chat::MessageAction {
                label: "A".to_string(),
                action_type: "operator_question_answer:oq-1:A".to_string(),
                thread_id: Some("thread-1".to_string()),
            }],
            ..Default::default()
        },
    });

    model.handle_client_event(ClientEvent::OperatorQuestionResolved {
        question_id: "oq-1".to_string(),
        answer: "A".to_string(),
    });

    let thread = model.chat.active_thread().expect("thread should exist");
    assert_eq!(thread.messages.len(), 1);
    let message = thread
        .messages
        .last()
        .expect("question message should exist");
    assert_eq!(message.operator_question_answer.as_deref(), Some("A"));
    assert!(message.actions.is_empty());
}

#[test]
fn operator_question_event_does_not_replace_existing_modal() {
    let mut model = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::CommandPalette));
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::OperatorQuestion {
        question_id: "oq-1".to_string(),
        content: "Approve this slice?\nA - proceed\nB - revise".to_string(),
        options: vec!["A".to_string(), "B".to_string()],
        session_id: None,
        thread_id: Some("thread-1".to_string()),
    });

    let thread = model.chat.active_thread().expect("thread should exist");
    assert_eq!(
        thread.messages.len(),
        1,
        "operator question should append an inline transcript message"
    );
    let message = thread
        .messages
        .last()
        .expect("question message should exist");
    assert!(message.is_operator_question);
    assert_eq!(message.operator_question_id.as_deref(), Some("oq-1"));
    assert_eq!(
        message.content,
        "Approve this slice?\nA - proceed\nB - revise"
    );
    assert_eq!(message.actions.len(), 2);
    assert_eq!(message.actions[0].label, "A");
    assert_eq!(message.actions[1].label, "B");
    assert_eq!(model.modal.top(), Some(modal::ModalKind::CommandPalette));
    model.modal.reduce(modal::ModalAction::Pop);
    assert_eq!(model.modal.top(), None);
    assert_ne!(
        model.modal.top(),
        Some(modal::ModalKind::OperatorQuestionOverlay)
    );
}

#[test]
fn operator_question_event_preserves_locked_chat_viewport() {
    let mut model = make_model();
    model.width = 100;
    model.height = 40;
    model.show_sidebar_override = Some(false);
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    for idx in 0..40 {
        model.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id: "thread-1".to_string(),
            message: chat::AgentMessage {
                role: chat::MessageRole::Assistant,
                content: format!("message {idx}"),
                ..Default::default()
            },
        });
    }
    model.chat.reduce(chat::ChatAction::ScrollChat(8));

    let before = widgets::chat::scrollbar_layout(
        model.pane_layout().chat,
        &model.chat,
        &model.theme,
        model.tick_counter,
        model.retry_wait_start_selected,
    )
    .expect("chat should produce a scrollbar layout");

    model.handle_operator_question_event(
        "oq-1".to_string(),
        "Approve this slice?\nA - proceed\nB - revise".to_string(),
        vec!["A".to_string(), "B".to_string()],
        Some("thread-1".to_string()),
    );

    let after = widgets::chat::scrollbar_layout(
        model.pane_layout().chat,
        &model.chat,
        &model.theme,
        model.tick_counter,
        model.retry_wait_start_selected,
    )
    .expect("chat should still produce a scrollbar layout");

    assert!(
        before.scroll > 0,
        "test setup should scroll away from the live bottom"
    );
    assert_eq!(
        after.scroll as isize - before.scroll as isize,
        after.max_scroll as isize - before.max_scroll as isize,
        "locked viewport should stay anchored when a new message extends the transcript"
    );
}

#[test]
fn operator_question_event_keeps_bottom_follow_when_chat_is_at_latest_message() {
    let mut model = make_model();
    model.width = 100;
    model.height = 40;
    model.show_sidebar_override = Some(false);
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    for idx in 0..40 {
        model.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id: "thread-1".to_string(),
            message: chat::AgentMessage {
                role: chat::MessageRole::Assistant,
                content: format!("message {idx}"),
                ..Default::default()
            },
        });
    }

    model.handle_operator_question_event(
        "oq-1".to_string(),
        "Approve this slice?\nA - proceed\nB - revise".to_string(),
        vec!["A".to_string(), "B".to_string()],
        Some("thread-1".to_string()),
    );

    let after = widgets::chat::scrollbar_layout(
        model.pane_layout().chat,
        &model.chat,
        &model.theme,
        model.tick_counter,
        model.retry_wait_start_selected,
    )
    .expect("chat should produce a scrollbar layout");

    assert_eq!(
        after.scroll, 0,
        "when already at the live bottom, new messages should continue following the stack"
    );
}

#[test]
fn operator_profile_workflow_warning_surfaces_retry_notice() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::WorkflowNotice {
        thread_id: None,
        kind: "operator-profile-warning".to_string(),
        message: "Operator profile operation failed".to_string(),
        details: Some("{\"retry_action\":\"request_concierge_welcome\"}".to_string()),
    });
    let rendered = model
        .input_notice_style()
        .expect("warning should be visible");
    assert!(
        rendered.0.contains("Ctrl+R"),
        "warning notice should include retry hint"
    );
}

#[test]
fn auto_compaction_workflow_notice_requests_active_compaction_window() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-compaction".to_string(),
        title: "Compaction".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-compaction".to_string(),
    ));

    model.handle_client_event(ClientEvent::WorkflowNotice {
        thread_id: Some("thread-compaction".to_string()),
        kind: "auto-compaction".to_string(),
        message: "Auto compaction applied using heuristic.".to_string(),
        details: Some("{\"split_at\":20,\"total_message_count\":121}".to_string()),
    });

    let (thread_id, message_limit, message_offset) =
        next_thread_request(&mut daemon_rx).expect("expected targeted thread reload request");
    assert_eq!(thread_id, "thread-compaction");
    assert_eq!(message_limit, Some(101));
    assert_eq!(message_offset, Some(0));
}

#[test]
fn auto_compaction_workflow_notice_invalidates_header_context_usage() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-compaction".to_string(),
        title: "Compaction".to_string(),
        total_message_count: 121,
        loaded_message_start: 0,
        loaded_message_end: 121,
        active_context_window_start: Some(0),
        active_context_window_end: Some(121),
        active_context_window_tokens: Some(239_700_000),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::User,
            content: "before compaction".to_string(),
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-compaction".to_string(),
    ));
    while daemon_rx.try_recv().is_ok() {}

    assert_eq!(
        model.current_header_usage_summary().current_tokens,
        239_700_000
    );

    model.handle_client_event(ClientEvent::WorkflowNotice {
        thread_id: Some("thread-compaction".to_string()),
        kind: "auto-compaction".to_string(),
        message: "Auto compaction applied using heuristic.".to_string(),
        details: Some("{\"split_at\":20,\"total_message_count\":121}".to_string()),
    });

    assert_eq!(
        model.current_header_usage_summary().current_tokens,
        0,
        "compaction notice should clear stale daemon context tokens while the post-compaction detail reload is pending"
    );
    assert!(matches!(
        next_thread_request(&mut daemon_rx),
        Some((thread_id, Some(101), Some(0))) if thread_id == "thread-compaction"
    ));
}

#[test]
fn status_diagnostics_warning_mentions_sync_state() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::StatusDiagnostics {
        operator_profile_sync_state: "dirty".to_string(),
        operator_profile_sync_dirty: true,
        operator_profile_scheduler_fallback: false,
        diagnostics_json: r#"{"operator_profile_sync_state":"dirty","aline":{"available":true}}"#
            .to_string(),
    });
    assert!(
        model.status_line.contains("sync state: dirty"),
        "status line should expose dirty sync diagnostics"
    );
    assert!(
        model
            .status_modal_diagnostics_json
            .as_deref()
            .is_some_and(|diagnostics| diagnostics.contains("\"aline\"")),
        "status diagnostics should retain the raw payload for the status modal"
    );
}

#[test]
fn status_diagnostics_warning_mentions_mesh_state() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::StatusDiagnostics {
        operator_profile_sync_state: "clean".to_string(),
        operator_profile_sync_dirty: false,
        operator_profile_scheduler_fallback: false,
        diagnostics_json: r#"{"skill_mesh":{"backend":"mesh","state":"degraded"}}"#.to_string(),
    });
    assert!(
        model.status_line.contains("skill mesh: degraded"),
        "status line should expose degraded mesh diagnostics"
    );
}

#[test]
fn full_status_event_caches_snapshot_for_status_modal() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::StatusSnapshot(
        crate::client::AgentStatusSnapshotVm {
            tier: "mission_control".to_string(),
            activity: "waiting_for_operator".to_string(),
            active_thread_id: Some("thread-1".to_string()),
            active_goal_run_id: Some("goal-1".to_string()),
            active_goal_run_title: Some("Close release gap".to_string()),
            provider_health_json: r#"{"openai":{"can_execute":true,"trip_count":0}}"#.to_string(),
            gateway_statuses_json: r#"{"slack":{"status":"connected"}}"#.to_string(),
            recent_actions_json:
                r#"[{"action_type":"tool_call","summary":"Ran status","timestamp":1712345678}]"#
                    .to_string(),
        },
    ));
    model.handle_client_event(ClientEvent::StatusDiagnostics {
        operator_profile_sync_state: "clean".to_string(),
        operator_profile_sync_dirty: false,
        operator_profile_scheduler_fallback: false,
        diagnostics_json: r#"{"aline":{"available":true,"watcher_state":"running"}}"#.to_string(),
    });

    let snapshot = model
        .status_modal_snapshot
        .as_ref()
        .expect("status snapshot should be cached");
    assert_eq!(snapshot.tier, "mission_control");
    assert_eq!(snapshot.activity, "waiting_for_operator");
    assert_eq!(snapshot.active_thread_id.as_deref(), Some("thread-1"));
    assert_eq!(model.recent_actions.len(), 1);
    assert_eq!(model.recent_actions[0].summary, "Ran status");
    assert!(model.status_modal_body().contains("Watcher:"));
}

#[test]
fn status_query_failure_resolves_loading_modal() {
    let mut model = make_model();
    model.status_modal_loading = true;

    model.handle_client_event(ClientEvent::Error("boom".to_string()));

    assert!(!model.status_modal_loading);
}

#[test]
fn status_modal_failure_replaces_loading_body_with_error_text() {
    let mut model = make_model();
    model.open_status_modal_loading();

    model.handle_client_event(ClientEvent::Error("daemon unavailable".to_string()));

    assert!(model.status_modal_body().contains("daemon unavailable"));
}

#[test]
fn pin_budget_exceeded_event_opens_app_flow() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::ThreadMessagePinResult(
        crate::client::ThreadMessagePinResultVm {
            ok: false,
            thread_id: "thread-1".to_string(),
            message_id: "message-1".to_string(),
            error: Some("pinned_budget_exceeded".to_string()),
            current_pinned_chars: 100,
            pinned_budget_chars: 120,
            candidate_pinned_chars: Some(160),
        },
    ));

    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::PinnedBudgetExceeded)
    );
    let payload = model
        .pending_pinned_budget_exceeded
        .as_ref()
        .expect("budget payload should be stored");
    assert_eq!(payload.current_pinned_chars, 100);
    assert_eq!(payload.pinned_budget_chars, 120);
    assert_eq!(payload.candidate_pinned_chars, 160);
}

#[test]
fn generic_pin_error_stays_status_only() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::ThreadMessagePinResult(
        crate::client::ThreadMessagePinResultVm {
            ok: false,
            thread_id: "thread-1".to_string(),
            message_id: "message-1".to_string(),
            error: Some("message_not_found".to_string()),
            current_pinned_chars: 0,
            pinned_budget_chars: 120,
            candidate_pinned_chars: None,
        },
    ));

    assert!(model.pending_pinned_budget_exceeded.is_none());
    assert_ne!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::PinnedBudgetExceeded)
    );
    assert_eq!(model.status_line, "Pin failed: message_not_found");
}

#[test]
fn sidebar_falls_back_to_todo_after_last_pin_removed() {
    let mut model = make_model();
    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Pinned".to_string(),
        messages: vec![crate::wire::AgentMessage {
            id: Some("message-1".to_string()),
            role: crate::wire::MessageRole::Assistant,
            content: "Pinned content".to_string(),
            pinned_for_compaction: true,
            ..Default::default()
        }],
        loaded_message_end: 1,
        total_message_count: 1,
        ..Default::default()
    });
    model.sidebar.reduce(sidebar::SidebarAction::SwitchTab(
        sidebar::SidebarTab::Pinned,
    ));

    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Pinned".to_string(),
        messages: vec![crate::wire::AgentMessage {
            id: Some("message-1".to_string()),
            role: crate::wire::MessageRole::Assistant,
            content: "Pinned content".to_string(),
            pinned_for_compaction: false,
            ..Default::default()
        }],
        loaded_message_end: 1,
        total_message_count: 1,
        ..Default::default()
    });

    assert_eq!(model.sidebar.active_tab(), sidebar::SidebarTab::Todos);
}

#[test]
fn status_modal_latest_response_replaces_stale_content() {
    let mut model = make_model();
    model.open_status_modal_loading();
    model.handle_client_event(ClientEvent::StatusDiagnostics {
        operator_profile_sync_state: "older".to_string(),
        operator_profile_sync_dirty: true,
        operator_profile_scheduler_fallback: false,
        diagnostics_json: r#"{"aline":{"available":false,"watcher_state":"unknown"}}"#.to_string(),
    });
    model.handle_client_event(ClientEvent::StatusSnapshot(
        crate::client::AgentStatusSnapshotVm {
            tier: "mission_control".to_string(),
            activity: "older".to_string(),
            active_thread_id: None,
            active_goal_run_id: None,
            active_goal_run_title: None,
            provider_health_json: "{}".to_string(),
            gateway_statuses_json: "{}".to_string(),
            recent_actions_json: "[]".to_string(),
        },
    ));
    model.handle_client_event(ClientEvent::StatusDiagnostics {
        operator_profile_sync_state: "clean".to_string(),
        operator_profile_sync_dirty: false,
        operator_profile_scheduler_fallback: false,
        diagnostics_json: r#"{"aline":{"available":true,"watcher_state":"running"}}"#.to_string(),
    });
    model.handle_client_event(ClientEvent::StatusSnapshot(
        crate::client::AgentStatusSnapshotVm {
            tier: "mission_control".to_string(),
            activity: "newer".to_string(),
            active_thread_id: None,
            active_goal_run_id: None,
            active_goal_run_title: None,
            provider_health_json: "{}".to_string(),
            gateway_statuses_json: "{}".to_string(),
            recent_actions_json: "[]".to_string(),
        },
    ));

    assert!(model.status_modal_body().contains("newer"));
    assert!(model.status_modal_body().contains("running"));
}

#[test]
fn repeated_gateway_status_does_not_keep_overwriting_status_line() {
    let mut model = make_model();
    model.status_line = "Prompt sent".to_string();

    model.handle_client_event(ClientEvent::GatewayStatus {
        platform: "discord".to_string(),
        status: "disconnected".to_string(),
        last_error: Some("socket closed".to_string()),
        consecutive_failures: 1,
    });
    assert_eq!(model.status_line, "🌐 Gateway discord: disconnected");

    model.status_line = "Prompt sent".to_string();
    model.handle_client_event(ClientEvent::GatewayStatus {
        platform: "discord".to_string(),
        status: "disconnected".to_string(),
        last_error: Some("socket closed".to_string()),
        consecutive_failures: 2,
    });

    assert_eq!(
        model.status_line, "Prompt sent",
        "repeated gateway status should not keep stealing the footer"
    );
}

#[test]
fn upgrade_notification_updates_status_line_and_inbox() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::NotificationUpsert(
        zorai_protocol::ZoraiUpdateStatus::from_versions("0.2.3", "0.2.4")
            .expect("status should parse valid versions")
            .into_notification(100),
    ));

    assert_eq!(model.notifications.unread_count(), 1);
    assert_eq!(model.status_line, "🔔 zorai 0.2.4 is available");
}

#[test]
fn operator_profile_question_event_shows_onboarding_notice() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::OperatorProfileSessionStarted {
        session_id: "sess-1".to_string(),
        kind: "first_run_onboarding".to_string(),
    });
    model.handle_client_event(ClientEvent::OperatorProfileQuestion {
        session_id: "sess-1".to_string(),
        question_id: "name".to_string(),
        field_key: "name".to_string(),
        prompt: "What should I call you?".to_string(),
        input_kind: "text".to_string(),
        optional: false,
    });

    assert!(model.should_show_operator_profile_onboarding());
    assert_eq!(
        model
            .operator_profile
            .question
            .as_ref()
            .map(|q| q.field_key.as_str()),
        Some("name")
    );
}

#[test]
fn operator_profile_progress_requests_next_question() {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, mut daemon_rx) = unbounded_channel();
    let mut model = TuiModel::new(event_rx, daemon_tx);
    model.handle_client_event(ClientEvent::OperatorProfileSessionStarted {
        session_id: "sess-1".to_string(),
        kind: "first_run_onboarding".to_string(),
    });

    model.handle_client_event(ClientEvent::OperatorProfileProgress {
        session_id: "sess-1".to_string(),
        answered: 1,
        remaining: 2,
        completion_ratio: 0.33,
    });

    let mut found_next = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(
            command,
            crate::state::DaemonCommand::NextOperatorProfileQuestion { .. }
        ) {
            found_next = true;
            break;
        }
    }
    assert!(found_next, "progress should trigger next-question command");
}

#[test]
fn weles_health_update_surfaces_degraded_status() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::WelesHealthUpdate {
        state: "degraded".to_string(),
        reason: Some("WELES review unavailable for guarded actions".to_string()),
        checked_at: 77,
    });

    assert_eq!(
        model
            .weles_health
            .as_ref()
            .map(|health| health.state.as_str()),
        Some("degraded")
    );
    assert!(
        model.status_line.contains("WELES degraded"),
        "status line should mention degraded WELES health"
    );
}

#[test]
fn models_fetched_updates_picker_count_for_open_model_picker() {
    let mut model = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
    model.modal.set_picker_item_count(1);

    model.handle_client_event(ClientEvent::ModelsFetched(vec![
        crate::wire::FetchedModel {
            id: "m1".to_string(),
            name: Some("Model One".to_string()),
            context_window: Some(128_000),
            pricing: None,
            metadata: None,
        },
        crate::wire::FetchedModel {
            id: "m2".to_string(),
            name: Some("Model Two".to_string()),
            context_window: Some(128_000),
            pricing: None,
            metadata: None,
        },
        crate::wire::FetchedModel {
            id: "m3".to_string(),
            name: Some("Model Three".to_string()),
            context_window: Some(128_000),
            pricing: None,
            metadata: None,
        },
    ]));

    model.modal.reduce(modal::ModalAction::Navigate(1));
    model.modal.reduce(modal::ModalAction::Navigate(1));

    assert_eq!(model.modal.picker_cursor(), 2);
}

#[test]
fn approval_required_in_current_thread_opens_blocking_modal() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Active Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-1".to_string(),
        title: "WELES review".to_string(),
        thread_id: Some("thread-1".to_string()),
        status: Some(crate::wire::TaskStatus::AwaitingApproval),
        awaiting_approval_id: Some("approval-1".to_string()),
        ..Default::default()
    }]);

    model.handle_client_event(ClientEvent::ApprovalRequired {
        approval_id: "approval-1".to_string(),
        command: "git push".to_string(),
        rationale: Some("Push release branch to origin".to_string()),
        reasons: vec!["network access requested".to_string()],
        risk_level: "high".to_string(),
        blast_radius: "repo".to_string(),
    });

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ApprovalOverlay));
    assert_eq!(model.approval.selected_approval_id(), Some("approval-1"));
}

#[test]
fn approval_required_in_background_thread_shows_notice_without_modal() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Active Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-2".to_string(),
        title: "Background Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-2".to_string(),
        title: "WELES review".to_string(),
        thread_id: Some("thread-2".to_string()),
        status: Some(crate::wire::TaskStatus::AwaitingApproval),
        awaiting_approval_id: Some("approval-2".to_string()),
        ..Default::default()
    }]);

    model.handle_client_event(ClientEvent::ApprovalRequired {
        approval_id: "approval-2".to_string(),
        command: "git clone".to_string(),
        rationale: Some("Clone support repository into workspace".to_string()),
        reasons: vec!["network access requested".to_string()],
        risk_level: "medium".to_string(),
        blast_radius: "workspace".to_string(),
    });

    assert_eq!(model.modal.top(), None);
    assert_eq!(model.approval.pending_approvals().len(), 1);
    assert!(model
        .input_notice_style()
        .expect("approval banner should be visible")
        .0
        .contains("Ctrl+A"));
}

#[test]
fn opening_thread_with_existing_pending_approval_opens_blocking_modal() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Active Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-2".to_string(),
        title: "Background Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-2".to_string(),
        title: "WELES review".to_string(),
        thread_id: Some("thread-2".to_string()),
        status: Some(crate::wire::TaskStatus::AwaitingApproval),
        awaiting_approval_id: Some("approval-2".to_string()),
        ..Default::default()
    }]);

    model.handle_client_event(ClientEvent::ApprovalRequired {
        approval_id: "approval-2".to_string(),
        command: "git clone".to_string(),
        rationale: Some("Clone support repository into workspace".to_string()),
        reasons: vec!["network access requested".to_string()],
        risk_level: "medium".to_string(),
        blast_radius: "workspace".to_string(),
    });
    assert_eq!(model.modal.top(), None);

    model.open_thread_conversation("thread-2".to_string());

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ApprovalOverlay));
    assert_eq!(model.approval.selected_approval_id(), Some("approval-2"));
}

#[test]
fn task_list_hydrates_pending_approvals_from_awaiting_approval_tasks() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Hydrated Thread".to_string(),
    });

    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-1".to_string(),
        title: "Hydrated approval".to_string(),
        thread_id: Some("thread-1".to_string()),
        status: Some(crate::wire::TaskStatus::AwaitingApproval),
        awaiting_approval_id: Some("approval-1".to_string()),
        blocked_reason: Some("waiting for operator approval".to_string()),
        ..Default::default()
    }]);

    let approval = model
        .approval
        .approval_by_id("approval-1")
        .expect("task snapshot should hydrate approval queue");
    assert_eq!(approval.task_id, "task-1");
    assert_eq!(approval.thread_title.as_deref(), Some("Hydrated Thread"));
}

#[test]
fn goal_run_update_hydrates_pending_approval_when_task_snapshot_is_missing() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Goal Thread".to_string(),
    });

    model.handle_goal_run_update_event(crate::wire::GoalRun {
        id: "goal-1".to_string(),
        title: "Goal plan review".to_string(),
        thread_id: Some("thread-1".to_string()),
        status: Some(crate::wire::GoalRunStatus::AwaitingApproval),
        current_step_title: Some("review plan".to_string()),
        approval_count: 1,
        awaiting_approval_id: Some("approval-1".to_string()),
        ..Default::default()
    });

    let approval = model
        .approval
        .approval_by_id("approval-1")
        .expect("goal run awaiting approval should hydrate approval queue");
    assert_eq!(approval.thread_id.as_deref(), Some("thread-1"));
    assert_eq!(approval.thread_title.as_deref(), Some("Goal Thread"));
    assert_eq!(approval.task_title.as_deref(), Some("Goal plan review"));
}

#[test]
fn goal_run_started_requests_authoritative_refresh_and_hydrates_pending_approval() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Goal Thread".to_string(),
    });

    model.handle_goal_run_started_event(crate::wire::GoalRun {
        id: "goal-1".to_string(),
        title: "Goal plan review".to_string(),
        thread_id: Some("thread-1".to_string()),
        status: Some(crate::wire::GoalRunStatus::AwaitingApproval),
        current_step_title: Some("review plan".to_string()),
        approval_count: 1,
        awaiting_approval_id: Some("approval-1".to_string()),
        ..Default::default()
    });

    assert!(matches!(
        &model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::GoalRun { goal_run_id, step_id: None })
            if goal_run_id == "goal-1"
    ));
    assert_eq!(
        next_goal_run_detail_request(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );
    assert_eq!(
        next_goal_run_checkpoints_request(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );
    assert_eq!(
        next_goal_hydration_schedule(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );
    assert!(
        model.pending_goal_hydration_refreshes.contains("goal-1"),
        "started goal should remain pending until authoritative hydration lands"
    );

    let approval = model
        .approval
        .approval_by_id("approval-1")
        .expect("started goal awaiting approval should hydrate approval queue");
    assert_eq!(approval.thread_id.as_deref(), Some("thread-1"));
    assert_eq!(approval.thread_title.as_deref(), Some("Goal Thread"));
    assert_eq!(approval.task_title.as_deref(), Some("Goal plan review"));
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ApprovalOverlay));
    assert_eq!(model.approval.selected_approval_id(), Some("approval-1"));
}

#[test]
fn approval_resolution_requests_authoritative_refresh_for_visible_goal_run() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_goal_run_detail_event(crate::wire::GoalRun {
        id: "goal-1".to_string(),
        title: "Goal plan review".to_string(),
        thread_id: Some("thread-1".to_string()),
        status: Some(crate::wire::GoalRunStatus::AwaitingApproval),
        current_step_title: Some("review plan".to_string()),
        approval_count: 1,
        awaiting_approval_id: Some("approval-1".to_string()),
        ..Default::default()
    });
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    model.handle_approval_resolved_event("approval-1".to_string(), "approved".to_string());

    assert_eq!(
        next_goal_run_detail_request(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );
    assert_eq!(
        next_goal_run_checkpoints_request(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );
}

#[test]
fn task_list_event_preserves_spawned_tree_metadata() {
    let mut model = make_model();

    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-1".to_string(),
        title: "Hydrated child".to_string(),
        thread_id: Some("thread-1".to_string()),
        parent_task_id: Some("task-root".to_string()),
        parent_thread_id: Some("thread-root".to_string()),
        created_at: 42,
        ..Default::default()
    }]);

    let task = model
        .tasks
        .task_by_id("task-1")
        .expect("task should be present after hydration");
    assert_eq!(task.parent_task_id.as_deref(), Some("task-root"));
    assert_eq!(task.parent_thread_id.as_deref(), Some("thread-root"));
    assert_eq!(task.created_at, 42);
}

#[test]
fn fallback_task_update_preserves_spawned_tree_metadata() {
    let mut model = make_model();

    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-1".to_string(),
        title: "Hydrated child".to_string(),
        thread_id: Some("thread-1".to_string()),
        parent_task_id: Some("task-root".to_string()),
        parent_thread_id: Some("thread-root".to_string()),
        created_at: 42,
        ..Default::default()
    }]);

    model.handle_task_update_event(crate::wire::AgentTask {
        id: "task-1".to_string(),
        title: "Hydrated child".to_string(),
        status: Some(crate::wire::TaskStatus::InProgress),
        progress: 75,
        ..Default::default()
    });

    let task = model
        .tasks
        .task_by_id("task-1")
        .expect("task should still be present after update");
    assert_eq!(task.parent_task_id.as_deref(), Some("task-root"));
    assert_eq!(task.parent_thread_id.as_deref(), Some("thread-root"));
    assert_eq!(task.created_at, 42);
}

#[test]
fn budget_exceeded_active_thread_surfaces_persistent_footer_notice() {
    let mut model = make_model();
    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-child".to_string(),
        title: "Child thread".to_string(),
        ..Default::default()
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-child".to_string()));

    model.handle_task_update_event(crate::wire::AgentTask {
        id: "task-child".to_string(),
        title: "Child task".to_string(),
        thread_id: Some("thread-child".to_string()),
        status: Some(crate::wire::TaskStatus::BudgetExceeded),
        blocked_reason: Some("execution budget exceeded for this thread".to_string()),
        created_at: 42,
        ..Default::default()
    });

    let notice = model
        .active_thread_budget_exceeded_notice()
        .expect("budget exceeded thread should surface footer notice");
    assert!(
        notice.contains("Thread budget exceeded"),
        "expected budget notice, got: {notice}"
    );
    assert!(
        notice.contains("thread-child"),
        "expected thread id in notice, got: {notice}"
    );
}

#[test]
fn submit_prompt_blocks_budget_exceeded_active_thread() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-child".to_string(),
        title: "Child thread".to_string(),
        ..Default::default()
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-child".to_string()));
    model.handle_task_update_event(crate::wire::AgentTask {
        id: "task-child".to_string(),
        title: "Child task".to_string(),
        thread_id: Some("thread-child".to_string()),
        status: Some(crate::wire::TaskStatus::BudgetExceeded),
        blocked_reason: Some("execution budget exceeded for this thread".to_string()),
        created_at: 42,
        ..Default::default()
    });
    while daemon_rx.try_recv().is_ok() {}

    model.submit_prompt("continue from here".to_string());

    assert_eq!(
        model.input.buffer(),
        "continue from here",
        "blocked submit should preserve operator text in the input"
    );
    let (notice, _) = model
        .input_notice_style()
        .expect("blocked submit should surface an input notice");
    assert!(
        notice.contains("Thread budget exceeded"),
        "expected budget exceeded notice, got: {notice}"
    );
    while let Ok(command) = daemon_rx.try_recv() {
        assert!(
            !matches!(command, DaemonCommand::SendMessage { .. }),
            "budget-exceeded thread should not emit a send command: {command:?}"
        );
    }
}

#[test]
fn unrelated_sync_does_not_clear_event_backed_pending_approval() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::ApprovalRequired {
        approval_id: "approval-1".to_string(),
        command: "git push".to_string(),
        rationale: Some("Push branch".to_string()),
        reasons: vec!["network access requested".to_string()],
        risk_level: "high".to_string(),
        blast_radius: "repo".to_string(),
    });

    model.handle_thread_list_event(vec![crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Thread".to_string(),
        ..Default::default()
    }]);

    assert!(
        model.approval.approval_by_id("approval-1").is_some(),
        "thread sync should not discard live approvals without an explicit resolution"
    );
}

#[test]
fn task_list_clears_approval_when_same_task_no_longer_waits_for_it() {
    let mut model = make_model();

    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-1".to_string(),
        title: "Hydrated approval".to_string(),
        thread_id: Some("thread-1".to_string()),
        status: Some(crate::wire::TaskStatus::AwaitingApproval),
        awaiting_approval_id: Some("approval-1".to_string()),
        blocked_reason: Some("waiting for operator approval".to_string()),
        ..Default::default()
    }]);
    assert!(model.approval.approval_by_id("approval-1").is_some());

    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-1".to_string(),
        title: "Hydrated approval".to_string(),
        thread_id: Some("thread-1".to_string()),
        status: Some(crate::wire::TaskStatus::Queued),
        awaiting_approval_id: None,
        ..Default::default()
    }]);

    assert!(
        model.approval.approval_by_id("approval-1").is_none(),
        "task snapshot should clear approvals only when the same task explicitly drops them"
    );
}

#[test]
fn task_update_clearing_approval_does_not_rehydrate_on_later_sync() {
    let mut model = make_model();

    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-1".to_string(),
        title: "Hydrated approval".to_string(),
        thread_id: Some("thread-1".to_string()),
        status: Some(crate::wire::TaskStatus::AwaitingApproval),
        awaiting_approval_id: Some("approval-1".to_string()),
        blocked_reason: Some("waiting for operator approval".to_string()),
        ..Default::default()
    }]);
    assert!(model.approval.approval_by_id("approval-1").is_some());

    model.handle_task_update_event(crate::wire::AgentTask {
        id: "task-1".to_string(),
        title: "Hydrated approval".to_string(),
        thread_id: Some("thread-1".to_string()),
        status: Some(crate::wire::TaskStatus::Queued),
        awaiting_approval_id: None,
        ..Default::default()
    });

    assert!(
        model.approval.approval_by_id("approval-1").is_none(),
        "task update should clear the pending approval immediately"
    );

    model.handle_thread_list_event(vec![crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Thread".to_string(),
        ..Default::default()
    }]);

    assert!(
        model.approval.approval_by_id("approval-1").is_none(),
        "later thread sync should not resurrect approvals that the task already cleared"
    );
}

#[test]
fn task_list_hydrates_policy_escalation_rationale_from_thread_messages() {
    let mut model = make_model();
    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Hydrated Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::System,
            content: "Policy escalation requested operator guidance: Cloning scientific skills repository from GitHub as part of WELES governance review task".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    });

    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-1".to_string(),
        title: "WELES".to_string(),
        thread_id: Some("thread-1".to_string()),
        status: Some(crate::wire::TaskStatus::AwaitingApproval),
        awaiting_approval_id: Some("approval-1".to_string()),
        blocked_reason: Some(
            "waiting for operator approval: orchestrator_policy_escalation".to_string(),
        ),
        ..Default::default()
    }]);

    let approval = model
        .approval
        .approval_by_id("approval-1")
        .expect("task snapshot should hydrate approval queue");
    assert_eq!(
        approval.rationale.as_deref(),
        Some(
            "Cloning scientific skills repository from GitHub as part of WELES governance review task"
        )
    );
}

#[test]
fn done_event_persists_final_reasoning_into_chat_message() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::Delta {
        thread_id: "thread-1".to_string(),
        content: "Answer".to_string(),
    });

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-1".to_string(),
        input_tokens: 10,
        output_tokens: 20,
        cost: None,
        provider: Some(PROVIDER_ID_GITHUB_COPILOT.to_string()),
        model: Some("gpt-5.4".to_string()),
        tps: None,
        generation_ms: None,
        reasoning: Some("Final reasoning summary".to_string()),
        provider_final_result_json: Some("result_json".to_string()),
    });

    let thread = model.chat.active_thread().expect("thread should exist");
    let last = thread
        .messages
        .last()
        .expect("assistant message should exist");
    assert_eq!(last.reasoning.as_deref(), Some("Final reasoning summary"));
}

#[test]
fn done_event_does_not_force_authoritative_refresh_for_participant_threads() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::User,
            content: "hello".to_string(),
            timestamp: 1,
            ..Default::default()
        }],
        thread_participants: vec![crate::wire::ThreadParticipantState {
            agent_id: "weles".to_string(),
            agent_name: "Weles".to_string(),
            instruction: "verify claims".to_string(),
            status: "active".to_string(),
            created_at: 1,
            updated_at: 1,
            last_contribution_at: None,
            deactivated_at: None,
            always_auto_response: false,
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-1".to_string(),
        input_tokens: 10,
        output_tokens: 20,
        cost: None,
        provider: Some(PROVIDER_ID_GITHUB_COPILOT.to_string()),
        model: Some("gpt-5.4".to_string()),
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: Some("result_json".to_string()),
    });

    assert!(
        next_thread_request(&mut daemon_rx).is_none(),
        "participant-thread done should rely on daemon reload events instead of forcing a TUI refresh"
    );
}

#[test]
fn thread_detail_event_hydrates_pinned_for_compaction_from_wire() {
    let mut model = make_model();

    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Pinned".to_string(),
        messages: vec![crate::wire::AgentMessage {
            id: Some("message-1".to_string()),
            role: crate::wire::MessageRole::Assistant,
            content: "Pinned content".to_string(),
            pinned_for_compaction: true,
            ..Default::default()
        }],
        loaded_message_end: 1,
        total_message_count: 1,
        ..Default::default()
    });

    let thread = model.chat.active_thread().expect("thread should exist");
    assert!(thread.messages[0].pinned_for_compaction);
}

#[test]
fn thread_detail_event_hydrates_tool_output_preview_path_from_wire() {
    let mut model = make_model();

    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Preview".to_string(),
        messages: vec![crate::wire::AgentMessage {
            id: Some("message-1".to_string()),
            role: crate::wire::MessageRole::Tool,
            tool_name: Some("bash_command".to_string()),
            tool_status: Some("done".to_string()),
            tool_output_preview_path: Some(
                "/tmp/.zorai/.cache/tools/thread-thread-1/bash_command-1700000123.txt".to_string(),
            ),
            content: "Tool result saved to preview file".to_string(),
            ..Default::default()
        }],
        loaded_message_end: 1,
        total_message_count: 1,
        ..Default::default()
    });

    let thread = model.chat.active_thread().expect("thread should exist");
    assert_eq!(
        thread.messages[0].tool_output_preview_path.as_deref(),
        Some("/tmp/.zorai/.cache/tools/thread-thread-1/bash_command-1700000123.txt")
    );
}

#[test]
fn stale_retry_status_after_done_does_not_restore_retrying_placeholder() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::User,
            content: "Retry this".to_string(),
            ..Default::default()
        },
    });

    model.handle_client_event(ClientEvent::RetryStatus {
        thread_id: "thread-1".to_string(),
        phase: "retrying".to_string(),
        attempt: 1,
        max_retries: 3,
        delay_ms: 1_000,
        failure_class: "temporary_upstream".to_string(),
        message: "upstream timeout".to_string(),
    });
    assert_eq!(model.footer_activity_text().as_deref(), Some("retrying"));
    assert!(model.chat.retry_status().is_some());

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-1".to_string(),
        content: "Recovered answer".to_string(),
    });

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-1".to_string(),
        input_tokens: 10,
        output_tokens: 20,
        cost: None,
        provider: Some(PROVIDER_ID_GITHUB_COPILOT.to_string()),
        model: Some("gpt-5.4".to_string()),
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: Some("result_json".to_string()),
    });

    assert!(
        model.footer_activity_text().is_none(),
        "done should clear the retrying placeholder"
    );
    assert!(
        model.chat.retry_status().is_none(),
        "done should clear visible retry status"
    );

    model.handle_client_event(ClientEvent::RetryStatus {
        thread_id: "thread-1".to_string(),
        phase: "retrying".to_string(),
        attempt: 1,
        max_retries: 3,
        delay_ms: 1_000,
        failure_class: "temporary_upstream".to_string(),
        message: "late retry status".to_string(),
    });

    assert!(
        model.footer_activity_text().is_none(),
        "late retry events for a completed turn must not restore the retrying placeholder"
    );
    assert!(
        model.chat.retry_status().is_none(),
        "late retry events for a completed turn must not restore retry status UI"
    );
}

#[test]
fn header_uses_rarog_daemon_runtime_metadata_after_first_reply() {
    let mut model = make_model();
    model.concierge.provider = Some("alibaba-coding-plan".to_string());
    model.concierge.model = Some("qwen3.6-plus".to_string());
    model.concierge.reasoning_effort = Some("none".to_string());

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-rarog".to_string(),
        title: "Rarog Thread".to_string(),
        agent_name: Some("Rarog".to_string()),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-rarog".to_string()));
    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-rarog".to_string(),
        content: "Runtime answer".to_string(),
    });

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-rarog".to_string(),
        input_tokens: 10,
        output_tokens: 20,
        cost: None,
        provider: Some("alibaba-coding-plan".to_string()),
        model: Some("MiniMax-M2.5".to_string()),
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: Some(r#"{"reasoning":{"effort":"low"}}"#.to_string()),
    });

    let profile = model.current_header_agent_profile();
    assert_eq!(profile.agent_label, "Rarog");
    assert_eq!(profile.provider, "alibaba-coding-plan");
    assert_eq!(profile.model, "MiniMax-M2.5");
    assert_eq!(profile.reasoning_effort.as_deref(), Some("low"));
}

fn make_goal_owner_profile(
    agent_label: &str,
    provider: &str,
    model: &str,
    reasoning_effort: Option<&str>,
) -> task::GoalRuntimeOwnerProfile {
    task::GoalRuntimeOwnerProfile {
        agent_label: agent_label.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        reasoning_effort: reasoning_effort.map(str::to_string),
    }
}

fn make_goal_run_for_header_tests(
    goal_run_id: &str,
    thread_id: Option<&str>,
    planner_owner_profile: Option<task::GoalRuntimeOwnerProfile>,
    current_step_owner_profile: Option<task::GoalRuntimeOwnerProfile>,
) -> task::GoalRun {
    task::GoalRun {
        id: goal_run_id.to_string(),
        title: "Goal".to_string(),
        thread_id: thread_id.map(str::to_string),
        planner_owner_profile,
        current_step_owner_profile,
        ..Default::default()
    }
}

fn make_goal_assignment(
    role_id: &str,
    provider: &str,
    model: &str,
    reasoning_effort: Option<&str>,
) -> task::GoalAgentAssignment {
    task::GoalAgentAssignment {
        role_id: role_id.to_string(),
        enabled: true,
        provider: provider.to_string(),
        model: model.to_string(),
        reasoning_effort: reasoning_effort.map(str::to_string),
        inherit_from_main: false,
    }
}

#[test]
fn header_uses_goal_current_step_owner_profile_for_goal_run_pane() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-active".to_string(),
        title: "Active Thread".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-active".to_string()));
    if let Some(thread) = model.chat.active_thread_mut() {
        thread.runtime_provider = Some("conversation-provider".to_string());
        thread.runtime_model = Some("conversation-model".to_string());
        thread.runtime_reasoning_effort = Some("conversation-effort".to_string());
    }

    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });
    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(
        make_goal_run_for_header_tests(
            "goal-1",
            None,
            Some(make_goal_owner_profile(
                "Planner Owner",
                "planner-provider",
                "planner-model",
                Some("planner-effort"),
            )),
            Some(make_goal_owner_profile(
                "Current Step Owner",
                "step-provider",
                "step-model",
                Some("step-effort"),
            )),
        ),
    ));

    let profile = model.current_header_agent_profile();
    assert_eq!(profile.agent_label, "Current Step Owner");
    assert_eq!(profile.provider, "step-provider");
    assert_eq!(profile.model, "step-model");
    assert_eq!(profile.reasoning_effort.as_deref(), Some("step-effort"));
}

#[test]
fn mission_control_header_prefers_active_execution_thread_runtime_and_usage() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    model.config.auth_source = "github_copilot".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.context_window_tokens = 400_000;

    for (thread_id, title, input_tokens, output_tokens, cost, provider, model_id, effort) in [
        (
            "thread-root",
            "Root Thread",
            40_u64,
            60_u64,
            Some(1.0_f64),
            "openai",
            "gpt-5.4",
            Some("medium"),
        ),
        (
            "thread-active",
            "Active Thread",
            10_u64,
            20_u64,
            Some(0.25_f64),
            "alibaba-coding-plan",
            "MiniMax-M2.5",
            Some("high"),
        ),
    ] {
        model.handle_client_event(ClientEvent::ThreadCreated {
            thread_id: thread_id.to_string(),
            title: title.to_string(),
            agent_name: Some("Swarog".to_string()),
        });
        model.handle_thread_detail_event(crate::wire::AgentThread {
            id: thread_id.to_string(),
            title: title.to_string(),
            messages: vec![crate::wire::AgentMessage {
                role: crate::wire::MessageRole::Assistant,
                content: format!("{title} output"),
                cost,
                input_tokens,
                output_tokens,
                message_kind: "normal".to_string(),
                ..Default::default()
            }],
            total_input_tokens: input_tokens,
            total_output_tokens: output_tokens,
            loaded_message_start: 0,
            loaded_message_end: 1,
            total_message_count: 1,
            created_at: 1,
            updated_at: 1,
            ..Default::default()
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread(thread_id.to_string()));
        if let Some(thread) = model.chat.active_thread_mut() {
            thread.runtime_provider = Some(provider.to_string());
            thread.runtime_model = Some(model_id.to_string());
            thread.runtime_reasoning_effort = effort.map(str::to_string);
        }
    }

    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-mission-control-active".to_string(),
        step_id: None,
    });
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-mission-control-active".to_string(),
            title: "Goal".to_string(),
            thread_id: Some("thread-root".to_string()),
            root_thread_id: Some("thread-root".to_string()),
            active_thread_id: Some("thread-active".to_string()),
            planner_owner_profile: Some(make_goal_owner_profile(
                "Planner Owner",
                "planner-provider",
                "planner-model",
                Some("low"),
            )),
            current_step_owner_profile: Some(make_goal_owner_profile(
                "Current Step Owner",
                "step-provider",
                "step-model",
                Some("medium"),
            )),
            ..Default::default()
        }));

    let profile = model.current_header_agent_profile();
    assert_eq!(profile.provider, "openai");
    assert_eq!(profile.model, "gpt-5.4");
    assert_eq!(profile.reasoning_effort.as_deref(), Some("medium"));

    let usage = model.current_header_usage_summary();
    assert_eq!(usage.total_thread_tokens, 100);
    assert_eq!(usage.total_cost_usd, Some(1.0));
    assert_eq!(usage.context_window_tokens, 1_000_000);
}

#[test]
fn header_profile_uses_thread_owner_not_latest_participant_author() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    model.config.auth_source = "github_copilot".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.context_window_tokens = 400_000;
    model.subagents.entries.push(crate::state::SubAgentEntry {
        id: "mokosh".to_string(),
        name: "Mokosh".to_string(),
        provider: "alibaba-coding-plan".to_string(),
        model: "glm-5".to_string(),
        role: Some("testing".to_string()),
        enabled: true,
        builtin: true,
        immutable_identity: true,
        disable_allowed: false,
        delete_allowed: false,
        protected_reason: None,
        reasoning_effort: Some("medium".to_string()),
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        raw_json: None,
    });

    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-owner-main".to_string(),
        agent_name: Some("Swarog".to_string()),
        title: "Main-owned thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::Assistant,
            content: "Participant contribution".to_string(),
            author_agent_id: Some("mokosh".to_string()),
            author_agent_name: Some("Mokosh".to_string()),
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        total_input_tokens: 10,
        total_output_tokens: 20,
        loaded_message_start: 0,
        loaded_message_end: 1,
        total_message_count: 1,
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-owner-main".to_string(),
    ));

    let profile = model.current_header_agent_profile();
    assert_eq!(profile.agent_label, "Swarog");
    assert_eq!(profile.provider, PROVIDER_ID_GITHUB_COPILOT);
    assert_eq!(profile.model, "gpt-5.4");

    let usage = model.current_header_usage_summary();
    assert_eq!(usage.context_window_tokens, 400_000);
}

#[test]
fn header_falls_back_to_goal_planner_owner_profile_for_goal_run_pane() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-active".to_string(),
        title: "Active Thread".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-active".to_string()));
    if let Some(thread) = model.chat.active_thread_mut() {
        thread.runtime_provider = Some("conversation-provider".to_string());
        thread.runtime_model = Some("conversation-model".to_string());
        thread.runtime_reasoning_effort = Some("conversation-effort".to_string());
    }

    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-2".to_string(),
        step_id: None,
    });
    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(
        make_goal_run_for_header_tests(
            "goal-2",
            None,
            Some(make_goal_owner_profile(
                "Planner Owner",
                "planner-provider",
                "planner-model",
                Some("planner-effort"),
            )),
            None,
        ),
    ));

    let profile = model.current_header_agent_profile();
    assert_eq!(profile.agent_label, "Planner Owner");
    assert_eq!(profile.provider, "planner-provider");
    assert_eq!(profile.model, "planner-model");
    assert_eq!(profile.reasoning_effort.as_deref(), Some("planner-effort"));
}

#[test]
fn mission_control_header_falls_back_to_root_thread_runtime_when_active_thread_missing() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    model.config.auth_source = "github_copilot".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.context_window_tokens = 400_000;

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-root-fallback".to_string(),
        title: "Root Thread".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-root-fallback".to_string(),
        title: "Root Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::Assistant,
            content: "Root output".to_string(),
            cost: Some(0.5),
            input_tokens: 12,
            output_tokens: 18,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        total_input_tokens: 12,
        total_output_tokens: 18,
        loaded_message_start: 0,
        loaded_message_end: 1,
        total_message_count: 1,
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-root-fallback".to_string(),
    ));
    if let Some(thread) = model.chat.active_thread_mut() {
        thread.runtime_provider = Some("alibaba-coding-plan".to_string());
        thread.runtime_model = Some("MiniMax-M2.5".to_string());
        thread.runtime_reasoning_effort = Some("high".to_string());
    }

    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-root-fallback".to_string(),
        step_id: None,
    });
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-root-fallback".to_string(),
            title: "Goal".to_string(),
            thread_id: Some("thread-legacy".to_string()),
            root_thread_id: Some("thread-root-fallback".to_string()),
            active_thread_id: None,
            ..Default::default()
        }));

    let profile = model.current_header_agent_profile();
    assert_eq!(profile.provider, "alibaba-coding-plan");
    assert_eq!(profile.model, "MiniMax-M2.5");
    assert_eq!(profile.reasoning_effort.as_deref(), Some("high"));

    let usage = model.current_header_usage_summary();
    assert_eq!(usage.total_thread_tokens, 30);
    assert_eq!(usage.total_cost_usd, Some(0.5));
    assert_eq!(usage.context_window_tokens, 205_000);
}

#[test]
fn header_uses_thread_profile_metadata_before_runtime_metadata_arrives() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    model.config.auth_source = "github_copilot".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.context_window_tokens = 400_000;

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-dazhbog-profile".to_string(),
        title: "Spawned thread".to_string(),
        agent_name: Some("Dazhbog".to_string()),
    });
    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-dazhbog-profile".to_string(),
        agent_name: Some("Dazhbog".to_string()),
        profile_provider: Some("alibaba-coding-plan".to_string()),
        profile_model: Some("kimi-k2.5".to_string()),
        profile_reasoning_effort: Some("high".to_string()),
        profile_context_window_tokens: Some(240_000),
        title: "Spawned thread".to_string(),
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-dazhbog-profile".to_string(),
    ));

    let profile = model.current_header_agent_profile();
    assert_eq!(profile.agent_label, "Dazhbog");
    assert_eq!(profile.provider, "alibaba-coding-plan");
    assert_eq!(profile.model, "kimi-k2.5");
    assert_eq!(profile.reasoning_effort.as_deref(), Some("high"));

    let usage = model.current_header_usage_summary();
    assert_eq!(usage.context_window_tokens, 240_000);
}

#[test]
fn mission_control_header_context_window_tracks_active_execution_thread_changes() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    model.config.auth_source = "github_copilot".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.context_window_tokens = 400_000;

    for (thread_id, title, provider, model_id) in [
        ("thread-root-window", "Root Thread", "openai", "gpt-5.4"),
        (
            "thread-active-window",
            "Active Thread",
            "alibaba-coding-plan",
            "MiniMax-M2.5",
        ),
    ] {
        model.handle_client_event(ClientEvent::ThreadCreated {
            thread_id: thread_id.to_string(),
            title: title.to_string(),
            agent_name: Some("Swarog".to_string()),
        });
        model.handle_thread_detail_event(crate::wire::AgentThread {
            id: thread_id.to_string(),
            title: title.to_string(),
            messages: vec![crate::wire::AgentMessage {
                role: crate::wire::MessageRole::Assistant,
                content: format!("{title} output"),
                input_tokens: 10,
                output_tokens: 10,
                message_kind: "normal".to_string(),
                ..Default::default()
            }],
            total_input_tokens: 10,
            total_output_tokens: 10,
            loaded_message_start: 0,
            loaded_message_end: 1,
            total_message_count: 1,
            created_at: 1,
            updated_at: 1,
            ..Default::default()
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread(thread_id.to_string()));
        if let Some(thread) = model.chat.active_thread_mut() {
            thread.runtime_provider = Some(provider.to_string());
            thread.runtime_model = Some(model_id.to_string());
        }
    }

    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-window-switch".to_string(),
        step_id: None,
    });
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-window-switch".to_string(),
            title: "Goal".to_string(),
            thread_id: Some("thread-root-window".to_string()),
            root_thread_id: Some("thread-root-window".to_string()),
            active_thread_id: Some("thread-root-window".to_string()),
            ..Default::default()
        }));

    let initial = model.current_header_usage_summary();
    assert_eq!(initial.context_window_tokens, 1_000_000);

    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-window-switch".to_string(),
            title: "Goal".to_string(),
            thread_id: Some("thread-root-window".to_string()),
            root_thread_id: Some("thread-root-window".to_string()),
            active_thread_id: Some("thread-active-window".to_string()),
            ..Default::default()
        }));

    let updated = model.current_header_usage_summary();
    assert_eq!(updated.context_window_tokens, 1_000_000);
}

#[test]
fn header_goal_run_pane_ignores_unrelated_conversation_runtime_metadata() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-active".to_string(),
        title: "Active Thread".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-active".to_string()));
    if let Some(thread) = model.chat.active_thread_mut() {
        thread.runtime_provider = Some("conversation-provider".to_string());
        thread.runtime_model = Some("conversation-model".to_string());
        thread.runtime_reasoning_effort = Some("conversation-effort".to_string());
    }

    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-3".to_string(),
        step_id: None,
    });
    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(
        make_goal_run_for_header_tests(
            "goal-3",
            None,
            Some(make_goal_owner_profile(
                "Planner Owner",
                "planner-provider",
                "planner-model",
                Some("planner-effort"),
            )),
            Some(make_goal_owner_profile(
                "Current Step Owner",
                "step-provider",
                "step-model",
                Some("step-effort"),
            )),
        ),
    ));

    let profile = model.current_header_agent_profile();
    assert_ne!(profile.provider, "conversation-provider");
    assert_ne!(profile.model, "conversation-model");
    assert_ne!(
        profile.reasoning_effort.as_deref(),
        Some("conversation-effort")
    );
    assert_eq!(profile.provider, "step-provider");
    assert_eq!(profile.model, "step-model");
    assert_eq!(profile.reasoning_effort.as_deref(), Some("step-effort"));
}

#[test]
fn mission_control_header_falls_back_to_launch_snapshot_before_generic_defaults() {
    let mut model = make_model();
    model.config.provider = "provider-generic".to_string();
    model.config.model = "model-generic".to_string();
    model.config.reasoning_effort = "low".to_string();
    model.config.context_window_tokens = 400_000;

    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-launch-snapshot".to_string(),
        step_id: None,
    });
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-launch-snapshot".to_string(),
            title: "Goal".to_string(),
            launch_assignment_snapshot: vec![make_goal_assignment(
                zorai_protocol::AGENT_ID_SWAROG,
                "alibaba-coding-plan",
                "MiniMax-M2.5",
                Some("high"),
            )],
            ..Default::default()
        }));

    let profile = model.current_header_agent_profile();
    assert_eq!(profile.agent_label, "Swarog");
    assert_eq!(profile.provider, "alibaba-coding-plan");
    assert_eq!(profile.model, "MiniMax-M2.5");
    assert_eq!(profile.reasoning_effort.as_deref(), Some("high"));

    let usage = model.current_header_usage_summary();
    assert_eq!(usage.total_thread_tokens, 0);
    assert_eq!(usage.context_window_tokens, 205_000);
}

#[test]
fn header_uses_goal_thread_usage_when_goal_run_thread_exists() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-active".to_string(),
        title: "Active Thread".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-active".to_string()));
    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-active".to_string(),
        title: "Active Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::User,
            content: "active thread content that should not be used".repeat(50),
            cost: Some(9.99),
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        total_input_tokens: 900,
        total_output_tokens: 1_100,
        loaded_message_start: 0,
        loaded_message_end: 1,
        total_message_count: 1,
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    });
    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-goal".to_string(),
        title: "Goal Thread".to_string(),
        messages: vec![
            crate::wire::AgentMessage {
                role: crate::wire::MessageRole::User,
                content: "goal".to_string(),
                cost: Some(0.25),
                message_kind: "normal".to_string(),
                ..Default::default()
            },
            crate::wire::AgentMessage {
                role: crate::wire::MessageRole::Assistant,
                content: "result".to_string(),
                cost: Some(0.75),
                message_kind: "normal".to_string(),
                ..Default::default()
            },
        ],
        total_input_tokens: 40,
        total_output_tokens: 60,
        loaded_message_start: 0,
        loaded_message_end: 2,
        total_message_count: 2,
        created_at: 2,
        updated_at: 2,
        ..Default::default()
    });

    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-usage".to_string(),
        step_id: None,
    });
    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(
        make_goal_run_for_header_tests(
            "goal-usage",
            Some("thread-goal"),
            Some(make_goal_owner_profile(
                "Planner Owner",
                "planner-provider",
                "planner-model",
                None,
            )),
            Some(make_goal_owner_profile(
                "Current Step Owner",
                "step-provider",
                "step-model",
                None,
            )),
        ),
    ));

    let usage = model.current_header_usage_summary();
    assert_eq!(usage.total_thread_tokens, 100);
    assert_eq!(usage.total_cost_usd, Some(1.0));
    assert_eq!(usage.current_tokens, 0);
    assert_ne!(usage.total_thread_tokens, 2_000);
    assert_ne!(usage.total_cost_usd, Some(9.99));
}

#[test]
fn header_goal_run_usage_defaults_when_goal_thread_missing() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    model.config.auth_source = "github_copilot".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.context_window_tokens = 400_000;
    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-active".to_string(),
        title: "Active Thread".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-active".to_string()));
    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-active".to_string(),
        title: "Active Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::User,
            content: "active thread content that should not be borrowed".repeat(50),
            cost: Some(9.99),
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        total_input_tokens: 900,
        total_output_tokens: 1_100,
        loaded_message_start: 0,
        loaded_message_end: 1,
        total_message_count: 1,
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    });

    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-missing-thread".to_string(),
        step_id: None,
    });
    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(
        make_goal_run_for_header_tests(
            "goal-missing-thread",
            Some("goal-thread-missing"),
            Some(make_goal_owner_profile(
                "Planner Owner",
                "planner-provider",
                "planner-model",
                None,
            )),
            None,
        ),
    ));

    let usage = model.current_header_usage_summary();
    assert_eq!(usage.total_thread_tokens, 0);
    assert_eq!(usage.current_tokens, 0);
    assert_eq!(usage.total_cost_usd, None);
    assert_eq!(usage.context_window_tokens, 400_000);
    assert!(usage.compaction_target_tokens > 0);
    assert_eq!(usage.utilization_pct, 0);
}

#[test]
fn header_goal_run_uses_generic_config_defaults_when_owner_profiles_missing() {
    let mut model = make_model();
    model.config.provider = "provider-generic".to_string();
    model.config.model = "model-generic".to_string();
    model.config.reasoning_effort = "low".to_string();
    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-active".to_string(),
        title: "Active Thread".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-active".to_string()));
    if let Some(thread) = model.chat.active_thread_mut() {
        thread.runtime_provider = Some("conversation-provider".to_string());
        thread.runtime_model = Some("conversation-model".to_string());
        thread.runtime_reasoning_effort = Some("conversation-effort".to_string());
    }

    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-generic".to_string(),
        step_id: None,
    });
    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(
        make_goal_run_for_header_tests("goal-generic", None, None, None),
    ));

    let profile = model.current_header_agent_profile();
    assert_eq!(profile.agent_label, "Swarog");
    assert_eq!(profile.provider, "provider-generic");
    assert_eq!(profile.model, "model-generic");
    assert_eq!(profile.reasoning_effort.as_deref(), Some("low"));
}

#[test]
fn header_usage_summary_uses_runtime_model_context_window_for_rarog() {
    let mut model = make_model();
    model.concierge.provider = Some("alibaba-coding-plan".to_string());
    model.concierge.model = Some("qwen3.6-plus".to_string());

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-rarog-usage".to_string(),
        title: "Rarog Thread".to_string(),
        agent_name: Some("Rarog".to_string()),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-rarog-usage".to_string(),
    ));
    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-rarog-usage".to_string(),
        content: "Runtime answer".to_string(),
    });
    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-rarog-usage".to_string(),
        input_tokens: 10,
        output_tokens: 20,
        cost: Some(0.25),
        provider: Some("alibaba-coding-plan".to_string()),
        model: Some("MiniMax-M2.5".to_string()),
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: None,
    });

    let usage = model.current_header_usage_summary();
    assert_eq!(usage.context_window_tokens, 205_000);
    assert_eq!(usage.compaction_target_tokens, 164_000);
    let total_cost = usage
        .total_cost_usd
        .expect("header should expose total cost");
    assert!(
        (total_cost - 0.25).abs() < 1e-9,
        "expected summed total cost to be 0.25, got {total_cost}"
    );
    assert_eq!(
        usage.current_tokens, 30,
        "header should use daemon-reported token usage from the latest completed turn"
    );
    assert!(usage.utilization_pct <= 100);
}

#[test]
fn header_profile_tracks_repeated_swarog_config_changes_after_runtime_metadata_exists() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-config".to_string(),
        title: "Config".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-config".to_string()));

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-config".to_string(),
        input_tokens: 10,
        output_tokens: 20,
        cost: None,
        provider: Some("provider-old".to_string()),
        model: Some("model-old".to_string()),
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: None,
    });

    let initial = model.current_header_agent_profile();
    assert_eq!(initial.provider, "provider-old");
    assert_eq!(initial.model, "model-old");

    model.handle_agent_config_event(crate::wire::AgentConfigSnapshot {
        provider: "provider-a".to_string(),
        base_url: "https://example.invalid/v1".to_string(),
        model: "model-a".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: "api_key".to_string(),
        api_transport: "responses".to_string(),
        reasoning_effort: "high".to_string(),
        context_window_tokens: 200_000,
    });

    let first = model.current_header_agent_profile();
    assert_eq!(first.provider, "provider-a");
    assert_eq!(first.model, "model-a");

    model.handle_agent_config_event(crate::wire::AgentConfigSnapshot {
        provider: "provider-b".to_string(),
        base_url: "https://example.invalid/v1".to_string(),
        model: "model-b".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: "api_key".to_string(),
        api_transport: "responses".to_string(),
        reasoning_effort: "medium".to_string(),
        context_window_tokens: 400_000,
    });

    let second = model.current_header_agent_profile();
    assert_eq!(second.provider, "provider-b");
    assert_eq!(second.model, "model-b");
}

#[test]
fn header_usage_summary_uses_primary_threshold_for_heuristic_compaction() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    model.config.auth_source = "github_copilot".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.context_window_tokens = 400_000;
    model.config.compact_threshold_pct = 80;
    model.config.compaction_strategy = "heuristic".to_string();

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-heuristic-target".to_string(),
        title: "Heuristic".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-heuristic-target".to_string(),
    ));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-heuristic-target".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::User,
            content: "A".repeat(20_000),
            ..Default::default()
        },
    });

    let usage = model.current_header_usage_summary();
    assert_eq!(
        usage.compaction_target_tokens, 320_000,
        "heuristic compaction should use the main model threshold"
    );
    assert_eq!(usage.context_window_tokens, 400_000);
}

#[test]
fn header_profile_tracks_weles_subagent_updates_after_runtime_metadata_exists() {
    let mut model = make_model();
    model.subagents.entries.push(crate::state::SubAgentEntry {
        id: "weles_builtin".to_string(),
        name: "WELES".to_string(),
        provider: "provider-old".to_string(),
        model: "model-old".to_string(),
        role: Some("testing".to_string()),
        enabled: true,
        builtin: true,
        immutable_identity: true,
        disable_allowed: false,
        delete_allowed: false,
        protected_reason: Some("Built-in reviewer".to_string()),
        reasoning_effort: Some("medium".to_string()),
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        raw_json: None,
    });
    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-weles-runtime".to_string(),
        title: "Reviewer Runtime".to_string(),
        agent_name: Some("Weles".to_string()),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-weles-runtime".to_string(),
    ));
    if let Some(thread) = model.chat.active_thread_mut() {
        thread.runtime_provider = Some("provider-runtime".to_string());
        thread.runtime_model = Some("model-runtime".to_string());
    }

    let before = model.current_header_agent_profile();
    assert_eq!(before.provider, "provider-runtime");
    assert_eq!(before.model, "model-runtime");

    model.handle_subagent_updated_event(crate::state::SubAgentEntry {
        id: "weles_builtin".to_string(),
        name: "WELES".to_string(),
        provider: "provider-new".to_string(),
        model: "model-new".to_string(),
        role: Some("testing".to_string()),
        enabled: true,
        builtin: true,
        immutable_identity: true,
        disable_allowed: false,
        delete_allowed: false,
        protected_reason: Some("Built-in reviewer".to_string()),
        reasoning_effort: Some("high".to_string()),
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        raw_json: None,
    });

    let after = model.current_header_agent_profile();
    assert_eq!(after.provider, "provider-new");
    assert_eq!(after.model, "model-new");
    assert_eq!(after.reasoning_effort.as_deref(), Some("high"));
}

#[test]
fn header_profile_switches_on_handoff_append_and_clears_stale_runtime() {
    let mut model = make_model();
    model.subagents.entries.push(crate::state::SubAgentEntry {
        id: "weles_builtin".to_string(),
        name: "Weles".to_string(),
        provider: "provider-weles".to_string(),
        model: "model-weles".to_string(),
        role: Some("review".to_string()),
        enabled: true,
        builtin: true,
        immutable_identity: true,
        disable_allowed: false,
        delete_allowed: false,
        protected_reason: Some("Built-in reviewer".to_string()),
        reasoning_effort: Some("high".to_string()),
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        raw_json: None,
    });
    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-handoff".to_string(),
        title: "Handoff".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-handoff".to_string()));
    if let Some(thread) = model.chat.active_thread_mut() {
        thread.runtime_provider = Some("provider-runtime".to_string());
        thread.runtime_model = Some("model-runtime".to_string());
    }

    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-handoff".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::System,
            content: "[[handoff_event]]{\"to_agent_id\":\"weles\",\"to_agent_name\":\"Weles\"}\nSwarog handed this thread to Weles.".to_string(),
            ..Default::default()
        },
    });

    let profile = model.current_header_agent_profile();
    assert_eq!(profile.agent_label, "Swarog");
    assert_eq!(profile.provider, model.config.provider);
    assert_eq!(profile.model, model.config.model);
    let thread = model
        .chat
        .active_thread()
        .expect("thread should remain active");
    assert_eq!(thread.runtime_provider, None);
    assert_eq!(thread.runtime_model, None);
}

#[test]
fn header_profile_resets_on_return_handoff_after_thread_detail_refresh() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-return".to_string(),
        title: "Return".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-return".to_string()));
    if let Some(thread) = model.chat.active_thread_mut() {
        thread.runtime_provider = Some("provider-weles-runtime".to_string());
        thread.runtime_model = Some("model-weles-runtime".to_string());
    }
    model.config.provider = "provider-svarog".to_string();
    model.config.model = "model-svarog".to_string();

    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-return".to_string(),
        title: "Return".to_string(),
        agent_name: Some("Swarog".to_string()),
        messages: vec![
            crate::wire::AgentMessage {
                role: crate::wire::MessageRole::System,
                content: "[[handoff_event]]{\"to_agent_id\":\"weles\",\"to_agent_name\":\"Weles\"}\nSwarog handed this thread to Weles.".to_string(),
                timestamp: 1,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
            crate::wire::AgentMessage {
                role: crate::wire::MessageRole::System,
                content: "[[handoff_event]]{\"to_agent_id\":\"svarog\",\"to_agent_name\":\"Swarog\"}\nWeles handed this thread to Swarog.".to_string(),
                timestamp: 2,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
        ],
        total_message_count: 2,
        loaded_message_start: 0,
        loaded_message_end: 2,
        created_at: 1,
        updated_at: 2,
        ..Default::default()
    });

    let profile = model.current_header_agent_profile();
    assert_eq!(profile.agent_label, "Swarog");
    assert_eq!(profile.provider, "provider-svarog");
    assert_eq!(profile.model, "model-svarog");
    let thread = model
        .chat
        .active_thread()
        .expect("thread should remain active");
    assert_eq!(thread.runtime_provider, None);
    assert_eq!(thread.runtime_model, None);
}

#[test]
fn header_usage_summary_caps_target_by_weles_compaction_window() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    model.config.auth_source = "github_copilot".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.context_window_tokens = 400_000;
    model.config.compact_threshold_pct = 80;
    model.config.compaction_strategy = "weles".to_string();
    model.config.compaction_weles_provider = "minimax-coding-plan".to_string();
    model.config.compaction_weles_model = "MiniMax-M2.7".to_string();

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-weles-target".to_string(),
        title: "Reviewer Target".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-weles-target".to_string(),
    ));

    let usage = model.current_header_usage_summary();
    assert_eq!(usage.compaction_target_tokens, 164_000);
    assert_eq!(usage.context_window_tokens, 400_000);
}

#[test]
fn header_usage_summary_caps_target_by_custom_compaction_window() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    model.config.auth_source = "github_copilot".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.context_window_tokens = 400_000;
    model.config.compact_threshold_pct = 80;
    model.config.compaction_strategy = "custom_model".to_string();
    model.config.compaction_custom_context_window_tokens = 160_000;

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-custom-target".to_string(),
        title: "Custom".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-custom-target".to_string(),
    ));

    let usage = model.current_header_usage_summary();
    assert_eq!(usage.compaction_target_tokens, 128_000);
    assert_eq!(usage.context_window_tokens, 400_000);
}

#[test]
fn header_usage_summary_does_not_estimate_after_compaction_artifact() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-compaction".to_string(),
        title: "Compaction".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-compaction".to_string(),
    ));

    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-compaction".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::User,
            content: "A".repeat(4_000),
            ..Default::default()
        },
    });
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-compaction".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "B".repeat(4_000),
            cost: Some(0.10),
            ..Default::default()
        },
    });

    let before = model.current_header_usage_summary();

    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-compaction".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "rule based".to_string(),
            message_kind: "compaction_artifact".to_string(),
            compaction_payload: Some("Older context compacted for continuity".to_string()),
            ..Default::default()
        },
    });
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-compaction".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "short follow-up".to_string(),
            cost: Some(0.15),
            ..Default::default()
        },
    });

    let after = model.current_header_usage_summary();
    assert_eq!(
        before.current_tokens, 0,
        "header should not estimate active context usage before daemon context state arrives"
    );
    assert_eq!(
        after.current_tokens, 0,
        "header should not estimate active context usage after compaction without daemon context state"
    );
    let total_cost = after
        .total_cost_usd
        .expect("header should include summed total cost after compaction");
    assert!(
        (total_cost - 0.25).abs() < 1e-9,
        "expected summed total cost to stay at 0.25, got {total_cost}"
    );
}

#[test]
fn header_usage_summary_ignores_loaded_messages_before_known_compaction_boundary() {
    let mut model = make_model();

    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-boundary".to_string(),
            title: "Boundary".to_string(),
            total_message_count: 4,
            loaded_message_start: 0,
            loaded_message_end: 4,
            active_compaction_window_start: Some(2),
            messages: vec![
                crate::state::chat::AgentMessage {
                    role: crate::state::chat::MessageRole::User,
                    content: "A".repeat(400),
                    ..Default::default()
                },
                crate::state::chat::AgentMessage {
                    role: crate::state::chat::MessageRole::Assistant,
                    content: "B".repeat(400),
                    ..Default::default()
                },
                crate::state::chat::AgentMessage {
                    role: crate::state::chat::MessageRole::Assistant,
                    content: "C".repeat(400),
                    ..Default::default()
                },
                crate::state::chat::AgentMessage {
                    role: crate::state::chat::MessageRole::User,
                    content: "D".repeat(400),
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    ));
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-boundary".to_string(),
    ));

    let usage = model.current_header_usage_summary();
    assert_eq!(
        usage.current_tokens, 0,
        "header should not estimate active context usage from loaded messages without daemon context state"
    );
}

#[test]
fn header_usage_summary_does_not_estimate_on_legacy_visible_compaction_artifact() {
    let mut model = make_model();

    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-legacy-compaction".to_string(),
        title: "Legacy Compaction".to_string(),
        total_message_count: 4,
        loaded_message_start: 0,
        loaded_message_end: 4,
        messages: vec![
            crate::wire::AgentMessage {
                role: crate::wire::MessageRole::User,
                content: "A".repeat(4_000),
                ..Default::default()
            },
            crate::wire::AgentMessage {
                role: crate::wire::MessageRole::Assistant,
                content: "B".repeat(4_000),
                ..Default::default()
            },
            crate::wire::AgentMessage {
                role: crate::wire::MessageRole::Assistant,
                content: "Pre-compaction context: ~842,460 / 400,000 tokens (threshold 320,000)\nTrigger: token-threshold\nStrategy: custom model generated summary.".to_string(),
                ..Default::default()
            },
            crate::wire::AgentMessage {
                role: crate::wire::MessageRole::Assistant,
                content: "short follow-up".to_string(),
                ..Default::default()
            },
        ],
        ..Default::default()
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-legacy-compaction".to_string(),
    ));

    let usage = model.current_header_usage_summary();
    assert_eq!(
        usage.current_tokens, 0,
        "legacy compaction artifacts should not drive a fallback header context estimate without daemon fields"
    );
}

#[test]
fn header_usage_summary_prefers_daemon_active_context_window_tokens() {
    let mut model = make_model();

    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-authoritative-context".to_string(),
        title: "Authoritative Context".to_string(),
        total_message_count: 4,
        loaded_message_start: 3,
        loaded_message_end: 4,
        active_context_window_start: Some(2),
        active_context_window_end: Some(4),
        active_context_window_tokens: Some(54),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::User,
            content: "C".repeat(80),
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-authoritative-context".to_string(),
    ));

    let usage = model.current_header_usage_summary();
    assert_eq!(
        usage.current_tokens, 54,
        "header should use daemon-calculated active context tokens instead of estimating the loaded page"
    );
}

#[test]
fn header_usage_summary_does_not_estimate_context_tokens_from_loaded_history() {
    let mut model = make_model();

    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-history-context".to_string(),
        title: "History Context".to_string(),
        total_message_count: 6,
        loaded_message_start: 4,
        loaded_message_end: 6,
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::Assistant,
            content: "latest".to_string(),
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-history-context".to_string(),
    ));

    let before = model.current_header_usage_summary();

    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-history-context".to_string(),
        title: "History Context".to_string(),
        total_message_count: 6,
        loaded_message_start: 0,
        loaded_message_end: 4,
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::User,
            content: "older ".repeat(8_000),
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    });

    let after = model.current_header_usage_summary();
    assert_eq!(before.current_tokens, 0);
    assert_eq!(
        after.current_tokens, before.current_tokens,
        "loading older history should not fake active context usage when daemon context state is absent"
    );
}

#[test]
fn header_usage_summary_uses_latest_daemon_turn_tokens_without_loaded_history_estimate() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-turn-usage".to_string(),
        title: "Turn Usage".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-turn-usage".to_string(),
    ));
    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-turn-usage".to_string(),
        content: "latest response".to_string(),
    });
    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-turn-usage".to_string(),
        input_tokens: 12_000,
        output_tokens: 3_000,
        cost: Some(0.25),
        provider: Some("openai".to_string()),
        model: Some("gpt-5.4".to_string()),
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: None,
    });

    let before = model.current_header_usage_summary();

    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-turn-usage".to_string(),
        title: "Turn Usage".to_string(),
        total_message_count: 4,
        loaded_message_start: 0,
        loaded_message_end: 2,
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::User,
            content: "older ".repeat(8_000),
            input_tokens: 80_000,
            output_tokens: 20_000,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    });

    let after = model.current_header_usage_summary();
    assert_eq!(
        before.current_tokens, 15_000,
        "header should use daemon-reported token usage from the latest completed turn"
    );
    assert_eq!(
        after.current_tokens, before.current_tokens,
        "loading older history should not replace current usage with older loaded message tokens"
    );
}

#[test]
fn internal_dm_thread_created_does_not_hijack_active_thread() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "dm:svarog:weles".to_string(),
        title: "Internal DM · Swarog ↔ WELES".to_string(),
        agent_name: None,
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
}

#[test]
fn hidden_handoff_thread_created_does_not_hijack_active_thread() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "handoff:thread-user:handoff-1".to_string(),
        title: "Handoff · Svarog -> Weles".to_string(),
        agent_name: None,
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert!(
        model
            .chat
            .threads()
            .iter()
            .all(|thread| thread.id != "handoff:thread-user:handoff-1"),
        "hidden handoff threads should not be added to visible chat state"
    );
}

#[test]
fn thread_created_event_preserves_agent_name_for_responder_fallback() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-weles".to_string(),
        title: "Governance".to_string(),
        agent_name: Some("Weles".to_string()),
    });

    let thread = model
        .chat
        .threads()
        .iter()
        .find(|thread| thread.id == "thread-weles")
        .expect("thread should be added to chat state");

    assert_eq!(thread.agent_name.as_deref(), Some("Weles"));
}

#[test]
fn hidden_handoff_thread_detail_is_ignored() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "handoff:thread-user:handoff-1".to_string(),
        title: "Handoff · Svarog -> Weles".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::System,
            content: "{\"kind\":\"thread_handoff_context\"}".to_string(),
            timestamp: 1,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert!(
        model
            .chat
            .threads()
            .iter()
            .all(|thread| thread.id != "handoff:thread-user:handoff-1"),
        "hidden handoff thread detail should not populate visible chat state"
    );
}

#[test]
fn hidden_handoff_threads_are_filtered_from_thread_list() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::ThreadList(vec![
        crate::wire::AgentThread {
            id: "thread-user".to_string(),
            title: "User Thread".to_string(),
            ..Default::default()
        },
        crate::wire::AgentThread {
            id: "handoff:thread-user:handoff-1".to_string(),
            title: "Handoff · Svarog -> Weles".to_string(),
            ..Default::default()
        },
    ]));

    let visible_ids: Vec<&str> = model
        .chat
        .threads()
        .iter()
        .map(|thread| thread.id.as_str())
        .collect();
    assert_eq!(visible_ids, vec!["thread-user"]);
}

#[test]
fn thread_list_requests_detail_for_selected_thread_with_only_summary_data() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 123;

    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        crate::state::chat::AgentThread {
            id: "thread-user".to_string(),
            title: "User Thread".to_string(),
            ..Default::default()
        },
    ]));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.handle_client_event(ClientEvent::ThreadList(vec![crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        ..Default::default()
    }]));

    assert_eq!(model.thread_loading_id.as_deref(), Some("thread-user"));
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        }) => {
            assert_eq!(thread_id, "thread-user");
            assert_eq!(message_limit, Some(123));
            assert_eq!(message_offset, Some(0));
        }
        other => panic!("expected thread detail request, got {other:?}"),
    }
}

#[test]
fn thread_detail_clears_loading_state() {
    let mut model = make_model();
    model.thread_loading_id = Some("thread-user".to_string());

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::Assistant,
            content: "Loaded".to_string(),
            timestamp: 1,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    assert!(model.thread_loading_id.is_none());
}

#[test]
fn workspace_task_update_does_not_reopen_loading_after_empty_thread_detail_arrives() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 77;
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "workspace-thread:task-1".to_string(),
            title: "Workspace Task".to_string(),
            messages: Vec::new(),
            ..Default::default()
        },
    ));
    model.chat.reduce(chat::ChatAction::SelectThread(
        "workspace-thread:task-1".to_string(),
    ));
    model.thread_loading_id = Some("workspace-thread:task-1".to_string());

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "workspace-thread:task-1".to_string(),
        title: "Workspace Task".to_string(),
        messages: Vec::new(),
        created_at: 1,
        updated_at: 1,
        total_message_count: 0,
        loaded_message_start: 0,
        loaded_message_end: 0,
        ..Default::default()
    })));
    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::WorkspaceTaskUpdated(
        zorai_protocol::WorkspaceTask {
            id: "task-1".to_string(),
            workspace_id: "main".to_string(),
            title: "Workspace Task".to_string(),
            task_type: zorai_protocol::WorkspaceTaskType::Thread,
            description: "Description".to_string(),
            definition_of_done: None,
            priority: zorai_protocol::WorkspacePriority::Low,
            status: zorai_protocol::WorkspaceTaskStatus::InProgress,
            sort_order: 1,
            reporter: zorai_protocol::WorkspaceActor::User,
            assignee: Some(zorai_protocol::WorkspaceActor::Agent(
                zorai_protocol::AGENT_ID_SWAROG.to_string(),
            )),
            reviewer: Some(zorai_protocol::WorkspaceActor::User),
            thread_id: Some("workspace-thread:task-1".to_string()),
            goal_run_id: None,
            runtime_history: Vec::new(),
            created_at: 1,
            updated_at: 2,
            started_at: Some(2),
            completed_at: None,
            deleted_at: None,
            last_notice_id: None,
        },
    ));

    assert_eq!(next_thread_request(&mut daemon_rx), None);
    assert!(
        model.thread_loading_id.is_none(),
        "workspace sync after an empty detail should not put the open thread back into loading"
    );
}

#[test]
fn missing_thread_detail_clears_active_loading_state_and_refreshes() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "workspace-thread:task-1".to_string(),
            title: "Workspace Task".to_string(),
            messages: Vec::new(),
            ..Default::default()
        },
    ));
    model.chat.reduce(chat::ChatAction::SelectThread(
        "workspace-thread:task-1".to_string(),
    ));
    model.thread_loading_id = Some("workspace-thread:task-1".to_string());

    model.handle_client_event(ClientEvent::ThreadDetail(None));

    assert!(model.thread_loading_id.is_none());
    assert!(
        std::iter::from_fn(|| daemon_rx.try_recv().ok())
            .any(|command| matches!(command, DaemonCommand::Refresh)),
        "a missing active thread should trigger a daemon refresh"
    );

    model.handle_client_event(ClientEvent::ThreadList(Vec::new()));
    assert_eq!(
        model.chat.active_thread_id(),
        Some("workspace-thread:task-1"),
        "refreshes before hydration should not drop the workspace runtime placeholder"
    );
}

#[test]
fn workspace_task_update_retries_empty_active_runtime_thread() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 77;
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "workspace-thread:task-1".to_string(),
            title: "Workspace Task".to_string(),
            messages: Vec::new(),
            ..Default::default()
        },
    ));
    model.chat.reduce(chat::ChatAction::SelectThread(
        "workspace-thread:task-1".to_string(),
    ));

    model.handle_client_event(ClientEvent::WorkspaceTaskUpdated(
        zorai_protocol::WorkspaceTask {
            id: "task-1".to_string(),
            workspace_id: "main".to_string(),
            title: "Workspace Task".to_string(),
            task_type: zorai_protocol::WorkspaceTaskType::Thread,
            description: "Description".to_string(),
            definition_of_done: None,
            priority: zorai_protocol::WorkspacePriority::Low,
            status: zorai_protocol::WorkspaceTaskStatus::InProgress,
            sort_order: 1,
            reporter: zorai_protocol::WorkspaceActor::User,
            assignee: Some(zorai_protocol::WorkspaceActor::Agent(
                zorai_protocol::AGENT_ID_SWAROG.to_string(),
            )),
            reviewer: Some(zorai_protocol::WorkspaceActor::User),
            thread_id: Some("workspace-thread:task-1".to_string()),
            goal_run_id: None,
            runtime_history: Vec::new(),
            created_at: 1,
            updated_at: 2,
            started_at: Some(2),
            completed_at: None,
            deleted_at: None,
            last_notice_id: None,
        },
    ));

    assert_eq!(
        next_thread_request(&mut daemon_rx),
        Some(("workspace-thread:task-1".to_string(), Some(77), Some(0)))
    );
    assert_eq!(
        model.thread_loading_id.as_deref(),
        Some("workspace-thread:task-1")
    );
}

#[test]
fn workspace_task_update_does_not_retry_runtime_thread_after_missing_detail() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 77;
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "workspace-thread:task-1".to_string(),
            title: "Workspace Task".to_string(),
            messages: Vec::new(),
            ..Default::default()
        },
    ));
    model.chat.reduce(chat::ChatAction::SelectThread(
        "workspace-thread:task-1".to_string(),
    ));
    model.thread_loading_id = Some("workspace-thread:task-1".to_string());

    model.handle_client_event(ClientEvent::ThreadDetail(None));
    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::WorkspaceTaskUpdated(
        zorai_protocol::WorkspaceTask {
            id: "task-1".to_string(),
            workspace_id: "main".to_string(),
            title: "Workspace Task".to_string(),
            task_type: zorai_protocol::WorkspaceTaskType::Thread,
            description: "Description".to_string(),
            definition_of_done: None,
            priority: zorai_protocol::WorkspacePriority::Low,
            status: zorai_protocol::WorkspaceTaskStatus::InProgress,
            sort_order: 1,
            reporter: zorai_protocol::WorkspaceActor::User,
            assignee: Some(zorai_protocol::WorkspaceActor::Agent(
                zorai_protocol::AGENT_ID_SWAROG.to_string(),
            )),
            reviewer: Some(zorai_protocol::WorkspaceActor::User),
            thread_id: Some("workspace-thread:task-1".to_string()),
            goal_run_id: None,
            runtime_history: Vec::new(),
            created_at: 1,
            updated_at: 2,
            started_at: Some(2),
            completed_at: None,
            deleted_at: None,
            last_notice_id: None,
        },
    ));

    assert_eq!(next_thread_request(&mut daemon_rx), None);
    assert!(
        model.thread_loading_id.is_none(),
        "missing workspace runtime thread should not be put back into loading"
    );
}

#[test]
fn created_runtime_thread_after_missing_detail_retries_active_workspace_thread() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 77;
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "workspace-thread:task-1".to_string(),
            title: "Workspace Task".to_string(),
            messages: Vec::new(),
            ..Default::default()
        },
    ));
    model.chat.reduce(chat::ChatAction::SelectThread(
        "workspace-thread:task-1".to_string(),
    ));
    model.thread_loading_id = Some("workspace-thread:task-1".to_string());

    model.handle_client_event(ClientEvent::ThreadDetail(None));
    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "workspace-thread:task-1".to_string(),
        title: "Workspace Task".to_string(),
        agent_name: Some("Svarog".to_string()),
    });

    assert_eq!(
        next_thread_request(&mut daemon_rx),
        Some(("workspace-thread:task-1".to_string(), Some(77), Some(0)))
    );
    assert_eq!(
        model.thread_loading_id.as_deref(),
        Some("workspace-thread:task-1")
    );
}

#[test]
fn on_tick_requests_next_older_thread_page_when_scrolled_to_top_of_loaded_window() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 123;
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-user".to_string(),
            title: "User Thread".to_string(),
            total_message_count: 120,
            loaded_message_start: 20,
            loaded_message_end: 120,
            messages: (20..120)
                .map(|index| crate::state::chat::AgentMessage {
                    id: Some(format!("msg-{index}")),
                    role: crate::state::chat::MessageRole::Assistant,
                    content: format!("msg {index}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        },
    ));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));
    model
        .chat
        .reduce(chat::ChatAction::ScrollChat(i32::MAX / 2));

    model.on_tick();

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        }) => {
            assert_eq!(thread_id, "thread-user");
            assert_eq!(message_limit, Some(123));
            assert_eq!(message_offset, Some(100));
        }
        other => panic!("expected older-page request, got {other:?}"),
    }
}

#[test]
fn on_tick_refreshes_spawned_sidebar_tasks_on_cooldown() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-parent".to_string(),
            title: "Parent Thread".to_string(),
            ..Default::default()
        },
    ));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-parent".to_string()));
    model
        .tasks
        .reduce(task::TaskAction::TaskListReceived(vec![task::AgentTask {
            id: "task-child".to_string(),
            title: "Spawned child".to_string(),
            description: "Spawned child task".to_string(),
            thread_id: Some("thread-child".to_string()),
            parent_task_id: Some("task-parent".to_string()),
            parent_thread_id: Some("thread-parent".to_string()),
            created_at: 1,
            status: Some(task::TaskStatus::InProgress),
            progress: 30,
            session_id: None,
            goal_run_id: None,
            goal_step_title: None,
            command: None,
            awaiting_approval_id: None,
            blocked_reason: None,
        }]));
    model.activate_sidebar_tab(SidebarTab::Spawned);

    model.on_tick();
    assert!(
        saw_list_tasks_command(&mut daemon_rx),
        "spawned sidebar should refresh as soon as the cooldown is eligible"
    );

    for _ in 0..19 {
        model.on_tick();
    }

    assert!(
        !saw_list_tasks_command(&mut daemon_rx),
        "spawned sidebar should not refresh again before the cooldown elapses"
    );

    model.on_tick();
    assert!(
        saw_list_tasks_command(&mut daemon_rx),
        "spawned sidebar should request another task refresh once the cooldown elapses"
    );
}

#[test]
fn on_tick_does_not_refresh_spawned_sidebar_tasks_while_thread_is_loading() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-parent".to_string(),
            title: "Parent Thread".to_string(),
            ..Default::default()
        },
    ));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-parent".to_string()));
    model
        .tasks
        .reduce(task::TaskAction::TaskListReceived(vec![task::AgentTask {
            id: "task-child".to_string(),
            title: "Spawned child".to_string(),
            description: "Spawned child task".to_string(),
            thread_id: Some("thread-child".to_string()),
            parent_task_id: Some("task-parent".to_string()),
            parent_thread_id: Some("thread-parent".to_string()),
            created_at: 1,
            status: Some(task::TaskStatus::InProgress),
            progress: 30,
            session_id: None,
            goal_run_id: None,
            goal_step_title: None,
            command: None,
            awaiting_approval_id: None,
            blocked_reason: None,
        }]));
    model.activate_sidebar_tab(SidebarTab::Spawned);
    model.thread_loading_id = Some("thread-parent".to_string());

    for _ in 0..25 {
        model.on_tick();
    }

    assert!(
        !saw_list_tasks_command(&mut daemon_rx),
        "spawned sidebar refresh should stay idle while the active thread is still loading"
    );
}

#[test]
fn on_tick_debounces_follow_up_older_thread_page_requests_after_reload() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 123;
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-user".to_string(),
            title: "User Thread".to_string(),
            total_message_count: 120,
            loaded_message_start: 20,
            loaded_message_end: 120,
            messages: (20..120)
                .map(|index| crate::state::chat::AgentMessage {
                    id: Some(format!("msg-{index}")),
                    role: crate::state::chat::MessageRole::Assistant,
                    content: format!("msg {index}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        },
    ));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));
    model
        .chat
        .reduce(chat::ChatAction::ScrollChat(i32::MAX / 2));

    model.on_tick();

    match next_thread_request(&mut daemon_rx) {
        Some((thread_id, message_limit, message_offset)) => {
            assert_eq!(thread_id, "thread-user");
            assert_eq!(message_limit, Some(123));
            assert_eq!(message_offset, Some(100));
        }
        other => panic!("expected first older-page request, got {other:?}"),
    }

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        total_message_count: 240,
        loaded_message_start: 5,
        loaded_message_end: 128,
        messages: (5..128)
            .map(|index| crate::wire::AgentMessage {
                id: Some(format!("msg-{index}")),
                role: crate::wire::MessageRole::Assistant,
                content: format!("msg {index}"),
                timestamp: index as u64,
                message_kind: "normal".to_string(),
                ..Default::default()
            })
            .collect(),
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    model.on_tick();
    assert!(
        next_thread_request(&mut daemon_rx).is_none(),
        "top-of-window reload should debounce follow-up history fetches"
    );

    for _ in 0..(chat::CHAT_HISTORY_FETCH_DEBOUNCE_TICKS - 1) {
        model.on_tick();
    }

    match next_thread_request(&mut daemon_rx) {
        Some((thread_id, message_limit, message_offset)) => {
            assert_eq!(thread_id, "thread-user");
            assert_eq!(message_limit, Some(123));
            assert_eq!(message_offset, Some(235));
        }
        other => panic!("expected debounced older-page request after cooldown, got {other:?}"),
    }
}

#[test]
fn prepending_older_history_releases_the_top_edge_until_user_scrolls_again() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 123;
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-user".to_string(),
            title: "User Thread".to_string(),
            total_message_count: 400,
            loaded_message_start: 277,
            loaded_message_end: 400,
            messages: (277..400)
                .map(|index| crate::state::chat::AgentMessage {
                    id: Some(format!("msg-{index}")),
                    role: crate::state::chat::MessageRole::Assistant,
                    content: format!("msg {index}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        },
    ));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));
    model
        .chat
        .reduce(chat::ChatAction::ScrollChat(i32::MAX / 2));

    model.on_tick();

    match next_thread_request(&mut daemon_rx) {
        Some((thread_id, message_limit, message_offset)) => {
            assert_eq!(thread_id, "thread-user");
            assert_eq!(message_limit, Some(123));
            assert_eq!(message_offset, Some(123));
        }
        other => panic!("expected first older-page request, got {other:?}"),
    }

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        total_message_count: 400,
        loaded_message_start: 154,
        loaded_message_end: 277,
        messages: (154..277)
            .map(|index| crate::wire::AgentMessage {
                id: Some(format!("msg-{index}")),
                role: crate::wire::MessageRole::Assistant,
                content: format!("msg {index}"),
                timestamp: index as u64,
                message_kind: "normal".to_string(),
                ..Default::default()
            })
            .collect(),
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    for _ in 0..chat::CHAT_HISTORY_FETCH_DEBOUNCE_TICKS {
        model.on_tick();
    }

    assert!(
        next_thread_request(&mut daemon_rx).is_none(),
        "prepend anchor should move the viewport below the new top so history does not auto-fetch again"
    );
}

#[test]
fn on_tick_requests_next_older_goal_run_page_when_scrolled_to_top_of_loaded_window() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Paged Goal".to_string(),
            loaded_step_start: 20,
            loaded_step_end: 40,
            total_step_count: 40,
            loaded_event_start: 60,
            loaded_event_end: 120,
            total_event_count: 120,
            steps: (20..40)
                .map(|idx| task::GoalRunStep {
                    id: format!("step-{idx}"),
                    title: format!("Step {idx}"),
                    instructions: format!("instructions {idx}"),
                    order: idx as u32,
                    ..Default::default()
                })
                .collect(),
            events: (60..120)
                .map(|idx| task::GoalRunEvent {
                    id: format!("event-{idx}"),
                    message: format!("event {idx}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }));

    model.on_tick();

    match next_goal_run_page_request(&mut daemon_rx) {
        Some((goal_run_id, step_offset, step_limit, event_offset, event_limit)) => {
            assert_eq!(goal_run_id, "goal-1");
            assert_eq!(step_offset, Some(0));
            assert_eq!(step_limit, Some(20));
            assert_eq!(event_offset, Some(0));
            assert_eq!(event_limit, Some(60));
        }
        other => panic!("expected older goal-run page request, got {other:?}"),
    }
}

#[test]
fn prepending_older_goal_run_history_releases_top_edge_until_user_scrolls_again() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Paged Goal".to_string(),
            loaded_step_start: 20,
            loaded_step_end: 40,
            total_step_count: 40,
            loaded_event_start: 60,
            loaded_event_end: 120,
            total_event_count: 120,
            steps: (20..40)
                .map(|idx| task::GoalRunStep {
                    id: format!("step-{idx}"),
                    title: format!("Step {idx}"),
                    instructions: format!("instructions {idx}"),
                    order: idx as u32,
                    ..Default::default()
                })
                .collect(),
            events: (60..120)
                .map(|idx| task::GoalRunEvent {
                    id: format!("event-{idx}"),
                    message: format!("event {idx}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }));

    model.on_tick();

    match next_goal_run_page_request(&mut daemon_rx) {
        Some((goal_run_id, step_offset, step_limit, event_offset, event_limit)) => {
            assert_eq!(goal_run_id, "goal-1");
            assert_eq!(step_offset, Some(0));
            assert_eq!(step_limit, Some(20));
            assert_eq!(event_offset, Some(0));
            assert_eq!(event_limit, Some(60));
        }
        other => panic!("expected initial older goal-run page request, got {other:?}"),
    }

    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Paged Goal".to_string(),
            loaded_step_start: 0,
            loaded_step_end: 20,
            total_step_count: 40,
            loaded_event_start: 0,
            loaded_event_end: 60,
            total_event_count: 120,
            steps: (0..20)
                .map(|idx| task::GoalRunStep {
                    id: format!("step-{idx}"),
                    title: format!("Step {idx}"),
                    instructions: format!("instructions {idx}"),
                    order: idx as u32,
                    ..Default::default()
                })
                .collect(),
            events: (0..60)
                .map(|idx| task::GoalRunEvent {
                    id: format!("event-{idx}"),
                    message: format!("event {idx}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }));
    model.handle_goal_run_detail_event(crate::wire::GoalRun {
        id: "goal-1".to_string(),
        title: "Paged Goal".to_string(),
        loaded_step_start: 0,
        loaded_step_end: 20,
        total_step_count: 40,
        loaded_event_start: 0,
        loaded_event_end: 60,
        total_event_count: 120,
        steps: (0..20)
            .map(|idx| crate::wire::GoalRunStep {
                id: format!("step-{idx}"),
                position: idx,
                title: format!("Step {idx}"),
                instructions: format!("instructions {idx}"),
                ..Default::default()
            })
            .collect(),
        events: (0..60)
            .map(|idx| crate::wire::GoalRunEvent {
                id: format!("event-{idx}"),
                message: format!("event {idx}"),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    });

    model.on_tick();

    assert!(
        next_goal_run_page_request(&mut daemon_rx).is_none(),
        "prepend anchor should move the viewport below the new top so goal history does not auto-fetch again"
    );
}

#[test]
fn active_goal_run_update_schedules_background_hydration_instead_of_immediate_refresh() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    model.handle_goal_run_update_event(crate::wire::GoalRun {
        id: "goal-1".to_string(),
        title: "Goal One".to_string(),
        status: Some(crate::wire::GoalRunStatus::Running),
        ..Default::default()
    });

    assert_eq!(
        next_goal_hydration_schedule(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );
    assert!(
        next_goal_run_detail_request(&mut daemon_rx).is_none(),
        "active-goal updates should no longer emit immediate detail refreshes"
    );
    assert!(
        next_goal_run_checkpoints_request(&mut daemon_rx).is_none(),
        "active-goal updates should no longer emit immediate checkpoint refreshes"
    );
}

#[test]
fn repeated_active_goal_updates_only_schedule_background_hydration_once_per_burst() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    model.handle_goal_run_update_event(crate::wire::GoalRun {
        id: "goal-1".to_string(),
        title: "Goal One".to_string(),
        status: Some(crate::wire::GoalRunStatus::Running),
        ..Default::default()
    });
    model.handle_goal_run_update_event(crate::wire::GoalRun {
        id: "goal-1".to_string(),
        title: "Goal One".to_string(),
        status: Some(crate::wire::GoalRunStatus::Running),
        ..Default::default()
    });

    assert_eq!(
        next_goal_hydration_schedule(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );
    assert!(
        next_goal_hydration_schedule(&mut daemon_rx).is_none(),
        "duplicate active-goal updates should coalesce into one pending hydration request"
    );
}

#[test]
fn active_goal_hydration_reschedules_after_authoritative_detail_arrives() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    model.handle_goal_run_update_event(crate::wire::GoalRun {
        id: "goal-1".to_string(),
        title: "Goal One".to_string(),
        status: Some(crate::wire::GoalRunStatus::Running),
        ..Default::default()
    });
    assert_eq!(
        next_goal_hydration_schedule(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );

    model.handle_goal_run_detail_event(crate::wire::GoalRun {
        id: "goal-1".to_string(),
        title: "Goal One".to_string(),
        ..Default::default()
    });
    model.handle_goal_run_update_event(crate::wire::GoalRun {
        id: "goal-1".to_string(),
        title: "Goal One".to_string(),
        status: Some(crate::wire::GoalRunStatus::Running),
        ..Default::default()
    });

    assert_eq!(
        next_goal_hydration_schedule(&mut daemon_rx).as_deref(),
        Some("goal-1"),
        "authoritative detail should clear the pending marker so later updates can reschedule"
    );
}

#[test]
fn goal_detail_placeholder_clears_pending_hydration_for_the_requested_goal() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    model.handle_goal_run_update_event(crate::wire::GoalRun {
        id: "goal-1".to_string(),
        title: "Goal One".to_string(),
        status: Some(crate::wire::GoalRunStatus::Running),
        ..Default::default()
    });
    assert_eq!(
        next_goal_hydration_schedule(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );
    assert!(
        model.pending_goal_hydration_refreshes.contains("goal-1"),
        "update should leave the goal pending until an authoritative response arrives"
    );

    model.schedule_goal_hydration_refresh("goal-2".to_string());
    assert_eq!(
        next_goal_hydration_schedule(&mut daemon_rx).as_deref(),
        Some("goal-2")
    );
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-2".to_string(),
        step_id: None,
    });

    model.handle_client_event(ClientEvent::GoalRunDetail(Some(crate::wire::GoalRun {
        id: "goal-1".to_string(),
        ..Default::default()
    })));

    assert!(
        !model.pending_goal_hydration_refreshes.contains("goal-1"),
        "placeholder detail should clear the stale pending hydration marker for the requested goal"
    );
    assert!(
        model.pending_goal_hydration_refreshes.contains("goal-2"),
        "changing panes before the empty response arrives should not clear the wrong goal"
    );
}

#[test]
fn goal_run_list_refresh_reconciles_pending_hydration_against_present_goals() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.schedule_goal_hydration_refresh("goal-1".to_string());
    model.schedule_goal_hydration_refresh("goal-2".to_string());
    assert_eq!(
        next_goal_hydration_schedule(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );
    assert_eq!(
        next_goal_hydration_schedule(&mut daemon_rx).as_deref(),
        Some("goal-2")
    );
    assert_eq!(model.pending_goal_hydration_refreshes.len(), 2);

    model.handle_goal_run_list_event(vec![crate::wire::GoalRun {
        id: "goal-2".to_string(),
        title: "Goal Two".to_string(),
        status: Some(crate::wire::GoalRunStatus::Running),
        ..Default::default()
    }]);

    assert!(
        !model.pending_goal_hydration_refreshes.contains("goal-1"),
        "goal list refresh should drop pending hydration for goals no longer present"
    );
    assert!(
        model.pending_goal_hydration_refreshes.contains("goal-2"),
        "goal list refresh should preserve pending hydration for goals still present"
    );
}

#[test]
fn empty_goal_checkpoint_refresh_clears_pending_hydration_for_requested_goal() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();

    model.schedule_goal_hydration_refresh("goal-1".to_string());
    model.handle_client_event(ClientEvent::GoalRunCheckpoints {
        goal_run_id: "goal-1".to_string(),
        checkpoints: vec![],
    });

    assert!(
        !model.pending_goal_hydration_refreshes.contains("goal-1"),
        "empty checkpoint lists should still clear the exact pending goal hydration marker"
    );
}

#[test]
fn goal_hydration_schedule_failed_clears_pending_marker_via_client_event() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();

    model.schedule_goal_hydration_refresh("goal-1".to_string());
    model.handle_client_event(ClientEvent::GoalHydrationScheduleFailed {
        goal_run_id: "goal-1".to_string(),
    });

    assert!(
        !model.pending_goal_hydration_refreshes.contains("goal-1"),
        "background hydration failure should clear the exact pending marker"
    );
}

#[test]
fn goal_scoped_thread_todos_for_active_goal_schedule_background_hydration() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.handle_goal_run_detail_event(crate::wire::GoalRun {
        id: "goal-1".to_string(),
        title: "Goal One".to_string(),
        status: Some(crate::wire::GoalRunStatus::Running),
        steps: vec![crate::wire::GoalRunStep {
            id: "step-1".to_string(),
            position: 0,
            title: "Plan".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    });
    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-1".to_string(),
        title: "Child task".to_string(),
        thread_id: Some("thread-task".to_string()),
        goal_run_id: Some("goal-1".to_string()),
        ..Default::default()
    }]);
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    model.handle_client_event(ClientEvent::ThreadTodos {
        thread_id: "thread-task".to_string(),
        goal_run_id: Some("goal-1".to_string()),
        step_index: Some(0),
        items: vec![crate::wire::TodoItem {
            id: "todo-1".to_string(),
            content: "child thread todo".to_string(),
            status: Some(crate::wire::TodoStatus::InProgress),
            position: 0,
            step_index: Some(0),
            ..Default::default()
        }],
    });

    assert_eq!(
        next_goal_hydration_schedule(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );
    assert!(
        model.pending_goal_hydration_refreshes.contains("goal-1"),
        "goal-scoped todo updates should arm background hydration for the visible goal"
    );
}

#[test]
fn work_context_for_active_goal_linked_thread_schedules_background_hydration() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.handle_goal_run_detail_event(crate::wire::GoalRun {
        id: "goal-1".to_string(),
        title: "Goal One".to_string(),
        status: Some(crate::wire::GoalRunStatus::Running),
        steps: vec![crate::wire::GoalRunStep {
            id: "step-1".to_string(),
            position: 0,
            title: "Plan".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    });
    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-1".to_string(),
        title: "Child task".to_string(),
        thread_id: Some("thread-task".to_string()),
        goal_run_id: Some("goal-1".to_string()),
        ..Default::default()
    }]);
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    model.handle_work_context_event(crate::wire::ThreadWorkContext {
        thread_id: "thread-task".to_string(),
        entries: vec![crate::wire::WorkContextEntry {
            path: "/tmp/skill_injection_gap.md".to_string(),
            goal_run_id: Some("goal-1".to_string()),
            step_index: Some(0),
            is_text: true,
            ..Default::default()
        }],
    });

    assert_eq!(
        next_goal_hydration_schedule(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );
    assert!(
        model.pending_goal_hydration_refreshes.contains("goal-1"),
        "work-context updates should arm background hydration for the visible goal"
    );
}

#[test]
fn active_goal_run_goal_refresh_preserves_or_clamps_goal_sidebar_selection() {
    let mut model = active_goal_run_sidebar_model();
    model.activate_goal_sidebar_tab(GoalSidebarTab::Tasks);
    model.select_goal_sidebar_row(1);
    assert_eq!(model.goal_sidebar.selected_row(), 1);

    model.handle_goal_run_update_event(crate::wire::GoalRun {
        id: "goal-1".to_string(),
        title: "Goal Title".to_string(),
        thread_id: Some("thread-1".to_string()),
        child_task_ids: vec!["task-1".to_string(), "task-2".to_string()],
        status: Some(crate::wire::GoalRunStatus::Running),
        ..Default::default()
    });

    assert_eq!(
        model.goal_sidebar.selected_row(),
        1,
        "goal updates should preserve the selected goal-sidebar row when it stays valid"
    );

    model.handle_goal_run_detail_event(crate::wire::GoalRun {
        id: "goal-1".to_string(),
        title: "Goal Title".to_string(),
        thread_id: Some("thread-1".to_string()),
        child_task_ids: vec!["task-1".to_string()],
        steps: vec![
            crate::wire::GoalRunStep {
                id: "step-1".to_string(),
                position: 0,
                title: "Plan".to_string(),
                ..Default::default()
            },
            crate::wire::GoalRunStep {
                id: "step-2".to_string(),
                position: 1,
                title: "Implement".to_string(),
                ..Default::default()
            },
        ],
        ..Default::default()
    });

    assert_eq!(
        model.goal_sidebar.selected_row(),
        0,
        "goal detail refresh should clamp the goal-sidebar row when the previous selection no longer exists"
    );
}

#[test]
fn active_goal_run_checkpoint_task_file_tabs_clamp_after_refreshes() {
    let mut model = active_goal_run_sidebar_model();

    model.activate_goal_sidebar_tab(GoalSidebarTab::Checkpoints);
    model.select_goal_sidebar_row(1);
    model.handle_goal_run_checkpoints_event(
        "goal-1".to_string(),
        vec![crate::wire::CheckpointSummary {
            id: "checkpoint-1".to_string(),
            checkpoint_type: "plan".to_string(),
            step_index: Some(0),
            context_summary_preview: Some("Checkpoint for Plan".to_string()),
            ..Default::default()
        }],
    );
    assert_eq!(model.goal_sidebar.selected_row(), 0);

    model.activate_goal_sidebar_tab(GoalSidebarTab::Tasks);
    model.select_goal_sidebar_row(1);
    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-1".to_string(),
        title: "Child Task One".to_string(),
        thread_id: Some("thread-1".to_string()),
        goal_run_id: Some("goal-1".to_string()),
        created_at: 10,
        ..Default::default()
    }]);
    assert_eq!(model.goal_sidebar.selected_row(), 0);

    model.activate_goal_sidebar_tab(GoalSidebarTab::Files);
    model.select_goal_sidebar_row(1);
    model.handle_work_context_event(crate::wire::ThreadWorkContext {
        thread_id: "thread-1".to_string(),
        entries: vec![crate::wire::WorkContextEntry {
            path: "/tmp/plan.md".to_string(),
            goal_run_id: Some("goal-1".to_string()),
            is_text: true,
            ..Default::default()
        }],
    });
    assert_eq!(model.goal_sidebar.selected_row(), 0);
}

#[test]
fn active_goal_run_selecting_step_in_main_pane_updates_goal_sidebar_highlight() {
    let mut model = active_goal_run_sidebar_model();
    model.activate_goal_sidebar_tab(GoalSidebarTab::Steps);
    model.select_goal_sidebar_row(0);

    assert!(model.select_goal_step_in_active_run("step-3".to_string()));

    assert_eq!(model.goal_sidebar.selected_row(), 2);
    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::GoalRun {
            ref goal_run_id,
            step_id: Some(ref step_id),
        }) if goal_run_id == "goal-1" && step_id == "step-3"
    ));
}

#[test]
fn active_goal_run_task_tab_preserves_selected_task_across_reorder_refresh() {
    let mut model = active_goal_run_sidebar_model();
    model.activate_goal_sidebar_tab(GoalSidebarTab::Tasks);
    model.select_goal_sidebar_row(1);

    model.handle_goal_run_detail_event(crate::wire::GoalRun {
        id: "goal-1".to_string(),
        title: "Goal Title".to_string(),
        thread_id: Some("thread-1".to_string()),
        child_task_ids: vec!["task-2".to_string(), "task-1".to_string()],
        steps: vec![
            crate::wire::GoalRunStep {
                id: "step-1".to_string(),
                position: 0,
                title: "Plan".to_string(),
                ..Default::default()
            },
            crate::wire::GoalRunStep {
                id: "step-2".to_string(),
                position: 1,
                title: "Implement".to_string(),
                ..Default::default()
            },
            crate::wire::GoalRunStep {
                id: "step-3".to_string(),
                position: 2,
                title: "Verify".to_string(),
                ..Default::default()
            },
        ],
        ..Default::default()
    });

    assert_eq!(
        model.goal_sidebar.selected_row(),
        0,
        "task-tab selection should follow the same task id when child task order changes"
    );
}

#[test]
fn active_goal_run_file_tab_restores_selected_file_after_partial_refresh() {
    let mut model = active_goal_run_sidebar_model();
    model.activate_goal_sidebar_tab(GoalSidebarTab::Files);
    model.select_goal_sidebar_row(1);

    model.handle_work_context_event(crate::wire::ThreadWorkContext {
        thread_id: "thread-1".to_string(),
        entries: vec![crate::wire::WorkContextEntry {
            path: "/tmp/plan.md".to_string(),
            goal_run_id: Some("goal-1".to_string()),
            is_text: true,
            ..Default::default()
        }],
    });
    assert_eq!(model.goal_sidebar.selected_row(), 0);

    model.handle_work_context_event(crate::wire::ThreadWorkContext {
        thread_id: "thread-1".to_string(),
        entries: vec![
            crate::wire::WorkContextEntry {
                path: "/tmp/plan.md".to_string(),
                goal_run_id: Some("goal-1".to_string()),
                is_text: true,
                ..Default::default()
            },
            crate::wire::WorkContextEntry {
                path: "/tmp/report.md".to_string(),
                goal_run_id: Some("goal-1".to_string()),
                is_text: true,
                ..Default::default()
            },
        ],
    });

    assert_eq!(
        model.goal_sidebar.selected_row(),
        1,
        "file-tab selection should restore the same path when a transient partial refresh drops it temporarily"
    );
}

#[test]
fn sidebar_enter_on_offscreen_pinned_message_requests_containing_page() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 100;
    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        messages: (200..300)
            .map(|index| crate::wire::AgentMessage {
                id: Some(format!("msg-{index}")),
                role: crate::wire::MessageRole::Assistant,
                content: format!("msg {index}"),
                timestamp: index as u64,
                message_kind: "normal".to_string(),
                ..Default::default()
            })
            .collect(),
        pinned_messages: vec![crate::wire::PinnedThreadMessage {
            message_id: "msg-50".to_string(),
            absolute_index: 50,
            role: crate::wire::MessageRole::User,
            content: "Pinned offscreen".to_string(),
        }],
        total_message_count: 300,
        loaded_message_start: 200,
        loaded_message_end: 300,
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));
    model.focus = FocusArea::Sidebar;
    model
        .sidebar
        .reduce(crate::state::sidebar::SidebarAction::SwitchTab(
            crate::state::sidebar::SidebarTab::Pinned,
        ));

    model.handle_sidebar_enter();

    match next_thread_request(&mut daemon_rx) {
        Some((thread_id, message_limit, message_offset)) => {
            assert_eq!(thread_id, "thread-user");
            assert_eq!(message_limit, Some(100));
            assert_eq!(message_offset, Some(200));
        }
        other => panic!("expected containing-page request, got {other:?}"),
    }
}

#[test]
fn thread_detail_preserves_message_author_metadata() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::Assistant,
            content: "Visible participant post.".to_string(),
            author_agent_id: Some("weles".to_string()),
            author_agent_name: Some("Weles".to_string()),
            timestamp: 1,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    let thread = model
        .chat
        .threads()
        .iter()
        .find(|thread| thread.id == "thread-user")
        .expect("thread detail should populate chat state");
    let message = thread
        .messages
        .first()
        .expect("thread should contain message");
    assert_eq!(message.author_agent_id.as_deref(), Some("weles"));
    assert_eq!(message.author_agent_name.as_deref(), Some("Weles"));
}

#[test]
fn internal_dm_threads_are_retained_in_thread_list_for_internal_picker() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::ThreadList(vec![
        crate::wire::AgentThread {
            id: "thread-user".to_string(),
            title: "User Thread".to_string(),
            ..Default::default()
        },
        crate::wire::AgentThread {
            id: "dm:svarog:weles".to_string(),
            title: "Internal DM · Svarog ↔ WELES".to_string(),
            ..Default::default()
        },
    ]));

    let visible_ids: Vec<&str> = model
        .chat
        .threads()
        .iter()
        .map(|thread| thread.id.as_str())
        .collect();
    assert_eq!(visible_ids, vec!["thread-user", "dm:svarog:weles"]);
}

#[test]
fn hidden_handoff_thread_reload_required_is_ignored() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "handoff:thread-user:handoff-1".to_string(),
    });

    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn active_thread_reload_required_requests_detail_and_sidebar_context() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 123;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "thread-user".to_string(),
    });

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        }) => {
            assert_eq!(thread_id, "thread-user");
            assert_eq!(message_limit, Some(123));
            assert_eq!(message_offset, Some(0));
        }
        other => panic!("expected thread detail request, got {other:?}"),
    }
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestThreadTodos(thread_id)) => {
            assert_eq!(thread_id, "thread-user");
        }
        other => panic!("expected todos request, got {other:?}"),
    }
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestThreadWorkContext(thread_id)) => {
            assert_eq!(thread_id, "thread-user");
        }
        other => panic!("expected work-context request, got {other:?}"),
    }
}

#[test]
fn active_thread_reload_required_invalidates_stale_header_context_tokens() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-compacted".to_string(),
        title: "Compacted".to_string(),
        total_message_count: 2,
        loaded_message_start: 0,
        loaded_message_end: 2,
        active_context_window_start: Some(0),
        active_context_window_end: Some(2),
        active_context_window_tokens: Some(239_700_000),
        messages: vec![
            crate::wire::AgentMessage {
                role: crate::wire::MessageRole::Assistant,
                content: "summary".to_string(),
                message_kind: "compaction_artifact".to_string(),
                compaction_payload: Some("P".repeat(40)),
                ..Default::default()
            },
            crate::wire::AgentMessage {
                role: crate::wire::MessageRole::User,
                content: "C".repeat(80),
                message_kind: "normal".to_string(),
                ..Default::default()
            },
        ],
        ..Default::default()
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-compacted".to_string(),
    ));
    while daemon_rx.try_recv().is_ok() {}

    assert_eq!(
        model.current_header_usage_summary().current_tokens,
        239_700_000
    );

    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "thread-compacted".to_string(),
    });

    assert_eq!(
        model.current_header_usage_summary().current_tokens,
        0,
        "reload should clear stale daemon context tokens while the authoritative detail is pending"
    );
    assert!(matches!(
        next_thread_request(&mut daemon_rx),
        Some((thread_id, _, _)) if thread_id == "thread-compacted"
    ));
}

#[test]
fn goal_header_thread_reload_required_refreshes_header_thread_even_when_not_selected() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "selected-thread".to_string(),
        title: "Selected".to_string(),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "selected-thread".to_string(),
    ));
    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "goal-thread".to_string(),
        title: "Goal Thread".to_string(),
        total_message_count: 2,
        loaded_message_start: 0,
        loaded_message_end: 2,
        active_context_window_start: Some(0),
        active_context_window_end: Some(2),
        active_context_window_tokens: Some(239_700_000),
        messages: vec![
            crate::wire::AgentMessage {
                role: crate::wire::MessageRole::Assistant,
                content: "summary".to_string(),
                message_kind: "compaction_artifact".to_string(),
                compaction_payload: Some("P".repeat(40)),
                ..Default::default()
            },
            crate::wire::AgentMessage {
                role: crate::wire::MessageRole::User,
                content: "C".repeat(80),
                message_kind: "normal".to_string(),
                ..Default::default()
            },
        ],
        ..Default::default()
    });
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-reload".to_string(),
            title: "Goal".to_string(),
            active_thread_id: Some("goal-thread".to_string()),
            ..Default::default()
        }));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-reload".to_string(),
        step_id: None,
    });
    while daemon_rx.try_recv().is_ok() {}

    assert_eq!(
        model.current_header_usage_summary().current_tokens,
        239_700_000
    );

    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "goal-thread".to_string(),
    });

    assert_eq!(model.chat.active_thread_id(), Some("selected-thread"));
    assert_eq!(model.current_header_usage_summary().current_tokens, 0);
    assert!(matches!(
        next_thread_request(&mut daemon_rx),
        Some((thread_id, _, _)) if thread_id == "goal-thread"
    ));
}

#[test]
fn active_thread_reload_required_reuses_loaded_page_window_when_viewing_older_history() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 123;
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-user".to_string(),
            title: "Discord Alice".to_string(),
            total_message_count: 400,
            loaded_message_start: 154,
            loaded_message_end: 277,
            messages: (154..277)
                .map(|index| crate::state::chat::AgentMessage {
                    id: Some(format!("msg-{index}")),
                    role: crate::state::chat::MessageRole::Assistant,
                    content: format!("msg {index}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        },
    ));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "thread-user".to_string(),
    });

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        }) => {
            assert_eq!(thread_id, "thread-user");
            assert_eq!(message_limit, Some(123));
            assert_eq!(message_offset, Some(123));
        }
        other => panic!("expected current page refresh request, got {other:?}"),
    }
}

#[test]
fn participant_managed_thread_reload_requests_expanded_latest_page() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 123;
    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        thread_participants: vec![crate::wire::ThreadParticipantState {
            agent_id: "weles".to_string(),
            agent_name: "Weles".to_string(),
            instruction: "verify claims".to_string(),
            status: "active".to_string(),
            created_at: 1,
            updated_at: 1,
            last_contribution_at: None,
            deactivated_at: None,
            always_auto_response: false,
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "thread-user".to_string(),
    });

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        }) => {
            assert_eq!(thread_id, "thread-user");
            assert_eq!(message_limit, Some(246));
            assert_eq!(message_offset, Some(0));
        }
        other => {
            panic!("expected expanded participant-managed thread detail request, got {other:?}")
        }
    }
}

#[test]
fn inactive_thread_reload_required_does_not_interrupt_selected_thread() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-other".to_string(),
        title: "Other Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));
    model.handle_client_event(ClientEvent::ToolCall {
        thread_id: "thread-user".to_string(),
        call_id: "user-call".to_string(),
        name: "bash_command".to_string(),
        arguments: "{\"command\":\"pwd\"}".to_string(),
        weles_review: None,
    });

    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "thread-other".to_string(),
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert_eq!(model.chat.active_tool_calls().len(), 1);
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn selected_internal_dm_thread_detail_is_loaded() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "dm:svarog:weles".to_string(),
        title: "Internal DM · Svarog ↔ WELES".to_string(),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "dm:svarog:weles".to_string(),
    ));

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "dm:svarog:weles".to_string(),
        title: "Internal DM · Svarog ↔ WELES".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::Assistant,
            content: "Keep reviewing the migration plan.".to_string(),
            timestamp: 1,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    let thread = model
        .chat
        .threads()
        .iter()
        .find(|thread| thread.id == "dm:svarog:weles")
        .expect("selected internal dm thread should remain in chat state");
    assert_eq!(model.chat.active_thread_id(), Some("dm:svarog:weles"));
    assert_eq!(thread.messages.len(), 1);
    assert_eq!(
        thread.messages[0].content,
        "Keep reviewing the migration plan."
    );
}

#[test]
fn selected_internal_dm_thread_detail_with_weles_persona_marker_is_loaded() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "dm:svarog:weles".to_string(),
        title: "Internal DM · Svarog ↔ WELES".to_string(),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "dm:svarog:weles".to_string(),
    ));

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "dm:svarog:weles".to_string(),
        title: "Internal DM · Svarog ↔ WELES".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::Assistant,
            content: "Agent persona id: weles\n\nKeep reviewing the migration plan.".to_string(),
            timestamp: 1,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        total_message_count: 150,
        loaded_message_start: 50,
        loaded_message_end: 150,
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    let thread = model
        .chat
        .threads()
        .iter()
        .find(|thread| thread.id == "dm:svarog:weles")
        .expect("selected internal dm thread should remain in chat state");
    assert_eq!(model.chat.active_thread_id(), Some("dm:svarog:weles"));
    assert_eq!(thread.messages.len(), 1);
    assert_eq!(thread.total_message_count, 150);
    assert_eq!(thread.loaded_message_start, 50);
    assert_eq!(thread.loaded_message_end, 150);
}

#[test]
fn internal_dm_tool_activity_does_not_block_normal_thread_completion() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.handle_client_event(ClientEvent::ToolCall {
        thread_id: "dm:svarog:weles".to_string(),
        call_id: "internal-call".to_string(),
        name: "message_agent".to_string(),
        arguments: "{}".to_string(),
        weles_review: None,
    });
    assert!(
        model.chat.active_tool_calls().is_empty(),
        "internal tool calls should not enter the visible running-tool tracker"
    );
    model.handle_client_event(ClientEvent::ToolCall {
        thread_id: "thread-user".to_string(),
        call_id: "user-call".to_string(),
        name: "bash_command".to_string(),
        arguments: "{\"command\":\"pwd\"}".to_string(),
        weles_review: None,
    });
    assert_eq!(model.chat.active_tool_calls().len(), 1);

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "dm:svarog:weles".to_string(),
        title: "Internal DM · Swarog ↔ WELES".to_string(),
        agent_name: None,
    });
    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-user".to_string(),
        input_tokens: 0,
        output_tokens: 0,
        cost: None,
        provider: None,
        model: None,
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: Some("result_json".to_string()),
    });

    assert!(
        model.chat.active_tool_calls().is_empty(),
        "visible thread completion should still clear running tools"
    );
    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
}

#[test]
fn inactive_thread_events_do_not_replace_selected_thread_activity_badge() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-other".to_string(),
        title: "Other Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.handle_client_event(ClientEvent::Reasoning {
        thread_id: "thread-other".to_string(),
        content: "background reasoning".to_string(),
    });
    model.handle_client_event(ClientEvent::ToolCall {
        thread_id: "thread-other".to_string(),
        call_id: "background-call".to_string(),
        name: "bash_command".to_string(),
        arguments: "{\"command\":\"pwd\"}".to_string(),
        weles_review: None,
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert!(model.footer_activity_text().is_none());
    assert_eq!(model.chat.streaming_content(), "");
    assert_eq!(model.chat.streaming_reasoning(), "");
    assert!(model.chat.active_tool_calls().is_empty());
}

#[test]
fn inactive_thread_workflow_notice_does_not_replace_selected_thread_footer_activity() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-other".to_string(),
        title: "Other Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.handle_client_event(ClientEvent::Reasoning {
        thread_id: "thread-user".to_string(),
        content: "active reasoning".to_string(),
    });
    assert_eq!(model.footer_activity_text().as_deref(), Some("reasoning"));

    model.handle_client_event(ClientEvent::WorkflowNotice {
        thread_id: Some("thread-other".to_string()),
        kind: "skill-gate".to_string(),
        message: "background skill gate".to_string(),
        details: Some(r#"{"recommended_skill":"onecontext"}"#.to_string()),
    });

    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("reasoning"),
        "background workflow notices must not replace the selected thread footer activity"
    );
}

#[test]
fn thread_footer_activity_remains_scoped_when_switching_between_busy_threads() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-other".to_string(),
        title: "Other Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.submit_prompt("investigate the auth regression".to_string());
    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));

    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::WorkflowNotice {
        thread_id: Some("thread-other".to_string()),
        kind: "skill-gate".to_string(),
        message: "background skill gate".to_string(),
        details: Some(r#"{"recommended_skill":"onecontext"}"#.to_string()),
    });

    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("thinking"),
        "selected thread should keep its own footer activity after background updates"
    );

    model.open_thread_conversation("thread-other".to_string());
    while daemon_rx.try_recv().is_ok() {}
    assert_eq!(model.footer_activity_text().as_deref(), Some("skill gate"));

    model.open_thread_conversation("thread-user".to_string());
    while daemon_rx.try_recv().is_ok() {}
    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));
}

#[test]
fn optimistic_new_thread_keeps_thinking_during_stale_thread_list_refresh() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;

    model.submit_prompt("investigate the auth regression".to_string());
    let optimistic_thread_id = model
        .chat
        .active_thread_id()
        .map(str::to_string)
        .expect("submit should create an optimistic thread");
    assert!(optimistic_thread_id.starts_with("local-"));
    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));

    model.handle_client_event(ClientEvent::ThreadList(vec![]));

    assert_eq!(
        model.chat.active_thread_id(),
        Some(optimistic_thread_id.as_str())
    );
    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("thinking"),
        "stale thread list refresh should not clear pending thinking on an optimistic thread"
    );
}

#[test]
fn reopened_thread_keeps_thinking_during_stale_thread_list_refresh() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-other".to_string(),
        title: "Other Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.submit_prompt("investigate the auth regression".to_string());
    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));

    model.handle_client_event(ClientEvent::ThreadList(vec![crate::wire::AgentThread {
        id: "thread-other".to_string(),
        title: "Other Thread".to_string(),
        ..Default::default()
    }]));

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("thinking"),
        "stale thread list refresh should not clear pending thinking on a reopened thread"
    );
}

#[test]
fn follow_up_prompt_keeps_thinking_during_stale_thread_list_refresh() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        messages: vec![
            crate::wire::AgentMessage {
                id: Some("msg-user-1".to_string()),
                role: crate::wire::MessageRole::User,
                content: "First question".to_string(),
                timestamp: 1,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
            crate::wire::AgentMessage {
                id: Some("msg-assistant-1".to_string()),
                role: crate::wire::MessageRole::Assistant,
                content: "First answer".to_string(),
                timestamp: 2,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
        ],
        created_at: 1,
        updated_at: 2,
        ..Default::default()
    })));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.submit_prompt("follow-up question".to_string());
    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));

    model.handle_client_event(ClientEvent::ThreadList(vec![]));

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("thinking"),
        "stale thread list refresh should not clear pending thinking on a follow-up prompt"
    );
}

#[test]
fn follow_up_prompt_keeps_thinking_across_reload_after_stale_thread_detail_replaces_tail() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        messages: vec![
            crate::wire::AgentMessage {
                role: crate::wire::MessageRole::User,
                content: "First question".to_string(),
                timestamp: 1,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
            crate::wire::AgentMessage {
                role: crate::wire::MessageRole::Assistant,
                content: "First answer".to_string(),
                timestamp: 2,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
        ],
        created_at: 1,
        updated_at: 2,
        ..Default::default()
    })));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.submit_prompt("follow-up question".to_string());
    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        messages: vec![
            crate::wire::AgentMessage {
                role: crate::wire::MessageRole::User,
                content: "First question".to_string(),
                timestamp: 100,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
            crate::wire::AgentMessage {
                role: crate::wire::MessageRole::Assistant,
                content: "First answer".to_string(),
                timestamp: 200,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
        ],
        total_message_count: 2,
        loaded_message_start: 0,
        loaded_message_end: 2,
        created_at: 1,
        updated_at: 2,
        ..Default::default()
    })));
    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "thread-user".to_string(),
    });

    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("thinking"),
        "reload should preserve thinking even if a stale thread detail temporarily drops the optimistic prompt tail"
    );
}

#[test]
fn follow_up_prompt_deduplicates_latest_page_when_wire_start_is_missing() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        messages: vec![
            crate::wire::AgentMessage {
                id: Some("msg-118".to_string()),
                role: crate::wire::MessageRole::User,
                content: "Earlier question".to_string(),
                timestamp: 118,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
            crate::wire::AgentMessage {
                id: Some("msg-119".to_string()),
                role: crate::wire::MessageRole::Assistant,
                content: "Earlier answer".to_string(),
                timestamp: 119,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
        ],
        total_message_count: 120,
        loaded_message_end: 120,
        ..Default::default()
    })));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.submit_prompt("follow-up question".to_string());
    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        messages: vec![
            crate::wire::AgentMessage {
                id: Some("msg-118".to_string()),
                role: crate::wire::MessageRole::User,
                content: "Earlier question".to_string(),
                timestamp: 118,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
            crate::wire::AgentMessage {
                id: Some("msg-119".to_string()),
                role: crate::wire::MessageRole::Assistant,
                content: "Earlier answer".to_string(),
                timestamp: 119,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
            crate::wire::AgentMessage {
                id: Some("msg-120".to_string()),
                role: crate::wire::MessageRole::User,
                content: "follow-up question".to_string(),
                timestamp: 120,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
        ],
        total_message_count: 121,
        loaded_message_end: 121,
        ..Default::default()
    })));

    let thread = model
        .chat
        .active_thread()
        .expect("thread should stay active");
    assert_eq!(
        thread
            .messages
            .iter()
            .filter(|message| message.role == chat::MessageRole::User
                && message.content == "follow-up question")
            .count(),
        1,
        "persisted latest-page echo should replace the optimistic prompt"
    );
    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));
}

#[test]
fn follow_up_prompt_keeps_reasoning_stream_across_reload_before_first_response() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        messages: vec![
            crate::wire::AgentMessage {
                id: Some("msg-user-1".to_string()),
                role: crate::wire::MessageRole::User,
                content: "First question".to_string(),
                timestamp: 1,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
            crate::wire::AgentMessage {
                id: Some("msg-assistant-1".to_string()),
                role: crate::wire::MessageRole::Assistant,
                content: "First answer".to_string(),
                timestamp: 2,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
        ],
        created_at: 1,
        updated_at: 2,
        ..Default::default()
    })));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.submit_prompt("follow-up question".to_string());
    model.handle_client_event(ClientEvent::Reasoning {
        thread_id: "thread-user".to_string(),
        content: "thinking about the follow-up".to_string(),
    });

    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "thread-user".to_string(),
    });

    assert_eq!(
        model.chat.streaming_reasoning(),
        "thinking about the follow-up",
        "reload should not clear live reasoning on follow-up prompts"
    );
    assert_eq!(model.footer_activity_text().as_deref(), Some("reasoning"));
}

#[test]
fn participant_playground_activity_surfaces_only_for_active_parent_thread() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-other".to_string(),
        title: "Other Thread".to_string(),
        agent_name: None,
    });
    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        thread_participants: vec![
            crate::wire::ThreadParticipantState {
                agent_id: "domowoj".to_string(),
                agent_name: "Domowoj".to_string(),
                instruction: "Look for gaps".to_string(),
                status: "active".to_string(),
                created_at: 1,
                updated_at: 1,
                last_contribution_at: None,
                deactivated_at: None,
                always_auto_response: false,
            },
            crate::wire::ThreadParticipantState {
                agent_id: "weles".to_string(),
                agent_name: "Weles".to_string(),
                instruction: "Verify risky changes".to_string(),
                status: "active".to_string(),
                created_at: 1,
                updated_at: 1,
                last_contribution_at: None,
                deactivated_at: None,
                always_auto_response: false,
            },
        ],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.handle_client_event(ClientEvent::Reasoning {
        thread_id: "playground:domowoj:thread-user".to_string(),
        content: "Hidden participant reasoning".to_string(),
    });
    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("Domowoj crafting response")
    );

    model.handle_client_event(ClientEvent::ToolCall {
        thread_id: "playground:weles:thread-user".to_string(),
        call_id: "hidden-call".to_string(),
        name: "bash_command".to_string(),
        arguments: "{\"command\":\"pwd\"}".to_string(),
        weles_review: None,
    });
    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("Domowoj +1 crafting responses")
    );

    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-other".to_string()));
    assert!(
        model.footer_activity_text().is_none(),
        "participant playground activity should stay scoped to the selected visible thread"
    );
}

#[test]
fn participant_playground_done_refreshes_active_visible_thread_and_surfaces_reply() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 123;
    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            id: Some("msg-1".to_string()),
            role: crate::wire::MessageRole::Assistant,
            content: "Main agent reply".to_string(),
            timestamp: 1,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        thread_participants: vec![crate::wire::ThreadParticipantState {
            agent_id: "domowoj".to_string(),
            agent_name: "Domowoj".to_string(),
            instruction: "Look for weak spots".to_string(),
            status: "active".to_string(),
            created_at: 1,
            updated_at: 1,
            last_contribution_at: None,
            deactivated_at: None,
            always_auto_response: false,
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "playground:domowoj:thread-user".to_string(),
        content: "Drafting visible reply".to_string(),
    });
    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("Domowoj crafting response")
    );

    model.handle_client_event(ClientEvent::Done {
        thread_id: "playground:domowoj:thread-user".to_string(),
        input_tokens: 0,
        output_tokens: 0,
        cost: None,
        provider: None,
        model: None,
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: None,
    });

    match next_thread_request(&mut daemon_rx) {
        Some((thread_id, message_limit, message_offset)) => {
            assert_eq!(thread_id, "thread-user");
            assert_eq!(message_limit, Some(246));
            assert_eq!(message_offset, Some(0));
        }
        other => {
            panic!("expected active visible thread refresh after playground done, got {other:?}")
        }
    }
    assert!(
        model.footer_activity_text().is_none(),
        "playground completion should clear the footer activity line"
    );

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        messages: vec![
            crate::wire::AgentMessage {
                id: Some("msg-1".to_string()),
                role: crate::wire::MessageRole::Assistant,
                content: "Main agent reply".to_string(),
                timestamp: 1,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
            crate::wire::AgentMessage {
                id: Some("msg-2".to_string()),
                role: crate::wire::MessageRole::Assistant,
                content: "Visible participant reply".to_string(),
                author_agent_id: Some("domowoj".to_string()),
                author_agent_name: Some("Domowoj".to_string()),
                timestamp: 2,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
        ],
        thread_participants: vec![crate::wire::ThreadParticipantState {
            agent_id: "domowoj".to_string(),
            agent_name: "Domowoj".to_string(),
            instruction: "Look for weak spots".to_string(),
            status: "active".to_string(),
            created_at: 1,
            updated_at: 2,
            last_contribution_at: Some(2),
            deactivated_at: None,
            always_auto_response: false,
        }],
        created_at: 1,
        updated_at: 2,
        ..Default::default()
    })));

    let thread = model.chat.active_thread().expect("thread should exist");
    assert!(
        thread.messages.iter().any(|message| {
            message.content == "Visible participant reply"
                && message.author_agent_name.as_deref() == Some("Domowoj")
        }),
        "authoritative refresh should surface participant-authored visible replies"
    );
}

#[test]
fn queued_prompt_flushes_after_last_tool_result_before_turn_done() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::ToolCall {
        thread_id: "thread-1".to_string(),
        call_id: "call-1".to_string(),
        name: "bash_command".to_string(),
        arguments: "{\"command\":\"pwd\"}".to_string(),
        weles_review: None,
    });

    model.submit_prompt("stay on the migration task".to_string());
    assert_eq!(model.queued_prompts.len(), 1);
    assert!(daemon_rx.try_recv().is_err());

    model.handle_client_event(ClientEvent::ToolResult {
        thread_id: "thread-1".to_string(),
        call_id: "call-1".to_string(),
        name: "bash_command".to_string(),
        content: "/repo".to_string(),
        is_error: false,
        weles_review: None,
    });

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::SendMessage {
            thread_id, content, ..
        }) => {
            assert_eq!(thread_id.as_deref(), Some("thread-1"));
            assert_eq!(content, "stay on the migration task");
        }
        other => panic!("expected queued send after tool result, got {:?}", other),
    }
    assert!(
        model.queued_prompts.is_empty(),
        "queued prompt should flush as soon as the last tool finishes"
    );
}

#[test]
fn prompt_during_text_stream_without_running_tools_waits_for_done() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-1".to_string(),
        content: "Partial answer".to_string(),
    });
    assert!(
        model.chat.active_tool_calls().is_empty(),
        "plain streaming should not fabricate running tools"
    );

    model.submit_prompt("switch to the auth bug instead".to_string());
    assert_eq!(model.queued_prompts.len(), 1);
    assert!(daemon_rx.try_recv().is_err());

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-1".to_string(),
        input_tokens: 10,
        output_tokens: 20,
        cost: None,
        provider: None,
        model: None,
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: Some("result_json".to_string()),
    });

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::SendMessage {
            thread_id, content, ..
        }) => {
            assert_eq!(thread_id.as_deref(), Some("thread-1"));
            assert_eq!(content, "switch to the auth bug instead");
        }
        other => panic!(
            "expected queued send after done when text is streaming, got {:?}",
            other
        ),
    }
    assert!(
        model.queued_prompts.is_empty(),
        "message should flush once the streaming assistant message completes"
    );
}

#[test]
fn participant_suggestion_event_queues_prompt_with_agent_name() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::ParticipantSuggestion {
        thread_id: "thread-1".to_string(),
        suggestion: crate::wire::ThreadParticipantSuggestion {
            id: "sugg-1".to_string(),
            target_agent_id: "weles".to_string(),
            target_agent_name: "Weles".to_string(),
            instruction: "check claim".to_string(),
            suggestion_kind: "prepared_message".to_string(),
            force_send: false,
            status: "queued".to_string(),
            created_at: 1,
            updated_at: 1,
            auto_send_at: None,
            source_message_timestamp: None,
            error: None,
        },
    });

    assert_eq!(model.queued_prompts.len(), 1);
    assert_eq!(
        model.queued_prompts[0].participant_agent_name.as_deref(),
        Some("Weles")
    );
    assert_eq!(model.queued_prompts[0].display_text(), "Weles: check claim");
}

#[test]
fn auto_response_participant_suggestion_does_not_enter_generic_queued_prompt_modal() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::ParticipantSuggestion {
        thread_id: "thread-1".to_string(),
        suggestion: crate::wire::ThreadParticipantSuggestion {
            id: "auto-1".to_string(),
            target_agent_id: "domowoj".to_string(),
            target_agent_name: "Domowoj".to_string(),
            instruction: "Respond to the latest main agent message.".to_string(),
            suggestion_kind: "auto_response".to_string(),
            force_send: false,
            status: "queued".to_string(),
            created_at: 1,
            updated_at: 1,
            auto_send_at: Some(61_000),
            source_message_timestamp: Some(55_000),
            error: None,
        },
    });

    assert!(
        model.queued_prompts.is_empty(),
        "auto-response suggestions should render in the participant banner instead of the generic queued modal"
    );
}

#[test]
fn due_auto_response_on_reopened_thread_dispatches_on_tick() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            id: Some("msg-1".to_string()),
            role: crate::wire::MessageRole::Assistant,
            content: "Main agent reply".to_string(),
            timestamp: 55_000,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        thread_participants: vec![crate::wire::ThreadParticipantState {
            agent_id: "domowoj".to_string(),
            agent_name: "Domowoj".to_string(),
            instruction: "push the work forward".to_string(),
            status: "active".to_string(),
            created_at: 1,
            updated_at: 2,
            last_contribution_at: Some(10),
            deactivated_at: None,
            always_auto_response: false,
        }],
        queued_participant_suggestions: vec![crate::wire::ThreadParticipantSuggestion {
            id: "auto-1".to_string(),
            target_agent_id: "domowoj".to_string(),
            target_agent_name: "Domowoj".to_string(),
            instruction: "Respond to the latest main agent message.".to_string(),
            suggestion_kind: "auto_response".to_string(),
            force_send: false,
            status: "queued".to_string(),
            created_at: 1,
            updated_at: 1,
            auto_send_at: Some(0),
            source_message_timestamp: Some(55_000),
            error: None,
        }],
        ..Default::default()
    })));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    while daemon_rx.try_recv().is_ok() {}

    model.on_tick();

    let mut saw_send = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(
            command,
            DaemonCommand::SendParticipantSuggestion { thread_id, suggestion_id }
                if thread_id == "thread-1" && suggestion_id == "auto-1"
        ) {
            saw_send = true;
            break;
        }
    }
    assert!(saw_send, "expected due auto-response send command on tick");
    assert!(
        model.queued_prompts.is_empty(),
        "tick-driven auto response should bypass the generic queued prompt list"
    );
}

#[test]
fn always_auto_response_participant_suggestion_sends_immediately_on_active_thread() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Thread".to_string(),
        thread_participants: vec![crate::wire::ThreadParticipantState {
            agent_id: "domowoj".to_string(),
            agent_name: "Domowoj".to_string(),
            instruction: "push the work forward".to_string(),
            status: "active".to_string(),
            created_at: 1,
            updated_at: 2,
            last_contribution_at: Some(10),
            deactivated_at: None,
            always_auto_response: true,
        }],
        ..Default::default()
    })));
    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::ParticipantSuggestion {
        thread_id: "thread-1".to_string(),
        suggestion: crate::wire::ThreadParticipantSuggestion {
            id: "auto-1".to_string(),
            target_agent_id: "domowoj".to_string(),
            target_agent_name: "Domowoj".to_string(),
            instruction: "Respond to the latest main agent message.".to_string(),
            suggestion_kind: "auto_response".to_string(),
            force_send: false,
            status: "queued".to_string(),
            created_at: 1,
            updated_at: 1,
            auto_send_at: Some(61_000),
            source_message_timestamp: Some(55_000),
            error: None,
        },
    });

    assert!(matches!(
        daemon_rx.try_recv(),
        Ok(DaemonCommand::SendParticipantSuggestion { thread_id, suggestion_id })
            if thread_id == "thread-1" && suggestion_id == "auto-1"
    ));
}

#[test]
fn reopened_thread_with_always_auto_response_dispatches_queued_suggestion_immediately() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            id: Some("msg-1".to_string()),
            role: crate::wire::MessageRole::Assistant,
            content: "Main agent reply".to_string(),
            timestamp: 55_000,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        thread_participants: vec![crate::wire::ThreadParticipantState {
            agent_id: "domowoj".to_string(),
            agent_name: "Domowoj".to_string(),
            instruction: "push the work forward".to_string(),
            status: "active".to_string(),
            created_at: 1,
            updated_at: 2,
            last_contribution_at: Some(10),
            deactivated_at: None,
            always_auto_response: true,
        }],
        queued_participant_suggestions: vec![crate::wire::ThreadParticipantSuggestion {
            id: "auto-1".to_string(),
            target_agent_id: "domowoj".to_string(),
            target_agent_name: "Domowoj".to_string(),
            instruction: "Respond to the latest main agent message.".to_string(),
            suggestion_kind: "auto_response".to_string(),
            force_send: false,
            status: "queued".to_string(),
            created_at: 1,
            updated_at: 1,
            auto_send_at: Some(61_000),
            source_message_timestamp: Some(55_000),
            error: None,
        }],
        ..Default::default()
    })));

    let mut saw_send = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(
            command,
            DaemonCommand::SendParticipantSuggestion { thread_id, suggestion_id }
                if thread_id == "thread-1" && suggestion_id == "auto-1"
        ) {
            saw_send = true;
            break;
        }
    }
    assert!(
        saw_send,
        "reopening a thread with an always auto-response participant should send the queued suggestion immediately"
    );
}

#[test]
fn active_thread_detail_requests_auto_response_for_latest_main_reply() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Thread".to_string(),
        messages: vec![
            crate::wire::AgentMessage {
                id: Some("msg-1".to_string()),
                role: crate::wire::MessageRole::User,
                content: "Keep going".to_string(),
                timestamp: 1,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
            crate::wire::AgentMessage {
                id: Some("msg-2".to_string()),
                role: crate::wire::MessageRole::Assistant,
                content: "I finished the patch; the next step is verifying the diff.".to_string(),
                timestamp: 2,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
        ],
        thread_participants: vec![
            crate::wire::ThreadParticipantState {
                agent_id: "weles".to_string(),
                agent_name: "Weles".to_string(),
                instruction: "verify claims".to_string(),
                status: "active".to_string(),
                created_at: 1,
                updated_at: 2,
                last_contribution_at: Some(10),
                deactivated_at: None,
                always_auto_response: false,
            },
            crate::wire::ThreadParticipantState {
                agent_id: "domowoj".to_string(),
                agent_name: "Domowoj".to_string(),
                instruction: "push the work forward".to_string(),
                status: "active".to_string(),
                created_at: 1,
                updated_at: 3,
                last_contribution_at: Some(20),
                deactivated_at: None,
                always_auto_response: false,
            },
        ],
        ..Default::default()
    })));

    let mut saw_auto_response = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(
            command,
            DaemonCommand::ThreadParticipantCommand {
                thread_id,
                target_agent_id,
                action,
                instruction,
                ..
            } if thread_id == "thread-1"
                && target_agent_id == "domowoj"
                && action == "auto_response"
                && instruction.is_none()
        ) {
            saw_auto_response = true;
            break;
        }
    }

    assert!(
        saw_auto_response,
        "opening the active thread should request an auto-response for the most active participant"
    );
}

#[test]
fn done_event_on_active_thread_requests_auto_response_for_live_main_reply() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            id: Some("msg-1".to_string()),
            role: crate::wire::MessageRole::User,
            content: "Keep going".to_string(),
            timestamp: 1,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        thread_participants: vec![
            crate::wire::ThreadParticipantState {
                agent_id: "weles".to_string(),
                agent_name: "Weles".to_string(),
                instruction: "verify claims".to_string(),
                status: "active".to_string(),
                created_at: 1,
                updated_at: 2,
                last_contribution_at: Some(10),
                deactivated_at: None,
                always_auto_response: false,
            },
            crate::wire::ThreadParticipantState {
                agent_id: "domowoj".to_string(),
                agent_name: "Domowoj".to_string(),
                instruction: "push the work forward".to_string(),
                status: "active".to_string(),
                created_at: 1,
                updated_at: 3,
                last_contribution_at: Some(20),
                deactivated_at: None,
                always_auto_response: false,
            },
        ],
        ..Default::default()
    })));
    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-1".to_string(),
        content: "I finished the patch; the next step is verifying the diff.".to_string(),
    });
    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-1".to_string(),
        input_tokens: 0,
        output_tokens: 0,
        cost: None,
        provider: None,
        model: None,
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: None,
    });

    let mut saw_auto_response = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(
            command,
            DaemonCommand::ThreadParticipantCommand {
                thread_id,
                target_agent_id,
                action,
                instruction,
                ..
            } if thread_id == "thread-1"
                && target_agent_id == "domowoj"
                && action == "auto_response"
                && instruction.is_none()
        ) {
            saw_auto_response = true;
            break;
        }
    }

    assert!(
        saw_auto_response,
        "finishing a live main-agent reply on the open thread should request auto-response immediately"
    );
}

#[test]
fn participant_suggestion_does_not_auto_flush_as_user_message_after_done() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::ParticipantSuggestion {
        thread_id: "thread-1".to_string(),
        suggestion: crate::wire::ThreadParticipantSuggestion {
            id: "sugg-1".to_string(),
            target_agent_id: "weles".to_string(),
            target_agent_name: "Weles".to_string(),
            instruction: "check claim".to_string(),
            suggestion_kind: "prepared_message".to_string(),
            force_send: false,
            status: "queued".to_string(),
            created_at: 1,
            updated_at: 1,
            auto_send_at: None,
            source_message_timestamp: None,
            error: None,
        },
    });

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-1".to_string(),
        input_tokens: 0,
        output_tokens: 0,
        cost: None,
        provider: None,
        model: None,
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: None,
    });

    assert!(
        daemon_rx.try_recv().is_err(),
        "participant suggestions must not auto-submit through the normal send-message path"
    );
    assert_eq!(model.queued_prompts.len(), 1);
    assert_eq!(
        model.queued_prompts[0].suggestion_id.as_deref(),
        Some("sugg-1")
    );
}

#[test]
fn new_subagent_conversation_keeps_header_after_thread_created_without_agent_name() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.subagents.entries.push(crate::state::SubAgentEntry {
        id: "domowoj".to_string(),
        name: "Domowoj".to_string(),
        provider: "openai".to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("testing".to_string()),
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: Some("medium".to_string()),
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        raw_json: None,
    });

    model.start_new_thread_view_for_agent(Some("domowoj"));
    model.submit_prompt("inspect this".to_string());

    let optimistic = model.current_header_agent_profile();
    assert_eq!(optimistic.agent_label, "Domowoj");

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-domowoj".to_string(),
        title: "inspect this".to_string(),
        agent_name: None,
    });

    let after_created = model.current_header_agent_profile();
    assert_eq!(after_created.agent_label, "Domowoj");
    assert_eq!(after_created.provider, "openai");
    assert_eq!(after_created.model, "gpt-5.4-mini");
}

#[test]
fn new_subagent_thread_view_ignores_background_parent_stream_delta() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-parent".to_string(),
        title: "Parent".to_string(),
    });

    model.start_new_thread_view_for_agent(Some("domowoj"));
    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-parent".to_string(),
        content: "background output".to_string(),
    });

    assert_eq!(model.chat.active_thread_id(), None);
    assert_eq!(model.chat.streaming_content(), "");
}

#[test]
fn new_subagent_conversation_done_clears_footer_activity_after_thread_creation() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.subagents.entries.push(crate::state::SubAgentEntry {
        id: "domowoj".to_string(),
        name: "Domowoj".to_string(),
        provider: "openai".to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("testing".to_string()),
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: Some("medium".to_string()),
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        raw_json: None,
    });

    model.start_new_thread_view_for_agent(Some("domowoj"));
    model.submit_prompt("inspect this".to_string());
    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-domowoj".to_string(),
        title: "inspect this".to_string(),
        agent_name: Some("Domowoj".to_string()),
    });
    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-domowoj".to_string(),
        content: "done".to_string(),
    });
    assert_eq!(model.footer_activity_text().as_deref(), Some("writing"));

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-domowoj".to_string(),
        input_tokens: 0,
        output_tokens: 0,
        cost: None,
        provider: None,
        model: None,
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: None,
    });

    assert!(
        model.footer_activity_text().is_none(),
        "completed subagent reply should clear footer activity"
    );
    assert!(
        !model.assistant_busy(),
        "completed subagent reply should not leave the thread busy"
    );
}

#[test]
fn new_subagent_conversation_keeps_thinking_after_thread_created_until_first_response() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.subagents.entries.push(crate::state::SubAgentEntry {
        id: "domowoj".to_string(),
        name: "Domowoj".to_string(),
        provider: "openai".to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("testing".to_string()),
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: Some("medium".to_string()),
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        raw_json: None,
    });

    model.start_new_thread_view_for_agent(Some("domowoj"));
    model.submit_prompt("inspect this".to_string());
    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-domowoj".to_string(),
        title: "inspect this".to_string(),
        agent_name: Some("Domowoj".to_string()),
    });

    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("thinking"),
        "thread creation should preserve the pending footer activity until output starts"
    );

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-domowoj".to_string(),
        content: "done".to_string(),
    });
    assert_eq!(model.footer_activity_text().as_deref(), Some("writing"));
}

#[test]
fn new_subagent_conversation_keeps_thinking_across_reload_before_first_response() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.subagents.entries.push(crate::state::SubAgentEntry {
        id: "domowoj".to_string(),
        name: "Domowoj".to_string(),
        provider: "openai".to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("testing".to_string()),
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: Some("medium".to_string()),
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        raw_json: None,
    });

    model.start_new_thread_view_for_agent(Some("domowoj"));
    model.submit_prompt("inspect this".to_string());
    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-domowoj".to_string(),
        title: "inspect this".to_string(),
        agent_name: Some("Domowoj".to_string()),
    });
    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "thread-domowoj".to_string(),
    });

    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("thinking"),
        "reload before first response should not clear pending thinking state"
    );
}

#[test]
fn new_subagent_conversation_keeps_reasoning_stream_across_reload_before_first_response() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.subagents.entries.push(crate::state::SubAgentEntry {
        id: "domowoj".to_string(),
        name: "Domowoj".to_string(),
        provider: "openai".to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("testing".to_string()),
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: Some("medium".to_string()),
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        raw_json: None,
    });

    model.start_new_thread_view_for_agent(Some("domowoj"));
    model.submit_prompt("inspect this".to_string());
    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-domowoj".to_string(),
        title: "inspect this".to_string(),
        agent_name: Some("Domowoj".to_string()),
    });
    model.handle_client_event(ClientEvent::Reasoning {
        thread_id: "thread-domowoj".to_string(),
        content: "checking the workspace".to_string(),
    });
    assert_eq!(model.chat.streaming_reasoning(), "checking the workspace");
    assert_eq!(model.footer_activity_text().as_deref(), Some("reasoning"));

    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "thread-domowoj".to_string(),
    });

    assert_eq!(
        model.chat.streaming_reasoning(),
        "checking the workspace",
        "reload should not clear live reasoning before answer text starts"
    );
    assert_eq!(model.footer_activity_text().as_deref(), Some("reasoning"));
}

#[test]
fn new_thread_generic_workflow_notice_does_not_break_thinking_preservation() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;

    model.start_new_thread_view();
    model.submit_prompt("do you have generate image now?".to_string());
    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-main".to_string(),
        title: "do you have generate image now?".to_string(),
        agent_name: None,
    });
    model.handle_client_event(ClientEvent::WorkflowNotice {
        thread_id: Some("thread-main".to_string()),
        kind: "transport-fallback".to_string(),
        message: "provider switched transport".to_string(),
        details: Some(r#"{"to":"responses"}"#.to_string()),
    });
    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "thread-main".to_string(),
    });

    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("thinking"),
        "non-activity workflow notices should not let reload clear thinking before output"
    );
}

#[test]
fn thread_detail_prunes_stale_participant_prompts_after_daemon_removes_suggestion() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::ParticipantSuggestion {
        thread_id: "thread-1".to_string(),
        suggestion: crate::wire::ThreadParticipantSuggestion {
            id: "sugg-1".to_string(),
            target_agent_id: "weles".to_string(),
            target_agent_name: "Weles".to_string(),
            instruction: "check claim".to_string(),
            suggestion_kind: "prepared_message".to_string(),
            force_send: false,
            status: "queued".to_string(),
            created_at: 1,
            updated_at: 1,
            auto_send_at: None,
            source_message_timestamp: None,
            error: None,
        },
    });
    assert_eq!(model.queued_prompts.len(), 1);

    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Thread".to_string(),
        messages: vec![],
        queued_participant_suggestions: vec![],
        created_at: 1,
        updated_at: 2,
        ..Default::default()
    });

    assert!(
        model.queued_prompts.is_empty(),
        "thread detail should clear stale participant prompts once the daemon no longer reports them"
    );
}

#[test]
fn queued_participant_send_now_stops_stream_and_sends_participant_command() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-1".to_string(),
        content: "streaming".to_string(),
    });
    model.queue_participant_suggestion(
        "thread-1".to_string(),
        "sugg-1".to_string(),
        "weles".to_string(),
        "Weles".to_string(),
        "urgent fix".to_string(),
        true,
    );
    model.open_queued_prompts_modal();

    model.execute_selected_queued_prompt_action();

    assert!(matches!(
        daemon_rx.try_recv(),
        Ok(DaemonCommand::StopStream { thread_id }) if thread_id == "thread-1"
    ));
    assert!(matches!(
        daemon_rx.try_recv(),
        Ok(DaemonCommand::SendParticipantSuggestion { thread_id, suggestion_id })
            if thread_id == "thread-1" && suggestion_id == "sugg-1"
    ));
}

#[test]
fn follow_up_prompt_after_cancel_keeps_processing_new_events_on_same_thread() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-1".to_string(),
        content: "Partial answer".to_string(),
    });
    model.cancelled_thread_id = Some("thread-1".to_string());
    model.chat.reduce(chat::ChatAction::ForceStopStreaming);

    model.submit_prompt("follow up".to_string());

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::SendMessage {
            thread_id, content, ..
        }) => {
            assert_eq!(thread_id.as_deref(), Some("thread-1"));
            assert_eq!(content, "follow up");
        }
        other => panic!("expected follow-up send on same thread, got {:?}", other),
    }

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-1".to_string(),
        content: "Visible answer".to_string(),
    });

    assert_eq!(
        model.chat.streaming_content(),
        "Visible answer",
        "new stream chunks on the same thread should not be dropped after a cancelled turn"
    );
}

#[test]
fn leading_internal_delegate_prompt_routes_to_internal_command() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.submit_prompt("!weles verify the auth regression".to_string());

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::InternalDelegate {
            thread_id,
            target_agent_id,
            content,
            ..
        }) => {
            assert_eq!(thread_id.as_deref(), Some("thread-1"));
            assert_eq!(target_agent_id, "weles");
            assert_eq!(content, "verify the auth regression");
        }
        other => panic!("expected internal delegate command, got {:?}", other),
    }
    assert!(
        model
            .chat
            .active_thread()
            .expect("thread should remain selected")
            .messages
            .is_empty(),
        "internal delegation should not append a visible user turn"
    );
}

#[test]
fn leading_internal_delegate_prompt_is_blocked_for_budget_exceeded_thread() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-budget".to_string(),
        title: "Budget exceeded".to_string(),
        ..Default::default()
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-budget".to_string()));
    model.handle_task_update_event(crate::wire::AgentTask {
        id: "task-budget".to_string(),
        title: "Budget exceeded child".to_string(),
        thread_id: Some("thread-budget".to_string()),
        status: Some(crate::wire::TaskStatus::BudgetExceeded),
        blocked_reason: Some("execution budget exceeded for this thread".to_string()),
        created_at: 42,
        ..Default::default()
    });
    while daemon_rx.try_recv().is_ok() {}

    model.submit_prompt("!weles verify the auth regression".to_string());

    assert_eq!(
        model.input.buffer(),
        "!weles verify the auth regression",
        "blocked internal delegate should preserve operator text in the input"
    );
    let (notice, _) = model
        .input_notice_style()
        .expect("blocked internal delegate should surface an input notice");
    assert!(
        notice.contains("Thread budget exceeded"),
        "expected budget exceeded notice, got: {notice}"
    );
    while let Ok(command) = daemon_rx.try_recv() {
        assert!(
            !matches!(command, DaemonCommand::InternalDelegate { .. }),
            "budget-exceeded thread should not emit an internal delegate command: {command:?}"
        );
    }
}

#[test]
fn known_agent_directive_aliases_keep_builtin_entries_canonical_lowercase() {
    let (model, _) = make_model_with_daemon_rx();
    let aliases = model.known_agent_directive_aliases();

    assert!(aliases.iter().any(|alias| alias == "swarozyc"));
    assert!(aliases.iter().any(|alias| alias == "mokosh"));
    assert!(aliases.iter().any(|alias| alias == "veles"));
    assert!(!aliases.iter().any(|alias| alias == "Swarozyc"));
    assert!(!aliases.iter().any(|alias| alias == "Mokosh"));
    assert!(!aliases.iter().any(|alias| alias == "Dazhbog"));
}

#[test]
fn leading_internal_delegate_prompt_routes_veles_alias_to_internal_command() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.submit_prompt("!veles verify the auth regression".to_string());

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::InternalDelegate {
            thread_id,
            target_agent_id,
            content,
            ..
        }) => {
            assert_eq!(thread_id.as_deref(), Some("thread-1"));
            assert_eq!(target_agent_id, "veles");
            assert_eq!(content, "verify the auth regression");
        }
        other => panic!("expected internal delegate command, got {:?}", other),
    }
    assert!(
        model
            .chat
            .active_thread()
            .expect("thread should remain selected")
            .messages
            .is_empty(),
        "internal delegation should not append a visible user turn"
    );
}

#[test]
fn leading_participant_prompt_routes_to_participant_command() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.submit_prompt("@weles verify claims before answering".to_string());

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::ThreadParticipantCommand {
            thread_id,
            target_agent_id,
            action,
            instruction,
            ..
        }) => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(target_agent_id, "weles");
            assert_eq!(action, "upsert");
            assert_eq!(
                instruction.as_deref(),
                Some("verify claims before answering")
            );
        }
        other => panic!("expected participant command, got {:?}", other),
    }
    assert!(
        model
            .chat
            .active_thread()
            .expect("thread should remain selected")
            .messages
            .is_empty(),
        "participant registration should not append a visible user turn"
    );
    let (notice, _) = model
        .input_notice_style()
        .expect("participant command should surface a visible notice");
    assert!(
        notice.contains("Weles"),
        "expected agent name in notice, got: {notice}"
    );
    assert!(
        notice.contains("joined") || notice.contains("updated"),
        "expected participant update wording in notice, got: {notice}"
    );
}

#[test]
fn unconfigured_mokosh_participant_prompt_opens_setup_and_retries_after_model_selection() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: zorai_shared::providers::PROVIDER_ID_ALIBABA_CODING_PLAN.to_string(),
        provider_name: "Alibaba Coding Plan".to_string(),
        authenticated: true,
        auth_source: "api_key".to_string(),
        model: "qwen3.6-plus".to_string(),
    }];
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.submit_prompt("@mokosh keep flow moving".to_string());

    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::ProviderPicker)
    );
    assert!(
        daemon_rx.try_recv().is_err(),
        "setup should happen before any daemon command is emitted"
    );

    let provider_index = widgets::provider_picker::available_provider_defs(&model.auth)
        .iter()
        .position(|provider| {
            provider.id == zorai_shared::providers::PROVIDER_ID_ALIBABA_CODING_PLAN
        })
        .expect("provider to exist");
    if provider_index > 0 {
        model
            .modal
            .reduce(crate::state::modal::ModalAction::Navigate(
                provider_index as i32,
            ));
    }

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        crate::state::modal::ModalKind::ProviderPicker,
    );
    assert!(!quit);
    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::ModelPicker)
    );

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        crate::state::modal::ModalKind::ModelPicker,
    );
    assert!(!quit);

    match daemon_rx
        .try_recv()
        .expect("expected targeted builtin persona config command")
    {
        DaemonCommand::SetTargetAgentProviderModel {
            target_agent_id,
            provider_id,
            model,
        } => {
            assert_eq!(target_agent_id, "mokosh");
            assert_eq!(
                provider_id,
                zorai_shared::providers::PROVIDER_ID_ALIBABA_CODING_PLAN
            );
            assert!(
                !model.trim().is_empty(),
                "setup flow should choose a concrete model"
            );
        }
        other => panic!("expected builtin persona provider/model command, got {other:?}"),
    }

    match daemon_rx
        .try_recv()
        .expect("expected retried participant command after setup")
    {
        DaemonCommand::ThreadParticipantCommand {
            thread_id,
            target_agent_id,
            action,
            instruction,
            ..
        } => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(target_agent_id, "mokosh");
            assert_eq!(action, "upsert");
            assert_eq!(instruction.as_deref(), Some("keep flow moving"));
        }
        other => panic!("expected participant command, got {other:?}"),
    }
}

#[test]
fn unconfigured_builtin_participant_prompt_opens_setup_and_retries_after_model_selection() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: zorai_shared::providers::PROVIDER_ID_ALIBABA_CODING_PLAN.to_string(),
        provider_name: "Alibaba Coding Plan".to_string(),
        authenticated: true,
        auth_source: "api_key".to_string(),
        model: "qwen3.6-plus".to_string(),
    }];
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.submit_prompt("@swarozyc verify claims before answering".to_string());

    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::ProviderPicker)
    );
    assert!(
        daemon_rx.try_recv().is_err(),
        "setup should happen before any daemon command is emitted"
    );

    let provider_index = widgets::provider_picker::available_provider_defs(&model.auth)
        .iter()
        .position(|provider| {
            provider.id == zorai_shared::providers::PROVIDER_ID_ALIBABA_CODING_PLAN
        })
        .expect("provider to exist");
    if provider_index > 0 {
        model
            .modal
            .reduce(crate::state::modal::ModalAction::Navigate(
                provider_index as i32,
            ));
    }

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        crate::state::modal::ModalKind::ProviderPicker,
    );
    assert!(!quit);
    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::ModelPicker)
    );

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        crate::state::modal::ModalKind::ModelPicker,
    );
    assert!(!quit);

    match daemon_rx
        .try_recv()
        .expect("expected targeted builtin persona config command")
    {
        DaemonCommand::SetTargetAgentProviderModel {
            target_agent_id,
            provider_id,
            model,
        } => {
            assert_eq!(target_agent_id, "swarozyc");
            assert_eq!(
                provider_id,
                zorai_shared::providers::PROVIDER_ID_ALIBABA_CODING_PLAN
            );
            assert!(
                !model.trim().is_empty(),
                "setup flow should choose a concrete model"
            );
        }
        other => panic!("expected builtin persona provider/model command, got {other:?}"),
    }

    match daemon_rx
        .try_recv()
        .expect("expected retried participant command after setup")
    {
        DaemonCommand::ThreadParticipantCommand {
            thread_id,
            target_agent_id,
            action,
            instruction,
            ..
        } => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(target_agent_id, "swarozyc");
            assert_eq!(action, "upsert");
            assert_eq!(
                instruction.as_deref(),
                Some("verify claims before answering")
            );
        }
        other => panic!("expected participant command, got {other:?}"),
    }
}

#[test]
fn subagent_error_requests_refresh_to_clear_rejected_optimistic_state() {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, mut daemon_rx) = unbounded_channel();
    let mut model = TuiModel::new(event_rx, daemon_tx);
    model.subagents.entries = vec![crate::state::SubAgentEntry {
        id: "weles_builtin".to_string(),
        name: "Legacy WELES".to_string(),
        provider: PROVIDER_ID_OPENAI.to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("testing".to_string()),
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: None,
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        raw_json: Some(serde_json::json!({
            "id": "weles_builtin",
            "name": "Legacy WELES"
        })),
    }];

    model.handle_client_event(ClientEvent::Error(
        "protected mutation: reserved built-in sub-agent".to_string(),
    ));

    assert_eq!(
        model.subagents.entries.len(),
        1,
        "stale optimistic entry remains until refresh arrives"
    );
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("subagent error should request authoritative refresh"),
        DaemonCommand::ListSubAgents
    ));
}

#[test]
fn openai_codex_auth_events_update_config_and_modal_state() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_client_event(ClientEvent::OpenAICodexAuthStatus(
        crate::client::OpenAICodexAuthStatusVm {
            available: false,
            auth_mode: Some("chatgpt_subscription".to_string()),
            account_id: None,
            expires_at: None,
            source: Some("zorai-daemon".to_string()),
            error: None,
            auth_url: None,
            status: Some("pending".to_string()),
        },
    ));

    assert!(!model.config.chatgpt_auth_available);
    assert_eq!(
        model.config.chatgpt_auth_source.as_deref(),
        Some("zorai-daemon")
    );

    model.handle_client_event(ClientEvent::OpenAICodexAuthLoginResult(
        crate::client::OpenAICodexAuthStatusVm {
            available: false,
            auth_mode: Some("chatgpt_subscription".to_string()),
            account_id: None,
            expires_at: None,
            source: Some("zorai-daemon".to_string()),
            error: None,
            auth_url: Some("https://auth.openai.com/oauth/authorize?flow=tui".to_string()),
            status: Some("pending".to_string()),
        },
    ));

    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::OpenAIAuth)
    );
    assert_eq!(
        model.openai_auth_url.as_deref(),
        Some("https://auth.openai.com/oauth/authorize?flow=tui")
    );
    assert!(model
        .openai_auth_status_text
        .as_deref()
        .is_some_and(|text| text.contains("complete ChatGPT authentication")));
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected auth state refresh after status"),
        DaemonCommand::GetProviderAuthStates
    ));
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected auth state refresh after login"),
        DaemonCommand::GetProviderAuthStates
    ));

    model.handle_client_event(ClientEvent::OpenAICodexAuthLogoutResult {
        ok: true,
        error: None,
    });

    assert!(!model.config.chatgpt_auth_available);
    assert!(model.config.chatgpt_auth_source.is_none());
    assert!(model.openai_auth_url.is_none());
    assert!(model.openai_auth_status_text.is_none());
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected auth state refresh after logout"),
        DaemonCommand::GetProviderAuthStates
    ));
    assert_eq!(model.status_line, "ChatGPT subscription auth cleared");
}

#[test]
fn first_raw_config_load_requests_openai_codex_auth_status_from_daemon() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.connected = true;
    model.agent_config_loaded = false;

    model.handle_agent_config_raw_event(serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
    }));

    let mut saw_auth_status = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(command, DaemonCommand::GetOpenAICodexAuthStatus) {
            saw_auth_status = true;
            break;
        }
    }

    assert!(
        saw_auth_status,
        "first config load should release deferred codex auth refresh"
    );
}

#[test]
fn provider_auth_states_overlay_chatgpt_auth_when_openai_is_configured_for_chatgpt_subscription() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_OPENAI.to_string();
    model.config.auth_source = "chatgpt_subscription".to_string();
    model.config.chatgpt_auth_available = true;
    model.config.chatgpt_auth_source = Some("zorai-daemon".to_string());

    model.handle_provider_auth_states_event(vec![crate::state::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: false,
        auth_source: "api_key".to_string(),
        model: "gpt-5.4".to_string(),
    }]);

    let openai = model
        .auth
        .entries
        .iter()
        .find(|entry| entry.provider_id == PROVIDER_ID_OPENAI)
        .expect("openai auth entry should exist");
    assert!(
        openai.authenticated,
        "chatgpt daemon auth should surface as connected"
    );
    assert_eq!(openai.auth_source, "chatgpt_subscription");
}

#[test]
fn provider_auth_states_overlay_chatgpt_auth_for_openai_even_when_another_provider_is_active() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    model.config.auth_source = "github_copilot".to_string();
    model.config.chatgpt_auth_available = true;
    model.config.chatgpt_auth_source = Some("zorai-daemon".to_string());

    model.handle_provider_auth_states_event(vec![crate::state::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: false,
        auth_source: "api_key".to_string(),
        model: "gpt-5.4".to_string(),
    }]);

    let openai = model
        .auth
        .entries
        .iter()
        .find(|entry| entry.provider_id == PROVIDER_ID_OPENAI)
        .expect("openai auth entry should exist");
    assert!(
        openai.authenticated,
        "chatgpt daemon auth should keep openai selectable in provider picker"
    );
    assert_eq!(openai.auth_source, "chatgpt_subscription");
}

#[test]
fn provider_validation_event_updates_auth_result_and_status_line() {
    let mut model = make_model();
    model.auth.entries = vec![crate::state::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: true,
        auth_source: "api_key".to_string(),
        model: "gpt-5.4".to_string(),
    }];
    model.auth.validating = Some(PROVIDER_ID_OPENAI.to_string());

    model.handle_client_event(ClientEvent::ProviderValidation {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        valid: false,
        error: Some("bad key".to_string()),
    });

    assert_eq!(model.auth.validating, None);
    assert_eq!(model.status_line, "OpenAI test failed: bad key");
    assert_eq!(
        model
            .auth
            .validation_results
            .get(PROVIDER_ID_OPENAI)
            .cloned(),
        Some((false, "Error: bad key".to_string()))
    );
}

#[test]
fn openai_codex_auth_status_event_clears_stale_modal_state() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.openai_auth_url = Some("https://stale.example/login".to_string());
    model.openai_auth_status_text = Some("stale".to_string());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::OpenAIAuth));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::CommandPalette));

    model.handle_client_event(ClientEvent::OpenAICodexAuthStatus(
        crate::client::OpenAICodexAuthStatusVm {
            available: false,
            auth_mode: Some("chatgpt_subscription".to_string()),
            account_id: None,
            expires_at: None,
            source: Some("zorai-daemon".to_string()),
            error: Some("Timed out waiting for callback".to_string()),
            auth_url: None,
            status: Some("error".to_string()),
        },
    ));

    assert!(model.openai_auth_url.is_none());
    assert_eq!(
        model.openai_auth_status_text.as_deref(),
        Some("Timed out waiting for callback")
    );
    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::OpenAIAuth)
    );
    model.close_top_modal();
    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::CommandPalette)
    );
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected auth state refresh after status"),
        DaemonCommand::GetProviderAuthStates
    ));
    assert_eq!(model.status_line, "Timed out waiting for callback");
}

#[test]
fn openai_codex_auth_status_event_removes_all_stale_nested_openai_modals() {
    let mut model = make_model();
    model.openai_auth_url = Some("https://stale.example/login".to_string());
    model.openai_auth_status_text = Some("stale".to_string());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::OpenAIAuth));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::CommandPalette));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::OpenAIAuth));

    model.handle_client_event(ClientEvent::OpenAICodexAuthStatus(
        crate::client::OpenAICodexAuthStatusVm {
            available: false,
            auth_mode: Some("chatgpt_subscription".to_string()),
            account_id: None,
            expires_at: None,
            source: Some("zorai-daemon".to_string()),
            error: Some("Timed out waiting for callback".to_string()),
            auth_url: None,
            status: Some("error".to_string()),
        },
    ));

    assert_eq!(model.modal.top(), Some(modal::ModalKind::OpenAIAuth));
    model.close_top_modal();
    assert_eq!(model.modal.top(), Some(modal::ModalKind::CommandPalette));
    model.close_top_modal();
    assert_eq!(model.modal.top(), Some(modal::ModalKind::Settings));
}

#[test]
fn disconnect_and_reconnect_clear_openai_auth_modal_even_when_nested() {
    let mut model = make_model();
    model.openai_auth_url = Some("https://auth.openai.com/oauth/authorize?flow=tui".to_string());
    model.openai_auth_status_text = Some("pending".to_string());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::OpenAIAuth));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::CommandPalette));

    model.handle_disconnected_event();

    assert!(model.openai_auth_url.is_none());
    assert!(model.openai_auth_status_text.is_none());
    assert_eq!(model.modal.top(), Some(modal::ModalKind::CommandPalette));

    model.openai_auth_url = Some("https://auth.openai.com/oauth/authorize?flow=tui".to_string());
    model.openai_auth_status_text = Some("pending".to_string());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::OpenAIAuth));

    model.handle_reconnecting_event(3);

    assert!(model.openai_auth_url.is_none());
    assert!(model.openai_auth_status_text.is_none());
    assert_eq!(model.modal.top(), Some(modal::ModalKind::CommandPalette));
}

#[test]
fn disconnect_clears_transport_send_errors_from_error_state() {
    let mut model = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ErrorViewer));

    model.handle_client_event(ClientEvent::Error(
        "Send error: Broken pipe (os error 32)".to_string(),
    ));
    assert_eq!(
        model.last_error.as_deref(),
        Some("Send error: Broken pipe (os error 32)")
    );
    assert!(model.error_active);

    model.handle_client_event(ClientEvent::Disconnected);

    assert!(model.last_error.is_none());
    assert!(!model.error_active);
    assert_ne!(model.modal.top(), Some(modal::ModalKind::ErrorViewer));
    assert_eq!(model.status_line, "Disconnected from daemon");
}
