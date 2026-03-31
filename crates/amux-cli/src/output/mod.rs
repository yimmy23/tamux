pub(crate) mod audit;
pub(crate) mod settings;

pub(crate) fn truncate_for_display(value: &str, max_chars: usize) -> String {
    let truncated: String = value.chars().take(max_chars).collect();
    if value.chars().count() > max_chars {
        format!("{truncated}…")
    } else {
        truncated
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
