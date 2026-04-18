use super::*;
use amux_shared::providers::PROVIDER_ID_CUSTOM;
use tempfile::tempdir;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

#[tokio::test]
async fn truncated_stream_with_accumulated_content_still_emits_done_event() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind truncated stream server");
    let addr = listener.local_addr().expect("truncated stream server addr");

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let _ = read_http_request(&mut socket, "truncated stream request").await;
        let response = concat!(
            "HTTP/1.1 200 OK\r\n",
            "content-type: text/event-stream\r\n",
            "cache-control: no-cache\r\n",
            "connection: close\r\n",
            "\r\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"Recovered terminal text\"}}]}\n\n"
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write truncated stream response");
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.subscribe();
    let thread_id = "thread-truncated-stream-done";
    engine.threads.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::AgentThread {
            id: thread_id.to_string(),
            agent_name: None,
            title: "Truncated stream done".to_string(),
            messages: vec![crate::agent::types::AgentMessage::user("hello", 1)],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
            created_at: 1,
            updated_at: 1,
        },
    );

    engine
        .send_message_inner(
            Some(thread_id),
            "hello",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("truncated stream should still complete the turn");

    let mut saw_done = false;
    while let Ok(event) = events.try_recv() {
        if let AgentEvent::Done {
            thread_id: event_thread_id,
            ..
        } = event
        {
            if event_thread_id == thread_id {
                saw_done = true;
            }
        }
    }

    assert!(saw_done, "truncated stream should still emit a done event");
    let thread = engine
        .get_thread(thread_id)
        .await
        .expect("thread should exist");
    assert_eq!(
        thread
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some("Recovered terminal text")
    );
}
