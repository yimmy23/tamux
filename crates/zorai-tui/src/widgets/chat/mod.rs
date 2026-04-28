use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::message::wrap_text;
use crate::state::chat::{
    AgentMessage, ChatHitTarget, ChatState, MessageRole, RetryPhase, TranscriptMode,
};
use crate::theme::ThemeTokens;

const MESSAGE_PADDING_X: usize = 2;
const MESSAGE_PADDING_Y: usize = 1;
const TOGGLE_BUTTON_HIT_WIDTH: usize = 2;
const SCROLLBAR_WIDTH: u16 = 1;

#[cfg(test)]
thread_local! {
    static BUILD_RENDERED_LINES_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

include!("part1.rs");
include!("part2.rs");
include!("part3.rs");
include!("part4.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::chat::{AgentThread, ChatAction, MessageRole};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn chat_with_messages(messages: Vec<AgentMessage>) -> ChatState {
        let mut chat = ChatState::new();
        chat.reduce(ChatAction::ThreadCreated {
            thread_id: "t1".into(),
            title: "Test".into(),
        });
        chat.reduce(ChatAction::ThreadDetailReceived(AgentThread {
            id: "t1".into(),
            title: "Test".into(),
            messages,
            ..Default::default()
        }));
        chat
    }

    include!("tests/tests_part1.rs");
    include!("tests/tests_part2.rs");
}
