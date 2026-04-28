//! Sub-agent management — tool filtering, context budgets, termination,
//! supervision, and lifecycle tracking.

pub mod context_budget;
pub mod lifecycle;
pub mod supervisor;
pub mod termination;
pub mod tool_filter;
pub mod tool_graph;
