//! Negative knowledge constraint graph: tracks ruled-out approaches,
//! impossible combinations, and known limitations with TTL expiry.

use super::{ConstraintState, ConstraintType, Episode, EpisodeOutcome, NegativeConstraint};
use crate::agent::engine::AgentEngine;

use anyhow::Result;
use rusqlite::params;

mod engine;
mod format;
mod logic;
mod storage;
#[cfg(test)]
mod tests;

pub(crate) use format::*;
pub(crate) use logic::*;
pub(crate) use storage::*;
