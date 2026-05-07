use super::*;
use crossterm::event::{KeyCode, KeyModifiers, ModifierKeyCode, MouseButton, MouseEvent, MouseEventKind};
use crate::widgets;
use crate::providers;
use ratatui::prelude::*;
use zorai_shared::providers::{
    AudioToolKind, PROVIDER_ID_AZURE_OPENAI, PROVIDER_ID_CUSTOM, PROVIDER_ID_GITHUB_COPILOT,
    PROVIDER_ID_GROQ, PROVIDER_ID_MINIMAX, PROVIDER_ID_MINIMAX_CODING_PLAN, PROVIDER_ID_OPENAI,
    PROVIDER_ID_OPENROUTER, PROVIDER_ID_XAI, PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
};

#[path = "current_settings_field_name_to_whatsapp_linking_allowed_parts/image_remote_model_fetch_output_modalities_to_fetched_model_supports.rs"]
mod image_remote_model_fetch_output_modalities_to_fetched_model_supports;
#[path = "current_settings_field_name_to_whatsapp_linking_allowed_parts/current_settings_field_name_to_whatsapp_linking_allowed.rs"]
mod current_settings_field_name_to_whatsapp_linking_allowed;
#[path = "current_settings_field_name_to_whatsapp_linking_allowed_parts/provider_auth_snapshot_to_open_subagent_editor_existing.rs"]
mod provider_auth_snapshot_to_open_subagent_editor_existing;
#[path = "current_settings_field_name_to_whatsapp_linking_allowed_parts/subagent_editor_system_prompt_override_to_close_subagent_editor.rs"]
mod subagent_editor_system_prompt_override_to_close_subagent_editor;
