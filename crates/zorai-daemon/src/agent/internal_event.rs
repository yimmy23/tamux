use tokio::sync::broadcast;

use super::types::TaskStatus;

/// Daemon-internal coordination events.
///
/// Distinct from [`super::types::AgentEvent`], which is serialized and streamed
/// to UI clients. Internal events never leave the process, so adding variants
/// here carries no client-protocol or serialization cost. Subscribers use them
/// to react immediately to lifecycle changes instead of waiting for the next
/// poll/supervision tick.
#[derive(Debug, Clone)]
pub(crate) enum InternalAgentEvent {
    /// A task reached a terminal status. Carries the parent task id (when the
    /// task is a subagent) so supervisors and dispatchers can resume or clean up
    /// the parent without polling.
    TaskTerminal {
        task_id: String,
        parent_task_id: Option<String>,
        status: TaskStatus,
    },
}

/// Capacity for the internal event broadcast channel. Internal events are small
/// and consumed promptly; a lagging subscriber simply misses an event and falls
/// back to its periodic tick, so a modest buffer is sufficient.
pub(crate) const INTERNAL_EVENT_CHANNEL_CAPACITY: usize = 256;

pub(crate) fn new_internal_event_channel() -> broadcast::Sender<InternalAgentEvent> {
    let (tx, _) = broadcast::channel(INTERNAL_EVENT_CHANNEL_CAPACITY);
    tx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_statuses_wake_internal_coordination() {
        for status in [
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::Cancelled,
            TaskStatus::BudgetExceeded,
            TaskStatus::FailedAnalyzing,
        ] {
            assert!(
                status.is_terminal(),
                "{status:?} should be treated as terminal"
            );
        }
    }

    #[test]
    fn in_flight_statuses_do_not_wake_internal_coordination() {
        // Waking on these would spuriously re-run dispatch/supervision and could
        // resume a parent while its child is still working.
        for status in [
            TaskStatus::Queued,
            TaskStatus::InProgress,
            TaskStatus::AwaitingApproval,
            TaskStatus::Blocked,
        ] {
            assert!(
                !status.is_terminal(),
                "{status:?} must not be treated as terminal"
            );
        }
    }

    #[tokio::test]
    async fn subscribers_receive_emitted_events() {
        let tx = new_internal_event_channel();
        let mut rx = tx.subscribe();

        tx.send(InternalAgentEvent::TaskTerminal {
            task_id: "child-1".to_string(),
            parent_task_id: Some("parent-1".to_string()),
            status: TaskStatus::Completed,
        })
        .expect("a live subscriber should accept the event");

        match rx.recv().await.expect("event delivered") {
            InternalAgentEvent::TaskTerminal {
                task_id,
                parent_task_id,
                status,
            } => {
                assert_eq!(task_id, "child-1");
                assert_eq!(parent_task_id.as_deref(), Some("parent-1"));
                assert_eq!(status, TaskStatus::Completed);
            }
        }
    }
}
