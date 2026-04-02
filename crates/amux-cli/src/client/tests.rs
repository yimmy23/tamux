use super::agent_api::parse_config_set_response;
use super::agent_bridge::handle_message_for_test as handle_bridge_message;
use super::agent_bridge::initial_bridge_messages;
use super::agent_protocol::AgentBridgeCommand;
use super::skill_api::{
    parse_skill_import_terminal_response, parse_skill_publish_terminal_response,
};
use amux_protocol::{ClientMessage, DaemonCodec, DaemonMessage};
use futures::SinkExt;
use tokio_util::codec::Framed;

#[test]
fn operator_profile_bridge_commands_deserialize() {
    let json = r#"{"type":"start-operator-profile-session","kind":"onboarding"}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("start-operator-profile-session must deserialize");
    match cmd {
        AgentBridgeCommand::StartOperatorProfileSession { kind } => {
            assert_eq!(kind, "onboarding");
        }
        _ => panic!("unexpected variant for start-operator-profile-session"),
    }

    let json = r#"{"type":"next-operator-profile-question","session_id":"s1"}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("next-operator-profile-question must deserialize");
    match cmd {
        AgentBridgeCommand::NextOperatorProfileQuestion { session_id } => {
            assert_eq!(session_id, "s1");
        }
        _ => panic!("unexpected variant for next-operator-profile-question"),
    }

    let json = r#"{"type":"submit-operator-profile-answer","session_id":"s1","question_id":"q1","answer_json":"\"yes\""}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("submit-operator-profile-answer must deserialize");
    match cmd {
        AgentBridgeCommand::SubmitOperatorProfileAnswer {
            session_id,
            question_id,
            answer_json,
        } => {
            assert_eq!(session_id, "s1");
            assert_eq!(question_id, "q1");
            assert_eq!(answer_json, "\"yes\"");
        }
        _ => panic!("unexpected variant for submit-operator-profile-answer"),
    }

    let json = r#"{"type":"skip-operator-profile-question","session_id":"s1","question_id":"q1"}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("skip-operator-profile-question must deserialize");
    match cmd {
        AgentBridgeCommand::SkipOperatorProfileQuestion {
            session_id,
            question_id,
            reason,
        } => {
            assert_eq!(session_id, "s1");
            assert_eq!(question_id, "q1");
            assert!(reason.is_none());
        }
        _ => panic!("unexpected variant for skip-operator-profile-question"),
    }

    let json = r#"{"type":"defer-operator-profile-question","session_id":"s1","question_id":"q1","defer_until_unix_ms":1700000000000}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("defer-operator-profile-question must deserialize");
    match cmd {
        AgentBridgeCommand::DeferOperatorProfileQuestion {
            session_id,
            question_id,
            defer_until_unix_ms,
        } => {
            assert_eq!(session_id, "s1");
            assert_eq!(question_id, "q1");
            assert_eq!(defer_until_unix_ms, Some(1700000000000));
        }
        _ => panic!("unexpected variant for defer-operator-profile-question"),
    }

    let json = r#"{"type":"get-operator-profile-summary"}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("get-operator-profile-summary must deserialize");
    assert!(
        matches!(cmd, AgentBridgeCommand::GetOperatorProfileSummary),
        "expected GetOperatorProfileSummary"
    );

    let json =
        r#"{"type":"set-operator-profile-consent","consent_key":"analytics","granted":true}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("set-operator-profile-consent must deserialize");
    match cmd {
        AgentBridgeCommand::SetOperatorProfileConsent {
            consent_key,
            granted,
        } => {
            assert_eq!(consent_key, "analytics");
            assert!(granted);
        }
        _ => panic!("unexpected variant for set-operator-profile-consent"),
    }
}

#[test]
fn operator_profile_bridge_commands_deserialize_failures() {
    let result: Result<AgentBridgeCommand, _> =
        serde_json::from_str(r#"{"type":"not-a-real-operator-command"}"#);
    assert!(result.is_err(), "unknown type should not deserialize");

    let result: Result<AgentBridgeCommand, _> =
        serde_json::from_str(r#"{"type":"start-operator-profile-session"}"#);
    assert!(result.is_err(), "missing `kind` should not deserialize");

    let result: Result<AgentBridgeCommand, _> =
        serde_json::from_str(r#"{"type":"next-operator-profile-question"}"#);
    assert!(
        result.is_err(),
        "missing `session_id` should not deserialize"
    );

    let result: Result<AgentBridgeCommand, _> = serde_json::from_str(
        r#"{"type":"set-operator-profile-consent","consent_key":"analytics","granted":"yes"}"#,
    );
    assert!(
        result.is_err(),
        "wrong field type for `granted` should not deserialize"
    );

    let result: Result<AgentBridgeCommand, _> = serde_json::from_str(
        r#"{"type":"submit-operator-profile-answer","session_id":"s1","answer_json":"\"v\""}"#,
    );
    assert!(
        result.is_err(),
        "missing `question_id` should not deserialize"
    );

    let result: Result<AgentBridgeCommand, _> = serde_json::from_str(
        r#"{"type":"defer-operator-profile-question","session_id":"s1","question_id":"q1","defer_until_unix_ms":"soon"}"#,
    );
    assert!(
        result.is_err(),
        "wrong type for `defer_until_unix_ms` should not deserialize"
    );
}

#[test]
fn agent_protocol_codex_auth_bridge_commands_deserialize() {
    fn parse(json: &str) -> AgentBridgeCommand {
        serde_json::from_str(json).expect("codex auth command must deserialize")
    }

    assert!(matches!(
        parse(r#"{"type":"openai-codex-auth-status"}"#),
        AgentBridgeCommand::GetOpenAICodexAuthStatus
    ));
    assert!(matches!(
        parse(r#"{"type":"openai-codex-auth-login"}"#),
        AgentBridgeCommand::LoginOpenAICodex
    ));
    assert!(matches!(
        parse(r#"{"type":"openai-codex-auth-logout"}"#),
        AgentBridgeCommand::LogoutOpenAICodex
    ));
}

#[test]
fn agent_bridge_declares_async_command_capability_on_connect() {
    let messages = initial_bridge_messages();
    assert_eq!(messages.len(), 2);
    assert!(matches!(messages[0], ClientMessage::AgentSubscribe));
    match &messages[1] {
        ClientMessage::AgentDeclareAsyncCommandCapability { capability } => {
            assert_eq!(capability.version, 1);
            assert!(capability.supports_operation_acceptance);
        }
        other => panic!("expected async command capability declaration, got {other:?}"),
    }
}

#[test]
fn skill_import_terminal_response_ignores_operation_acceptance() {
    let response = parse_skill_import_terminal_response(DaemonMessage::OperationAccepted {
        operation_id: "op-skill-import-1".to_string(),
        kind: "skill_import".to_string(),
        dedup: None,
        revision: 1,
    });
    assert!(response.is_none(), "operation acceptance is not terminal");
}

#[test]
fn skill_import_terminal_response_extracts_result_payload() {
    let response = parse_skill_import_terminal_response(DaemonMessage::SkillImportResult {
        operation_id: Some("op-skill-import-1".to_string()),
        success: true,
        message: "Imported skill".to_string(),
        variant_id: Some("variant-1".to_string()),
        scan_verdict: Some("warn".to_string()),
        findings_count: 0,
    })
    .expect("terminal response")
    .expect("successful parse");

    assert_eq!(
        response,
        (
            true,
            "Imported skill".to_string(),
            Some("variant-1".to_string()),
            Some("warn".to_string()),
            0
        )
    );
}

#[test]
fn skill_publish_terminal_response_ignores_operation_acceptance() {
    let response = parse_skill_publish_terminal_response(DaemonMessage::OperationAccepted {
        operation_id: "op-skill-publish-1".to_string(),
        kind: "skill_publish".to_string(),
        dedup: None,
        revision: 1,
    });
    assert!(response.is_none(), "operation acceptance is not terminal");
}

#[test]
fn skill_publish_terminal_response_extracts_result_payload() {
    let response = parse_skill_publish_terminal_response(DaemonMessage::SkillPublishResult {
        operation_id: Some("op-skill-publish-1".to_string()),
        success: true,
        message: "Published skill".to_string(),
    })
    .expect("terminal response")
    .expect("successful parse");

    assert_eq!(response, (true, "Published skill".to_string()));
}

#[test]
fn config_set_response_accepts_operation_acceptance() {
    parse_config_set_response(DaemonMessage::OperationAccepted {
        operation_id: "op-config-set-1".to_string(),
        kind: "config_set_item".to_string(),
        dedup: None,
        revision: 1,
    })
    .expect("operation acceptance should be treated as success");
}

#[tokio::test]
async fn agent_bridge_ignores_operation_acceptance_messages() {
    let (client_side, server_side) = tokio::io::duplex(1024);
    let mut bridge = Framed::new(client_side, amux_protocol::AmuxCodec);
    let mut daemon = Framed::new(server_side, DaemonCodec);

    daemon
        .send(DaemonMessage::OperationAccepted {
            operation_id: "op-bridge-1".to_string(),
            kind: "agent_set_sub_agent".to_string(),
            dedup: None,
            revision: 1,
        })
        .await
        .expect("send operation acceptance");

    let should_continue = handle_bridge_message(&mut bridge)
        .await
        .expect("operation acceptance should not fail bridge handling");

    assert!(should_continue);
}
