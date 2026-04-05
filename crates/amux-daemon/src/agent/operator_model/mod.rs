//! Local aggregate-only operator model.

use std::collections::HashMap;

use amux_protocol::ApprovalDecision;
use serde::{Deserialize, Serialize};

use super::*;

mod metrics;
mod model;
mod persistence;
mod profile;
mod reading;
mod runtime;
#[cfg(test)]
mod tests;

pub(crate) use metrics::*;
pub(crate) use model::*;
pub(crate) use persistence::*;
pub(crate) use reading::*;
