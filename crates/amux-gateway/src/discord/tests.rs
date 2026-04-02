use serde_json::json;

use super::*;
use crate::runtime::GatewayProviderEvent;
use crate::test_support::{HttpResponse, TestHttpServer};

#[test]
fn discord_timestamp_secs_decodes_snowflake_epoch() {
    assert_eq!(discord_timestamp_secs("175928847299117063"), 1462015105);
}

#[tokio::test]
async fn discord_provider_polls_and_filters_bot_messages() {
    let server = TestHttpServer::spawn(vec![
        HttpResponse::ok(json!({ "id": "bot-user" }).to_string()),
        HttpResponse::ok(
            json!([
                {
                    "id": "100",
                    "content": "historical",
                    "author": { "username": "seed-user", "bot": false }
                }
            ])
            .to_string(),
        ),
        HttpResponse::ok(
            json!([
                {
                    "id": "102",
                    "content": "ignore me",
                    "author": { "username": "gateway-bot", "bot": true }
                },
                {
                    "id": "101",
                    "content": "hello from discord",
                    "author": { "username": "alice", "bot": false }
                }
            ])
            .to_string(),
        ),
    ])
    .await
    .expect("spawn server");

    let bootstrap = GatewayProviderBootstrap {
        platform: "discord".to_string(),
        enabled: true,
        credentials_json: json!({ "token": "discord-token" }).to_string(),
        config_json: json!({
            "api_base": server.base_url,
            "channel_filter": "D123",
            "poll_interval_ms": 0
        })
        .to_string(),
    };

    let mut provider = DiscordProvider::from_bootstrap(&bootstrap)
        .expect("provider bootstrap")
        .expect("provider enabled");
    provider.connect().await.expect("connect succeeds");

    let seeded = provider.recv().await.expect("seed poll succeeds");
    assert!(seeded.is_some(), "first poll should seed a cursor boundary");

    loop {
        let event = provider
            .recv()
            .await
            .expect("poll succeeds")
            .expect("provider should emit an event");
        if let GatewayProviderEvent::Incoming(message) = event {
            assert_eq!(message.platform, "discord");
            assert_eq!(message.channel_id, "D123");
            assert_eq!(message.user_id, "alice");
            assert_eq!(message.text, "hello from discord");
            break;
        }
    }
}

#[tokio::test]
async fn discord_provider_waits_for_poll_interval_before_refetching() {
    let server = TestHttpServer::spawn(vec![
        HttpResponse::ok(json!({ "id": "bot-user" }).to_string()),
        HttpResponse::ok(
            json!([
                {
                    "id": "100",
                    "content": "historical",
                    "author": { "username": "seed-user", "bot": false }
                }
            ])
            .to_string(),
        ),
        HttpResponse::ok(
            json!([
                {
                    "id": "101",
                    "content": "should not be fetched yet",
                    "author": { "username": "alice", "bot": false }
                }
            ])
            .to_string(),
        ),
    ])
    .await
    .expect("spawn server");

    let bootstrap = GatewayProviderBootstrap {
        platform: "discord".to_string(),
        enabled: true,
        credentials_json: json!({ "token": "discord-token" }).to_string(),
        config_json: json!({
            "api_base": server.base_url,
            "channel_filter": "D123"
        })
        .to_string(),
    };

    let mut provider = DiscordProvider::from_bootstrap(&bootstrap)
        .expect("provider bootstrap")
        .expect("provider enabled");
    provider.connect().await.expect("connect succeeds");

    let seeded = provider.recv().await.expect("seed poll succeeds");
    assert!(seeded.is_some(), "first poll should seed a cursor boundary");

    let next = provider
        .recv()
        .await
        .expect("recv should not fail during poll cooldown");
    assert!(
        next.is_none(),
        "provider should wait for the poll interval before refetching"
    );
    assert_eq!(
        server.requests().len(),
        2,
        "provider should only perform connect + initial seed requests"
    );
}
