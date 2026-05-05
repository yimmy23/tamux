fn submit_input_for_history(model: &mut TuiModel, text: &str) {
    for ch in text.chars() {
        model.input.reduce(input::InputAction::InsertChar(ch));
    }
    model.input.reduce(input::InputAction::Submit);
    let _ = model.input.take_submitted();
}

#[test]
fn focused_empty_input_uses_up_down_for_sent_prompt_history() {
    let mut model = build_model();
    submit_input_for_history(&mut model, "first prompt");
    submit_input_for_history(&mut model, "second prompt");

    model.handle_key(KeyCode::Up, KeyModifiers::NONE);

    assert_eq!(model.input.buffer(), "second prompt");

    model.handle_key(KeyCode::Up, KeyModifiers::NONE);

    assert_eq!(model.input.buffer(), "first prompt");

    model.handle_key(KeyCode::Down, KeyModifiers::NONE);

    assert_eq!(model.input.buffer(), "second prompt");

    model.handle_key(KeyCode::Down, KeyModifiers::NONE);

    assert_eq!(model.input.buffer(), "");
}

#[test]
fn left_arrow_commits_selected_history_to_normal_composer_editing() {
    let mut model = build_model();
    submit_input_for_history(&mut model, "sent prompt");

    model.handle_key(KeyCode::Up, KeyModifiers::NONE);
    model.handle_key(KeyCode::Left, KeyModifiers::NONE);
    model.handle_key(KeyCode::Char('!'), KeyModifiers::NONE);

    assert_eq!(model.input.buffer(), "sent promp!t");
}
