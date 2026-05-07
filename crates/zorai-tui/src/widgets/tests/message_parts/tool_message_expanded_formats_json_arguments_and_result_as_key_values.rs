use super::super::*;
use super::markdown_renders_bold_to_apply_patch_tool_message_expanded_renders_diff::*;
use crate::state::chat::{AgentMessage, MessageRole, TranscriptMode};
use crate::theme::ThemeTokens;
#[test]
fn tool_message_expanded_formats_json_arguments_and_result_as_key_values() {
    let msg = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("run_terminal_command".into()),
        tool_status: Some("done".into()),
        tool_arguments: Some(
            serde_json::json!({
                "command": "cargo test -p zorai-tui",
                "cwd": "/tmp/workspace",
                "options": {
                    "timeout_ms": 120000,
                    "background": false
                }
            })
            .to_string(),
        ),
        content: serde_json::json!({
            "status": "ok",
            "exit_code": 0,
            "stdout_lines": 24
        })
        .to_string(),
        ..Default::default()
    };

    let mut exp_tools = empty_tools();
    exp_tools.insert(0);
    let lines = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        80,
        &empty_expanded(),
        &exp_tools,
    );

    let plain = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        plain.contains("command: cargo test -p zorai-tui"),
        "expected flattened command field, got: {plain}"
    );
    assert!(
        plain.contains("options.timeout_ms: 120000"),
        "expected flattened nested field, got: {plain}"
    );
    assert!(
        plain.contains("exit_code: 0"),
        "expected structured JSON result field, got: {plain}"
    );
    assert!(
        !plain.contains("{\"command\":"),
        "expected structured view instead of raw JSON blob, got: {plain}"
    );
}

#[test]
fn tool_message_with_content_renders_compact() {
    let msg = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("list_workspaces".into()),
        tool_status: Some("done".into()),
        content: "Workspace Default:\n  Surface: Infinite Canvas".into(),
        ..Default::default()
    };
    let lines = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        80,
        &empty_expanded(),
        &empty_tools(),
    );
    assert_eq!(lines.len(), 1);
}

#[test]
fn tool_message_blocked_and_flagged_render_distinct_markers() {
    let blocked_msg = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("bash_command".into()),
        tool_status: Some("error".into()),
        weles_review: Some(crate::state::chat::WelesReviewMetaVm {
            weles_reviewed: true,
            verdict: "block".into(),
            reasons: vec!["network access requested".into()],
            security_override_mode: None,
            audit_id: Some("audit-block-1".into()),
        }),
        ..Default::default()
    };
    let flagged_msg = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("bash_command".into()),
        tool_status: Some("done".into()),
        weles_review: Some(crate::state::chat::WelesReviewMetaVm {
            weles_reviewed: true,
            verdict: "flag_only".into(),
            reasons: vec!["shell-based Python bypass".into()],
            security_override_mode: Some("yolo".into()),
            audit_id: Some("audit-flag-1".into()),
        }),
        ..Default::default()
    };

    let blocked_lines = message_to_lines(
        &blocked_msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        80,
        &empty_expanded(),
        &empty_tools(),
    );
    let flagged_lines = message_to_lines(
        &flagged_msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        80,
        &empty_expanded(),
        &empty_tools(),
    );

    let blocked_plain = blocked_lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect::<String>();
    let flagged_plain = flagged_lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect::<String>();

    assert!(
        blocked_plain.contains("blocked"),
        "expected blocked marker, got: {blocked_plain}"
    );
    assert!(
        flagged_plain.contains("flagged"),
        "expected flagged marker, got: {flagged_plain}"
    );
}

#[test]
fn tool_message_unreviewed_allow_does_not_render_reviewed_marker() {
    let msg = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("list_workspaces".into()),
        tool_status: Some("done".into()),
        weles_review: Some(crate::state::chat::WelesReviewMetaVm {
            weles_reviewed: false,
            verdict: "allow".into(),
            reasons: vec!["governance_not_run".into()],
            security_override_mode: None,
            audit_id: None,
        }),
        ..Default::default()
    };

    let mut exp_tools = empty_tools();
    exp_tools.insert(0);
    let lines = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        80,
        &empty_expanded(),
        &exp_tools,
    );
    let plain = lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect::<String>();

    assert!(
        plain.contains("unreviewed"),
        "expected unreviewed marker, got: {plain}"
    );
    assert!(
        !plain.contains("weles: reviewed degraded"),
        "unexpected contradictory marker, got: {plain}"
    );
}

#[test]
fn tool_message_expanded_shows_weles_rationale_and_degraded_state() {
    let msg = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("bash_command".into()),
        tool_status: Some("done".into()),
        tool_arguments: Some("python -c 'print(1)'".into()),
        content: "ok".into(),
        weles_review: Some(crate::state::chat::WelesReviewMetaVm {
            weles_reviewed: false,
            verdict: "flag_only".into(),
            reasons: vec!["WELES unavailable; policy downgraded under yolo".into()],
            security_override_mode: Some("yolo".into()),
            audit_id: Some("audit-degraded-1".into()),
        }),
        ..Default::default()
    };
    let mut exp_tools = empty_tools();
    exp_tools.insert(0);

    let lines = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        80,
        &empty_expanded(),
        &exp_tools,
    );
    let plain = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        plain.contains("degraded"),
        "expected degraded marker, got: {plain}"
    );
    assert!(
        plain.contains("yolo"),
        "expected yolo override marker, got: {plain}"
    );
    assert!(
        plain.contains("WELES unavailable"),
        "expected rationale in expanded tool view, got: {plain}"
    );
}

#[test]
fn operator_question_message_renders_pending_state_and_option_legend() {
    let msg = AgentMessage {
        role: MessageRole::Assistant,
        content: "Approve this slice?\nA - proceed\nB - revise".into(),
        is_operator_question: true,
        operator_question_id: Some("oq-1".into()),
        ..Default::default()
    };

    let lines = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        80,
        &empty_expanded(),
        &empty_tools(),
    );

    let plain = plain_lines(&lines).join("\n");

    assert!(
        plain.contains("awaiting answer"),
        "expected pending state marker, got: {plain}"
    );
    assert!(plain.contains("A"), "expected option label, got: {plain}");
    assert!(
        plain.contains("proceed"),
        "expected option text, got: {plain}"
    );
}

#[test]
fn operator_question_message_keeps_free_form_body_lines_out_of_options() {
    let msg = AgentMessage {
        role: MessageRole::Assistant,
        content: "Approve this slice?\nReason: investigate regressions\nContext: release branch"
            .into(),
        is_operator_question: true,
        operator_question_id: Some("oq-1".into()),
        ..Default::default()
    };

    let lines = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        80,
        &empty_expanded(),
        &empty_tools(),
    );

    let plain = plain_lines(&lines).join("\n");

    assert!(
        plain.contains("Reason: investigate regressions"),
        "expected body line to remain verbatim, got: {plain}"
    );
    assert!(
        plain.contains("Context: release branch"),
        "expected body line to remain verbatim, got: {plain}"
    );
    assert!(
        !plain.contains("options"),
        "unexpected inferred option section, got: {plain}"
    );
    assert!(
        !plain.contains("[Reason]"),
        "unexpected option-like rendering for body line, got: {plain}"
    );
    assert!(
        !plain.contains("[Context]"),
        "unexpected option-like rendering for body line, got: {plain}"
    );
}

#[test]
fn operator_question_message_accepts_lowercase_compact_labels_and_matches_answer() {
    let msg = AgentMessage {
        role: MessageRole::Assistant,
        content: "Approve this slice?\na - proceed\nb1 - revise".into(),
        is_operator_question: true,
        operator_question_id: Some("oq-lower".into()),
        operator_question_answer: Some("B1".into()),
        ..Default::default()
    };

    let lines = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        80,
        &empty_expanded(),
        &empty_tools(),
    );

    let plain = plain_lines(&lines).join("\n");

    assert!(
        plain.contains("[a] proceed"),
        "expected lowercase option label, got: {plain}"
    );
    assert!(
        plain.contains("[b1] revise"),
        "expected lowercase alphanumeric option label, got: {plain}"
    );
    assert!(
        plain.contains("answered: [b1] revise"),
        "expected lowercase answer match in summary, got: {plain}"
    );
    assert!(
        !plain.contains("Context:"),
        "unexpected free-form body parsing, got: {plain}"
    );
}

#[test]
fn operator_question_message_wraps_status_and_option_rows_to_width() {
    let msg = AgentMessage {
        role: MessageRole::Assistant,
        content: "Approve this slice?\nC1 - proceed with a very detailed explanation that should wrap across multiple lines".into(),
        is_operator_question: true,
        operator_question_id: Some("oq-2".into()),
        ..Default::default()
    };

    let lines = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        24,
        &empty_expanded(),
        &empty_tools(),
    );

    let plain = plain_lines(&lines);

    assert!(
        plain.iter().any(|line| line == "operator question"),
        "expected wrapped status prefix, got: {:?}",
        plain
    );
    assert!(
        plain.iter().any(|line| line.contains("awaiting answer")),
        "expected wrapped status summary, got: {:?}",
        plain
    );
    assert!(
        plain.iter().any(|line| line.contains("proceed with a")),
        "expected wrapped option text, got: {:?}",
        plain
    );
    assert!(
        plain.iter().all(|line| line.chars().count() <= 24),
        "expected every rendered line to fit width 24, got: {:?}",
        plain
    );
}

#[test]
fn reasoning_before_content() {
    let msg = AgentMessage {
        role: MessageRole::Assistant,
        content: "Here is my answer".into(),
        reasoning: Some("Let me think...".into()),
        ..Default::default()
    };
    let lines = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        80,
        &empty_expanded(),
        &empty_tools(),
    );
    assert!(!lines.is_empty());
    let first_text: String = lines[0]
        .spans
        .iter()
        .map(|span| span.content.to_string())
        .collect();
    assert!(
        first_text.contains("Reasoning"),
        "First line should be reasoning hint, got: {}",
        first_text
    );
}
