//! tamux-protocol: Shared message types and codec for daemon <-> client IPC.
//!
//! This crate defines the wire protocol used between the tamux daemon and any
//! client (Tauri UI, CLI, future multiplayer connections). Messages are
//! length-prefixed bincode frames sent over the configured IPC transport.

mod codec;
mod config;
mod logging;
mod messages;
mod runtime_paths;
mod update;

pub use codec::{
    client_message_fits_ipc, client_message_payload_len, daemon_message_fits_ipc,
    daemon_message_payload_len, validate_client_message_size, validate_daemon_message_size,
    AmuxCodec, DaemonCodec, MAX_IPC_FRAME_SIZE_BYTES,
};
pub use config::{
    amux_data_dir, default_tcp_addr, ensure_amux_data_dir, has_whatsapp_allowed_contacts,
    log_file_path, normalize_whatsapp_phone_like_identifier, parse_whatsapp_allowed_contacts,
    AmuxConfig, DEFAULT_TCP_HOST, DEFAULT_TCP_PORT,
};
pub use logging::{dated_log_file_name, dated_log_file_path, DailyLogWriter};
pub use messages::*;
pub use runtime_paths::{
    legacy_agent_skills_dir, tamux_guidelines_dir, tamux_root_dir, tamux_skills_dir,
    thread_artifacts_dir, thread_media_dir, thread_previews_dir, thread_root_dir, thread_specs_dir,
};
pub use update::*;
