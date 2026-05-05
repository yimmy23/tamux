#[cfg(test)]
use super::*;

fn insert_text(state: &mut InputState, text: &str) {
    for ch in text.chars() {
        state.reduce(InputAction::InsertChar(ch));
    }
}

fn submit_text(state: &mut InputState, text: &str) {
    insert_text(state, text);
    state.reduce(InputAction::Submit);
    let _ = state.take_submitted();
}

#[test]
fn submitted_messages_are_available_from_empty_composer_history() {
    let mut state = InputState::new();
    submit_text(&mut state, "first prompt");
    submit_text(&mut state, "second prompt");

    state.reduce(InputAction::HistoryPrevious);

    assert_eq!(state.buffer(), "second prompt");
    assert_eq!(state.cursor_pos(), "second prompt".len());

    state.reduce(InputAction::HistoryPrevious);

    assert_eq!(state.buffer(), "first prompt");
    assert_eq!(state.cursor_pos(), "first prompt".len());

    state.reduce(InputAction::HistoryNext);

    assert_eq!(state.buffer(), "second prompt");
    assert_eq!(state.cursor_pos(), "second prompt".len());

    state.reduce(InputAction::HistoryNext);

    assert_eq!(state.buffer(), "");
    assert_eq!(state.cursor_pos(), 0);
}

#[test]
fn history_navigation_does_not_replace_user_typed_input() {
    let mut state = InputState::new();
    submit_text(&mut state, "sent prompt");
    insert_text(&mut state, "draft");

    state.reduce(InputAction::HistoryPrevious);

    assert_eq!(state.buffer(), "draft");
}

#[test]
fn typing_commits_selected_history_entry_and_edits_normally() {
    let mut state = InputState::new();
    submit_text(&mut state, "sent prompt");

    state.reduce(InputAction::HistoryPrevious);
    state.reduce(InputAction::MoveCursorHome);
    state.reduce(InputAction::InsertChar('>'));

    assert_eq!(state.buffer(), ">sent prompt");
    assert_eq!(state.cursor_pos(), 1);
}

#[test]
fn enter_submits_selected_history_entry() {
    let mut state = InputState::new();
    submit_text(&mut state, "sent prompt");

    state.reduce(InputAction::HistoryPrevious);
    state.reduce(InputAction::Submit);

    assert_eq!(state.take_submitted(), Some("sent prompt".to_string()));
    assert_eq!(state.buffer(), "");
}
