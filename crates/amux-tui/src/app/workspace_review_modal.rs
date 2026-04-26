use amux_protocol::{WorkspaceReviewSubmission, WorkspaceReviewVerdict, WorkspaceTaskStatus};
use crossterm::event::{KeyCode, KeyModifiers};

use crate::state::{modal, DaemonCommand};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkspaceReviewField {
    Verdict,
    Message,
    Submit,
    Cancel,
}

impl WorkspaceReviewField {
    const ALL: [Self; 4] = [Self::Verdict, Self::Message, Self::Submit, Self::Cancel];

    fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    fn previous(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0);
        Self::ALL[if index == 0 {
            Self::ALL.len() - 1
        } else {
            index - 1
        }]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorkspaceReviewForm {
    pub(super) task_id: String,
    pub(super) verdict: WorkspaceReviewVerdict,
    pub(super) message: String,
    pub(super) field: WorkspaceReviewField,
}

impl WorkspaceReviewForm {
    pub(super) fn new(task_id: String) -> Self {
        Self {
            task_id,
            verdict: WorkspaceReviewVerdict::Pass,
            message: String::new(),
            field: WorkspaceReviewField::Verdict,
        }
    }

    pub(super) fn toggle_verdict(&mut self) {
        self.verdict = match self.verdict {
            WorkspaceReviewVerdict::Pass => WorkspaceReviewVerdict::Fail,
            WorkspaceReviewVerdict::Fail => WorkspaceReviewVerdict::Pass,
        };
    }

    pub(super) fn to_submission(&self) -> WorkspaceReviewSubmission {
        WorkspaceReviewSubmission {
            task_id: self.task_id.clone(),
            verdict: self.verdict.clone(),
            message: self
                .message
                .trim()
                .is_empty()
                .then_some(None)
                .unwrap_or_else(|| Some(self.message.trim().to_string())),
        }
    }

    fn next_field(&mut self) {
        self.field = self.field.next();
    }

    fn previous_field(&mut self) {
        self.field = self.field.previous();
    }

    fn insert_char(&mut self, ch: char) {
        if self.field == WorkspaceReviewField::Message {
            self.message.push(ch);
        }
    }

    fn backspace(&mut self) {
        if self.field == WorkspaceReviewField::Message {
            self.message.pop();
        }
    }
}

pub(super) fn workspace_review_action_opens_modal(status: &WorkspaceTaskStatus) -> bool {
    *status == WorkspaceTaskStatus::InReview
}

pub(super) fn workspace_review_modal_body(form: &WorkspaceReviewForm) -> String {
    let verdict = match form.verdict {
        WorkspaceReviewVerdict::Pass => "pass",
        WorkspaceReviewVerdict::Fail => "fail",
    };
    let rows = [
        (WorkspaceReviewField::Verdict, format!("Verdict: {verdict}")),
        (
            WorkspaceReviewField::Message,
            format!("Message: {}", form.message),
        ),
        (WorkspaceReviewField::Submit, "Submit".to_string()),
        (WorkspaceReviewField::Cancel, "Cancel".to_string()),
    ];
    let mut body = format!(
        "Task: {}\n\n",
        form.task_id.chars().take(12).collect::<String>()
    );
    for (field, text) in rows {
        let marker = if field == form.field { ">" } else { " " };
        body.push_str(&format!("{marker} {text}\n"));
    }
    body.push_str("\nEnter toggle/submit - Tab navigate - Esc cancel");
    body
}

impl super::TuiModel {
    pub(super) fn open_workspace_review_modal(&mut self, task_id: String) {
        self.pending_workspace_review_form = Some(WorkspaceReviewForm::new(task_id));
        self.modal.reduce(modal::ModalAction::Push(
            modal::ModalKind::WorkspaceReviewTask,
        ));
        self.status_line = "Review workspace task".to_string();
    }

    pub(super) fn submit_workspace_review_modal(&mut self) {
        let Some(form) = self.pending_workspace_review_form.clone() else {
            self.close_top_modal();
            return;
        };
        let submission = form.to_submission();
        self.close_top_modal();
        self.send_daemon_command(DaemonCommand::SubmitWorkspaceReview(submission));
        self.main_pane_view = super::MainPaneView::Workspace;
        self.status_line = "Submitting workspace review...".to_string();
    }

    pub(super) fn workspace_review_modal_body(&self) -> String {
        self.pending_workspace_review_form
            .as_ref()
            .map(workspace_review_modal_body)
            .unwrap_or_else(|| "No workspace review selected".to_string())
    }

    pub(super) fn handle_workspace_review_modal_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        match code {
            KeyCode::Esc => self.close_top_modal(),
            KeyCode::Tab | KeyCode::Down => {
                if let Some(form) = self.pending_workspace_review_form.as_mut() {
                    form.next_field();
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                if let Some(form) = self.pending_workspace_review_form.as_mut() {
                    form.previous_field();
                }
            }
            KeyCode::Backspace => {
                if let Some(form) = self.pending_workspace_review_form.as_mut() {
                    form.backspace();
                }
            }
            KeyCode::Enter => {
                let field = self
                    .pending_workspace_review_form
                    .as_ref()
                    .map(|form| form.field);
                match field {
                    Some(WorkspaceReviewField::Verdict) => {
                        if let Some(form) = self.pending_workspace_review_form.as_mut() {
                            form.toggle_verdict();
                        }
                    }
                    Some(WorkspaceReviewField::Submit) => self.submit_workspace_review_modal(),
                    Some(WorkspaceReviewField::Cancel) => self.close_top_modal(),
                    Some(WorkspaceReviewField::Message) | None => {}
                }
            }
            KeyCode::Char(ch)
                if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                if let Some(form) = self.pending_workspace_review_form.as_mut() {
                    form.insert_char(ch);
                }
            }
            _ => {}
        }
        false
    }
}
