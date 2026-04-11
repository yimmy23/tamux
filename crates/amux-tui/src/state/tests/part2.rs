#[cfg(test)]
use super::*;
use std::fs;
use std::path::PathBuf;

fn make_temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tamux-tui-input-refs-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("temporary directory should be creatable");
    dir
}

#[test]
fn undo_on_empty_is_noop() {
    let mut state = InputState::new();
    state.reduce(InputAction::Undo);
    assert_eq!(state.buffer(), "");
}

#[test]
fn redo_after_undo() {
    let mut state = InputState::new();
    state.reduce(InputAction::InsertChar('a'));
    state.reduce(InputAction::InsertChar('b'));
    state.reduce(InputAction::Undo);
    assert_eq!(state.buffer(), "a");

    state.reduce(InputAction::Redo);
    assert_eq!(state.buffer(), "ab");
}

#[test]
fn redo_cleared_on_new_edit() {
    let mut state = InputState::new();
    state.reduce(InputAction::InsertChar('a'));
    state.reduce(InputAction::InsertChar('b'));
    state.reduce(InputAction::Undo);
    assert_eq!(state.buffer(), "a");

    // New edit clears redo stack
    state.reduce(InputAction::InsertChar('c'));
    assert_eq!(state.buffer(), "ac");

    // Redo should be empty now
    state.reduce(InputAction::Redo);
    assert_eq!(state.buffer(), "ac"); // no change
}

#[test]
fn undo_clamps_cursor() {
    let mut state = InputState::new();
    for c in "hello".chars() {
        state.reduce(InputAction::InsertChar(c));
    }
    assert_eq!(state.cursor_pos(), 5);

    // Undo back to "hell"
    state.reduce(InputAction::Undo);
    // Cursor should be clamped to buffer length
    assert!(state.cursor_pos() <= state.buffer().len());
}

#[test]
fn path_resolution_keeps_nonexistent_references_as_plain_text() {
    let cwd = make_temp_dir();

    assert!(
        crate::state::input_refs::resolve_reference_path("missing-file.txt", &cwd).is_none(),
        "nonexistent references should remain plain text"
    );
}

#[test]
fn completion_extends_single_match_and_keeps_directory_trailing_slash() {
    let cwd = make_temp_dir();
    let docs_dir = cwd.join("docs");
    fs::create_dir_all(&docs_dir).expect("test directory should be creatable");
    fs::write(docs_dir.join("notes.txt"), "hello").expect("file should be writable");

    let buffer = "@do";
    let outcome = crate::state::input_refs::complete_active_at_token(buffer, buffer.len(), &cwd);

    assert!(
        outcome.consumed,
        "single-match completion should consume Tab"
    );
    let replacement = outcome
        .replacement
        .expect("single directory match should produce a replacement");
    assert_eq!(replacement.text, "@docs/");
    assert_eq!(replacement.range.start, 0);
    assert_eq!(replacement.range.end, buffer.len());
}

#[test]
fn backslash_completion_preserves_separator_style() {
    let cwd = make_temp_dir();
    let foo_dir = cwd.join("foo");
    let bar_dir = foo_dir.join("bar");
    fs::create_dir_all(&bar_dir).expect("nested test directory should be creatable");

    let buffer = "@foo\\ba";
    let outcome = crate::state::input_refs::complete_active_at_token(buffer, buffer.len(), &cwd);

    assert!(outcome.consumed);
    assert_eq!(
        outcome
            .replacement
            .as_ref()
            .expect("single match should produce a replacement")
            .text,
        "@foo\\bar\\"
    );
}

#[test]
fn ambiguous_matches_extend_shared_prefix() {
    let cwd = make_temp_dir();
    fs::write(cwd.join("alpha-one"), "one").expect("first file should be writable");
    fs::write(cwd.join("alpha-two"), "two").expect("second file should be writable");

    let buffer = "@alp";
    let outcome = crate::state::input_refs::complete_active_at_token(buffer, buffer.len(), &cwd);

    assert!(outcome.consumed);
    assert_eq!(
        outcome
            .replacement
            .as_ref()
            .expect("shared prefix should produce a replacement")
            .text,
        "@alpha-"
    );
    assert!(outcome
        .notice
        .as_deref()
        .expect("ambiguous matches should report a notice")
        .contains("Multiple matches"));
}

#[test]
fn leading_agent_completion_prefers_known_agents_before_files() {
    let cwd = make_temp_dir();
    fs::create_dir_all(cwd.join("weles-notes")).expect("test directory should be creatable");

    let buffer = "@we";
    let outcome = crate::state::input_refs::complete_active_at_token_with_agents(
        buffer,
        buffer.len(),
        &cwd,
        &["weles".to_string(), "reviewer".to_string()],
    );

    assert!(outcome.consumed);
    assert_eq!(
        outcome
            .replacement
            .as_ref()
            .expect("agent completion should replace the token")
            .text,
        "@weles "
    );
}

#[test]
fn parse_leading_internal_delegate_directive() {
    let directive = crate::state::input_refs::parse_leading_agent_directive(
        "!weles verify @src/main.rs",
        &["weles".to_string()],
    )
    .expect("directive should parse");

    assert_eq!(
        directive.kind,
        crate::state::input_refs::LeadingAgentDirectiveKind::InternalDelegate
    );
    assert_eq!(directive.agent_alias, "weles");
    assert_eq!(directive.body, "verify @src/main.rs");
}

#[test]
fn parse_leading_participant_stop_directive() {
    let directive = crate::state::input_refs::parse_leading_agent_directive(
        "@weles stop",
        &["weles".to_string()],
    )
    .expect("directive should parse");

    assert_eq!(
        directive.kind,
        crate::state::input_refs::LeadingAgentDirectiveKind::ParticipantDeactivate
    );
    assert_eq!(directive.agent_alias, "weles");
}

#[test]
fn tilde_resolution_handles_forward_and_backslash_separators() {
    let cwd = make_temp_dir();
    let home = make_temp_dir();
    let documents = home.join("Documents");
    fs::create_dir_all(&documents).expect("documents directory should be creatable");

    let forward = crate::state::input_refs::resolve_reference_path_with_home(
        "~/Documents",
        &cwd,
        Some(home.as_path()),
    )
    .expect("forward-slash home path should resolve");
    let backslash = crate::state::input_refs::resolve_reference_path_with_home(
        "~\\Documents",
        &cwd,
        Some(home.as_path()),
    )
    .expect("backslash home path should resolve");

    assert_eq!(forward, documents);
    assert_eq!(backslash, documents);
}

// ── UTF-8 cursor tests ───────────────────────────────────────────────────

#[test]
fn cursor_movement_with_multibyte_chars() {
    let mut state = InputState::new();
    // Insert some multi-byte characters
    state.reduce(InputAction::InsertChar('a'));
    state.reduce(InputAction::InsertChar('\u{00e9}')); // é (2 bytes)
    state.reduce(InputAction::InsertChar('b'));
    assert_eq!(state.buffer(), "a\u{00e9}b");
    assert_eq!(state.cursor_pos(), 4); // 1 + 2 + 1

    state.reduce(InputAction::MoveCursorLeft);
    assert_eq!(state.cursor_pos(), 3); // before 'b'

    state.reduce(InputAction::MoveCursorLeft);
    assert_eq!(state.cursor_pos(), 1); // before 'é'

    state.reduce(InputAction::MoveCursorRight);
    assert_eq!(state.cursor_pos(), 3); // after 'é'
}

#[test]
fn delete_word_at_cursor_in_middle() {
    let mut state = InputState::new();
    for c in "hello world end".chars() {
        state.reduce(InputAction::InsertChar(c));
    }
    // Move cursor back to after "world " (before "end")
    state.reduce(InputAction::MoveCursorLeft);
    state.reduce(InputAction::MoveCursorLeft);
    state.reduce(InputAction::MoveCursorLeft);
    assert_eq!(state.cursor_pos(), 12);

    state.reduce(InputAction::DeleteWord);
    assert_eq!(state.buffer(), "hello end");
    assert_eq!(state.cursor_pos(), 6);
}

#[test]
fn line_col_conversion_roundtrip() {
    let state = {
        let mut s = InputState::new();
        for c in "abc\nde\nfghij".chars() {
            s.reduce(InputAction::InsertChar(c));
        }
        s
    };
    // Line 0: "abc" (len 3)
    assert_eq!(state.line_col_to_offset_public(0, 0), 0);
    assert_eq!(state.line_col_to_offset_public(0, 3), 3);
    assert_eq!(state.line_col_to_offset_public(0, 10), 3); // clamped

    // Line 1: "de" (len 2), starts at offset 4
    assert_eq!(state.line_col_to_offset_public(1, 0), 4);
    assert_eq!(state.line_col_to_offset_public(1, 2), 6);

    // Line 2: "fghij" (len 5), starts at offset 7
    assert_eq!(state.line_col_to_offset_public(2, 0), 7);
    assert_eq!(state.line_col_to_offset_public(2, 5), 12);

    // Beyond last line
    assert_eq!(state.line_col_to_offset_public(5, 0), 12);
}

// ── Paste-block tests ────────────────────────────────────────────────────

#[test]
fn single_line_paste_inserts_normally() {
    let mut state = InputState::new();
    state.insert_paste_block("hello".to_string());
    assert_eq!(state.buffer(), "hello");
    assert!(state.paste_blocks().is_empty());
}

#[test]
fn multiline_paste_stores_block_and_placeholder() {
    let mut state = InputState::new();
    state.insert_paste_block("line1\nline2\nline3".to_string());
    assert_eq!(state.paste_blocks().len(), 1);
    assert_eq!(state.paste_blocks()[0].line_count, 3);
    // Buffer should contain the placeholder token, not the raw text
    assert!(state.buffer().contains("\x00PASTE:0\x00"));
    assert!(!state.buffer().contains("line1"));
}

#[test]
fn paste_block_ids_increment() {
    let mut state = InputState::new();
    state.insert_paste_block("a\nb".to_string());
    state.insert_paste_block("c\nd".to_string());
    assert_eq!(state.paste_blocks().len(), 2);
    assert_eq!(state.paste_blocks()[0].id, 0);
    assert_eq!(state.paste_blocks()[1].id, 1);
}

#[test]
fn expand_paste_blocks_replaces_placeholders() {
    let mut state = InputState::new();
    state.reduce(InputAction::InsertChar('x'));
    state.reduce(InputAction::InsertChar(' '));
    state.insert_paste_block("foo\nbar".to_string());
    state.reduce(InputAction::InsertChar(' '));
    state.reduce(InputAction::InsertChar('y'));
    let expanded = state.expand_paste_blocks(state.buffer());
    assert!(expanded.contains("foo\nbar"));
    assert!(expanded.starts_with("x "));
    assert!(expanded.ends_with(" y"));
}

#[test]
fn submit_expands_paste_blocks_in_submitted_text() {
    let mut state = InputState::new();
    state.insert_paste_block("first\nsecond".to_string());
    state.reduce(InputAction::Submit);
    let submitted = state.take_submitted().expect("should have submitted text");
    assert!(submitted.contains("first\nsecond"));
    assert!(state.paste_blocks().is_empty());
    assert_eq!(state.buffer(), "");
}

#[test]
fn clear_removes_paste_blocks() {
    let mut state = InputState::new();
    state.insert_paste_block("a\nb".to_string());
    assert_eq!(state.paste_blocks().len(), 1);
    state.reduce(InputAction::Clear);
    assert!(state.paste_blocks().is_empty());
}

#[test]
fn clear_line_removes_paste_blocks() {
    let mut state = InputState::new();
    state.insert_paste_block("a\nb".to_string());
    state.reduce(InputAction::ClearLine);
    assert!(state.paste_blocks().is_empty());
}

#[test]
fn backspace_removes_entire_paste_placeholder_from_end() {
    let mut state = InputState::new();
    state.insert_paste_block("first\nsecond".to_string());

    state.reduce(InputAction::Backspace);

    assert_eq!(state.buffer(), "");
    assert!(state.paste_blocks().is_empty());
    assert_eq!(state.cursor_pos(), 0);
}

#[test]
fn backspace_removes_entire_paste_placeholder_when_cursor_is_inside_token() {
    let mut state = InputState::new();
    state.insert_paste_block("first\nsecond".to_string());
    let inside_placeholder = state
        .buffer()
        .find("STE")
        .expect("placeholder should be present");
    let buffer = state.buffer().to_string();
    state.set_buffer_and_cursor(&buffer, inside_placeholder);

    state.reduce(InputAction::Backspace);

    assert_eq!(state.buffer(), "");
    assert!(state.paste_blocks().is_empty());
    assert_eq!(state.cursor_pos(), 0);
}

#[test]
fn delete_word_removes_entire_paste_placeholder() {
    let mut state = InputState::new();
    state.insert_paste_block("first\nsecond".to_string());

    state.reduce(InputAction::DeleteWord);

    assert_eq!(state.buffer(), "");
    assert!(state.paste_blocks().is_empty());
    assert_eq!(state.cursor_pos(), 0);
}

#[test]
fn paste_block_display_formats_label() {
    let blocks = vec![PasteBlock {
        id: 0,
        content: "fix this bug\nand that one\nand another".to_string(),
        line_count: 3,
    }];
    let label = InputState::paste_block_display(0, &blocks).unwrap();
    assert!(label.contains("[Pasted text #1"));
    assert!(label.contains("+3 lines"));
    assert!(label.contains("fix this bug"));
}

#[test]
fn paste_block_display_truncates_long_first_line() {
    let long_line = "a".repeat(30);
    let content = format!("{}\nmore", long_line);
    let blocks = vec![PasteBlock {
        id: 0,
        content,
        line_count: 2,
    }];
    let label = InputState::paste_block_display(0, &blocks).unwrap();
    assert!(label.contains("..."));
}

#[test]
fn paste_block_display_returns_none_for_unknown_id() {
    let blocks: Vec<PasteBlock> = Vec::new();
    assert!(InputState::paste_block_display(42, &blocks).is_none());
}

#[test]
fn display_buffer_expands_paste_placeholders() {
    let mut state = InputState::new();
    state.reduce(InputAction::InsertChar('a'));
    state.insert_paste_block("hello\nworld".to_string());
    state.reduce(InputAction::InsertChar('z'));

    let display = state.display_buffer();
    assert!(display.contains("[Pasted text #1 +2 lines: hello]"));
    assert!(display.starts_with('a'));
    assert!(display.ends_with('z'));
}

#[test]
fn display_offset_round_trips_regular_text() {
    let mut state = InputState::new();
    for c in "hello".chars() {
        state.reduce(InputAction::InsertChar(c));
    }

    for offset in 0..=state.display_buffer().len() {
        assert_eq!(state.display_offset_to_buffer_offset(offset), offset);
    }
}

#[test]
fn display_offset_maps_inside_paste_placeholder() {
    let mut state = InputState::new();
    state.insert_paste_block("line one\nline two".to_string());

    let display = state.display_buffer();
    let midpoint = display.len() / 2;
    let mapped = state.display_offset_to_buffer_offset(midpoint);
    assert!(mapped <= state.buffer().len());
}

#[test]
fn wrapped_display_inserts_soft_breaks() {
    let mut state = InputState::new();
    for c in "abcdefghij".chars() {
        state.reduce(InputAction::InsertChar(c));
    }

    let wrapped = state.wrapped_display_buffer(4);
    assert_eq!(wrapped, "abcd\nefgh\nij");
}

#[test]
fn wrapped_display_offset_maps_back_to_buffer() {
    let mut state = InputState::new();
    for c in "abcdefghij".chars() {
        state.reduce(InputAction::InsertChar(c));
    }

    let wrapped = state.wrapped_display_buffer(4);
    let click_offset = wrapped.find('\n').unwrap_or(0);
    let mapped = state.wrapped_display_offset_to_buffer_offset(click_offset, 4);
    assert_eq!(mapped, 4);
}
