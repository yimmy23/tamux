use super::*;
use crate::session_manager::SessionManager;
use bytes::BytesMut;
use tempfile::tempdir;
use tokio_util::codec::Encoder;

const MAX_FRAME_SIZE_BYTES: usize = 16 * 1024 * 1024;

fn make_thread(
    id: &str,
    agent_name: Option<&str>,
    title: &str,
    pinned: bool,
    created_at: u64,
    updated_at: u64,
    messages: Vec<AgentMessage>,
) -> AgentThread {
    AgentThread {
        id: id.to_string(),
        agent_name: agent_name.map(ToOwned::to_owned),
        title: title.to_string(),
        messages,
        pinned,
        upstream_thread_id: None,
        upstream_transport: None,
        upstream_provider: None,
        upstream_model: None,
        upstream_assistant_id: None,
        created_at,
        updated_at,
        total_input_tokens: 0,
        total_output_tokens: 0,
    }
}

fn assistant_message(content: impl Into<String>, ts: u64) -> AgentMessage {
    let mut message = AgentMessage::user(content, ts);
    message.role = MessageRole::Assistant;
    message
}

fn weles_internal_message(ts: u64) -> AgentMessage {
    assistant_message(
        crate::agent::agent_identity::build_weles_persona_prompt(
            crate::agent::agent_identity::WELES_GOVERNANCE_SCOPE,
        ),
        ts,
    )
}

fn list_ids(threads: &[AgentThread]) -> Vec<&str> {
    threads.iter().map(|thread| thread.id.as_str()).collect()
}

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
            agent_name: Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string()),
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
    assert_eq!(listed[0].agent_name.as_deref(), Some("Weles"));

    let serialized = serde_json::to_string(&listed).expect("serialize thread summaries");
    assert!(
        serialized.len() < MAX_FRAME_SIZE_BYTES,
        "thread list payload should stay below the IPC frame cap"
    );
}

#[tokio::test]
async fn list_threads_filters_visible_threads_by_default() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut threads = engine.threads.write().await;
    threads.insert(
        "visible-main".to_string(),
        make_thread(
            "visible-main",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Visible",
            false,
            10,
            30,
            vec![AgentMessage::user("operator message", 10)],
        ),
    );
    threads.insert(
        "weles-hidden".to_string(),
        make_thread(
            "weles-hidden",
            Some(crate::agent::agent_identity::WELES_AGENT_NAME),
            "Hidden Weles",
            false,
            11,
            29,
            vec![weles_internal_message(11)],
        ),
    );
    threads.insert(
        "handoff:hidden".to_string(),
        make_thread(
            "handoff:hidden",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Hidden handoff",
            false,
            12,
            28,
            vec![AgentMessage::user("handoff", 12)],
        ),
    );
    drop(threads);

    let listed = engine
        .list_threads_filtered(&ThreadListFilter::default())
        .await;

    assert_eq!(list_ids(&listed), vec!["visible-main"]);
    assert!(listed[0].messages.is_empty());
}

#[tokio::test]
async fn list_threads_include_internal_reveals_hidden_threads() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut threads = engine.threads.write().await;
    threads.insert(
        "visible-main".to_string(),
        make_thread(
            "visible-main",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Visible",
            false,
            10,
            30,
            vec![AgentMessage::user("operator message", 10)],
        ),
    );
    threads.insert(
        "weles-hidden".to_string(),
        make_thread(
            "weles-hidden",
            Some(crate::agent::agent_identity::WELES_AGENT_NAME),
            "Hidden Weles",
            false,
            11,
            29,
            vec![weles_internal_message(11)],
        ),
    );
    threads.insert(
        "handoff:hidden".to_string(),
        make_thread(
            "handoff:hidden",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Hidden handoff",
            false,
            12,
            28,
            vec![AgentMessage::user("handoff", 12)],
        ),
    );
    drop(threads);

    let listed = engine
        .list_threads_filtered(&ThreadListFilter {
            include_internal: true,
            ..ThreadListFilter::default()
        })
        .await;

    assert_eq!(
        list_ids(&listed),
        vec!["visible-main", "weles-hidden", "handoff:hidden"]
    );
}

#[tokio::test]
async fn pin_rejected_when_budget_would_be_exceeded() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.context_window_tokens = 100;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-pin-budget";

    let mut already_pinned = AgentMessage::user("a".repeat(80), 1);
    already_pinned.pinned_for_compaction = true;
    let candidate = AgentMessage::user("b".repeat(30), 2);
    let candidate_id = candidate.id.clone();

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Pin budget",
            false,
            1,
            2,
            vec![already_pinned, candidate],
        ),
    );

    let result = engine
        .pin_thread_message_for_compaction(thread_id, &candidate_id)
        .await;

    assert!(!result.ok, "pin should be rejected when it exceeds budget");
    assert_eq!(result.thread_id, thread_id);
    assert_eq!(result.message_id, candidate_id);
    assert_eq!(result.current_pinned_chars, 80);
    assert_eq!(result.candidate_pinned_chars, Some(110));
    assert_eq!(result.pinned_budget_chars, 100);

    let thread = engine
        .threads
        .read()
        .await
        .get(thread_id)
        .cloned()
        .expect("thread should still exist");
    assert_eq!(
        thread
            .messages
            .iter()
            .filter(|message| message.pinned_for_compaction)
            .count(),
        1
    );
}

#[tokio::test]
async fn deleting_pinned_message_removes_pin_state() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-delete-pinned";

    let message = AgentMessage::user("keep this pinned", 1);
    let message_id = message.id.clone();
    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Delete pin",
            false,
            1,
            1,
            vec![message],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    let result = engine
        .pin_thread_message_for_compaction(thread_id, &message_id)
        .await;
    assert!(result.ok, "pin should succeed before deletion");

    let deleted = engine
        .delete_thread_messages(thread_id, std::slice::from_ref(&message_id))
        .await
        .expect("delete should succeed");
    assert_eq!(deleted, 1);

    let thread = engine
        .threads
        .read()
        .await
        .get(thread_id)
        .cloned()
        .expect("thread should still exist");
    assert!(
        thread
            .messages
            .iter()
            .all(|message| !message.pinned_for_compaction),
        "deleting the source message should remove the pin with it"
    );
}

#[tokio::test]
async fn list_threads_dm_visibility_is_unchanged() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let dm_thread_id = crate::agent::agent_identity::internal_dm_thread_id(
        crate::agent::agent_identity::MAIN_AGENT_ID,
        crate::agent::agent_identity::CONCIERGE_AGENT_ID,
    );

    engine.threads.write().await.insert(
        dm_thread_id.clone(),
        make_thread(
            &dm_thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Internal DM",
            false,
            10,
            30,
            vec![AgentMessage::user("dm content", 10)],
        ),
    );

    let default_list = engine
        .list_threads_filtered(&ThreadListFilter::default())
        .await;
    let internal_list = engine
        .list_threads_filtered(&ThreadListFilter {
            include_internal: true,
            ..ThreadListFilter::default()
        })
        .await;

    assert_eq!(list_ids(&default_list), vec![dm_thread_id.as_str()]);
    assert_eq!(list_ids(&internal_list), vec![dm_thread_id.as_str()]);
}

#[tokio::test]
async fn list_threads_filters_agent_aliases_and_absent_agent_name_as_main() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut threads = engine.threads.write().await;
    threads.insert(
        "main-none".to_string(),
        make_thread(
            "main-none",
            None,
            "Main none",
            false,
            1,
            30,
            vec![AgentMessage::user("main none", 1)],
        ),
    );
    threads.insert(
        "main-empty".to_string(),
        make_thread(
            "main-empty",
            Some(""),
            "Main empty",
            false,
            2,
            20,
            vec![AgentMessage::user("main empty", 2)],
        ),
    );
    threads.insert(
        "weles".to_string(),
        make_thread(
            "weles",
            Some(crate::agent::agent_identity::WELES_AGENT_NAME),
            "Weles visible",
            false,
            3,
            10,
            vec![AgentMessage::user("not hidden", 3)],
        ),
    );
    drop(threads);

    let main_list = engine
        .list_threads_filtered(&ThreadListFilter {
            agent_name: Some("main-agent".to_string()),
            ..ThreadListFilter::default()
        })
        .await;
    let weles_list = engine
        .list_threads_filtered(&ThreadListFilter {
            agent_name: Some("weles".to_string()),
            ..ThreadListFilter::default()
        })
        .await;

    assert_eq!(list_ids(&main_list), vec!["main-none", "main-empty"]);
    assert_eq!(list_ids(&weles_list), vec!["weles"]);
}

#[tokio::test]
async fn list_threads_empty_title_query_is_a_no_op() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut threads = engine.threads.write().await;
    threads.insert(
        "alpha".to_string(),
        make_thread(
            "alpha",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Alpha",
            false,
            1,
            20,
            vec![AgentMessage::user("a", 1)],
        ),
    );
    threads.insert(
        "beta".to_string(),
        make_thread(
            "beta",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Beta",
            false,
            2,
            10,
            vec![AgentMessage::user("b", 2)],
        ),
    );
    drop(threads);

    let baseline = engine
        .list_threads_filtered(&ThreadListFilter::default())
        .await;
    let with_empty_title = engine
        .list_threads_filtered(&ThreadListFilter {
            title_query: Some("   ".to_string()),
            ..ThreadListFilter::default()
        })
        .await;

    assert_eq!(list_ids(&baseline), list_ids(&with_empty_title));
}

#[tokio::test]
async fn list_threads_limit_and_offset_obey_boundaries() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut threads = engine.threads.write().await;
    for (id, updated_at) in [("one", 30), ("two", 20), ("three", 10)] {
        threads.insert(
            id.to_string(),
            make_thread(
                id,
                Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
                id,
                false,
                updated_at - 1,
                updated_at,
                vec![AgentMessage::user(id, updated_at - 1)],
            ),
        );
    }
    drop(threads);

    let zero_limit = engine
        .list_threads_filtered(&ThreadListFilter {
            limit: Some(0),
            ..ThreadListFilter::default()
        })
        .await;
    let zero_offset = engine
        .list_threads_filtered(&ThreadListFilter {
            offset: 0,
            limit: Some(2),
            ..ThreadListFilter::default()
        })
        .await;
    let oversized_limit = engine
        .list_threads_filtered(&ThreadListFilter {
            offset: 1,
            limit: Some(99),
            ..ThreadListFilter::default()
        })
        .await;
    let offset_past_end = engine
        .list_threads_filtered(&ThreadListFilter {
            offset: 3,
            ..ThreadListFilter::default()
        })
        .await;

    assert!(zero_limit.is_empty());
    assert_eq!(list_ids(&zero_offset), vec!["one", "two"]);
    assert_eq!(list_ids(&oversized_limit), vec!["two", "three"]);
    assert!(offset_past_end.is_empty());
}

#[tokio::test]
async fn list_threads_orders_same_updated_at_deterministically() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut threads = engine.threads.write().await;
    for id in ["thread-c", "thread-a", "thread-b"] {
        threads.insert(
            id.to_string(),
            make_thread(
                id,
                Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
                id,
                false,
                1,
                10,
                vec![AgentMessage::user(id, 1)],
            ),
        );
    }
    drop(threads);

    let listed = engine
        .list_threads_filtered(&ThreadListFilter::default())
        .await;

    assert_eq!(list_ids(&listed), vec!["thread-a", "thread-b", "thread-c"]);
}

#[tokio::test]
async fn get_thread_filtered_truncates_to_last_n_messages() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine.threads.write().await.insert(
        "thread-a".to_string(),
        make_thread(
            "thread-a",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Thread A",
            false,
            1,
            4,
            vec![
                AgentMessage::user("one", 1),
                assistant_message("two", 2),
                AgentMessage::user("three", 3),
                assistant_message("four", 4),
            ],
        ),
    );

    let detail = engine
        .get_thread_filtered("thread-a", false, Some(2), 0)
        .await
        .expect("visible thread should load");

    let contents = detail
        .thread
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>();
    assert_eq!(contents, vec!["three", "four"]);
    assert!(detail.messages_truncated);

    let untruncated = engine
        .get_thread_filtered("thread-a", false, Some(99), 0)
        .await
        .expect("oversized limit should still return thread");
    assert_eq!(untruncated.thread.messages.len(), 4);
    assert!(!untruncated.messages_truncated);
}

#[tokio::test]
async fn get_thread_capped_for_ipc_truncates_oversized_thread_detail_payload() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let huge_message = "x".repeat(MAX_FRAME_SIZE_BYTES + 1024);

    engine.threads.write().await.insert(
        "thread-huge".to_string(),
        make_thread(
            "thread-huge",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Huge thread",
            false,
            1,
            3,
            vec![
                AgentMessage::user(huge_message, 1),
                assistant_message("recent tail", 2),
            ],
        ),
    );

    let detail = engine
        .get_thread_capped_for_ipc("thread-huge", false)
        .await
        .expect("visible thread should load");

    let contents = detail
        .thread
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>();
    assert_eq!(contents, vec!["recent tail"]);
    assert!(detail.messages_truncated);

    let thread_json =
        serde_json::to_string(&Some(detail.thread)).expect("serialize capped thread detail json");
    let mut frame = BytesMut::new();
    amux_protocol::DaemonCodec::default()
        .encode(
            amux_protocol::DaemonMessage::AgentThreadDetail { thread_json },
            &mut frame,
        )
        .expect("serialize capped daemon message");
    assert!(
        frame.len().saturating_sub(4) <= MAX_FRAME_SIZE_BYTES,
        "capped thread detail should stay below the IPC frame cap"
    );
}

#[tokio::test]
async fn get_thread_filtered_hides_internal_threads_unless_requested() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine.threads.write().await.insert(
        "weles-hidden".to_string(),
        make_thread(
            "weles-hidden",
            Some(crate::agent::agent_identity::WELES_AGENT_NAME),
            "Hidden Weles",
            false,
            1,
            2,
            vec![weles_internal_message(1)],
        ),
    );

    assert!(engine
        .get_thread_filtered("weles-hidden", false, None, 0)
        .await
        .is_none());

    let detail = engine
        .get_thread_filtered("weles-hidden", true, None, 0)
        .await
        .expect("include_internal should reveal hidden thread");
    assert_eq!(detail.thread.id, "weles-hidden");
    assert!(!detail.messages_truncated);
}

#[tokio::test]
async fn thread_persistence_round_trips_offload_and_structural_refs() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-offload-refs";

    let mut message = AgentMessage::user("persist me", 1_000);
    message.offloaded_payload_id = Some("payload-123".to_string());
    message.structural_refs = vec![
        "artifact://summary/1".to_string(),
        "skill://brainstorming".to_string(),
    ];

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Thread offload refs",
            false,
            1_000,
            1_000,
            vec![message],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    rehydrated.hydrate().await.expect("hydrate should succeed");

    let thread = rehydrated
        .get_thread(thread_id)
        .await
        .expect("thread should be restored after hydrate");
    let restored = thread
        .messages
        .first()
        .expect("thread should restore first message");

    assert_eq!(
        restored.offloaded_payload_id.as_deref(),
        Some("payload-123")
    );
    assert_eq!(
        restored.structural_refs,
        vec![
            "artifact://summary/1".to_string(),
            "skill://brainstorming".to_string(),
        ]
    );
}
