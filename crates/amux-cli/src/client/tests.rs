use super::agent_protocol::AgentBridgeCommand;

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
