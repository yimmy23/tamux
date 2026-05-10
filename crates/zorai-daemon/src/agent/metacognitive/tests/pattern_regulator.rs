use crate::agent::types::{AgentConfig, AgentMessage, AgentThread, MessageRole};
use crate::agent::{now_millis, AgentEngine};
use crate::session_manager::SessionManager;
use tempfile::tempdir;

#[tokio::test]
async fn repeated_metacognitive_system_messages_compact_in_place() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-metacognitive-dedupe";
    let now = now_millis();
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Metacognition dedupe".to_string(),
                messages: vec![AgentMessage::user("debug repeated tool call", now)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                created_at: now,
                updated_at: now,
                total_input_tokens: 0,
                total_output_tokens: 0,
            },
        );
    }

    let warning = "Meta-cognitive intervention: warning before tool execution.\n\
Planned tool: read_file\n\
Arguments: {\"path\":\"/tmp/super_secret_large_argument_that_should_not_repeat\"}\n\
Detected risks:\n\
- confirmation: verify with independent evidence\n\
Before continuing, briefly reflect on whether this is the best next step.";

    engine
        .append_metacognitive_system_message(thread_id, warning)
        .await;
    engine
        .append_metacognitive_system_message(thread_id, warning)
        .await;
    engine
        .append_metacognitive_system_message(thread_id, warning)
        .await;

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    let meta_messages = thread
        .messages
        .iter()
        .filter(|message| {
            message.role == MessageRole::System
                && message.content.starts_with("Meta-cognitive intervention:")
        })
        .collect::<Vec<_>>();

    assert_eq!(
        meta_messages.len(),
        1,
        "repeated same-signature metacognitive interventions should not append unbounded system messages"
    );
    let content = &meta_messages[0].content;
    assert!(
        content.contains("Repeated count: 3"),
        "compacted intervention should preserve repeat count: {content}"
    );
    assert!(
        !content.contains("super_secret_large_argument_that_should_not_repeat"),
        "compacted repeat should omit repeated full arguments: {content}"
    );
}

#[tokio::test]
async fn varied_metacognitive_system_messages_group_in_place() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-metacognitive-group";
    let now = now_millis();
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Metacognition group".to_string(),
                messages: vec![AgentMessage::user("debug varied tool warnings", now)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                created_at: now,
                updated_at: now,
                total_input_tokens: 0,
                total_output_tokens: 0,
            },
        );
    }

    let warnings = [
        "Meta-cognitive intervention: warning before tool execution.\n\
Planned tool: read_file\n\
Arguments: {\"path\":\"/tmp/first_secret_payload\"}\n\
Detected risks:\n\
- confirmation: verify with independent evidence\n\
Before continuing, briefly reflect on whether this is the best next step.",
        "Meta-cognitive intervention: warning before tool execution.\n\
Planned tool: search_files\n\
Arguments: {\"query\":\"needle\"}\n\
Detected risks:\n\
- sunk_cost: inspect fresh evidence\n\
Before continuing, briefly reflect on whether this is the best next step.",
        "Meta-cognitive intervention: warning before tool execution.\n\
Planned tool: update_todo\n\
Arguments: {\"items\":[{\"content\":\"done\",\"status\":\"completed\"}]}\n\
Detected risks:\n\
- overconfidence: verify the task is actually complete\n\
Before continuing, briefly reflect on whether this is the best next step.",
    ];

    for warning in warnings {
        engine
            .append_metacognitive_system_message(thread_id, warning)
            .await;
    }

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    let meta_messages = thread
        .messages
        .iter()
        .filter(|message| {
            message.role == MessageRole::System
                && message.content.starts_with("Meta-cognitive intervention:")
        })
        .collect::<Vec<_>>();

    assert_eq!(
        meta_messages.len(),
        1,
        "varied metacognitive interventions should group into one system message"
    );
    let content = &meta_messages[0].content;
    assert!(
        content.contains("Grouped count: 3"),
        "grouped intervention should preserve count: {content}"
    );
    assert!(
        content.contains("read_file")
            && content.contains("search_files")
            && content.contains("update_todo"),
        "grouped intervention should retain affected tool names: {content}"
    );
    assert!(
        !content.contains("first_secret_payload"),
        "grouped intervention should omit prior full arguments: {content}"
    );
}
