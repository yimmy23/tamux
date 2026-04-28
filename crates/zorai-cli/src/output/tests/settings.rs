use serde_json::json;

use crate::output::settings::{flatten_json, is_sensitive_key, resolve_dot_path};

#[test]
fn flatten_json_redacts_sensitive_keys_and_skips_nulls() {
    let value = json!({
        "provider": {
            "api_key": "secret-value",
            "model": "gpt-test",
            "optional": null
        },
        "token": "hidden"
    });

    let mut pairs = Vec::new();
    flatten_json("", &value, &mut pairs);

    assert!(pairs.contains(&(String::from("provider.api_key"), String::from("***"))));
    assert!(pairs.contains(&(String::from("provider.model"), String::from("gpt-test"))));
    assert!(pairs.contains(&(String::from("token"), String::from("***"))));
    assert!(!pairs.iter().any(|(key, _)| key == "provider.optional"));
}

#[test]
fn resolve_dot_path_returns_nested_values() {
    let value = json!({
        "provider": {
            "model": "gpt-test"
        }
    });

    assert_eq!(
        resolve_dot_path(&value, "provider.model").and_then(|v| v.as_str()),
        Some("gpt-test")
    );
    assert!(resolve_dot_path(&value, "provider.missing").is_none());
}

#[test]
fn is_sensitive_key_matches_common_secret_names() {
    assert!(is_sensitive_key("provider.api_key"));
    assert!(is_sensitive_key("auth.token"));
    assert!(is_sensitive_key("db.secret"));
    assert!(!is_sensitive_key("provider.model"));
}
