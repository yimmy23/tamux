//! Core agent loop - LLM streaming, tool execution, and turn management.

use super::*;

#[path = "agent_loop/cost.rs"]
mod cost;
#[path = "agent_loop/plugin_commands.rs"]
mod plugin_commands;
#[path = "agent_loop/policy.rs"]
mod policy;
#[path = "agent_loop/recovery.rs"]
mod recovery;
#[path = "agent_loop/send_message/mod.rs"]
pub(crate) mod send_message;
#[path = "agent_loop/thread_state.rs"]
mod thread_state;

#[allow(unused_imports)]
use cost::*;
#[allow(unused_imports)]
use plugin_commands::*;
#[allow(unused_imports)]
use policy::*;
#[allow(unused_imports)]
use recovery::*;
#[allow(unused_imports)]
use send_message::*;
#[allow(unused_imports)]
use thread_state::*;

#[cfg(test)]
#[path = "agent_loop/tests/mod.rs"]
mod tests;
