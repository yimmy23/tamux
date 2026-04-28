//! Agent configuration get/set.

use super::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod engine;
mod sanitize;
mod subagents;
mod value;
mod weles;

pub(in crate::agent) use sanitize::*;
pub(in crate::agent) use subagents::*;
pub(in crate::agent) use value::*;
pub(in crate::agent) use weles::*;

pub(crate) use weles::{
    canonicalize_weles_client_update, ConfigEffectiveRuntimeState, ConfigReconcileState,
    ConfigRuntimeProjection,
};

#[cfg(test)]
#[path = "../tests/config/mod.rs"]
mod tests;
