//! Meta-cognitive loop — self-assessment, dynamic re-planning, escalation
//! pathways, and resource allocation for autonomous agent execution.

pub mod escalation;
pub mod introspector;
pub mod pattern_regulator;
pub mod persistence;
pub mod replanning;
pub mod resource_alloc;
pub mod self_assessment;
pub mod types;

#[cfg(test)]
#[path = "tests/persistence.rs"]
mod persistence_tests;

#[cfg(test)]
#[path = "tests/pattern_regulator.rs"]
mod pattern_regulator_tests;
