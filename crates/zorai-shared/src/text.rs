//! UTF-8-safe string slicing helpers shared across the workspace.
//!
//! Several crates need to cut strings at byte budgets without splitting a
//! multi-byte character. Keep all boundary-walking logic here so a hardening
//! change (e.g. grapheme-cluster awareness) lands in exactly one place.

/// Largest index `<= index` that lies on a char boundary of `text`.
pub fn floor_char_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// Smallest index `>= index` that lies on a char boundary of `text`.
pub fn ceil_char_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

/// Prefix of `input` ending at or before `end`, never splitting a char.
pub fn utf8_prefix(input: &str, end: usize) -> &str {
    &input[..floor_char_boundary(input, end)]
}

/// Suffix of `input` starting at or after `start`, never splitting a char.
pub fn utf8_suffix(input: &str, start: usize) -> &str {
    &input[ceil_char_boundary(input, start)..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundaries_never_split_multibyte_chars() {
        let text = "aé€b";
        for index in 0..=text.len() + 2 {
            let floor = floor_char_boundary(text, index);
            let ceil = ceil_char_boundary(text, index);
            assert!(text.is_char_boundary(floor));
            assert!(text.is_char_boundary(ceil));
            assert!(floor <= index.min(text.len()));
            assert!(ceil >= index.min(text.len()) || ceil == text.len());
        }
    }

    #[test]
    fn prefix_and_suffix_round_trip_ascii() {
        assert_eq!(utf8_prefix("hello", 3), "hel");
        assert_eq!(utf8_suffix("hello", 3), "lo");
        assert_eq!(utf8_prefix("hello", 99), "hello");
        assert_eq!(utf8_suffix("hello", 99), "");
    }

    #[test]
    fn prefix_backs_off_and_suffix_advances_on_multibyte() {
        let text = "é"; // 2 bytes
        assert_eq!(utf8_prefix(text, 1), "");
        assert_eq!(utf8_suffix(text, 1), "");
        assert_eq!(utf8_prefix(text, 2), "é");
    }
}
