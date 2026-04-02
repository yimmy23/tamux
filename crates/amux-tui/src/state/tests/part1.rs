#[cfg(test)]
use super::*;

#[test]
fn insert_char_appends_to_buffer() {
    let mut state = InputState::new();
    state.reduce(InputAction::InsertChar('h'));
    state.reduce(InputAction::InsertChar('i'));
    assert_eq!(state.buffer(), "hi");
    assert_eq!(state.cursor_pos(), 2);
}

#[test]
fn active_at_token_detects_token_under_cursor() {
    let buffer = "before @nested/path after";
    let cursor = buffer.find("nested").expect("token should exist") + 2;

    let token = crate::state::input_refs::active_at_token(buffer, cursor)
        .expect("cursor should be inside the reference token");

    assert_eq!(token.text, "@nested/path");
    assert_eq!(token.path, "nested/path");
    assert_eq!(
        token.range.start,
        buffer.find('@').expect("reference should exist")
    );
    assert_eq!(token.range.end, token.range.start + "@nested/path".len());
}

#[test]
fn active_at_token_returns_none_when_cursor_is_outside_reference() {
    let buffer = "before @nested/path after";
    let cursor = buffer.find("after").expect("trailing text should exist");

    assert!(crate::state::input_refs::active_at_token(buffer, cursor).is_none());
}

#[test]
fn backspace_removes_char_before_cursor() {
    let mut state = InputState::new();
    state.reduce(InputAction::InsertChar('a'));
    state.reduce(InputAction::InsertChar('b'));
    state.reduce(InputAction::Backspace);
    assert_eq!(state.buffer(), "a");
    assert_eq!(state.cursor_pos(), 1);
}

#[test]
fn backspace_on_empty_is_noop() {
    let mut state = InputState::new();
    state.reduce(InputAction::Backspace);
    assert_eq!(state.buffer(), "");
    assert_eq!(state.cursor_pos(), 0);
}

#[test]
fn submit_returns_buffer_and_clears() {
    let mut state = InputState::new();
    state.reduce(InputAction::InsertChar('h'));
    state.reduce(InputAction::InsertChar('i'));
    let submitted = state.take_submitted();
    // submit hasn't been called yet
    assert!(submitted.is_none());

    state.reduce(InputAction::Submit);
    let submitted = state.take_submitted();
    assert_eq!(submitted, Some("hi".to_string()));
    assert_eq!(state.buffer(), "");
    assert_eq!(state.cursor_pos(), 0);
}

#[test]
fn submit_empty_is_noop() {
    let mut state = InputState::new();
    state.reduce(InputAction::Submit);
    assert!(state.take_submitted().is_none());
}

#[test]
fn submit_whitespace_only_is_noop() {
    let mut state = InputState::new();
    state.reduce(InputAction::InsertChar(' '));
    state.reduce(InputAction::InsertChar(' '));
    state.reduce(InputAction::Submit);
    assert!(state.take_submitted().is_none());
}

#[test]
fn toggle_mode_switches_between_normal_and_insert() {
    let mut state = InputState::new();
    assert_eq!(state.mode(), InputMode::Insert);
    state.reduce(InputAction::ToggleMode);
    assert_eq!(state.mode(), InputMode::Normal);
    state.reduce(InputAction::ToggleMode);
    assert_eq!(state.mode(), InputMode::Insert);
}

#[test]
fn newline_inserts_newline_char() {
    let mut state = InputState::new();
    state.reduce(InputAction::InsertChar('a'));
    state.reduce(InputAction::InsertNewline);
    state.reduce(InputAction::InsertChar('b'));
    assert_eq!(state.buffer(), "a\nb");
    assert!(state.multiline());
    assert_eq!(state.cursor_pos(), 3);
}

#[test]
fn clear_empties_buffer() {
    let mut state = InputState::new();
    state.reduce(InputAction::InsertChar('x'));
    state.reduce(InputAction::Clear);
    assert_eq!(state.buffer(), "");
    assert!(!state.multiline());
    assert_eq!(state.cursor_pos(), 0);
}

#[test]
fn delete_word_removes_last_word() {
    let mut state = InputState::new();
    for c in "hello world".chars() {
        state.reduce(InputAction::InsertChar(c));
    }
    state.reduce(InputAction::DeleteWord);
    assert_eq!(state.buffer(), "hello ");
    assert_eq!(state.cursor_pos(), 6);
}

#[test]
fn delete_word_on_single_word_clears() {
    let mut state = InputState::new();
    for c in "hello".chars() {
        state.reduce(InputAction::InsertChar(c));
    }
    state.reduce(InputAction::DeleteWord);
    assert_eq!(state.buffer(), "");
    assert_eq!(state.cursor_pos(), 0);
}

#[test]
fn delete_word_on_empty_is_noop() {
    let mut state = InputState::new();
    state.reduce(InputAction::DeleteWord);
    assert_eq!(state.buffer(), "");
}

#[test]
fn delete_word_with_trailing_spaces() {
    let mut state = InputState::new();
    for c in "hello world  ".chars() {
        state.reduce(InputAction::InsertChar(c));
    }
    state.reduce(InputAction::DeleteWord);
    assert_eq!(state.buffer(), "hello ");
    assert_eq!(state.cursor_pos(), 6);
}

#[test]
fn clear_line_empties_buffer() {
    let mut state = InputState::new();
    for c in "some text here".chars() {
        state.reduce(InputAction::InsertChar(c));
    }
    state.reduce(InputAction::ClearLine);
    assert_eq!(state.buffer(), "");
    assert_eq!(state.cursor_pos(), 0);
}

// ── Cursor movement tests ────────────────────────────────────────────────

#[test]
fn cursor_left_right() {
    let mut state = InputState::new();
    for c in "abc".chars() {
        state.reduce(InputAction::InsertChar(c));
    }
    assert_eq!(state.cursor_pos(), 3);

    state.reduce(InputAction::MoveCursorLeft);
    assert_eq!(state.cursor_pos(), 2);

    state.reduce(InputAction::MoveCursorLeft);
    assert_eq!(state.cursor_pos(), 1);

    state.reduce(InputAction::MoveCursorRight);
    assert_eq!(state.cursor_pos(), 2);
}

#[test]
fn cursor_left_at_start_is_noop() {
    let mut state = InputState::new();
    state.reduce(InputAction::InsertChar('a'));
    state.reduce(InputAction::MoveCursorLeft);
    state.reduce(InputAction::MoveCursorLeft); // already at 0
    assert_eq!(state.cursor_pos(), 0);
}

#[test]
fn cursor_right_at_end_is_noop() {
    let mut state = InputState::new();
    state.reduce(InputAction::InsertChar('a'));
    state.reduce(InputAction::MoveCursorRight); // already at end
    assert_eq!(state.cursor_pos(), 1);
}

#[test]
fn insert_at_middle_of_buffer() {
    let mut state = InputState::new();
    for c in "ac".chars() {
        state.reduce(InputAction::InsertChar(c));
    }
    // Cursor is at 2, move left once to position 1
    state.reduce(InputAction::MoveCursorLeft);
    assert_eq!(state.cursor_pos(), 1);

    // Insert 'b' at position 1
    state.reduce(InputAction::InsertChar('b'));
    assert_eq!(state.buffer(), "abc");
    assert_eq!(state.cursor_pos(), 2);
}

#[test]
fn backspace_at_middle_of_buffer() {
    let mut state = InputState::new();
    for c in "abc".chars() {
        state.reduce(InputAction::InsertChar(c));
    }
    // Move to position 2 (between 'b' and 'c')
    state.reduce(InputAction::MoveCursorLeft);
    assert_eq!(state.cursor_pos(), 2);

    // Backspace deletes 'b'
    state.reduce(InputAction::Backspace);
    assert_eq!(state.buffer(), "ac");
    assert_eq!(state.cursor_pos(), 1);
}

#[test]
fn cursor_up_down_multiline() {
    let mut state = InputState::new();
    // Build "abc\ndef\nghi"
    for c in "abc".chars() {
        state.reduce(InputAction::InsertChar(c));
    }
    state.reduce(InputAction::InsertNewline);
    for c in "def".chars() {
        state.reduce(InputAction::InsertChar(c));
    }
    state.reduce(InputAction::InsertNewline);
    for c in "ghi".chars() {
        state.reduce(InputAction::InsertChar(c));
    }
    // Cursor at end of "ghi" (line 2, col 3)
    assert_eq!(state.cursor_pos(), 11);

    // Move up to line 1 (same col 3 => end of "def")
    state.reduce(InputAction::MoveCursorUp);
    let (line, col) = state.cursor_line_col_public();
    assert_eq!(line, 1);
    assert_eq!(col, 3);

    // Move up to line 0
    state.reduce(InputAction::MoveCursorUp);
    let (line, col) = state.cursor_line_col_public();
    assert_eq!(line, 0);
    assert_eq!(col, 3);

    // Move up again — already at line 0, should stay
    state.reduce(InputAction::MoveCursorUp);
    let (line, _) = state.cursor_line_col_public();
    assert_eq!(line, 0);

    // Move down to line 1
    state.reduce(InputAction::MoveCursorDown);
    let (line, _) = state.cursor_line_col_public();
    assert_eq!(line, 1);

    // Move down to line 2
    state.reduce(InputAction::MoveCursorDown);
    let (line, _) = state.cursor_line_col_public();
    assert_eq!(line, 2);

    // Move down again — already at last line, should stay
    state.reduce(InputAction::MoveCursorDown);
    let (line, _) = state.cursor_line_col_public();
    assert_eq!(line, 2);
}

#[test]
fn cursor_up_clamps_col_to_shorter_line() {
    let mut state = InputState::new();
    // Build "abcdef\nhi" — line 0 has 6 chars, line 1 has 2 chars
    for c in "abcdef".chars() {
        state.reduce(InputAction::InsertChar(c));
    }
    state.reduce(InputAction::InsertNewline);
    for c in "hi".chars() {
        state.reduce(InputAction::InsertChar(c));
    }
    // Cursor at end of line 1, col 2
    assert_eq!(state.cursor_pos(), 9);

    // Move to end of line 0 (col 6) via Home on line 1 then Up
    // Actually: go up from col 2 => line 0 col 2
    state.reduce(InputAction::MoveCursorUp);
    let (line, col) = state.cursor_line_col_public();
    assert_eq!(line, 0);
    assert_eq!(col, 2);

    // Move cursor to end of line 0 (col 6)
    state.reduce(InputAction::MoveCursorEnd);
    let (_, col) = state.cursor_line_col_public();
    assert_eq!(col, 6);

    // Move down to line 1 — col 6 should clamp to line 1 length (2)
    state.reduce(InputAction::MoveCursorDown);
    let (line, col) = state.cursor_line_col_public();
    assert_eq!(line, 1);
    assert_eq!(col, 2); // clamped
}

#[test]
fn home_and_end() {
    let mut state = InputState::new();
    for c in "abc\ndef".chars() {
        state.reduce(InputAction::InsertChar(c));
    }
    // Cursor at end of "def" (line 1, col 3)
    assert_eq!(state.cursor_pos(), 7);

    // Home moves to start of line 1 (position 4)
    state.reduce(InputAction::MoveCursorHome);
    assert_eq!(state.cursor_pos(), 4);

    // End moves to end of line 1 (position 7)
    state.reduce(InputAction::MoveCursorEnd);
    assert_eq!(state.cursor_pos(), 7);

    // Move up to line 0
    state.reduce(InputAction::MoveCursorUp);
    // Home on line 0 moves to position 0
    state.reduce(InputAction::MoveCursorHome);
    assert_eq!(state.cursor_pos(), 0);

    // End on line 0 moves to position 3 (before newline)
    state.reduce(InputAction::MoveCursorEnd);
    assert_eq!(state.cursor_pos(), 3);
}

#[test]
fn move_cursor_to_pos() {
    let mut state = InputState::new();
    for c in "hello".chars() {
        state.reduce(InputAction::InsertChar(c));
    }
    state.reduce(InputAction::MoveCursorToPos(2));
    assert_eq!(state.cursor_pos(), 2);

    // Clamp to buffer length
    state.reduce(InputAction::MoveCursorToPos(100));
    assert_eq!(state.cursor_pos(), 5);
}

// ── Undo / Redo tests ────────────────────────────────────────────────────

#[test]
fn undo_restores_previous_state() {
    let mut state = InputState::new();
    state.reduce(InputAction::InsertChar('a'));
    state.reduce(InputAction::InsertChar('b'));
    assert_eq!(state.buffer(), "ab");

    state.reduce(InputAction::Undo);
    assert_eq!(state.buffer(), "a");

    state.reduce(InputAction::Undo);
    assert_eq!(state.buffer(), "");
}
