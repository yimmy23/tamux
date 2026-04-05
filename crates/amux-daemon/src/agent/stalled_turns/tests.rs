use super::analysis::{classify_stalled_turn, follow_through_observed};
use super::runtime::StalledTurnCandidate;
use super::types::{StalledTurnClass, TurnEvidence};
use crate::agent::types::{
    AgentConfig, AgentMessage, AgentTask, AgentThread, ApiTransport, MessageRole, TaskPriority,
    TaskStatus, ToolCall, ToolFunction,
};
use crate::agent::{
    StreamProgressKind, ThreadHandoffState, ThreadResponderFrame, CONCIERGE_AGENT_ID,
    CONCIERGE_AGENT_NAME, MAIN_AGENT_ID, MAIN_AGENT_NAME, WELES_AGENT_ID,
};
use crate::session_manager::SessionManager;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn base_evidence() -> TurnEvidence {
    TurnEvidence {
        last_assistant_message: "Working. Let me draft the redesigned content now.".to_string(),
        preceded_by_tool_result: false,
        new_tool_call_followed: false,
        new_substantive_assistant_message_followed: false,
        task_or_goal_progressed: false,
        user_replied: false,
    }
}

#[test]
fn classify_promise_without_action_for_progress_filler_message() {
    let evidence = base_evidence();
    let actual = classify_stalled_turn(&evidence);
    assert_eq!(actual, Some(StalledTurnClass::PromiseWithoutAction));
}

#[test]
fn classify_post_tool_result_no_follow_through_when_promise_follows_tool_result() {
    let mut evidence = base_evidence();
    evidence.preceded_by_tool_result = true;

    let actual = classify_stalled_turn(&evidence);
    assert_eq!(
        actual,
        Some(StalledTurnClass::PostToolResultNoFollowThrough)
    );
}

#[test]
fn ignores_message_when_concrete_follow_through_happened() {
    let mut evidence = base_evidence();
    evidence.new_tool_call_followed = true;

    assert!(follow_through_observed(&evidence));
    assert_eq!(classify_stalled_turn(&evidence), None);
}

#[test]
fn ignores_message_when_user_replied() {
    let mut evidence = base_evidence();
    evidence.user_replied = true;

    assert!(follow_through_observed(&evidence));
    assert_eq!(classify_stalled_turn(&evidence), None);
}

#[test]
fn ignores_non_promise_message() {
    let mut evidence = base_evidence();
    evidence.last_assistant_message =
        "The redesign is complete. Here is the new landing page copy.".to_string();

    assert!(!follow_through_observed(&evidence));
    assert_eq!(classify_stalled_turn(&evidence), None);
}

#[test]
fn stalled_turn_candidate_starts_with_thirty_second_grace_window() {
    let candidate =
        StalledTurnCandidate::new("thread-1", StalledTurnClass::PromiseWithoutAction, 1_000);
    assert_eq!(candidate.retries_sent, 0);
    assert_eq!(candidate.next_evaluation_at, 31_000);
}

#[test]
fn stalled_turn_candidate_uses_increasing_retry_backoff() {
    let mut candidate =
        StalledTurnCandidate::new("thread-1", StalledTurnClass::PromiseWithoutAction, 1_000);

    candidate.record_retry(31_000);
    assert_eq!(candidate.retries_sent, 1);
    assert_eq!(candidate.next_evaluation_at, 91_000);

    candidate.record_retry(91_000);
    assert_eq!(candidate.retries_sent, 2);
    assert_eq!(candidate.next_evaluation_at, 211_000);
}

#[test]
fn stalled_turn_candidate_marks_escalation_ready_after_third_retry_window() {
    let mut candidate =
        StalledTurnCandidate::new("thread-1", StalledTurnClass::PromiseWithoutAction, 1_000);

    candidate.record_retry(31_000);
    candidate.record_retry(91_000);
    candidate.record_retry(211_000);

    assert_eq!(candidate.retries_sent, 3);
    assert!(!candidate.escalation_ready(330_999));
    assert!(candidate.escalation_ready(331_000));
}

async fn spawn_stub_assistant_server(response_text: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stub assistant server");
    let addr = listener
        .local_addr()
        .expect("stub assistant server local addr");
    let response_text = response_text.to_string();

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let response_text = response_text.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 65536];
                let _ = socket.read(&mut buffer).await;
                let response = format!(
                    concat!(
                        "HTTP/1.1 200 OK\r\n",
                        "content-type: text/event-stream\r\n",
                        "cache-control: no-cache\r\n",
                        "connection: close\r\n",
                        "\r\n",
                        "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{}\"}}}}]}}\n\n",
                        "data: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":7,\"completion_tokens\":3}}}}\n\n",
                        "data: [DONE]\n\n"
                    ),
                    response_text
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write stub assistant response");
            });
use amux_shared::providers::PROVIDER_ID_OPENAI;

        }
    });

    format!("http://{addr}/v1")
}

async fn build_test_engine(response_text: &str) -> Arc<crate::agent::AgentEngine> {
    let root = tempdir().expect("tempdir should succeed").into_path();
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.provider = amux_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_stub_assistant_server(response_text).await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    crate::agent::AgentEngine::new_test(manager, config, &root).await
}

#[tokio::test]
async fn collect_stalled_turn_observations_skips_active_tool_turns() {
    let engine = build_test_engine("Acknowledged.").await;
    let now = super::now_millis();
    let thread_id = "thread-active-tool";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Active tool turn".to_string(),
                messages: vec![AgentMessage {
                    id: "assistant-tool".to_string(),
                    role: MessageRole::Assistant,
                    content: String::new(),
                    tool_calls: Some(vec![ToolCall::with_default_weles_review(
                        "call-1".to_string(),
                        ToolFunction {
                            name: "read_file".to_string(),
                            arguments: "{}".to_string(),
                        },
                    )]),
                    tool_call_id: None,
                    tool_name: None,
                    tool_arguments: None,
                    tool_status: None,
                    weles_review: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    provider: None,
                    model: None,
                    api_transport: None,
                    response_id: None,
                    upstream_message: None,
                    provider_final_result: None,
                    reasoning: None,
                    message_kind: crate::agent::types::AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
                    timestamp: now.saturating_sub(60_000),
                }],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now,
                updated_at: now,
            },
        );
    }

    let observations = engine.collect_stalled_turn_observations().await;
    assert!(observations.is_empty());
}

#[tokio::test]
async fn collect_stalled_turn_observations_detects_promise_without_action() {
    let engine = build_test_engine("Acknowledged.").await;
    let now = super::now_millis();
    let thread_id = "thread-promise-stall";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Promise stall".to_string(),
                messages: vec![
                    AgentMessage::user("Design the page", now.saturating_sub(61_000)),
                    AgentMessage {
                        id: "assistant-stall".to_string(),
                        role: MessageRole::Assistant,
                        content: "Working. Let me draft the redesigned content now.".to_string(),
                        tool_calls: None,
                        tool_call_id: None,
                        tool_name: None,
                        tool_arguments: None,
                        tool_status: None,
                        weles_review: None,
                        input_tokens: 0,
                        output_tokens: 0,
                        provider: None,
                        model: None,
                        api_transport: None,
                        response_id: None,
                        upstream_message: None,
                        provider_final_result: None,
                        reasoning: None,
                        message_kind: crate::agent::types::AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        timestamp: now.saturating_sub(60_000),
                    },
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now,
                updated_at: now,
            },
        );
    }

    let observations = engine.collect_stalled_turn_observations().await;
    assert_eq!(observations.len(), 1);
    assert_eq!(
        observations[0].class,
        StalledTurnClass::PromiseWithoutAction
    );
    assert_eq!(observations[0].stream_progress_kind, None);
}

#[tokio::test]
async fn supervise_stalled_turns_retries_with_internal_ping_and_continue() {
    let engine = build_test_engine("Recovered.").await;
    let mut events = engine.subscribe();
    let now = super::now_millis();
    let thread_id = "thread-recovery";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Recovery thread".to_string(),
                messages: vec![
                    AgentMessage::user("Make the redesign", now.saturating_sub(61_000)),
                    AgentMessage {
                        id: "assistant-recovery".to_string(),
                        role: MessageRole::Assistant,
                        content: "Excellent. Let me start drafting the redesigned landing page."
                            .to_string(),
                        tool_calls: None,
                        tool_call_id: None,
                        tool_name: None,
                        tool_arguments: None,
                        tool_status: None,
                        weles_review: None,
                        input_tokens: 0,
                        output_tokens: 0,
                        provider: None,
                        model: None,
                        api_transport: None,
                        response_id: None,
                        upstream_message: None,
                        provider_final_result: None,
                        reasoning: None,
                        message_kind: crate::agent::types::AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        timestamp: now.saturating_sub(60_000),
                    },
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now,
                updated_at: now,
            },
        );
    }

    engine
        .supervise_stalled_turns()
        .await
        .expect("stalled-turn supervision should retry");

    let mut saw_reload = false;
    while let Ok(event) = events.try_recv() {
        if let crate::agent::types::AgentEvent::ThreadReloadRequired {
            thread_id: event_thread_id,
        } = event
        {
            if event_thread_id == thread_id {
                saw_reload = true;
                break;
            }
        }
    }
    assert!(
        saw_reload,
        "stalled-turn retry should emit thread reload so TUI refreshes the recovery message"
    );

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::System
            && message.content.contains("WELES stalled-turn recovery")
    }));
    assert!(
        !thread
            .messages
            .iter()
            .any(|message| { message.role == MessageRole::User && message.content == "continue" }),
        "stalled-turn retries should not append a synthetic user 'continue' message"
    );
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant && message.content.contains("Recovered.")
    }));
    drop(threads);

    let dm_thread_id = crate::agent::agent_identity::internal_dm_thread_id(WELES_AGENT_ID, MAIN_AGENT_ID);
    let threads = engine.threads.read().await;
    let dm_thread = threads.get(&dm_thread_id).expect("internal recovery DM should exist");
    assert_eq!(dm_thread.agent_name.as_deref(), Some(MAIN_AGENT_NAME));
    assert!(dm_thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant && message.content.contains("Recovered.")
    }));
}

#[tokio::test]
async fn collect_stalled_turn_observations_detects_idle_active_reasoning_stream() {
    let engine = build_test_engine("Acknowledged.").await;
    let now = super::now_millis();
    let thread_id = "thread-idle-stream";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(CONCIERGE_AGENT_NAME.to_string()),
                title: "Idle stream".to_string(),
                messages: vec![AgentMessage::user("Keep going", now.saturating_sub(61_000))],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now,
                updated_at: now,
            },
        );
    }
    engine
        .set_thread_handoff_state(
            thread_id,
            ThreadHandoffState {
                origin_agent_id: MAIN_AGENT_ID.to_string(),
                active_agent_id: CONCIERGE_AGENT_ID.to_string(),
                responder_stack: vec![
                    ThreadResponderFrame {
                        agent_id: MAIN_AGENT_ID.to_string(),
                        agent_name: MAIN_AGENT_NAME.to_string(),
                        entered_at: now.saturating_sub(61_000),
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    },
                    ThreadResponderFrame {
                        agent_id: CONCIERGE_AGENT_ID.to_string(),
                        agent_name: CONCIERGE_AGENT_NAME.to_string(),
                        entered_at: now.saturating_sub(60_000),
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    },
                ],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    let (generation, _, _) = engine.begin_stream_cancellation(thread_id).await;
    engine
        .note_stream_progress(
            thread_id,
            generation,
            StreamProgressKind::Reasoning,
            "thinking through the next step",
        )
        .await;
    {
        let mut streams = engine.stream_cancellations.lock().await;
        let entry = streams.get_mut(thread_id).expect("active stream entry should exist");
        entry.last_progress_at = now.saturating_sub(31_000);
    }

    let observations = engine.collect_stalled_turn_observations().await;
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].class, StalledTurnClass::ActiveStreamIdle);
    assert_eq!(
        observations[0].stream_progress_kind,
        Some(StreamProgressKind::Reasoning)
    );
}

#[tokio::test]
async fn supervise_stalled_turns_recovers_idle_reasoning_stream_via_internal_dm() {
    let engine = build_test_engine("Recovered.").await;
    let now = super::now_millis();
    let thread_id = "thread-idle-stream-recovery";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(CONCIERGE_AGENT_NAME.to_string()),
                title: "Idle stream recovery".to_string(),
                messages: vec![AgentMessage::user("Continue the unfinished job", now.saturating_sub(61_000))],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now,
                updated_at: now,
            },
        );
    }
    engine
        .set_thread_handoff_state(
            thread_id,
            ThreadHandoffState {
                origin_agent_id: MAIN_AGENT_ID.to_string(),
                active_agent_id: CONCIERGE_AGENT_ID.to_string(),
                responder_stack: vec![
                    ThreadResponderFrame {
                        agent_id: MAIN_AGENT_ID.to_string(),
                        agent_name: MAIN_AGENT_NAME.to_string(),
                        entered_at: now.saturating_sub(61_000),
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    },
                    ThreadResponderFrame {
                        agent_id: CONCIERGE_AGENT_ID.to_string(),
                        agent_name: CONCIERGE_AGENT_NAME.to_string(),
                        entered_at: now.saturating_sub(60_000),
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    },
                ],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    let (generation, _, _) = engine.begin_stream_cancellation(thread_id).await;
    engine
        .note_stream_progress(
            thread_id,
            generation,
            StreamProgressKind::Reasoning,
            "thinking through the next step",
        )
        .await;
    {
        let mut streams = engine.stream_cancellations.lock().await;
        let entry = streams.get_mut(thread_id).expect("active stream entry should exist");
        entry.last_progress_at = now.saturating_sub(31_000);
    }

    engine
        .supervise_stalled_turns()
        .await
        .expect("idle active stream should recover");

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::System
            && message.content.contains("stream went idle before completion")
    }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant && message.content.contains("Recovered.")
    }));

    let dm_thread_id =
        crate::agent::agent_identity::internal_dm_thread_id(WELES_AGENT_ID, CONCIERGE_AGENT_ID);
    let dm_thread = threads.get(&dm_thread_id).expect("recovery DM should exist");
    assert_eq!(dm_thread.agent_name.as_deref(), Some(CONCIERGE_AGENT_NAME));
    assert!(dm_thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant && message.content.contains("Recovered.")
    }));
}

#[tokio::test]
async fn supervise_stalled_turns_escalates_after_third_retry_window() {
    let engine = build_test_engine("Recovered.").await;
    let now = super::now_millis();
    let thread_id = "thread-escalate";
    let task_id = "task-escalate";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Escalation thread".to_string(),
                messages: vec![
                    AgentMessage::user("Do the work", now.saturating_sub(301_000)),
                    AgentMessage {
                        id: "assistant-escalate".to_string(),
                        role: MessageRole::Assistant,
                        content: "Working. Let me draft the redesigned content now.".to_string(),
                        tool_calls: None,
                        tool_call_id: None,
                        tool_name: None,
                        tool_arguments: None,
                        tool_status: None,
                        weles_review: None,
                        input_tokens: 0,
                        output_tokens: 0,
                        provider: None,
                        model: None,
                        api_transport: None,
                        response_id: None,
                        upstream_message: None,
                        provider_final_result: None,
                        reasoning: None,
                        message_kind: crate::agent::types::AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        timestamp: now.saturating_sub(300_000),
                    },
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now,
                updated_at: now,
            },
        );
    }
    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(AgentTask {
            id: task_id.to_string(),
            title: "Escalation task".to_string(),
            description: "Escalation task".to_string(),
            status: TaskStatus::Blocked,
            priority: TaskPriority::Normal,
            progress: 0,
            created_at: now,
            started_at: Some(now),
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some(thread_id.to_string()),
            source: "goal_run".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: None,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: None,
            awaiting_approval_id: None,
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            tool_whitelist: None,
            tool_blacklist: None,
            override_provider: None,
            override_model: None,
            override_system_prompt: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            sub_agent_def_id: None,
        });
    }
    {
        let mut candidates = engine.stalled_turn_candidates.lock().await;
        let mut candidate =
            StalledTurnCandidate::new(thread_id, StalledTurnClass::PromiseWithoutAction, now);
        candidate.last_message_id = "assistant-escalate".to_string();
        candidate.last_message_at = now.saturating_sub(300_000);
        candidate.last_message_excerpt =
            "Working. Let me draft the redesigned content now.".to_string();
        candidate.task_id = Some(task_id.to_string());
        candidate.retries_sent = 3;
        candidate.next_evaluation_at = now.saturating_sub(1_000);
        candidates.insert(thread_id.to_string(), candidate);
    }

    engine
        .supervise_stalled_turns()
        .await
        .expect("stalled-turn supervision should escalate");

    let tasks = engine.tasks.lock().await;
    let task = tasks
        .iter()
        .find(|task| task.id == task_id)
        .expect("task should remain present");
    assert_eq!(task.blocked_reason.as_deref(), Some("stuck_needs_recovery"));
}
