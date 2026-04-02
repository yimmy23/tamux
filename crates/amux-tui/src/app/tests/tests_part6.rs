use std::fs;
use std::path::{Path, PathBuf};

fn make_temp_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tamux-tui-tab-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("temporary directory should be creatable");
    dir
}

struct CurrentDirGuard(PathBuf);

impl CurrentDirGuard {
    fn enter(dir: &Path) -> Self {
        let previous = std::env::current_dir().expect("current dir should be readable");
        std::env::set_current_dir(dir).expect("temporary dir should be settable");
        Self(previous)
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

#[test]
fn submit_operator_profile_answer_allows_empty_input_when_question_is_optional() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "nickname".to_string(),
        field_key: "nickname".to_string(),
        prompt: "Nickname?".to_string(),
        input_kind: "text".to_string(),
        optional: true,
    });

    assert!(model.submit_operator_profile_answer());
    assert!(
        model.operator_profile.loading,
        "optional empty answer should begin submission"
    );
    assert!(
        model.operator_profile.question.is_none(),
        "question should clear when submission starts"
    );

    let sent = cmd_rx
        .try_recv()
        .expect("submitting optional empty answer should emit daemon command");
    match sent {
        DaemonCommand::SubmitOperatorProfileAnswer {
            session_id,
            question_id,
            answer_json,
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(question_id, "nickname");
            assert_eq!(answer_json, "null");
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn submit_operator_profile_answer_blocks_empty_input_when_question_is_required() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "name".to_string(),
        field_key: "name".to_string(),
        prompt: "What should I call you?".to_string(),
        input_kind: "text".to_string(),
        optional: false,
    });

    assert!(model.submit_operator_profile_answer());
    assert!(
        !model.operator_profile.loading,
        "required empty answer should not start submission"
    );
    assert!(
        model.operator_profile.question.is_some(),
        "question should remain while awaiting required answer"
    );
    assert!(
        cmd_rx.try_recv().is_err(),
        "required empty answer should not emit daemon command"
    );
}

#[test]
fn skip_operator_profile_question_clears_stale_question_immediately() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "name".to_string(),
        field_key: "name".to_string(),
        prompt: "What should I call you?".to_string(),
        input_kind: "text".to_string(),
        optional: false,
    });

    assert!(model.skip_operator_profile_question());
    assert!(model.operator_profile.loading);
    assert!(
        model.operator_profile.question.is_none(),
        "question should clear when skip starts"
    );

    let sent = cmd_rx.try_recv().expect("skip should emit daemon command");
    match sent {
        DaemonCommand::SkipOperatorProfileQuestion {
            session_id,
            question_id,
            reason,
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(question_id, "name");
            assert_eq!(reason.as_deref(), Some("tui_skip_shortcut"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn defer_operator_profile_question_clears_stale_question_immediately() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "name".to_string(),
        field_key: "name".to_string(),
        prompt: "What should I call you?".to_string(),
        input_kind: "text".to_string(),
        optional: false,
    });

    assert!(model.defer_operator_profile_question());
    assert!(model.operator_profile.loading);
    assert!(
        model.operator_profile.question.is_none(),
        "question should clear when defer starts"
    );

    let sent = cmd_rx.try_recv().expect("defer should emit daemon command");
    match sent {
        DaemonCommand::DeferOperatorProfileQuestion {
            session_id,
            question_id,
            defer_until_unix_ms,
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(question_id, "name");
            assert!(defer_until_unix_ms.is_some());
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn tab_completes_active_file_reference_instead_of_changing_focus() {
    let mut model = build_model();
    let cwd = make_temp_dir();
    let docs_dir = cwd.join("docs");
    fs::create_dir_all(&docs_dir).expect("docs directory should be creatable");
    fs::write(docs_dir.join("notes.txt"), "hello").expect("file should be writable");
    let reference = format!("@{}/do", cwd.display());
    model.input.set_text(&reference);

    let handled = model.handle_key(KeyCode::Tab, KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Input);
    assert_eq!(model.input.buffer(), format!("@{}/docs/", cwd.display()));
}

#[test]
fn tab_focus_cycles_when_not_inside_file_reference() {
    let mut model = build_model();
    model.focus = FocusArea::Input;
    model.input.set_text("hello world");

    let handled = model.handle_key(KeyCode::Tab, KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Chat);
    assert_eq!(model.input.buffer(), "hello world");
}

#[test]
fn tab_inside_unmatched_file_reference_keeps_input_focus() {
    let mut model = build_model();
    let cwd = make_temp_dir();
    let _guard = CurrentDirGuard::enter(&cwd);
    model.focus = FocusArea::Input;
    model.input.set_text("@missing");

    let handled = model.handle_key(KeyCode::Tab, KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Input);
    assert_eq!(model.input.buffer(), "@missing");
    assert!(
        model.status_line.contains("No matches"),
        "unmatched completion should surface a notice"
    );
}

#[test]
fn submit_prompt_appends_referenced_files_footer() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    let cwd = make_temp_dir();
    let source_dir = cwd.join("src");
    fs::create_dir_all(&source_dir).expect("source directory should be creatable");
    let main_rs = source_dir.join("main.rs");
    fs::write(&main_rs, "fn main() {}\n").expect("fixture file should be writable");

    model.submit_prompt(format!(
        "Please inspect @{} before editing",
        main_rs.display()
    ));

    let expected = format!(
        "Please inspect @{} before editing\n\nReferenced files: {}\nInspect these with read_file before making assumptions.",
        main_rs.display(),
        main_rs.display()
    );

    match cmd_rx.try_recv() {
        Ok(DaemonCommand::SendMessage { content, .. }) => {
            assert_eq!(content, expected);
        }
        other => panic!("expected send-message command, got {:?}", other),
    }

    assert_eq!(
        model
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.last())
            .map(|message| message.content.as_str()),
        Some(expected.as_str())
    );
}

#[test]
fn submit_prompt_does_not_inline_referenced_file_contents() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;

    let cwd = make_temp_dir();
    let notes = cwd.join("notes.txt");
    fs::write(&notes, "top secret contents\n").expect("fixture file should be writable");

    model.submit_prompt(format!("Review @{}", notes.display()));

    let sent_content = match cmd_rx.try_recv() {
        Ok(DaemonCommand::SendMessage { content, .. }) => content,
        other => panic!("expected send-message command, got {:?}", other),
    };

    assert!(
        sent_content.contains("Referenced files:"),
        "resolved references should append footer metadata"
    );
    assert!(
        sent_content.contains(&notes.display().to_string()),
        "footer should include the normalized absolute path"
    );
    assert!(
        !sent_content.contains("top secret contents"),
        "submit_prompt should not inline referenced file contents"
    );
    assert!(
        !sent_content.contains("<attached_file"),
        "file references should not create synthetic attachments"
    );
}

#[test]
fn submit_prompt_deduplicates_referenced_files() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;

    let cwd = make_temp_dir();
    let lib_rs = cwd.join("lib.rs");
    fs::write(&lib_rs, "pub fn demo() {}\n").expect("fixture file should be writable");

    model.submit_prompt(format!(
        "Check @{0} and again @{0}",
        lib_rs.display()
    ));

    let sent_content = match cmd_rx.try_recv() {
        Ok(DaemonCommand::SendMessage { content, .. }) => content,
        other => panic!("expected send-message command, got {:?}", other),
    };

    let footer = format!("Referenced files: {}", lib_rs.display());
    assert_eq!(sent_content.matches(&footer).count(), 1);
}

#[test]
fn submit_prompt_ignores_nonexistent_referenced_files() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;

    let missing = make_temp_dir().join("missing.txt");
    let prompt = format!("Investigate @{} later", missing.display());

    model.submit_prompt(prompt.clone());

    let sent_content = match cmd_rx.try_recv() {
        Ok(DaemonCommand::SendMessage { content, .. }) => content,
        other => panic!("expected send-message command, got {:?}", other),
    };

    assert_eq!(sent_content, prompt);
    assert!(
        !sent_content.contains("Referenced files:"),
        "nonexistent references should remain plain text without footer metadata"
    );
}
