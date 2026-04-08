use super::*;
use crate::agent::llm_client::{StructuredUpstreamFailure, UPSTREAM_DIAGNOSTICS_MARKER};
use crate::agent::types::{AgentEvent, TaskStatus};
use crate::session_manager::SessionManager;
use rusqlite::OptionalExtension;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

mod part1;
mod part2;
mod part3;
mod part4;
mod part5;
mod part6;

fn http_request_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(headers: &str) -> Option<usize> {
    headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())
            .flatten()
    })
}

async fn read_http_request(socket: &mut tokio::net::TcpStream, context: &str) -> String {
    let mut buffer = Vec::new();
    let mut chunk = vec![0u8; 8192];
    let mut expected_body_len = None::<usize>;
    let mut header_end = None::<usize>;

    loop {
        let read = socket.read(&mut chunk).await.unwrap_or_else(|error| {
            panic!("read {context}: {error}");
        });
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);

        if header_end.is_none() {
            header_end = http_request_header_end(&buffer);
            if let Some(end) = header_end {
                let headers = String::from_utf8_lossy(&buffer[..end]);
                expected_body_len = parse_content_length(&headers);
            }
        }

        if let Some(end) = header_end {
            let body_len = buffer.len().saturating_sub(end + 4);
            if expected_body_len.is_none_or(|expected| body_len >= expected) {
                break;
            }
        }
    }

    String::from_utf8_lossy(&buffer).to_string()
}

fn request_body(request: &str) -> String {
    request
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or_default()
        .to_string()
}

async fn spawn_tool_call_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind tool call server");
    let addr = listener.local_addr().expect("tool call server local addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 65536];
                let _ = socket.read(&mut buffer).await;
                let response = concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "content-type: text/event-stream\r\n",
                    "cache-control: no-cache\r\n",
                    "connection: close\r\n",
                    "\r\n",
                    "data: {\"choices\":[{\"delta\":{\"content\":\"Checking state\"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_policy_1\",\"function\":{\"name\":\"definitely_unknown_tool\",\"arguments\":\"{}\"}}]}}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                    "data: [DONE]\n\n"
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write tool call response");
            });
        }
    });

    format!("http://{addr}/v1")
}

async fn spawn_recording_assistant_server(
    recorded_bodies: Arc<StdMutex<VecDeque<String>>>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind recording assistant server");
    let addr = listener
        .local_addr()
        .expect("recording assistant server local addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let recorded_bodies = recorded_bodies.clone();
            tokio::spawn(async move {
                let request = read_http_request(&mut socket, "recording assistant request").await;
                let body = request_body(&request);
                recorded_bodies
                    .lock()
                    .expect("lock recorded assistant request log")
                    .push_back(body);

                let response = concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "content-type: text/event-stream\r\n",
                    "cache-control: no-cache\r\n",
                    "connection: close\r\n",
                    "\r\n",
                    "data: {\"choices\":[{\"delta\":{\"content\":\"Acknowledged.\"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                    "data: [DONE]\n\n"
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write recording assistant response");
            });
        }
    });

    format!("http://{addr}/v1")
}

async fn spawn_recording_request_server(
    recorded_requests: Arc<StdMutex<VecDeque<String>>>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind recording request server");
    let addr = listener
        .local_addr()
        .expect("recording request server local addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let recorded_requests = recorded_requests.clone();
            tokio::spawn(async move {
                let request = read_http_request(&mut socket, "recording request").await;
                recorded_requests
                    .lock()
                    .expect("lock recorded requests")
                    .push_back(request);

                let response = concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "content-type: text/event-stream\r\n",
                    "cache-control: no-cache\r\n",
                    "connection: close\r\n",
                    "\r\n",
                    "data: {\"choices\":[{\"delta\":{\"content\":\"Acknowledged.\"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                    "data: [DONE]\n\n"
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write recording request response");
            });
        }
    });

    format!("http://{addr}/v1")
}

async fn spawn_scripted_tool_call_server(script: Vec<(String, String)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind scripted tool call server");
    let addr = listener
        .local_addr()
        .expect("scripted tool call server local addr");
    let script = Arc::new(script);
    let next_response = Arc::new(AtomicUsize::new(0));

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let script = script.clone();
            let next_response = next_response.clone();
            tokio::spawn(async move {
                let _request = read_http_request(&mut socket, "scripted tool call request").await;
                let index = next_response.fetch_add(1, Ordering::SeqCst);
                let response = if let Some((tool_name, arguments)) = script.get(index) {
                    let chunk = serde_json::json!({
                        "choices": [{
                            "delta": {
                                "tool_calls": [{
                                    "index": 0,
                                    "id": format!("call_scripted_{index}"),
                                    "function": {
                                        "name": tool_name,
                                        "arguments": arguments,
                                    }
                                }]
                            }
                        }],
                        "usage": {
                            "prompt_tokens": 7,
                            "completion_tokens": 3
                        }
                    })
                    .to_string();
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\ndata: {chunk}\n\ndata: [DONE]\n\n"
                    )
                } else {
                    concat!(
                        "HTTP/1.1 200 OK\r\n",
                        "content-type: text/event-stream\r\n",
                        "cache-control: no-cache\r\n",
                        "connection: close\r\n",
                        "\r\n",
                        "data: {\"choices\":[{\"delta\":{\"content\":\"Acknowledged.\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                        "data: [DONE]\n\n"
                    )
                    .to_string()
                };
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write scripted tool call response");
            });
        }
    });

    format!("http://{addr}/v1")
}

async fn spawn_policy_pivot_tool_call_server(
    recorded_bodies: Arc<StdMutex<VecDeque<String>>>,
    readable_path: String,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind policy pivot tool server");
    let addr = listener
        .local_addr()
        .expect("policy pivot tool server local addr");
    let assistant_turns = Arc::new(AtomicUsize::new(0));

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let recorded_bodies = recorded_bodies.clone();
            let assistant_turns = assistant_turns.clone();
            let readable_path = readable_path.clone();
            tokio::spawn(async move {
                let request = read_http_request(&mut socket, "policy pivot tool request").await;
                let body = request_body(&request);
                recorded_bodies
                    .lock()
                    .expect("lock policy pivot request log")
                    .push_back(body.clone());

                let response = if body.contains(
                    "tamux orchestrator should continue, pivot, escalate, or halt_retries",
                ) {
                    String::from(concat!(
                        "HTTP/1.1 200 OK\r\n",
                        "content-type: text/event-stream\r\n",
                        "cache-control: no-cache\r\n",
                        "connection: close\r\n",
                        "\r\n",
                        "data: {\"choices\":[{\"delta\":{\"content\":\"{\\\"action\\\":\\\"pivot\\\",\\\"reason\\\":\\\"Low progress suggests a fresh strategy.\\\",\\\"strategy_hint\\\":\\\"Inspect the workspace state before more reads.\\\"}\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                        "data: [DONE]\n\n"
                    ))
                } else if assistant_turns.fetch_add(1, Ordering::SeqCst) == 0 {
                    let tool_args = serde_json::json!({
                        "path": readable_path,
                        "offset": 0,
                        "limit": 1,
                    })
                    .to_string();
                    let chunk_one = serde_json::json!({
                        "choices": [{
                            "delta": {
                                "content": "Checking state"
                            }
                        }]
                    })
                    .to_string();
                    let chunk_two = serde_json::json!({
                        "choices": [{
                            "delta": {
                                "tool_calls": [{
                                    "index": 0,
                                    "id": "call_policy_read_1",
                                    "function": {
                                        "name": "read_file",
                                        "arguments": tool_args,
                                    }
                                }]
                            }
                        }],
                        "usage": {
                            "prompt_tokens": 7,
                            "completion_tokens": 3,
                        }
                    })
                    .to_string();
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\ndata: {chunk_one}\n\ndata: {chunk_two}\n\ndata: [DONE]\n\n"
                    )
                } else {
                    String::from(concat!(
                        "HTTP/1.1 200 OK\r\n",
                        "content-type: text/event-stream\r\n",
                        "cache-control: no-cache\r\n",
                        "connection: close\r\n",
                        "\r\n",
                        "data: {\"choices\":[{\"delta\":{\"content\":\"Done. I will inspect the workspace next.\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                        "data: [DONE]\n\n"
                    ))
                };
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write policy pivot tool response");
            });
        }
    });

    format!("http://{addr}/v1")
}

async fn spawn_transport_incompatibility_server(request_counter: Arc<AtomicUsize>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind transport incompatibility server");
    let addr = listener
        .local_addr()
        .expect("transport incompatibility server addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let request_counter = request_counter.clone();
            tokio::spawn(async move {
                request_counter.fetch_add(1, Ordering::SeqCst);
                let mut buffer = vec![0u8; 65536];
                let _ = socket
                    .read(&mut buffer)
                    .await
                    .expect("read incompatibility request");
                let body = r#"{"error":{"message":"Responses API not supported for this provider endpoint"}}"#;
                let response = format!(
                    "HTTP/1.1 405 Method Not Allowed\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write incompatibility response");
            });
        }
    });

    format!("http://{addr}/v1")
}

async fn spawn_transient_transport_failure_server(request_counter: Arc<AtomicUsize>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind transient transport failure server");
    let addr = listener
        .local_addr()
        .expect("transient transport failure server addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let request_counter = request_counter.clone();
            tokio::spawn(async move {
                request_counter.fetch_add(1, Ordering::SeqCst);
                let mut buffer = vec![0u8; 65536];
                let _ = socket
                    .read(&mut buffer)
                    .await
                    .expect("read transient transport request");
                let _ = socket.shutdown().await;
            });
        }
    });

    format!("http://{addr}/v1")
}

async fn spawn_transient_failure_then_blocking_server(
    request_counter: Arc<AtomicUsize>,
    release_second_request: Arc<tokio::sync::Notify>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind transient blocking retry server");
    let addr = listener
        .local_addr()
        .expect("transient blocking retry server addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let request_counter = request_counter.clone();
            let release_second_request = release_second_request.clone();
            tokio::spawn(async move {
                let attempt = request_counter.fetch_add(1, Ordering::SeqCst);
                let mut buffer = vec![0u8; 65536];
                let _ = socket
                    .read(&mut buffer)
                    .await
                    .expect("read transient blocking retry request");

                if attempt == 0 {
                    let _ = socket.shutdown().await;
                    return;
                }

                release_second_request.notified().await;
                let _ = socket.shutdown().await;
            });
        }
    });

    format!("http://{addr}/v1")
}

async fn spawn_transient_failures_then_blocking_success_server(
    request_counter: Arc<AtomicUsize>,
    failures_before_success: usize,
    success_request_started: Arc<tokio::sync::Notify>,
    release_success_request: Arc<tokio::sync::Notify>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind transient blocking success server");
    let addr = listener
        .local_addr()
        .expect("transient blocking success server addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let request_counter = request_counter.clone();
            let success_request_started = success_request_started.clone();
            let release_success_request = release_success_request.clone();
            tokio::spawn(async move {
                let attempt = request_counter.fetch_add(1, Ordering::SeqCst);
                let _request =
                    read_http_request(&mut socket, "transient blocking success request").await;

                if attempt < failures_before_success {
                    let _ = socket.shutdown().await;
                    return;
                }

                success_request_started.notify_waiters();
                release_success_request.notified().await;
                let response = concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "content-type: text/event-stream\r\n",
                    "cache-control: no-cache\r\n",
                    "connection: close\r\n",
                    "\r\n",
                    "data: {\"choices\":[{\"delta\":{\"content\":\"Recovered on a fresh stream\"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                    "data: [DONE]\n\n"
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write transient blocking success response");
            });
        }
    });

    format!("http://{addr}/v1")
}

async fn spawn_timeout_failures_then_blocking_success_server(
    request_counter: Arc<AtomicUsize>,
    failures_before_success: usize,
    success_request_started: Arc<tokio::sync::Notify>,
    release_success_request: Arc<tokio::sync::Notify>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind timeout blocking success server");
    let addr = listener
        .local_addr()
        .expect("timeout blocking success server addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let request_counter = request_counter.clone();
            let success_request_started = success_request_started.clone();
            let release_success_request = release_success_request.clone();
            tokio::spawn(async move {
                let attempt = request_counter.fetch_add(1, Ordering::SeqCst);
                let _request =
                    read_http_request(&mut socket, "timeout blocking success request").await;

                if attempt < failures_before_success {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    let _ = socket.shutdown().await;
                    return;
                }

                success_request_started.notify_waiters();
                release_success_request.notified().await;
                let response = concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "content-type: text/event-stream\r\n",
                    "cache-control: no-cache\r\n",
                    "connection: close\r\n",
                    "\r\n",
                    "data: {\"choices\":[{\"delta\":{\"content\":\"Recovered after timeout stream restart\"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                    "data: [DONE]\n\n"
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write timeout blocking success response");
            });
        }
    });

    format!("http://{addr}/v1")
}

async fn spawn_anthropic_rebuild_sensitive_retry_server(
    recorded_bodies: Arc<StdMutex<VecDeque<String>>>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind anthropic fresh retry server");
    let addr = listener
        .local_addr()
        .expect("anthropic fresh retry server addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let recorded_bodies = recorded_bodies.clone();
            tokio::spawn(async move {
                let request = read_http_request(&mut socket, "anthropic fresh retry request").await;
                let body = request_body(&request);
                recorded_bodies
                    .lock()
                    .expect("lock recorded anthropic bodies")
                    .push_back(body.clone());

                if body.contains("hello again") {
                    let response_body = concat!(
                        "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":1}}}\n\n",
                        "data: {\"type\":\"content_block_start\",\"content_block\":{\"type\":\"text\"}}\n\n",
                        "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"recovered\"}}\n\n",
                        "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":1}}\n\n",
                        "data: {\"type\":\"message_stop\"}\n\n"
                    );
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        response_body.len(),
                        response_body
                    );
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write anthropic recovery response");
                    return;
                }

                socket
                    .write_all(b"HTTP/9.9 200 OK\r\ncontent-length: 0\r\n\r\n")
                    .await
                    .expect("write malformed anthropic retry response");
            });
        }
    });

    format!("http://{addr}/anthropic")
}

async fn latest_trace_outcome_for_task(root: &std::path::Path, task_id: &str) -> Option<String> {
    let store = crate::history::HistoryStore::new_test_store(root)
        .await
        .expect("open history store");
    let task_id = task_id.to_string();
    store
        .conn
        .call(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT outcome FROM execution_traces WHERE task_id = ?1 ORDER BY created_at DESC LIMIT 1",
                    rusqlite::params![task_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?)
        })
        .await
        .expect("query trace outcome")
}
