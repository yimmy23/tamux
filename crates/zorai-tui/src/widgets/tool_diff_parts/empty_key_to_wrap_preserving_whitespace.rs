use unicode_width::UnicodeWidthChar;

pub(super) fn empty_key(prefix: &str) -> String {
    if prefix.is_empty() {
        "value".to_string()
    } else {
        prefix.to_string()
    }
}

pub(super) fn wrap_preserving_whitespace(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut wrapped = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
        if current_width > 0 && current_width + ch_width > width {
            wrapped.push(current);
            current = String::new();
            current_width = 0;
        }
        current.push(ch);
        current_width += ch_width;
    }

    if !current.is_empty() {
        wrapped.push(current);
    }

    if wrapped.is_empty() {
        wrapped.push(String::new());
    }
    wrapped
}
