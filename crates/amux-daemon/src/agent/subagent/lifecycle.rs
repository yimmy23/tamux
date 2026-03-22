//! Sub-agent lifecycle state machine — valid transitions and tracking.

use serde::{Deserialize, Serialize};

/// Possible states for a sub-agent during its lifetime.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SubagentLifecycleState {
    /// Waiting to be dispatched.
    Queued,
    /// Setting up context/tools.
    Initializing,
    /// Actively executing.
    Running,
    /// Temporarily paused (user request).
    Paused,
    /// Blocked on approval or user input.
    WaitingForInput,
    /// Finished successfully.
    Completed,
    /// Finished with error.
    Failed,
    /// Manually cancelled.
    Cancelled,
}

impl SubagentLifecycleState {
    /// Returns the set of states reachable from `self` via a single valid
    /// transition, or an empty slice for terminal states.
    fn valid_targets(self) -> &'static [SubagentLifecycleState] {
        use SubagentLifecycleState::*;
        match self {
            Queued => &[Initializing, Cancelled],
            Initializing => &[Running, Failed, Cancelled],
            Running => &[Paused, WaitingForInput, Completed, Failed, Cancelled],
            Paused => &[Running, Cancelled],
            WaitingForInput => &[Running, Cancelled],
            // Terminal states — no outgoing transitions.
            Completed | Failed | Cancelled => &[],
        }
    }

    /// Whether this state is terminal (no further transitions allowed).
    fn is_terminal(self) -> bool {
        matches!(
            self,
            SubagentLifecycleState::Completed
                | SubagentLifecycleState::Failed
                | SubagentLifecycleState::Cancelled
        )
    }
}

/// Record of a single state transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleTransition {
    pub from: SubagentLifecycleState,
    pub to: SubagentLifecycleState,
    pub timestamp: u64,
    pub reason: Option<String>,
}

/// Tracks the current state and full transition history of a sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentLifecycle {
    state: SubagentLifecycleState,
    transitions: Vec<LifecycleTransition>,
    created_at: u64,
}

impl SubagentLifecycle {
    /// Create a new lifecycle starting in the `Queued` state.
    pub fn new(now: u64) -> Self {
        Self {
            state: SubagentLifecycleState::Queued,
            transitions: Vec::new(),
            created_at: now,
        }
    }

    /// The current lifecycle state.
    pub fn state(&self) -> SubagentLifecycleState {
        self.state
    }

    /// Immutable access to the full transition history.
    pub fn transitions(&self) -> &[LifecycleTransition] {
        &self.transitions
    }

    /// Validate and apply a state transition.
    ///
    /// Returns `Err` with a human-readable message when the requested
    /// transition is not allowed by the state machine rules.
    pub fn transition(
        &mut self,
        to: SubagentLifecycleState,
        now: u64,
        reason: Option<String>,
    ) -> Result<(), String> {
        let valid = self.state.valid_targets();
        if !valid.contains(&to) {
            return Err(format!("invalid transition: {:?} -> {:?}", self.state, to));
        }

        let from = self.state;
        self.state = to;
        self.transitions.push(LifecycleTransition {
            from,
            to,
            timestamp: now,
            reason,
        });

        Ok(())
    }

    /// Whether the lifecycle has reached a terminal state
    /// (`Completed`, `Failed`, or `Cancelled`).
    pub fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }

    /// Milliseconds elapsed since the lifecycle was created.
    pub fn elapsed_ms(&self, now: u64) -> u64 {
        now.saturating_sub(self.created_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_lifecycle_starts_in_queued() {
        let lc = SubagentLifecycle::new(100);
        assert_eq!(lc.state(), SubagentLifecycleState::Queued);
        assert!(lc.transitions().is_empty());
    }

    #[test]
    fn valid_full_path_queued_to_completed() {
        let mut lc = SubagentLifecycle::new(0);
        lc.transition(SubagentLifecycleState::Initializing, 1, None)
            .unwrap();
        lc.transition(SubagentLifecycleState::Running, 2, None)
            .unwrap();
        lc.transition(SubagentLifecycleState::Completed, 3, None)
            .unwrap();
        assert_eq!(lc.state(), SubagentLifecycleState::Completed);
        assert_eq!(lc.transitions().len(), 3);
    }

    #[test]
    fn invalid_transition_queued_to_running() {
        let mut lc = SubagentLifecycle::new(0);
        let err = lc
            .transition(SubagentLifecycleState::Running, 1, None)
            .unwrap_err();
        assert!(err.contains("invalid transition"));
        // State must remain unchanged after a rejected transition.
        assert_eq!(lc.state(), SubagentLifecycleState::Queued);
    }

    #[test]
    fn invalid_transition_from_terminal_completed() {
        let mut lc = SubagentLifecycle::new(0);
        lc.transition(SubagentLifecycleState::Initializing, 1, None)
            .unwrap();
        lc.transition(SubagentLifecycleState::Running, 2, None)
            .unwrap();
        lc.transition(SubagentLifecycleState::Completed, 3, None)
            .unwrap();

        let err = lc
            .transition(SubagentLifecycleState::Running, 4, None)
            .unwrap_err();
        assert!(err.contains("invalid transition"));
        assert_eq!(lc.state(), SubagentLifecycleState::Completed);
    }

    #[test]
    fn invalid_transition_from_terminal_failed() {
        let mut lc = SubagentLifecycle::new(0);
        lc.transition(SubagentLifecycleState::Initializing, 1, None)
            .unwrap();
        lc.transition(SubagentLifecycleState::Failed, 2, Some("oops".into()))
            .unwrap();

        let err = lc
            .transition(SubagentLifecycleState::Running, 3, None)
            .unwrap_err();
        assert!(err.contains("invalid transition"));
        assert_eq!(lc.state(), SubagentLifecycleState::Failed);
    }

    #[test]
    fn terminal_state_detection() {
        let cases = [
            (SubagentLifecycleState::Queued, false),
            (SubagentLifecycleState::Initializing, false),
            (SubagentLifecycleState::Running, false),
            (SubagentLifecycleState::Paused, false),
            (SubagentLifecycleState::WaitingForInput, false),
            (SubagentLifecycleState::Completed, true),
            (SubagentLifecycleState::Failed, true),
            (SubagentLifecycleState::Cancelled, true),
        ];

        for (state, expected) in cases {
            assert_eq!(
                state.is_terminal(),
                expected,
                "{:?} terminal mismatch",
                state
            );
        }
    }

    #[test]
    fn transition_records_are_preserved() {
        let mut lc = SubagentLifecycle::new(0);
        lc.transition(
            SubagentLifecycleState::Initializing,
            10,
            Some("boot".into()),
        )
        .unwrap();
        lc.transition(SubagentLifecycleState::Running, 20, None)
            .unwrap();

        let t = lc.transitions();
        assert_eq!(t.len(), 2);

        assert_eq!(t[0].from, SubagentLifecycleState::Queued);
        assert_eq!(t[0].to, SubagentLifecycleState::Initializing);
        assert_eq!(t[0].timestamp, 10);
        assert_eq!(t[0].reason.as_deref(), Some("boot"));

        assert_eq!(t[1].from, SubagentLifecycleState::Initializing);
        assert_eq!(t[1].to, SubagentLifecycleState::Running);
        assert_eq!(t[1].timestamp, 20);
        assert!(t[1].reason.is_none());
    }

    #[test]
    fn elapsed_time_calculation() {
        let lc = SubagentLifecycle::new(100);
        assert_eq!(lc.elapsed_ms(100), 0);
        assert_eq!(lc.elapsed_ms(350), 250);
        // Underflow protection via saturating_sub.
        assert_eq!(lc.elapsed_ms(50), 0);
    }

    #[test]
    fn all_valid_transitions_from_running() {
        let targets = [
            SubagentLifecycleState::Paused,
            SubagentLifecycleState::WaitingForInput,
            SubagentLifecycleState::Completed,
            SubagentLifecycleState::Failed,
            SubagentLifecycleState::Cancelled,
        ];

        for target in targets {
            let mut lc = SubagentLifecycle::new(0);
            lc.transition(SubagentLifecycleState::Initializing, 1, None)
                .unwrap();
            lc.transition(SubagentLifecycleState::Running, 2, None)
                .unwrap();
            lc.transition(target, 3, None).unwrap();
            assert_eq!(lc.state(), target);
        }
    }

    #[test]
    fn cancelled_from_every_non_terminal_state() {
        // Queued -> Cancelled
        let mut lc = SubagentLifecycle::new(0);
        lc.transition(SubagentLifecycleState::Cancelled, 1, None)
            .unwrap();
        assert!(lc.is_terminal());

        // Initializing -> Cancelled
        let mut lc = SubagentLifecycle::new(0);
        lc.transition(SubagentLifecycleState::Initializing, 1, None)
            .unwrap();
        lc.transition(SubagentLifecycleState::Cancelled, 2, None)
            .unwrap();
        assert!(lc.is_terminal());

        // Running -> Cancelled
        let mut lc = SubagentLifecycle::new(0);
        lc.transition(SubagentLifecycleState::Initializing, 1, None)
            .unwrap();
        lc.transition(SubagentLifecycleState::Running, 2, None)
            .unwrap();
        lc.transition(SubagentLifecycleState::Cancelled, 3, None)
            .unwrap();
        assert!(lc.is_terminal());

        // Paused -> Cancelled
        let mut lc = SubagentLifecycle::new(0);
        lc.transition(SubagentLifecycleState::Initializing, 1, None)
            .unwrap();
        lc.transition(SubagentLifecycleState::Running, 2, None)
            .unwrap();
        lc.transition(SubagentLifecycleState::Paused, 3, None)
            .unwrap();
        lc.transition(SubagentLifecycleState::Cancelled, 4, None)
            .unwrap();
        assert!(lc.is_terminal());

        // WaitingForInput -> Cancelled
        let mut lc = SubagentLifecycle::new(0);
        lc.transition(SubagentLifecycleState::Initializing, 1, None)
            .unwrap();
        lc.transition(SubagentLifecycleState::Running, 2, None)
            .unwrap();
        lc.transition(SubagentLifecycleState::WaitingForInput, 3, None)
            .unwrap();
        lc.transition(SubagentLifecycleState::Cancelled, 4, None)
            .unwrap();
        assert!(lc.is_terminal());
    }

    #[test]
    fn full_lifecycle_path_with_reasons() {
        let mut lc = SubagentLifecycle::new(0);

        lc.transition(
            SubagentLifecycleState::Initializing,
            10,
            Some("dispatcher picked up".into()),
        )
        .unwrap();
        lc.transition(
            SubagentLifecycleState::Running,
            20,
            Some("context loaded".into()),
        )
        .unwrap();
        lc.transition(
            SubagentLifecycleState::WaitingForInput,
            30,
            Some("needs user approval".into()),
        )
        .unwrap();
        lc.transition(
            SubagentLifecycleState::Running,
            50,
            Some("user approved".into()),
        )
        .unwrap();
        lc.transition(
            SubagentLifecycleState::Paused,
            60,
            Some("user paused".into()),
        )
        .unwrap();
        lc.transition(
            SubagentLifecycleState::Running,
            80,
            Some("user resumed".into()),
        )
        .unwrap();
        lc.transition(
            SubagentLifecycleState::Completed,
            100,
            Some("task done".into()),
        )
        .unwrap();

        assert!(lc.is_terminal());
        assert_eq!(lc.elapsed_ms(100), 100);
        assert_eq!(lc.transitions().len(), 7);

        // Verify every reason was recorded.
        let reasons: Vec<&str> = lc
            .transitions()
            .iter()
            .map(|t| t.reason.as_deref().unwrap())
            .collect();
        assert_eq!(
            reasons,
            vec![
                "dispatcher picked up",
                "context loaded",
                "needs user approval",
                "user approved",
                "user paused",
                "user resumed",
                "task done",
            ]
        );
    }

    #[test]
    fn pause_and_resume_round_trip() {
        let mut lc = SubagentLifecycle::new(0);
        lc.transition(SubagentLifecycleState::Initializing, 1, None)
            .unwrap();
        lc.transition(SubagentLifecycleState::Running, 2, None)
            .unwrap();
        lc.transition(SubagentLifecycleState::Paused, 3, None)
            .unwrap();
        assert_eq!(lc.state(), SubagentLifecycleState::Paused);
        lc.transition(SubagentLifecycleState::Running, 4, None)
            .unwrap();
        assert_eq!(lc.state(), SubagentLifecycleState::Running);
    }

    #[test]
    fn waiting_for_input_and_resume() {
        let mut lc = SubagentLifecycle::new(0);
        lc.transition(SubagentLifecycleState::Initializing, 1, None)
            .unwrap();
        lc.transition(SubagentLifecycleState::Running, 2, None)
            .unwrap();
        lc.transition(SubagentLifecycleState::WaitingForInput, 3, None)
            .unwrap();
        assert_eq!(lc.state(), SubagentLifecycleState::WaitingForInput);
        assert!(!lc.is_terminal());
        lc.transition(SubagentLifecycleState::Running, 4, None)
            .unwrap();
        assert_eq!(lc.state(), SubagentLifecycleState::Running);
    }
}
