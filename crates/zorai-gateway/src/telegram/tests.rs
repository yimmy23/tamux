use serde_json::json;
use zorai_protocol::{GatewayCursorState, GatewaySendRequest};

use super::*;
use crate::runtime::GatewayProviderEvent;
use crate::test_support::{HttpResponse, TestHttpServer};

#[tokio::test]
async fn telegram_provider_long_poll_updates_cursor_and_sends_replies() {
    let server = TestHttpServer::spawn(vec![
        HttpResponse::ok(
            json!({ "ok": true, "result": { "id": 1, "username": "zorai" } }).to_string(),
        ),
        HttpResponse::ok(
            json!({
                "ok": true,
                "result": [
                    {
                        "update_id": 101,
                        "message": {
                            "message_id": 42,
                            "text": "hello from telegram",
                            "chat": { "id": 777 },
                            "from": { "username": "alice" }
                        }
                    }
                ]
            })
            .to_string(),
        ),
        HttpResponse::ok(json!({ "ok": true, "result": { "message_id": 99 } }).to_string()),
    ])
    .await
    .expect("spawn server");

    let bootstrap = GatewayProviderBootstrap {
        platform: "telegram".to_string(),
        enabled: true,
        credentials_json: json!({ "token": "telegram-token" }).to_string(),
        config_json: json!({
            "api_base": server.base_url,
            "allowed_chats": "777"
        })
        .to_string(),
    };

    let mut provider = TelegramProvider::from_bootstrap(&bootstrap)
        .expect("provider bootstrap")
        .expect("provider enabled");
    provider.connect().await.expect("connect succeeds");

    let mut saw_incoming = false;
    let mut saw_cursor = false;
    for _ in 0..4 {
        let event = provider
            .recv()
            .await
            .expect("recv succeeds")
            .expect("provider should emit an event");
        match event {
            GatewayProviderEvent::Incoming(message) => {
                saw_incoming = true;
                assert_eq!(message.platform, "telegram");
                assert_eq!(message.channel_id, "777");
                assert_eq!(message.user_id, "alice");
                assert_eq!(message.text, "hello from telegram");
            }
            GatewayProviderEvent::CursorUpdate(GatewayCursorState {
                platform,
                channel_id,
                cursor_value,
                ..
            }) => {
                saw_cursor = true;
                assert_eq!(platform, "telegram");
                assert_eq!(channel_id, "global");
                assert_eq!(cursor_value, "101");
            }
            GatewayProviderEvent::HealthUpdate(_) => {}
            other => panic!("unexpected telegram event: {other:?}"),
        }
        if saw_incoming && saw_cursor {
            break;
        }
    }
    assert!(saw_incoming);
    assert!(saw_cursor);

    let outcome = provider
        .send(GatewaySendRequest {
            correlation_id: "send-1".to_string(),
            platform: "telegram".to_string(),
            channel_id: "777".to_string(),
            thread_id: Some("42".to_string()),
            content: "reply back".to_string(),
        })
        .await
        .expect("send succeeds");

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].method, "GET");
    assert!(requests[0].path.contains("/getMe"));
    assert_eq!(requests[1].method, "GET");
    assert!(requests[1].path.contains("/getUpdates"));
    assert_eq!(requests[2].method, "POST");
    assert!(requests[2].path.contains("/sendMessage"));
    assert!(requests[2].body.contains("\"chat_id\":\"777\""));
    assert!(requests[2].body.contains("\"reply_to_message_id\":42"));
    assert_eq!(outcome.channel_id, "777");
    assert_eq!(outcome.delivery_id.as_deref(), Some("99"));
}
