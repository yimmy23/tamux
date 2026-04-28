/// Flatten a JSON value into dot-notation key=value pairs.
pub(crate) fn flatten_json(
    prefix: &str,
    value: &serde_json::Value,
    out: &mut Vec<(String, String)>,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                let full_key = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                flatten_json(&full_key, value, out);
            }
        }
        serde_json::Value::Null => {}
        serde_json::Value::String(string) => {
            let display = if is_sensitive_key(prefix) {
                "***".to_string()
            } else {
                string.clone()
            };
            out.push((prefix.to_string(), display));
        }
        other => {
            let display = if is_sensitive_key(prefix) {
                "***".to_string()
            } else {
                other.to_string()
            };
            out.push((prefix.to_string(), display));
        }
    }
}

/// Resolve a dot-notation path to a JSON value reference.
pub(crate) fn resolve_dot_path<'a>(
    value: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

/// Check if a key name refers to a sensitive value that should be redacted.
pub(crate) fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    lower.contains("api_key") || lower.contains("token") || lower.contains("secret")
}
