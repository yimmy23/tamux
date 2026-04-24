use super::*;

fn empty_expanded() -> ExpandedReasoning {
    ExpandedReasoning::new()
}

fn empty_tools() -> ExpandedTools {
    ExpandedTools::new()
}

fn plain_lines(lines: &[Line<'_>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn markdown_renders_bold() {
    let lines = render_markdown("**bold text** normal", 80);
    assert!(!lines.is_empty(), "Markdown should produce lines");
    let has_bold = lines.iter().any(|line| {
        line.spans.iter().any(|span| {
            span.style
                .add_modifier
                .contains(ratatui::style::Modifier::BOLD)
        })
    });
    let debug: Vec<Vec<String>> = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| format!("'{}' mods={:?}", span.content, span.style.add_modifier))
                .collect()
        })
        .collect();
    assert!(has_bold, "Expected BOLD in markdown output: {:?}", debug);
}

#[test]
fn markdown_heading_keeps_line_style() {
    let lines = render_markdown("## Heading", 80);
    assert!(!lines.is_empty());
    assert!(
        lines[0].style.add_modifier.contains(Modifier::BOLD),
        "Expected heading line style to keep bold modifier, got {:?}",
        lines[0].style
    );
}

#[test]
fn markdown_wraps_to_requested_width() {
    let lines = render_markdown("**alpha beta gamma delta**", 10);
    assert!(
        lines.len() > 1,
        "Expected markdown to wrap, got {:?}",
        lines
    );
}

#[test]
fn markdown_empty_code_fence_language_is_normalized() {
    let sanitized = normalize_markdown_for_tui("before\n```\nplain text\n```\nafter");

    assert!(
        sanitized.contains("```text\nplain text\n```"),
        "expected empty fence language to be normalized, got {sanitized:?}"
    );
}

#[test]
fn markdown_tables_render_as_columns() {
    let lines = render_markdown(
        "| Skill | Size | Purpose |\n|---|---|---|\n| tamux-rust-dev.md | 3.4KB | Build and test Rust crates |",
        80,
    );
    let plain = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    assert!(
        plain.iter().any(|line| line.contains("│")),
        "Expected rendered column separators, got {:?}",
        plain
    );
    assert!(
        plain.iter().all(|line| !line.contains("|---")),
        "Expected markdown separator row to be rendered, got {:?}",
        plain
    );
}

#[test]
fn markdown_tables_wrap_long_cells_instead_of_truncating() {
    let lines = render_markdown(
        "| Spec | Idea | Why |\n|---|---|---|\n| NEGATIVE_KNOWLEDGE | The agent should track negative knowledge explicitly instead of compressing it into binary success and failure states | This preserves the actual content for the operator |",
        40,
    );
    let plain = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>();

    assert!(
        plain.len() > 3,
        "Expected wrapped multi-line table rows, got {:?}",
        plain
    );
    assert!(
        plain.iter().all(|line| !line.contains('…')),
        "Expected wrapped cells without truncation, got {:?}",
        plain
    );
    let joined = plain.join("\n");
    assert!(
        joined.contains("The agent") && joined.contains("negative") && joined.contains("states"),
        "Expected wrapped table output to preserve the long cell content, got {:?}",
        plain
    );
}

#[test]
fn wrap_text_basic() {
    let lines = wrap_text("hello world foo bar", 12);
    assert_eq!(lines, vec!["hello world", "foo bar"]);
}

#[test]
fn wrap_text_preserves_newlines() {
    let lines = wrap_text("line1\nline2", 80);
    assert_eq!(lines, vec!["line1", "line2"]);
}

#[test]
fn user_message_has_badge() {
    let msg = AgentMessage {
        role: MessageRole::User,
        content: "Hello".into(),
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
}

#[test]
fn tool_message_shows_gear_icon() {
    let msg = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("bash_command".into()),
        tool_status: Some("done".into()),
        content: "some output here".into(),
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
fn tool_message_expanded_shows_details() {
    let msg = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("bash_command".into()),
        tool_status: Some("done".into()),
        tool_arguments: Some("ls -la /home/user".into()),
        content: "total 208\ndrwxr-xr-x 15 user user 4096 Jan 1 00:00 .".into(),
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
    assert!(
        lines.len() > 1,
        "Expanded tool should have more than 1 line, got {}",
        lines.len()
    );
}

#[test]
fn tool_message_expanded_preserves_full_arguments_and_result() {
    let long_args = serde_json::json!({
        "command": "python - <<'PY'\n".to_string() + &"x".repeat(120) + "\nPY",
    })
    .to_string();
    let long_result = (0..8)
        .map(|index| format!("line-{index}: {}", "y".repeat(40)))
        .collect::<Vec<_>>()
        .join("\n");
    let msg = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("bash_command".into()),
        tool_status: Some("done".into()),
        tool_arguments: Some(long_args.clone()),
        content: long_result.clone(),
        ..Default::default()
    };

    let mut exp_tools = empty_tools();
    exp_tools.insert(0);
    let lines = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        50,
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
        plain.contains("python -"),
        "missing argument prefix: {plain}"
    );
    assert!(plain.contains("<<'PY'"), "missing heredoc marker: {plain}");
    assert!(
        plain.chars().filter(|ch| *ch == 'x').count() >= 100,
        "missing long argument body: {plain}"
    );
    assert!(
        plain.contains("line-7:"),
        "missing later result lines: {plain}"
    );
    assert!(
        plain.contains(&"y".repeat(30)),
        "missing long result body: {plain}"
    );
    assert!(
        !plain.contains("..."),
        "expanded tool output should not be truncated: {plain}"
    );
}

#[test]
fn apply_patch_tool_message_expanded_renders_diff_like_sections() {
    let patch = [
        "*** Begin Patch",
        "*** Update File: /tmp/example.rs",
        "@@ fn example()",
        "-    let before = 1;",
        "+    let after = 2;",
        "*** End Patch",
    ]
    .join("\n");
    let msg = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("apply_patch".into()),
        tool_status: Some("done".into()),
        tool_arguments: Some(serde_json::json!({ "input": patch }).to_string()),
        content: "Patch applied successfully".into(),
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
        plain.contains("/tmp/example.rs"),
        "expected file header in rendered diff, got: {plain}"
    );
    assert!(
        plain.contains("-    let before = 1;"),
        "expected removed line in rendered diff, got: {plain}"
    );
    assert!(
        plain.contains("+    let after = 2;"),
        "expected added line in rendered diff, got: {plain}"
    );
    assert!(
        !plain.contains("{\"input\":"),
        "expected formatted diff instead of raw JSON arguments, got: {plain}"
    );
}

#[test]
fn tool_message_expanded_formats_json_arguments_and_result_as_key_values() {
    let msg = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("run_terminal_command".into()),
        tool_status: Some("done".into()),
        tool_arguments: Some(
            serde_json::json!({
                "command": "cargo test -p tamux-tui",
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
        plain.contains("command: cargo test -p tamux-tui"),
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

#[test]
fn reasoning_renders_before_multiline_content() {
    let msg = AgentMessage {
        role: MessageRole::Assistant,
        content: "First line that wraps a bit for the test".into(),
        reasoning: Some("Let me think...".into()),
        ..Default::default()
    };
    let lines = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        20,
        &empty_expanded(),
        &empty_tools(),
    );
    let first_text: String = lines[0]
        .spans
        .iter()
        .map(|span| span.content.to_string())
        .collect();
    let second_text: String = lines[1]
        .spans
        .iter()
        .map(|span| span.content.to_string())
        .collect();
    assert!(
        first_text.contains("Reasoning"),
        "First line should be reasoning, got: {}",
        first_text
    );
    assert!(
        !second_text.contains("Reasoning"),
        "Content should start after reasoning, got: {}",
        second_text
    );
}

#[test]
fn reasoning_expandable() {
    let msg = AgentMessage {
        role: MessageRole::Assistant,
        content: "Answer".into(),
        reasoning: Some("Thinking step by step".into()),
        ..Default::default()
    };
    let collapsed = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        80,
        &empty_expanded(),
        &empty_tools(),
    );
    let mut exp = empty_expanded();
    exp.insert(0);
    let expanded = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        80,
        &exp,
        &empty_tools(),
    );
    assert!(
        expanded.len() > collapsed.len(),
        "Expanded should have more lines"
    );
}

#[test]
fn meta_cognition_message_collapses_by_default() {
    let msg = AgentMessage {
        role: MessageRole::System,
        content: "Meta-cognitive intervention: warning before tool execution.\nPlanned tool: read_file\nDetected risks:\n- overconfidence".into(),
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
        plain.contains("Meta-cognition"),
        "collapsed meta-cognition should show a disclosure header: {plain}"
    );
    assert!(
        !plain.contains("Planned tool: read_file") && !plain.contains("overconfidence"),
        "collapsed meta-cognition should hide details: {plain}"
    );
}

#[test]
fn meta_cognition_message_expands_with_reasoning_state() {
    let msg = AgentMessage {
        role: MessageRole::System,
        content: "Meta-cognitive intervention: warning before tool execution.\nPlanned tool: read_file\nDetected risks:\n- overconfidence".into(),
        ..Default::default()
    };
    let mut expanded = empty_expanded();
    expanded.insert(0);

    let lines = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        80,
        &expanded,
        &empty_tools(),
    );
    let plain = plain_lines(&lines).join("\n");

    assert!(
        plain.contains("Planned tool: read_file") && plain.contains("overconfidence"),
        "expanded meta-cognition should show details: {plain}"
    );
}

#[test]
fn tools_mode_skips_non_tool_messages() {
    let msg = AgentMessage {
        role: MessageRole::User,
        content: "Hello".into(),
        ..Default::default()
    };
    let lines = message_to_lines(
        &msg,
        0,
        TranscriptMode::Tools,
        &ThemeTokens::default(),
        80,
        &empty_expanded(),
        &empty_tools(),
    );
    assert!(lines.is_empty());
}

#[test]
fn wrap_text_empty_string() {
    let lines = wrap_text("", 80);
    assert_eq!(lines, vec![""]);
}

#[test]
fn wrap_text_zero_width() {
    let lines = wrap_text("hello", 0);
    assert_eq!(lines, vec!["hello"]);
}
