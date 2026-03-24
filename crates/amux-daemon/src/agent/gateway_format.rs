//! Gateway platform message formatting and chunking.
//!
//! Pure-function module — no I/O, no async, fully testable.
//! Converts markdown to platform-specific formats and chunks long messages
//! at natural boundaries.

/// Maximum message length for Slack (per D-10).
pub const SLACK_MAX_CHARS: usize = 4000;
/// Maximum message length for Discord (per D-10).
pub const DISCORD_MAX_CHARS: usize = 2000;
/// Maximum message length for Telegram (per D-10).
pub const TELEGRAM_MAX_CHARS: usize = 4096;

/// Convert standard markdown to Slack mrkdwn format (per D-09).
///
/// - `**bold**` -> `*bold*`
/// - `~~strikethrough~~` -> `~strikethrough~`
/// - Preserves code blocks and inline code unchanged.
pub fn markdown_to_slack_mrkdwn(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut in_code_block = false;
    let mut in_inline_code = false;

    while i < len {
        // Track code block state (```)
        if i + 2 < len && chars[i] == '`' && chars[i + 1] == '`' && chars[i + 2] == '`' {
            in_code_block = !in_code_block;
            result.push_str("```");
            i += 3;
            continue;
        }

        // Track inline code state (`)
        if chars[i] == '`' && !in_code_block {
            in_inline_code = !in_inline_code;
            result.push('`');
            i += 1;
            continue;
        }

        // Inside code — pass through unchanged
        if in_code_block || in_inline_code {
            result.push(chars[i]);
            i += 1;
            continue;
        }

        // Convert **bold** to *bold*
        if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
            // Find closing **
            if let Some(close) = find_closing_marker(&chars, i + 2, &['*', '*']) {
                result.push('\x01'); // sentinel for bold start
                i += 2;
                while i < close {
                    result.push(chars[i]);
                    i += 1;
                }
                result.push('\x01'); // sentinel for bold end
                i += 2; // skip closing **
                continue;
            }
        }

        // Convert ~~strikethrough~~ to ~strikethrough~
        if i + 1 < len && chars[i] == '~' && chars[i + 1] == '~' {
            if let Some(close) = find_closing_marker(&chars, i + 2, &['~', '~']) {
                result.push('~');
                i += 2;
                while i < close {
                    result.push(chars[i]);
                    i += 1;
                }
                result.push('~');
                i += 2; // skip closing ~~
                continue;
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    // Replace sentinel with single * for bold
    result.replace('\x01', "*")
}

/// Find the closing position of a two-character marker starting from `start`.
/// Returns the index of the first character of the closing marker, or None.
fn find_closing_marker(chars: &[char], start: usize, marker: &[char; 2]) -> Option<usize> {
    let mut j = start;
    while j + 1 < chars.len() {
        if chars[j] == marker[0] && chars[j + 1] == marker[1] {
            return Some(j);
        }
        j += 1;
    }
    None
}

/// Convert markdown to Discord format (per D-09).
///
/// Discord natively supports standard markdown, so this is a passthrough.
pub fn markdown_to_discord(input: &str) -> String {
    input.to_string()
}

/// Convert markdown to Telegram MarkdownV2 format (per D-09/Pitfall 2).
///
/// Escapes all 18 special characters that Telegram MarkdownV2 requires.
/// This is the "safe" version that escapes everything; a formatting-preserving
/// version is future work.
pub fn markdown_to_telegram_v2(input: &str) -> String {
    let mut result = String::with_capacity(input.len() * 2);
    for ch in input.chars() {
        if is_telegram_special(ch) {
            result.push('\\');
        }
        result.push(ch);
    }
    result
}

/// Check if a character is one of Telegram MarkdownV2's 18 special characters.
fn is_telegram_special(ch: char) -> bool {
    matches!(
        ch,
        '_' | '*'
            | '['
            | ']'
            | '('
            | ')'
            | '~'
            | '`'
            | '>'
            | '#'
            | '+'
            | '-'
            | '='
            | '|'
            | '{'
            | '}'
            | '.'
            | '!'
    )
}

/// Strip all markdown formatting markers, producing plain text.
pub fn markdown_to_plain(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Skip code block markers (```)
        if i + 2 < len && chars[i] == '`' && chars[i + 1] == '`' && chars[i + 2] == '`' {
            i += 3;
            continue;
        }

        // Skip bold/italic markers (**), (__)
        if i + 1 < len
            && ((chars[i] == '*' && chars[i + 1] == '*')
                || (chars[i] == '_' && chars[i + 1] == '_'))
        {
            i += 2;
            continue;
        }

        // Skip strikethrough markers (~~)
        if i + 1 < len && chars[i] == '~' && chars[i + 1] == '~' {
            i += 2;
            continue;
        }

        // Skip single formatting markers (*, _, `)
        if matches!(chars[i], '*' | '_' | '`') {
            i += 1;
            continue;
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Split a message into chunks that fit within `max_chars` (per D-10/Pitfall 7).
///
/// Splitting priority:
/// 1. At last newline before limit
/// 2. At last sentence boundary (". ") before limit
/// 3. At last whitespace before limit
/// 4. Hard split at max_chars
pub fn chunk_message(message: &str, max_chars: usize) -> Vec<String> {
    if message.len() <= max_chars {
        return vec![message.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = message;

    while !remaining.is_empty() {
        if remaining.len() <= max_chars {
            chunks.push(remaining.to_string());
            break;
        }

        let window = &remaining[..max_chars];

        // Try to split at last newline
        if let Some(pos) = window.rfind('\n') {
            if pos > 0 {
                chunks.push(remaining[..pos].to_string());
                remaining = &remaining[pos + 1..];
                continue;
            }
        }

        // Try to split at last sentence boundary (". ")
        if let Some(pos) = window.rfind(". ") {
            if pos > 0 {
                chunks.push(remaining[..=pos].to_string());
                remaining = &remaining[pos + 2..];
                continue;
            }
        }

        // Try to split at last whitespace
        if let Some(pos) = window.rfind(char::is_whitespace) {
            if pos > 0 {
                chunks.push(remaining[..pos].to_string());
                remaining = &remaining[pos + 1..];
                continue;
            }
        }

        // Hard split at max_chars
        chunks.push(remaining[..max_chars].to_string());
        remaining = &remaining[max_chars..];
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Slack mrkdwn conversion
    // -----------------------------------------------------------------------

    #[test]
    fn slack_converts_bold() {
        assert_eq!(
            markdown_to_slack_mrkdwn("This is **bold** text"),
            "This is *bold* text"
        );
    }

    #[test]
    fn slack_converts_strikethrough() {
        assert_eq!(
            markdown_to_slack_mrkdwn("This is ~~struck~~ text"),
            "This is ~struck~ text"
        );
    }

    #[test]
    fn slack_preserves_code_blocks() {
        let input = "```\n**not bold**\n```";
        let output = markdown_to_slack_mrkdwn(input);
        assert_eq!(output, input);
    }

    #[test]
    fn slack_preserves_inline_code() {
        let input = "Use `**not bold**` here";
        let output = markdown_to_slack_mrkdwn(input);
        assert_eq!(output, input);
    }

    // -----------------------------------------------------------------------
    // Discord passthrough
    // -----------------------------------------------------------------------

    #[test]
    fn discord_passes_through_unchanged() {
        let input = "**bold** and ~~strike~~ and `code`";
        assert_eq!(markdown_to_discord(input), input);
    }

    // -----------------------------------------------------------------------
    // Telegram MarkdownV2 escaping
    // -----------------------------------------------------------------------

    #[test]
    fn telegram_escapes_all_special_chars() {
        // All 18 special chars: _*[]()~`>#+-=|{}.!
        let input = "_*[]()~`>#+-=|{}.!";
        let output = markdown_to_telegram_v2(input);
        assert_eq!(
            output,
            "\\_\\*\\[\\]\\(\\)\\~\\`\\>\\#\\+\\-\\=\\|\\{\\}\\.\\!"
        );
    }

    #[test]
    fn telegram_leaves_normal_text_unchanged() {
        let input = "Hello world 123";
        assert_eq!(markdown_to_telegram_v2(input), input);
    }

    // -----------------------------------------------------------------------
    // Plain text stripping
    // -----------------------------------------------------------------------

    #[test]
    fn plain_strips_formatting() {
        let input = "This is **bold** and ~~struck~~ and `code`";
        let output = markdown_to_plain(input);
        assert_eq!(output, "This is bold and struck and code");
    }

    // -----------------------------------------------------------------------
    // Message chunking
    // -----------------------------------------------------------------------

    #[test]
    fn chunk_returns_single_chunk_when_under_limit() {
        let msg = "Short message";
        let chunks = chunk_message(msg, 100);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], msg);
    }

    #[test]
    fn chunk_splits_at_newline_boundary() {
        let msg = "Line one\nLine two\nLine three";
        let chunks = chunk_message(msg, 18); // "Line one\nLine two" is 18 chars
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "Line one\nLine two");
        assert_eq!(chunks[1], "Line three");
    }

    #[test]
    fn chunk_splits_at_sentence_boundary() {
        let msg = "First sentence. Second sentence. Third sentence.";
        let chunks = chunk_message(msg, 35);
        assert!(chunks.len() >= 2);
        // First chunk should end at a sentence boundary
        assert!(chunks[0].ends_with('.'));
    }

    #[test]
    fn chunk_splits_at_whitespace_as_fallback() {
        // No newlines, no sentence boundaries — just words
        let msg = "word1 word2 word3 word4 word5";
        let chunks = chunk_message(msg, 12);
        assert!(chunks.len() >= 2);
        // Chunks should not start or end with broken words
        for chunk in &chunks {
            assert!(!chunk.starts_with(' '));
        }
    }

    #[test]
    fn chunk_respects_slack_limit() {
        let msg = "a".repeat(SLACK_MAX_CHARS + 100);
        let chunks = chunk_message(&msg, SLACK_MAX_CHARS);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            assert!(chunk.len() <= SLACK_MAX_CHARS);
        }
    }

    #[test]
    fn chunk_respects_discord_limit() {
        let msg = "a".repeat(DISCORD_MAX_CHARS + 100);
        let chunks = chunk_message(&msg, DISCORD_MAX_CHARS);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            assert!(chunk.len() <= DISCORD_MAX_CHARS);
        }
    }

    #[test]
    fn chunk_respects_telegram_limit() {
        let msg = "a".repeat(TELEGRAM_MAX_CHARS + 100);
        let chunks = chunk_message(&msg, TELEGRAM_MAX_CHARS);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            assert!(chunk.len() <= TELEGRAM_MAX_CHARS);
        }
    }
}
