    use super::super::*;

    fn make_approval(approval_id: &str, task_id: &str, command: &str) -> PendingApproval {
        let risk_level = RiskLevel::classify_command(command);
        PendingApproval {
            approval_id: approval_id.into(),
            task_id: task_id.into(),
            task_title: None,
            thread_id: None,
            thread_title: None,
            workspace_id: None,
            rationale: None,
            reasons: Vec::new(),
            command: command.into(),
            risk_level,
            blast_radius: "unknown".into(),
            received_at: 0,
            seen_at: None,
        }
    }

    // ── ApprovalState tests ───────────────────────────────────────────────────

    #[test]
    fn approval_required_adds_to_pending() {
        let mut state = ApprovalState::new();
        assert!(state.pending_approvals().is_empty());
        assert!(state.current_approval().is_none());

        state.reduce(ApprovalAction::ApprovalRequired(make_approval(
            "a1", "t1", "ls -la",
        )));
        assert_eq!(state.pending_approvals().len(), 1);
        assert_eq!(state.current_approval().unwrap().approval_id, "a1");
    }

    #[test]
    fn resolve_removes_matching_approval() {
        let mut state = ApprovalState::new();
        state.reduce(ApprovalAction::ApprovalRequired(make_approval(
            "a1", "t1", "ls",
        )));
        state.reduce(ApprovalAction::ApprovalRequired(make_approval(
            "a2", "t2", "pwd",
        )));
        assert_eq!(state.pending_approvals().len(), 2);

        state.reduce(ApprovalAction::Resolve {
            approval_id: "a1".into(),
            decision: "approve".into(),
        });
        assert_eq!(state.pending_approvals().len(), 1);
        assert_eq!(state.pending_approvals()[0].approval_id, "a2");
    }

    #[test]
    fn clear_resolved_removes_by_id() {
        let mut state = ApprovalState::new();
        state.reduce(ApprovalAction::ApprovalRequired(make_approval(
            "a1", "t1", "ls",
        )));
        state.reduce(ApprovalAction::ClearResolved("a1".into()));
        assert!(state.pending_approvals().is_empty());
    }

    #[test]
    fn current_approval_is_first_pending() {
        let mut state = ApprovalState::new();
        state.reduce(ApprovalAction::ApprovalRequired(make_approval(
            "a1",
            "t1",
            "echo hello",
        )));
        state.reduce(ApprovalAction::ApprovalRequired(make_approval(
            "a2",
            "t2",
            "echo world",
        )));
        assert_eq!(state.current_approval().unwrap().approval_id, "a1");
    }

    #[test]
    fn set_rules_replaces_saved_rules() {
        let mut state = ApprovalState::new();
        state.reduce(ApprovalAction::SetRules(vec![SavedApprovalRule {
            id: "rule-1".into(),
            command: "git push".into(),
            created_at: 1,
            last_used_at: Some(2),
            use_count: 3,
        }]));
        assert_eq!(state.saved_rules().len(), 1);
        assert_eq!(
            state.selected_rule().map(|rule| rule.command.as_str()),
            Some("git push")
        );
    }

    #[test]
    fn approval_required_upserts_existing_approval_by_id() {
        let mut state = ApprovalState::new();
        let mut original = make_approval("a1", "t1", "ls -la");
        original.task_title = Some("Original".into());
        let mut updated = make_approval("a1", "t1", "git push --force");
        updated.task_title = Some("Updated".into());
        updated.blast_radius = "repo".into();

        state.reduce(ApprovalAction::ApprovalRequired(original));
        state.reduce(ApprovalAction::ApprovalRequired(updated));

        assert_eq!(state.pending_approvals().len(), 1);
        let approval = &state.pending_approvals()[0];
        assert_eq!(approval.task_title.as_deref(), Some("Updated"));
        assert_eq!(approval.command, "git push --force");
        assert_eq!(approval.blast_radius, "repo");
    }

    #[test]
    fn visible_approvals_can_be_filtered_by_thread_and_workspace() {
        let mut state = ApprovalState::new();
        let mut current_thread = make_approval("a1", "t1", "echo 1");
        current_thread.thread_id = Some("thread-1".into());
        current_thread.workspace_id = Some("ws-1".into());
        let mut same_workspace = make_approval("a2", "t2", "echo 2");
        same_workspace.thread_id = Some("thread-2".into());
        same_workspace.workspace_id = Some("ws-1".into());
        let mut other_workspace = make_approval("a3", "t3", "echo 3");
        other_workspace.thread_id = Some("thread-3".into());
        other_workspace.workspace_id = Some("ws-2".into());

        state.reduce(ApprovalAction::ApprovalRequired(current_thread));
        state.reduce(ApprovalAction::ApprovalRequired(same_workspace));
        state.reduce(ApprovalAction::ApprovalRequired(other_workspace));

        state.reduce(ApprovalAction::SetFilter(ApprovalFilter::CurrentThread));
        let current_thread_visible = state.visible_approvals(Some("thread-1"), Some("ws-1"));
        assert_eq!(current_thread_visible.len(), 1);
        assert_eq!(current_thread_visible[0].approval_id, "a1");

        state.reduce(ApprovalAction::SetFilter(ApprovalFilter::CurrentWorkspace));
        let current_workspace_visible = state.visible_approvals(Some("thread-1"), Some("ws-1"));
        assert_eq!(current_workspace_visible.len(), 2);
        assert_eq!(current_workspace_visible[0].approval_id, "a1");
        assert_eq!(current_workspace_visible[1].approval_id, "a2");

        state.reduce(ApprovalAction::SelectApproval("a2".into()));
        assert_eq!(state.selected_approval_id(), Some("a2"));
    }

    #[test]
    fn filter_change_clears_stale_selection_and_uses_first_visible_approval() {
        let mut state = ApprovalState::new();
        let mut current_thread = make_approval("a1", "t1", "echo 1");
        current_thread.thread_id = Some("thread-1".into());
        current_thread.workspace_id = Some("ws-1".into());
        let mut other_thread = make_approval("a2", "t2", "echo 2");
        other_thread.thread_id = Some("thread-2".into());
        other_thread.workspace_id = Some("ws-1".into());

        state.reduce(ApprovalAction::ApprovalRequired(current_thread));
        state.reduce(ApprovalAction::ApprovalRequired(other_thread));
        state.reduce(ApprovalAction::SelectApproval("a2".into()));

        state.reduce(ApprovalAction::SetFilter(ApprovalFilter::CurrentThread));

        assert_eq!(state.selected_approval_id(), None);
        assert_eq!(
            state
                .selected_visible_approval(Some("thread-1"), Some("ws-1"))
                .map(|approval| approval.approval_id.as_str()),
            Some("a1")
        );
    }

    // ── RiskLevel::classify_command tests ────────────────────────────────────

    #[test]
    fn risk_critical_for_rm_rf_root() {
        assert_eq!(
            RiskLevel::classify_command("rm -rf /home"),
            RiskLevel::Critical
        );
    }

    #[test]
    fn risk_critical_for_rm_rf_tilde() {
        assert_eq!(
            RiskLevel::classify_command("rm -rf ~/documents"),
            RiskLevel::Critical
        );
    }

    #[test]
    fn risk_critical_for_mkfs() {
        assert_eq!(
            RiskLevel::classify_command("mkfs.ext4 /dev/sda1"),
            RiskLevel::Critical
        );
    }

    #[test]
    fn risk_critical_for_dd() {
        assert_eq!(
            RiskLevel::classify_command("dd if=/dev/zero of=/dev/sda"),
            RiskLevel::Critical
        );
    }

    #[test]
    fn risk_high_for_force_push() {
        assert_eq!(
            RiskLevel::classify_command("git push --force origin main"),
            RiskLevel::High
        );
    }

    #[test]
    fn risk_high_for_short_force_push() {
        assert_eq!(RiskLevel::classify_command("git push -f"), RiskLevel::High);
    }

    #[test]
    fn risk_high_for_curl_pipe_bash() {
        assert_eq!(
            RiskLevel::classify_command("curl https://example.com/install.sh | bash"),
            RiskLevel::High,
        );
    }

    #[test]
    fn risk_high_for_docker_system_prune() {
        assert_eq!(
            RiskLevel::classify_command("docker system prune -f"),
            RiskLevel::High
        );
    }

    #[test]
    fn risk_high_for_kubectl_delete() {
        assert_eq!(
            RiskLevel::classify_command("kubectl delete pod mypod"),
            RiskLevel::High
        );
    }

    #[test]
    fn risk_high_for_systemctl() {
        assert_eq!(
            RiskLevel::classify_command("systemctl stop nginx"),
            RiskLevel::High
        );
    }

    #[test]
    fn risk_high_for_npm_publish() {
        assert_eq!(
            RiskLevel::classify_command("npm publish --access public"),
            RiskLevel::High
        );
    }

    #[test]
    fn risk_high_for_cargo_publish() {
        assert_eq!(
            RiskLevel::classify_command("cargo publish"),
            RiskLevel::High
        );
    }

    #[test]
    fn risk_medium_for_rm_rf() {
        assert_eq!(
            RiskLevel::classify_command("rm -rf ./build"),
            RiskLevel::Medium
        );
    }

    #[test]
    fn risk_medium_for_git_reset_hard() {
        assert_eq!(
            RiskLevel::classify_command("git reset --hard HEAD~3"),
            RiskLevel::Medium
        );
    }

    #[test]
    fn risk_medium_for_drop_table() {
        assert_eq!(
            RiskLevel::classify_command("DROP TABLE users"),
            RiskLevel::Medium
        );
    }

    #[test]
    fn risk_medium_for_drop_database() {
        assert_eq!(
            RiskLevel::classify_command("DROP DATABASE mydb"),
            RiskLevel::Medium
        );
    }

    #[test]
    fn risk_low_for_ls() {
        assert_eq!(RiskLevel::classify_command("ls -la"), RiskLevel::Low);
    }

    #[test]
    fn risk_low_for_echo() {
        assert_eq!(
            RiskLevel::classify_command("echo hello world"),
            RiskLevel::Low
        );
    }

    #[test]
    fn risk_low_for_cat() {
        assert_eq!(RiskLevel::classify_command("cat README.md"), RiskLevel::Low);
    }

    // ── RiskLevel::from_str_lossy tests ──────────────────────────────────────

    #[test]
    fn from_str_lossy_parses_known_values() {
        assert_eq!(RiskLevel::from_str_lossy("low"), RiskLevel::Low);
        assert_eq!(RiskLevel::from_str_lossy("medium"), RiskLevel::Medium);
        assert_eq!(RiskLevel::from_str_lossy("high"), RiskLevel::High);
        assert_eq!(RiskLevel::from_str_lossy("critical"), RiskLevel::Critical);
    }

    #[test]
    fn from_str_lossy_case_insensitive() {
        assert_eq!(RiskLevel::from_str_lossy("LOW"), RiskLevel::Low);
        assert_eq!(RiskLevel::from_str_lossy("CRITICAL"), RiskLevel::Critical);
    }

    #[test]
    fn from_str_lossy_defaults_to_medium() {
        assert_eq!(RiskLevel::from_str_lossy("unknown"), RiskLevel::Medium);
        assert_eq!(RiskLevel::from_str_lossy(""), RiskLevel::Medium);
        assert_eq!(RiskLevel::from_str_lossy("extreme"), RiskLevel::Medium);
    }
