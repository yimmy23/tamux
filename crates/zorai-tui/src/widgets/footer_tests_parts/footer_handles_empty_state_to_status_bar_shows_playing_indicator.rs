use super::super::*;
use crate::state::input::InputState;
use crate::state::InputMode;
use crate::theme::ThemeTokens;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

#[test]
fn footer_handles_empty_state() {
    let input = InputState::new();
    let _theme = ThemeTokens::default();
    assert_eq!(input.mode(), InputMode::Insert);
}

#[test]
fn status_bar_lists_notifications_hotkey() {
    let backend = TestBackend::new(120, 1);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");

    terminal
        .draw(|frame| {
            render_status_bar(
                frame,
                Rect::new(0, 0, 120, 1),
                &ThemeTokens::default(),
                true,
                false,
                false,
                0,
                0,
                false,
                false,
                0,
                "ready",
            );
        })
        .expect("status bar render should succeed");

    let buffer = terminal.backend().buffer();
    let row = (0..120)
        .filter_map(|x| buffer.cell((x, 0)).map(|cell| cell.symbol()))
        .collect::<String>();

    assert!(
        row.contains("ctrl+n"),
        "missing notifications hotkey: {row}"
    );
}

#[test]
fn status_bar_shows_playing_indicator_when_audio_is_playing() {
    let backend = TestBackend::new(120, 1);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");

    terminal
        .draw(|frame| {
            render_status_bar(
                frame,
                Rect::new(0, 0, 120, 1),
                &ThemeTokens::default(),
                true,
                false,
                false,
                10,
                0,
                false,
                true,
                0,
                "playing",
            );
        })
        .expect("status bar render should succeed");

    let buffer = terminal.backend().buffer();
    let row = (0..120)
        .filter_map(|x| buffer.cell((x, 0)).map(|cell| cell.symbol()))
        .collect::<String>();

    assert!(row.contains("PLAYING"), "missing PLAYING indicator: {row}");
}
