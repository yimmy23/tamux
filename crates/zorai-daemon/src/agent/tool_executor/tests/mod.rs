use super::*;
use zorai_protocol::tool_names;

// Shared imports for descendant test modules (previously inherited via include! flattening).
use crate::agent::{
    types::{AgentConfig, AgentEvent, ToolCall, ToolFunction},
    AgentEngine,
};
use crate::history::SkillVariantRecord;
use crate::session_manager::SessionManager;
use base64::Engine;
use std::fs;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use tokio::sync::broadcast;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;
use zorai_protocol::{DaemonMessage, GatewaySendResult, SessionId};

mod part1;
mod part10;
mod part2;
mod part3;
mod part4;
mod part5;
mod part6;
mod part7;
mod part8;
mod part9;

pub(super) fn current_dir_test_lock() -> &'static std::sync::Mutex<()> {
    crate::test_support::env_test_mutex()
}
