use super::*;

fn canonical_enum_value(value: &str, default: &str, aliases: &[(&str, &str)]) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return default.to_string();
    }
    let lowercase = trimmed.to_ascii_lowercase();
    for (alias, canonical) in aliases {
        if lowercase == *alias {
            return (*canonical).to_string();
        }
    }
    lowercase
}

fn normalize_string_enum_field(
    object: &mut serde_json::Map<String, Value>,
    key: &str,
    default: &str,
    aliases: &[(&str, &str)],
) {
    let normalized = match object.get(key) {
        Some(Value::String(value)) => canonical_enum_value(value, default, aliases),
        Some(Value::Null) | None => default.to_string(),
        Some(other) => canonical_enum_value(&other.to_string(), default, aliases),
    };
    object.insert(key.to_string(), Value::String(normalized));
}

pub(crate) fn sanitize_config_value(config: &mut Value) {
    normalize_config_keys_to_snake_case(config);
    let Some(root) = config.as_object_mut() else {
        return;
    };
    sanitize_weles_builtin_overrides(root);

    normalize_string_enum_field(
        root,
        "auth_source",
        "api_key",
        &[
            ("apikey", "api_key"),
            ("api-key", "api_key"),
            ("chatgptsubscription", "chatgpt_subscription"),
            ("chatgpt-subscription", "chatgpt_subscription"),
            ("chatgpt subscription", "chatgpt_subscription"),
            ("githubcopilot", "github_copilot"),
            ("github-copilot", "github_copilot"),
            ("github copilot", "github_copilot"),
        ],
    );
    normalize_string_enum_field(
        root,
        "api_transport",
        "responses",
        &[
            ("responses", "responses"),
            ("chatcompletions", "chat_completions"),
            ("chat-completions", "chat_completions"),
            ("chat completions", "chat_completions"),
            ("nativeassistant", "native_assistant"),
            ("native-assistant", "native_assistant"),
            ("native assistant", "native_assistant"),
        ],
    );
    normalize_string_enum_field(
        root,
        "agent_backend",
        "daemon",
        &[
            ("daemon", "daemon"),
            ("openclaw", "openclaw"),
            ("hermes", "hermes"),
            ("legacy", "legacy"),
        ],
    );

    if let Some(Value::Object(compliance)) = root.get_mut("compliance") {
        normalize_string_enum_field(
            compliance,
            "mode",
            "standard",
            &[
                ("standard", "standard"),
                ("soc2", "soc2"),
                ("hipaa", "hipaa"),
                ("fedramp", "fedramp"),
            ],
        );
    }

    if let Some(Value::Object(concierge)) = root.get_mut("concierge") {
        normalize_string_enum_field(
            concierge,
            "detail_level",
            "proactive_triage",
            &[
                ("minimal", "minimal"),
                ("contextsummary", "context_summary"),
                ("context-summary", "context_summary"),
                ("context summary", "context_summary"),
                ("proactivetriage", "proactive_triage"),
                ("proactive-triage", "proactive_triage"),
                ("proactive triage", "proactive_triage"),
                ("dailybriefing", "daily_briefing"),
                ("daily-briefing", "daily_briefing"),
                ("daily briefing", "daily_briefing"),
            ],
        );
    }

    if let Some(Value::Object(providers)) = root.get_mut("providers") {
        for provider in providers.values_mut() {
            if let Value::Object(provider_obj) = provider {
                for required_field in ["base_url", "model", "api_key"] {
                    if !provider_obj.contains_key(required_field) {
                        provider_obj
                            .insert(required_field.to_string(), Value::String(String::new()));
                    }
                }
                normalize_string_enum_field(
                    provider_obj,
                    "auth_source",
                    "api_key",
                    &[
                        ("apikey", "api_key"),
                        ("api-key", "api_key"),
                        ("chatgptsubscription", "chatgpt_subscription"),
                        ("chatgpt-subscription", "chatgpt_subscription"),
                        ("chatgpt subscription", "chatgpt_subscription"),
                        ("githubcopilot", "github_copilot"),
                        ("github-copilot", "github_copilot"),
                        ("github copilot", "github_copilot"),
                    ],
                );
                normalize_string_enum_field(
                    provider_obj,
                    "api_transport",
                    "responses",
                    &[
                        ("responses", "responses"),
                        ("chatcompletions", "chat_completions"),
                        ("chat-completions", "chat_completions"),
                        ("chat completions", "chat_completions"),
                        ("nativeassistant", "native_assistant"),
                        ("native-assistant", "native_assistant"),
                        ("native assistant", "native_assistant"),
                    ],
                );
            }
        }
    }
}
