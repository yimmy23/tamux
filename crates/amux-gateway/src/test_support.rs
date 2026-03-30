use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status_code: u16,
    pub body: String,
}

impl HttpResponse {
    pub fn ok(body: impl Into<String>) -> Self {
        Self {
            status_code: 200,
            body: body.into(),
        }
    }
}

pub struct TestHttpServer {
    pub base_url: String,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
}

impl TestHttpServer {
    pub async fn spawn(responses: Vec<HttpResponse>) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let requests = Arc::new(Mutex::new(Vec::new()));
        let requests_task = Arc::clone(&requests);
        let responses = Arc::new(Mutex::new(VecDeque::from(responses)));
        let responses_task = Arc::clone(&responses);

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = vec![0_u8; 16 * 1024];
                let read = match socket.read(&mut buffer).await {
                    Ok(read) => read,
                    Err(_) => continue,
                };
                if read == 0 {
                    continue;
                }

                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let mut lines = request.lines();
                let request_line = lines.next().unwrap_or_default();
                let mut request_parts = request_line.split_whitespace();
                let method = request_parts.next().unwrap_or_default().to_string();
                let path = request_parts.next().unwrap_or_default().to_string();
                let body = request
                    .split("\r\n\r\n")
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();

                requests_task.lock().unwrap().push(RecordedRequest {
                    method,
                    path,
                    body,
                });

                let response = responses_task
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or_else(|| HttpResponse::ok("{}"));
                let payload = format!(
                    "HTTP/1.1 {} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    response.status_code,
                    response.body.len(),
                    response.body
                );
                let _ = socket.write_all(payload.as_bytes()).await;
            }
        });

        Ok(Self {
            base_url: format!("http://{addr}"),
            requests,
        })
    }

    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }
}
