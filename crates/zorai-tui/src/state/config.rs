#![allow(dead_code)]

#[path = "config_parts/json_u32_to_from_config.rs"]
mod json_u32_to_from_config;

#[path = "config_parts/new_to_default.rs"]
mod new_to_default;

pub use json_u32_to_from_config::*;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers;
    use zorai_shared::providers::{PROVIDER_ID_MINIMAX_CODING_PLAN, PROVIDER_ID_OPENAI};

    fn make_snapshot(provider: &str, model: &str) -> AgentConfigSnapshot {
        AgentConfigSnapshot {
            provider: provider.into(),
            model: model.into(),
            custom_model_name: String::new(),
            base_url: "https://api.example.com".into(),
            api_key: "sk-test".into(),
            assistant_id: "asst_test".into(),
            auth_source: "api_key".into(),
            reasoning_effort: "high".into(),
            api_transport: "responses".into(),
            context_window_tokens: 128_000,
        }
    }

    #[test]
    fn config_received_populates_all_fields() {
        let mut state = ConfigState::new();
        state.reduce(ConfigAction::ConfigReceived(make_snapshot(
            PROVIDER_ID_OPENAI,
            "gpt-4o",
        )));
        assert_eq!(state.provider(), PROVIDER_ID_OPENAI);
        assert_eq!(state.model(), "gpt-4o");
        assert_eq!(state.base_url(), "https://api.example.com");
        assert_eq!(state.api_key(), "sk-test");
        assert_eq!(state.reasoning_effort(), "high");
    }

    #[test]
    fn models_fetched_replaces_list() {
        let mut state = ConfigState::new();
        state.reduce(ConfigAction::ModelsFetched(vec![
            FetchedModel {
                id: "m1".into(),
                name: Some("Model One".into()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
            FetchedModel {
                id: "m2".into(),
                name: None,
                context_window: None,
                pricing: None,
                metadata: None,
            },
        ]));
        assert_eq!(state.fetched_models().len(), 2);
        assert_eq!(state.fetched_models()[0].id, "m1");

        state.reduce(ConfigAction::ModelsFetched(vec![]));
        assert_eq!(state.fetched_models().len(), 0);
    }

    #[test]
    fn set_provider_resets_base_url_and_model_to_definition_defaults() {
        let mut state = ConfigState::new();
        state.reduce(ConfigAction::ConfigReceived(make_snapshot(
            PROVIDER_ID_OPENAI,
            "gpt-4o",
        )));
        state.reduce(ConfigAction::SetProvider(
            PROVIDER_ID_MINIMAX_CODING_PLAN.into(),
        ));
        assert_eq!(state.provider(), PROVIDER_ID_MINIMAX_CODING_PLAN);
        let def = providers::find_by_id(PROVIDER_ID_MINIMAX_CODING_PLAN).unwrap();
        assert_eq!(state.base_url(), def.default_base_url);
        assert_eq!(state.model(), def.default_model);
        assert_eq!(state.api_key(), "sk-test");
    }

    #[test]
    fn set_model_updates_only_model() {
        let mut state = ConfigState::new();
        state.reduce(ConfigAction::ConfigReceived(make_snapshot(
            PROVIDER_ID_OPENAI,
            "gpt-4o",
        )));
        state.reduce(ConfigAction::SetModel("gpt-4o-mini".into()));
        assert_eq!(state.model(), "gpt-4o-mini");
        assert_eq!(state.provider(), PROVIDER_ID_OPENAI);
    }

    #[test]
    fn config_raw_received_stores_json() {
        let mut state = ConfigState::new();
        assert!(state.agent_config_raw().is_none());

        let raw = serde_json::json!({ "key": "value", "number": 42 });
        state.reduce(ConfigAction::ConfigRawReceived(raw.clone()));
        assert!(state.agent_config_raw().is_some());
        assert_eq!(state.agent_config_raw().unwrap()["key"], "value");
    }

    #[test]
    fn config_received_twice_overwrites() {
        let mut state = ConfigState::new();
        state.reduce(ConfigAction::ConfigReceived(make_snapshot(
            PROVIDER_ID_OPENAI,
            "gpt-4o",
        )));
        state.reduce(ConfigAction::ConfigReceived(make_snapshot(
            zorai_shared::providers::PROVIDER_ID_ANTHROPIC,
            "claude-3-5-sonnet",
        )));
        assert_eq!(
            state.provider(),
            zorai_shared::providers::PROVIDER_ID_ANTHROPIC
        );
        assert_eq!(state.model(), "claude-3-5-sonnet");
    }
}

#[cfg(test)]
#[path = "tests/config_audio.rs"]
mod config_audio_tests;
