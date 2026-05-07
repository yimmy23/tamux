    use super::*;
    use crate::state::approval::ApprovalFilter;
    use crate::state::{ApprovalAction, ApprovalState, PendingApproval, RiskLevel};
    use crate::theme::ThemeTokens;
    use ratatui::backend::TestBackend;
    use ratatui::layout::{Position, Rect};
    use ratatui::Terminal;

    fn make_approval(
        approval_id: &str,
        task_title: &str,
        thread_id: &str,
        workspace_id: &str,
    ) -> PendingApproval {
        PendingApproval {
            approval_id: approval_id.into(),
            task_id: format!("task-{approval_id}"),
            task_title: Some(task_title.into()),
            thread_id: Some(thread_id.into()),
            thread_title: Some(format!("Thread {thread_id}")),
            workspace_id: Some(workspace_id.into()),
            rationale: Some("Needed to continue execution".into()),
            reasons: vec!["network access requested".into()],
            command: "git clone https://example.com/repo.git".into(),
            risk_level: RiskLevel::High,
            blast_radius: "workspace".into(),
            received_at: 1,
            seen_at: None,
        }
    }

    #[test]
    fn approval_center_renders_without_panicking() {
        let mut approvals = ApprovalState::new();
        approvals.reduce(ApprovalAction::ApprovalRequired(make_approval(
            "a1", "WELES", "thread-1", "ws-1",
        )));
        approvals.reduce(ApprovalAction::SetFilter(ApprovalFilter::AllPending));

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");

        terminal
            .draw(|frame| {
                render(
                    frame,
                    Rect::new(0, 0, 100, 30),
                    &approvals,
                    Some("thread-1"),
                    Some("ws-1"),
                    &ThemeTokens::default(),
                )
            })
            .expect("approval center render should succeed");
    }

    #[test]
    fn hit_test_targets_queue_row() {
        let mut approvals = ApprovalState::new();
        approvals.reduce(ApprovalAction::ApprovalRequired(make_approval(
            "a1", "WELES", "thread-1", "ws-1",
        )));

        let hit = hit_test(
            Rect::new(0, 0, 100, 30),
            &approvals,
            Some("thread-1"),
            Some("ws-1"),
            Position::new(3, 5),
        );

        assert_eq!(hit, Some(ApprovalCenterHitTarget::Row(0)));
    }

    #[test]
    fn hit_test_targets_filter_after_double_digit_pending_prefix() {
        let mut approvals = ApprovalState::new();
        for index in 0..10 {
            approvals.reduce(ApprovalAction::ApprovalRequired(make_approval(
                &format!("a{index}"),
                "WELES",
                "thread-1",
                "ws-1",
            )));
        }

        let prefix_width = format!("{} pending  ", approvals.pending_approvals().len())
            .chars()
            .count() as u16;
        let hit = hit_test(
            Rect::new(0, 0, 100, 30),
            &approvals,
            Some("thread-1"),
            Some("ws-1"),
            Position::new(1 + prefix_width + 1, 1),
        );

        assert_eq!(
            hit,
            Some(ApprovalCenterHitTarget::Filter(ApprovalFilter::AllPending))
        );
    }

    #[test]
    fn hit_test_targets_approve_action() {
        let mut approvals = ApprovalState::new();
        approvals.reduce(ApprovalAction::ApprovalRequired(make_approval(
            "a1", "WELES", "thread-1", "ws-1",
        )));

        let hit = hit_test(
            Rect::new(0, 0, 100, 30),
            &approvals,
            Some("thread-1"),
            Some("ws-1"),
            Position::new(64, 26),
        );

        assert_eq!(
            hit,
            Some(ApprovalCenterHitTarget::ApproveOnce("a1".to_string()))
        );
    }

    #[test]
    fn approval_center_wraps_long_detail_text() {
        let mut approvals = ApprovalState::new();
        let mut approval = make_approval("a1", "WELES", "thread-1", "ws-1");
        approval.rationale = Some(
            "Cloning scientific skills repository from GitHub as part of WELES governance review task TAILTOKEN"
                .to_string(),
        );
        approval.command = "git clone https://github.com/example/scientific-skills.git".to_string();
        approvals.reduce(ApprovalAction::ApprovalRequired(approval));

        let backend = TestBackend::new(70, 20);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");

        terminal
            .draw(|frame| {
                render(
                    frame,
                    Rect::new(0, 0, 70, 20),
                    &approvals,
                    Some("thread-1"),
                    Some("ws-1"),
                    &ThemeTokens::default(),
                )
            })
            .expect("approval center render should succeed");

        let buffer = terminal.backend().buffer();
        let rendered = (0..20)
            .map(|y| {
                (0..70)
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            rendered.contains("TAILTOKEN"),
            "long approval rationale should wrap instead of truncating: {rendered}"
        );
    }
