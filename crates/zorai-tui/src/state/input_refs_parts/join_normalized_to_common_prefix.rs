use std::path::{Path, PathBuf};

pub(super) fn join_normalized(base: &Path, raw_path: &str) -> PathBuf {
    let mut path = base.to_path_buf();
    for component in raw_path.split(['/', '\\']) {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            path.pop();
            continue;
        }
        path.push(component);
    }
    path
}

pub(super) fn last_separator_index(raw_path: &str) -> Option<usize> {
    raw_path.rfind(|ch| matches!(ch, '/' | '\\'))
}

pub(super) fn common_prefix(values: &[String]) -> String {
    let Some(first) = values.first() else {
        return String::new();
    };

    let mut prefix = first.clone();
    for value in values.iter().skip(1) {
        let shared_len = prefix
            .chars()
            .zip(value.chars())
            .take_while(|(a, b)| a == b)
            .count();
        prefix = prefix.chars().take(shared_len).collect();
        if prefix.is_empty() {
            break;
        }
    }

    prefix
}
