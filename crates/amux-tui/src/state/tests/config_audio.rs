// TDD tests for audio configuration parsing from agent_config_raw
// Audio settings are stored in agent_config_raw.extra via flatten

use crate::state::config::ConfigAction;
use crate::state::config::ConfigState;
use serde_json::json;

fn make_config_raw_with_audio() -> serde_json::Value {
    json!({
        "provider": "openai",
        "model": "gpt-4o",
        "extra": {
            "audio_stt_enabled": true,
            "audio_stt_provider": "openai",
            "audio_stt_model": "whisper-1",
            "audio_tts_enabled": true,
            "audio_tts_provider": "openai",
            "audio_tts_model": "tts-1",
            "audio_tts_voice": "alloy"
        }
    })
}

fn make_config_raw_without_audio() -> serde_json::Value {
    json!({
        "provider": "openai",
        "model": "gpt-4o",
        "extra": {}
    })
}

fn make_config_raw_partial_audio() -> serde_json::Value {
    json!({
        "provider": "openai",
        "model": "gpt-4o",
        "extra": {
            "audio_stt_enabled": true,
            "audio_stt_provider": "openai"
        }
    })
}

fn make_config_raw_nested_audio() -> serde_json::Value {
    json!({
        "provider": "openai",
        "model": "gpt-4o",
        "audio": {
            "stt": {
                "enabled": true,
                "provider": "openai",
                "model": "gpt-4o-transcribe",
                "language": "pl"
            },
            "tts": {
                "enabled": true,
                "provider": "custom",
                "model": "gpt-4o-mini-tts",
                "voice": "verse",
                "auto_speak": true
            }
        }
    })
}

fn make_config_raw_flat_legacy_audio() -> serde_json::Value {
    json!({
        "provider": "openai",
        "model": "gpt-4o",
        "audio_stt_enabled": true,
        "audio_stt_provider": "openai",
        "audio_stt_model": "whisper-1",
        "audio_tts_enabled": true,
        "audio_tts_provider": "openai",
        "audio_tts_model": "gpt-4o-mini-tts",
        "audio_tts_voice": "alloy"
    })
}

#[test]
fn audio_stt_enabled_parses_from_extra() {
    let mut state = ConfigState::new();
    state.reduce(ConfigAction::ConfigRawReceived(make_config_raw_with_audio()));

    assert_eq!(state.audio_stt_enabled(), true);
}

#[test]
fn audio_stt_provider_parses_from_extra() {
    let mut state = ConfigState::new();
    state.reduce(ConfigAction::ConfigRawReceived(make_config_raw_with_audio()));

    assert_eq!(state.audio_stt_provider(), "openai");
}

#[test]
fn audio_stt_model_parses_from_extra() {
    let mut state = ConfigState::new();
    state.reduce(ConfigAction::ConfigRawReceived(make_config_raw_with_audio()));

    assert_eq!(state.audio_stt_model(), "whisper-1");
}

#[test]
fn audio_tts_enabled_parses_from_extra() {
    let mut state = ConfigState::new();
    state.reduce(ConfigAction::ConfigRawReceived(make_config_raw_with_audio()));

    assert_eq!(state.audio_tts_enabled(), true);
}

#[test]
fn audio_tts_provider_parses_from_extra() {
    let mut state = ConfigState::new();
    state.reduce(ConfigAction::ConfigRawReceived(make_config_raw_with_audio()));

    assert_eq!(state.audio_tts_provider(), "openai");
}

#[test]
fn audio_tts_model_parses_from_extra() {
    let mut state = ConfigState::new();
    state.reduce(ConfigAction::ConfigRawReceived(make_config_raw_with_audio()));

    assert_eq!(state.audio_tts_model(), "tts-1");
}

#[test]
fn audio_tts_voice_parses_from_extra() {
    let mut state = ConfigState::new();
    state.reduce(ConfigAction::ConfigRawReceived(make_config_raw_with_audio()));

    assert_eq!(state.audio_tts_voice(), "alloy");
}

#[test]
fn audio_fields_default_when_missing() {
    let mut state = ConfigState::new();
    state.reduce(ConfigAction::ConfigRawReceived(
        make_config_raw_without_audio(),
    ));

    assert_eq!(state.audio_stt_enabled(), false);
    assert_eq!(state.audio_stt_provider(), "");
    assert_eq!(state.audio_stt_model(), "");
    assert_eq!(state.audio_tts_enabled(), false);
    assert_eq!(state.audio_tts_provider(), "");
    assert_eq!(state.audio_tts_model(), "");
    assert_eq!(state.audio_tts_voice(), "");
}

#[test]
fn audio_partial_fields_parse_with_defaults() {
    let mut state = ConfigState::new();
    state.reduce(ConfigAction::ConfigRawReceived(
        make_config_raw_partial_audio(),
    ));

    assert_eq!(state.audio_stt_enabled(), true);
    assert_eq!(state.audio_stt_provider(), "openai");
    assert_eq!(state.audio_stt_model(), ""); // missing, defaults to empty
    assert_eq!(state.audio_tts_enabled(), false); // missing, defaults to false
    assert_eq!(state.audio_tts_provider(), ""); // missing
}

#[test]
fn audio_fields_handle_none_agent_config_raw() {
    let state = ConfigState::new();

    assert_eq!(state.audio_stt_enabled(), false);
    assert_eq!(state.audio_stt_provider(), "");
    assert_eq!(state.audio_stt_model(), "");
    assert_eq!(state.audio_tts_enabled(), false);
    assert_eq!(state.audio_tts_provider(), "");
    assert_eq!(state.audio_tts_model(), "");
    assert_eq!(state.audio_tts_voice(), "");
}

#[test]
fn audio_nested_fields_parse_from_canonical_audio_section() {
    let mut state = ConfigState::new();
    state.reduce(ConfigAction::ConfigRawReceived(
        make_config_raw_nested_audio(),
    ));

    assert_eq!(state.audio_stt_enabled(), true);
    assert_eq!(state.audio_stt_provider(), "openai");
    assert_eq!(state.audio_stt_model(), "gpt-4o-transcribe");
    assert_eq!(state.audio_tts_enabled(), true);
    assert_eq!(state.audio_tts_provider(), "custom");
    assert_eq!(state.audio_tts_model(), "gpt-4o-mini-tts");
    assert_eq!(state.audio_tts_voice(), "verse");
}

#[test]
fn audio_fields_parse_from_flattened_legacy_keys() {
    let mut state = ConfigState::new();
    state.reduce(ConfigAction::ConfigRawReceived(
        make_config_raw_flat_legacy_audio(),
    ));

    assert_eq!(state.audio_stt_enabled(), true);
    assert_eq!(state.audio_stt_provider(), "openai");
    assert_eq!(state.audio_stt_model(), "whisper-1");
    assert_eq!(state.audio_tts_enabled(), true);
    assert_eq!(state.audio_tts_provider(), "openai");
    assert_eq!(state.audio_tts_model(), "gpt-4o-mini-tts");
    assert_eq!(state.audio_tts_voice(), "alloy");
}
