//! Gateway platform message formatting and chunking.

pub const SLACK_MAX_CHARS: usize = 4000;
pub const DISCORD_MAX_CHARS: usize = 2000;
pub const TELEGRAM_MAX_CHARS: usize = 4096;

pub fn markdown_to_slack_mrkdwn(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut in_code_block = false;
    let mut in_inline_code = false;

    while i < len {
        if i + 2 < len && chars[i] == '`' && chars[i + 1] == '`' && chars[i + 2] == '`' {
            in_code_block = !in_code_block;
            result.push_str("```");
            i += 3;
            continue;
        }

        if chars[i] == '`' && !in_code_block {
            in_inline_code = !in_inline_code;
            result.push('`');
            i += 1;
            continue;
        }

        if in_code_block || in_inline_code {
            result.push(chars[i]);
            i += 1;
            continue;
        }

        if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
            if let Some(close) = find_closing_marker(&chars, i + 2, &['*', '*']) {
                result.push('\x01');
                i += 2;
                while i < close {
                    result.push(chars[i]);
                    i += 1;
                }
                result.push('\x01');
                i += 2;
                continue;
            }
        }

        if i + 1 < len && chars[i] == '~' && chars[i + 1] == '~' {
            if let Some(close) = find_closing_marker(&chars, i + 2, &['~', '~']) {
                result.push('~');
                i += 2;
                while i < close {
                    result.push(chars[i]);
                    i += 1;
                }
                result.push('~');
                i += 2;
                continue;
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    result.replace('\x01', "*")
}

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

pub fn markdown_to_discord(input: &str) -> String {
    input.to_string()
}

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

pub fn markdown_to_plain(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if i + 2 < len && chars[i] == '`' && chars[i + 1] == '`' && chars[i + 2] == '`' {
            i += 3;
            continue;
        }
        if i + 1 < len
            && ((chars[i] == '*' && chars[i + 1] == '*')
                || (chars[i] == '_' && chars[i + 1] == '_'))
        {
            i += 2;
            continue;
        }
        if i + 1 < len && chars[i] == '~' && chars[i + 1] == '~' {
            i += 2;
            continue;
        }
        if matches!(chars[i], '*' | '_' | '`') {
            i += 1;
            continue;
        }
        result.push(chars[i]);
        i += 1;
    }

    result
}

pub fn chunk_message(message: &str, max_chars: usize) -> Vec<String> {
    if message.chars().count() <= max_chars {
        return vec![message.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = message;

    while !remaining.is_empty() {
        if remaining.chars().count() <= max_chars {
            chunks.push(remaining.to_string());
            break;
        }

        let split_at = char_boundary_after_chars(remaining, max_chars);
        let window = &remaining[..split_at];

        if let Some(pos) = window.rfind('\n') {
            if pos > 0 {
                chunks.push(remaining[..pos].to_string());
                remaining = &remaining[pos + 1..];
                continue;
            }
        }

        if let Some(pos) = window.rfind(". ") {
            if pos > 0 {
                chunks.push(remaining[..=pos].to_string());
                remaining = &remaining[pos + 2..];
                continue;
            }
        }

        if let Some(pos) = window.rfind(char::is_whitespace) {
            if pos > 0 {
                chunks.push(remaining[..pos].to_string());
                remaining = &remaining[pos + 1..];
                continue;
            }
        }

        chunks.push(window.to_string());
        remaining = &remaining[split_at..];
    }

    chunks
}

fn char_boundary_after_chars(value: &str, max_chars: usize) -> usize {
    if max_chars == 0 {
        return 0;
    }

    match value.char_indices().nth(max_chars) {
        Some((index, _)) => index,
        None => value.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_message_preserves_utf8_boundaries() {
        let message = "zażółćgęśląjaźń";
        let chunks = chunk_message(message, 5);
        assert!(!chunks.is_empty());
        assert_eq!(chunks.concat(), message);
        assert!(chunks.iter().all(|chunk| std::str::from_utf8(chunk.as_bytes()).is_ok()));
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 5));
    }
}
