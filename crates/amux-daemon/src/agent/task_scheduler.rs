//! Task queue state machine — scheduling, lane management, and ready-task selection.

use std::collections::{HashMap, HashSet, VecDeque};

use super::now_millis;
use super::types::*;
mod classification;
mod selection;

pub(in crate::agent) use classification::{classify_task, project_task_runs};
pub(in crate::agent) use selection::{
    compute_task_backoff_ms, current_task_lane_key, describe_scheduled_time,
    is_task_terminal_status, make_task_log_entry, refresh_task_queue_state,
    select_ready_task_indices, status_message, subagent_parent_key, task_enforces_workspace_lock,
    task_lane_key, task_priority_rank, task_workspace_key,
};

#[cfg(test)]
#[path = "task_scheduler/tests.rs"]
mod tests;
