use super::*;
use crate::session_manager::SessionManager;
use tempfile::tempdir;

const MAX_FRAME_SIZE_BYTES: usize = 16 * 1024 * 1024;

#[tokio::test]
async fn list_threads_omits_message_history_from_thread_summaries() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let huge_message = "x".repeat(MAX_FRAME_SIZE_BYTES + 1024);

    engine.threads.write().await.insert(
        "thread-big".to_string(),
        AgentThread {
            id: "thread-big".to_string(),
            title: "Big thread".to_string(),
            messages: vec![AgentMessage::user(huge_message, 1)],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: 1,
            updated_at: 2,
            total_input_tokens: 0,
            total_output_tokens: 0,
        },
    );

    let listed = engine.list_threads().await;
    assert_eq!(listed.len(), 1);
    assert!(
        listed[0].messages.is_empty(),
        "thread list payload should not include full message history"
    );

    let serialized = serde_json::to_string(&listed).expect("serialize thread summaries");
    assert!(
        serialized.len() < MAX_FRAME_SIZE_BYTES,
        "thread list payload should stay below the IPC frame cap"
    );
}