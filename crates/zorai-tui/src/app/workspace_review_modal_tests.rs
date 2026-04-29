use super::workspace_review_modal::*;
use crate::state::{modal, DaemonCommand};
use tokio::sync::mpsc::unbounded_channel;
use zorai_protocol::{WorkspaceReviewVerdict, WorkspaceTaskStatus};

#[test]
fn review_form_defaults_to_pass_and_builds_submission() {
    let mut form = WorkspaceReviewForm::new("task-1".to_string());
    form.message = "Looks good".to_string();

    let submission = form.to_submission();

    assert_eq!(submission.task_id, "task-1");
    assert_eq!(submission.verdict, WorkspaceReviewVerdict::Pass);
    assert_eq!(submission.message, Some("Looks good".to_string()));
}

#[test]
fn review_form_can_toggle_fail_and_trim_empty_message() {
    let mut form = WorkspaceReviewForm::new("task-2".to_string());

    form.toggle_verdict();
    form.message = "   ".to_string();
    let submission = form.to_submission();

    assert_eq!(submission.verdict, WorkspaceReviewVerdict::Fail);
    assert_eq!(submission.message, None);
}

#[test]
fn model_submits_review_modal_as_daemon_command() {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, mut daemon_rx) = unbounded_channel();
    let mut model = crate::app::TuiModel::new(event_rx, daemon_tx);

    model.open_workspace_review_modal("task-1".to_string());
    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::WorkspaceReviewTask)
    );

    let form = model.pending_workspace_review_form.as_mut().expect("form");
    form.toggle_verdict();
    form.message = "Need tests".to_string();

    model.submit_workspace_review_modal();

    match daemon_rx.try_recv().expect("review command") {
        DaemonCommand::SubmitWorkspaceReview(review) => {
            assert_eq!(review.task_id, "task-1");
            assert_eq!(review.verdict, WorkspaceReviewVerdict::Fail);
            assert_eq!(review.message, Some("Need tests".to_string()));
        }
        other => panic!("unexpected command: {other:?}"),
    }
    assert_eq!(model.modal.top(), None);
}

#[test]
fn review_action_opens_modal_only_for_in_review_tasks() {
    assert!(workspace_review_action_opens_modal(
        &WorkspaceTaskStatus::InReview
    ));
    assert!(!workspace_review_action_opens_modal(
        &WorkspaceTaskStatus::Todo
    ));
    assert!(!workspace_review_action_opens_modal(
        &WorkspaceTaskStatus::InProgress
    ));
    assert!(!workspace_review_action_opens_modal(
        &WorkspaceTaskStatus::Done
    ));
}
