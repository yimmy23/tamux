//! First-run setup wizard for zorai.
//!
//! Connects to the daemon via IPC socket and configures the agent through
//! protocol messages. All config writes go through daemon IPC -- config.json
//! is never written or referenced as a daemon config source.
//!
//! Navigation uses crossterm arrow-key selection (not number input).
//! Provider list is queried from the daemon at runtime (no hardcoded list).

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::{self, Stylize};
use crossterm::terminal;
use futures::{SinkExt, StreamExt};
use std::io::{self, Write};
use tokio_util::codec::Framed;
use zorai_protocol::{parse_whatsapp_allowed_contacts, ClientMessage, DaemonMessage, ZoraiCodec};

mod agents;
mod flow;
mod ipc;
mod steps;
mod terminal_ui;
#[cfg(test)]
mod tests;
mod tiering;
mod types;
mod whatsapp;

pub(crate) use flow::{probe_setup_via_ipc, run_setup_wizard, SetupProbe};
pub(crate) use ipc::ensure_daemon_running;
pub use types::PostSetupAction;

use agents::*;
use ipc::*;
use steps::*;
use terminal_ui::*;
use tiering::*;
use types::*;
use whatsapp::*;
