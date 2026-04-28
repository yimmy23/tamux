#![allow(dead_code)]

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollaborationEscalationVm {
    pub from_level: String,
    pub to_level: String,
    pub reason: String,
    pub attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollaborationDisagreementVm {
    pub id: String,
    pub topic: String,
    pub positions: Vec<String>,
    pub vote_count: usize,
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollaborationSessionVm {
    pub id: String,
    pub parent_task_id: Option<String>,
    pub parent_thread_id: Option<String>,
    pub agent_count: usize,
    pub disagreement_count: usize,
    pub consensus_summary: Option<String>,
    pub escalation: Option<CollaborationEscalationVm>,
    pub disagreements: Vec<CollaborationDisagreementVm>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollaborationRowVm {
    Session {
        session_id: String,
    },
    Disagreement {
        session_id: String,
        disagreement_id: String,
    },
}

impl CollaborationRowVm {
    pub fn session_id(&self) -> &str {
        match self {
            CollaborationRowVm::Session { session_id }
            | CollaborationRowVm::Disagreement { session_id, .. } => session_id,
        }
    }

    pub fn disagreement_id(&self) -> Option<&str> {
        match self {
            CollaborationRowVm::Session { .. } => None,
            CollaborationRowVm::Disagreement {
                disagreement_id, ..
            } => Some(disagreement_id.as_str()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollaborationPaneFocus {
    Navigator,
    Detail,
}

#[derive(Debug, Clone)]
pub enum CollaborationAction {
    SessionsLoaded(Vec<CollaborationSessionVm>),
    SelectRow(usize),
    StepDetailAction(i32),
    SetFocus(CollaborationPaneFocus),
}

pub struct CollaborationState {
    sessions: Vec<CollaborationSessionVm>,
    rows: Vec<CollaborationRowVm>,
    selected_row_index: usize,
    selected_detail_action_index: usize,
    focus: CollaborationPaneFocus,
}

impl CollaborationState {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            rows: Vec::new(),
            selected_row_index: 0,
            selected_detail_action_index: 0,
            focus: CollaborationPaneFocus::Navigator,
        }
    }

    pub fn rows(&self) -> &[CollaborationRowVm] {
        &self.rows
    }

    pub fn selected_row_index(&self) -> usize {
        self.selected_row_index
    }

    pub fn selected_row(&self) -> Option<&CollaborationRowVm> {
        self.rows.get(self.selected_row_index)
    }

    pub fn selected_detail_action_index(&self) -> usize {
        self.selected_detail_action_index
    }

    pub fn focus(&self) -> CollaborationPaneFocus {
        self.focus
    }

    pub fn selected_session(&self) -> Option<&CollaborationSessionVm> {
        let session_id = self.selected_row()?.session_id();
        self.sessions
            .iter()
            .find(|session| session.id == session_id)
    }

    pub fn selected_disagreement(&self) -> Option<&CollaborationDisagreementVm> {
        let disagreement_id = self.selected_row()?.disagreement_id()?;
        self.sessions
            .iter()
            .flat_map(|session| session.disagreements.iter())
            .find(|disagreement| disagreement.id == disagreement_id)
    }

    pub fn selected_position(&self) -> Option<&str> {
        let disagreement = self.selected_disagreement()?;
        disagreement
            .positions
            .get(self.selected_detail_action_index)
            .map(String::as_str)
    }

    pub fn reduce(&mut self, action: CollaborationAction) {
        match action {
            CollaborationAction::SessionsLoaded(sessions) => {
                let selected_session_id =
                    self.selected_row().map(|row| row.session_id().to_string());
                let selected_disagreement_id = self
                    .selected_row()
                    .and_then(CollaborationRowVm::disagreement_id)
                    .map(ToOwned::to_owned);

                self.sessions = sessions;
                self.rows = flatten_rows(&self.sessions);
                self.selected_row_index = self
                    .rows
                    .iter()
                    .position(|row| {
                        if let Some(disagreement_id) = selected_disagreement_id.as_deref() {
                            row.disagreement_id() == Some(disagreement_id)
                        } else if let Some(session_id) = selected_session_id.as_deref() {
                            row.session_id() == session_id && row.disagreement_id().is_none()
                        } else {
                            false
                        }
                    })
                    .unwrap_or_else(|| {
                        self.selected_row_index
                            .min(self.rows.len().saturating_sub(1))
                    });
                self.clamp_detail_action_index();
            }
            CollaborationAction::SelectRow(index) => {
                if self.rows.is_empty() {
                    self.selected_row_index = 0;
                } else {
                    self.selected_row_index = index.min(self.rows.len().saturating_sub(1));
                }
                self.selected_detail_action_index = 0;
            }
            CollaborationAction::StepDetailAction(delta) => {
                let max_index = self
                    .selected_disagreement()
                    .map(|disagreement| disagreement.positions.len().saturating_sub(1))
                    .unwrap_or(0) as i32;
                let next = (self.selected_detail_action_index as i32 + delta).clamp(0, max_index);
                self.selected_detail_action_index = next as usize;
            }
            CollaborationAction::SetFocus(focus) => {
                self.focus = focus;
            }
        }
    }

    fn clamp_detail_action_index(&mut self) {
        let max_index = self
            .selected_disagreement()
            .map(|disagreement| disagreement.positions.len().saturating_sub(1))
            .unwrap_or(0);
        self.selected_detail_action_index = self.selected_detail_action_index.min(max_index);
    }
}

fn flatten_rows(sessions: &[CollaborationSessionVm]) -> Vec<CollaborationRowVm> {
    let mut rows = Vec::new();
    for session in sessions {
        rows.push(CollaborationRowVm::Session {
            session_id: session.id.clone(),
        });
        for disagreement in &session.disagreements {
            rows.push(CollaborationRowVm::Disagreement {
                session_id: session.id.clone(),
                disagreement_id: disagreement.id.clone(),
            });
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_sessions() -> Vec<CollaborationSessionVm> {
        vec![
            CollaborationSessionVm {
                id: "session-1".to_string(),
                parent_task_id: Some("task-1".to_string()),
                parent_thread_id: None,
                agent_count: 2,
                disagreement_count: 2,
                consensus_summary: None,
                escalation: Some(CollaborationEscalationVm {
                    from_level: "L1".to_string(),
                    to_level: "L2".to_string(),
                    reason: "needs operator vote".to_string(),
                    attempts: 1,
                }),
                disagreements: vec![
                    CollaborationDisagreementVm {
                        id: "disagreement-1".to_string(),
                        topic: "deployment strategy".to_string(),
                        positions: vec!["roll forward".to_string(), "roll back".to_string()],
                        vote_count: 1,
                        resolution: None,
                    },
                    CollaborationDisagreementVm {
                        id: "disagreement-2".to_string(),
                        topic: "test scope".to_string(),
                        positions: vec!["focused".to_string(), "broad".to_string()],
                        vote_count: 0,
                        resolution: Some("focused".to_string()),
                    },
                ],
            },
            CollaborationSessionVm {
                id: "session-2".to_string(),
                parent_task_id: Some("task-2".to_string()),
                parent_thread_id: None,
                agent_count: 3,
                disagreement_count: 1,
                consensus_summary: Some("all agents aligned after review".to_string()),
                escalation: None,
                disagreements: vec![CollaborationDisagreementVm {
                    id: "disagreement-3".to_string(),
                    topic: "release timing".to_string(),
                    positions: vec!["ship now".to_string(), "delay".to_string()],
                    vote_count: 2,
                    resolution: None,
                }],
            },
        ]
    }

    #[test]
    fn collaboration_sessions_flatten_into_selectable_rows() {
        let mut state = CollaborationState::new();

        state.reduce(CollaborationAction::SessionsLoaded(sample_sessions()));

        let rows = state.rows();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].session_id(), "session-1");
        assert_eq!(rows[1].disagreement_id(), Some("disagreement-1"));
        assert_eq!(rows[2].disagreement_id(), Some("disagreement-2"));
        assert_eq!(rows[3].session_id(), "session-2");
        assert_eq!(rows[4].disagreement_id(), Some("disagreement-3"));
    }

    #[test]
    fn collaboration_reload_preserves_selected_disagreement_when_present() {
        let mut state = CollaborationState::new();
        state.reduce(CollaborationAction::SessionsLoaded(sample_sessions()));
        state.reduce(CollaborationAction::SelectRow(1));

        let mut refreshed = sample_sessions();
        refreshed[0].disagreements[0].vote_count = 3;
        state.reduce(CollaborationAction::SessionsLoaded(refreshed));

        assert_eq!(state.selected_row_index(), 1);
        assert_eq!(
            state
                .selected_row()
                .and_then(CollaborationRowVm::disagreement_id),
            Some("disagreement-1")
        );
    }

    #[test]
    fn collaboration_detail_action_selection_steps_and_clamps() {
        let mut state = CollaborationState::new();
        state.reduce(CollaborationAction::SessionsLoaded(sample_sessions()));
        state.reduce(CollaborationAction::SelectRow(1));

        assert_eq!(state.selected_detail_action_index(), 0);

        state.reduce(CollaborationAction::StepDetailAction(1));
        assert_eq!(state.selected_detail_action_index(), 1);

        state.reduce(CollaborationAction::StepDetailAction(10));
        assert_eq!(state.selected_detail_action_index(), 1);

        state.reduce(CollaborationAction::StepDetailAction(-10));
        assert_eq!(state.selected_detail_action_index(), 0);
    }
}
