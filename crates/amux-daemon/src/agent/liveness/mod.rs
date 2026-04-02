//! Agent liveness architecture — checkpointing, health monitoring, stuck
//! detection, and recovery for long-running goal runs.

pub mod checkpoint;
pub mod health_monitor;
pub mod recovery;
pub mod state_layers;
pub mod stuck_detection;
