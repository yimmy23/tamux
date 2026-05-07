#[path = "conversion_parts/convert_thread_to_convert_todo_with_fallback_step.rs"]
mod convert_thread_to_convert_todo_with_fallback_step;

#[path = "conversion_parts/convert_work_context_to_copy_to_clipboard.rs"]
mod convert_work_context_to_copy_to_clipboard;

pub(crate) use convert_thread_to_convert_todo_with_fallback_step::*;
pub(crate) use convert_work_context_to_copy_to_clipboard::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::chat;

    #[test]
    fn convert_thread_preserves_operator_question_metadata() {
        let thread = crate::wire::AgentThread {
            id: "thread-1".into(),
            title: "Thread".into(),
            messages: vec![crate::wire::AgentMessage {
                role: crate::wire::MessageRole::Assistant,
                content: "Approve this slice?\na - proceed".into(),
                is_operator_question: true,
                operator_question_id: Some("oq-1".into()),
                operator_question_answer: Some("a".into()),
                ..Default::default()
            }],
            ..Default::default()
        };

        let converted = convert_thread(thread);
        let message = &converted.messages[0];

        assert!(message.is_operator_question);
        assert_eq!(message.operator_question_id.as_deref(), Some("oq-1"));
        assert_eq!(message.operator_question_answer.as_deref(), Some("a"));
    }

    #[test]
    fn convert_thread_preserves_image_content_blocks() {
        let thread = crate::wire::AgentThread {
            id: "thread-1".into(),
            title: "Thread".into(),
            messages: vec![crate::wire::AgentMessage {
                role: crate::wire::MessageRole::Assistant,
                content: "Generated image.".into(),
                content_blocks: vec![crate::wire::AgentContentBlock::Image {
                    url: Some("file:///tmp/thread-files/generated.png".into()),
                    data_url: None,
                    mime_type: Some("image/png".into()),
                }],
                ..Default::default()
            }],
            ..Default::default()
        };

        let converted = convert_thread(thread);
        let message = &converted.messages[0];

        assert!(matches!(
            message.content_blocks.first(),
            Some(chat::AgentContentBlock::Image {
                url: Some(url),
                mime_type: Some(mime_type),
                ..
            }) if url == "file:///tmp/thread-files/generated.png" && mime_type == "image/png"
        ));
    }

    #[test]
    fn copy_to_clipboard_keeps_owner_alive_after_write() {
        reset_last_copied_text();

        copy_to_clipboard("hello");

        assert_eq!(last_copied_text().as_deref(), Some("hello"));
        assert!(
            test_clipboard_owner_held(),
            "clipboard owner should stay alive after copy so Linux clipboard managers can read it"
        );
    }
}
