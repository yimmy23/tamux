#![allow(dead_code)]

use std::collections::HashSet;

// ── RiskLevel ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    /// Parse from string, defaulting to Medium for unknown values.
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "critical" => Self::Critical,
            _ => Self::Medium,
        }
    }

    /// Heuristic: classify risk from command text.
    pub fn classify_command(command: &str) -> Self {
        let cmd = command.to_lowercase();

        if cmd.contains("rm -rf /")
            || cmd.contains("rm -rf ~/")
            || cmd.contains("mkfs")
            || cmd.contains("dd if=")
        {
            return Self::Critical;
        }

        if cmd.contains("git push --force")
            || cmd.contains("git push -f")
            || (cmd.contains("curl") && cmd.contains("| bash"))
            || cmd.contains("docker system prune")
            || cmd.contains("kubectl delete")
            || cmd.contains("systemctl")
            || cmd.contains("npm publish")
            || cmd.contains("cargo publish")
        {
            return Self::High;
        }

        if cmd.contains("rm -rf")
            || cmd.contains("git reset --hard")
            || cmd.contains("drop table")
            || cmd.contains("drop database")
        {
            return Self::Medium;
        }

        Self::Low
    }
}

// ── PendingApproval ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PendingApproval {
    pub approval_id: String,
    pub task_id: String,
    pub task_title: Option<String>,
    pub thread_id: Option<String>,
    pub thread_title: Option<String>,
    pub workspace_id: Option<String>,
    pub rationale: Option<String>,
    pub reasons: Vec<String>,
    /// The command text extracted (heuristically) from blocked_reason.
    pub command: String,
    pub risk_level: RiskLevel,
    pub blast_radius: String,
    pub received_at: u64,
    pub seen_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalFilter {
    AllPending,
    CurrentThread,
    CurrentWorkspace,
}

// ── ApprovalAction ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ApprovalAction {
    ApprovalRequired(PendingApproval),
    SelectApproval(String),
    SetFilter(ApprovalFilter),
    Resolve {
        approval_id: String,
        decision: String,
    },
    AllowSession(String),  // command pattern to allow for this session
    ClearResolved(String), // remove by approval_id
}

// ── ApprovalState ─────────────────────────────────────────────────────────────

pub struct ApprovalState {
    pending_approvals: Vec<PendingApproval>,
    session_allowlist: HashSet<String>,
    selected_approval_id: Option<String>,
    filter: ApprovalFilter,
}

impl ApprovalState {
    pub fn new() -> Self {
        Self {
            pending_approvals: Vec::new(),
            session_allowlist: HashSet::new(),
            selected_approval_id: None,
            filter: ApprovalFilter::AllPending,
        }
    }

    pub fn pending_approvals(&self) -> &[PendingApproval] {
        &self.pending_approvals
    }

    pub fn selected_approval_id(&self) -> Option<&str> {
        self.selected_approval_id.as_deref()
    }

    pub fn filter(&self) -> ApprovalFilter {
        self.filter
    }

    pub fn selected_approval(&self) -> Option<&PendingApproval> {
        self.selected_approval_id
            .as_deref()
            .and_then(|approval_id| self.approval_by_id(approval_id))
            .or_else(|| self.current_approval())
    }

    pub fn selected_visible_approval<'a>(
        &'a self,
        current_thread_id: Option<&str>,
        current_workspace_id: Option<&str>,
    ) -> Option<&'a PendingApproval> {
        let visible = self.visible_approvals(current_thread_id, current_workspace_id);
        self.selected_approval_id
            .as_deref()
            .and_then(|approval_id| {
                visible
                    .iter()
                    .find(|approval| approval.approval_id == approval_id)
                    .copied()
            })
            .or_else(|| visible.first().copied())
    }

    pub fn approval_by_id(&self, approval_id: &str) -> Option<&PendingApproval> {
        self.pending_approvals
            .iter()
            .find(|approval| approval.approval_id == approval_id)
    }

    pub fn visible_approvals<'a>(
        &'a self,
        current_thread_id: Option<&str>,
        current_workspace_id: Option<&str>,
    ) -> Vec<&'a PendingApproval> {
        self.pending_approvals
            .iter()
            .filter(|approval| match self.filter {
                ApprovalFilter::AllPending => true,
                ApprovalFilter::CurrentThread => approval.thread_id.as_deref() == current_thread_id,
                ApprovalFilter::CurrentWorkspace => {
                    approval.workspace_id.as_deref() == current_workspace_id
                }
            })
            .collect()
    }

    /// The first pending approval (the one currently shown to the user).
    pub fn current_approval(&self) -> Option<&PendingApproval> {
        self.pending_approvals.first()
    }

    /// Returns true if the given command pattern has been allow-listed for this session.
    pub fn is_allowed(&self, pattern: &str) -> bool {
        self.session_allowlist.contains(pattern)
    }

    pub fn reduce(&mut self, action: ApprovalAction) {
        match action {
            ApprovalAction::ApprovalRequired(approval) => {
                if let Some(existing) = self
                    .pending_approvals
                    .iter_mut()
                    .find(|existing| existing.approval_id == approval.approval_id)
                {
                    *existing = approval;
                } else {
                    if self.selected_approval_id.is_none() {
                        self.selected_approval_id = Some(approval.approval_id.clone());
                    }
                    self.pending_approvals.push(approval);
                }
            }

            ApprovalAction::SelectApproval(approval_id) => {
                if self
                    .pending_approvals
                    .iter()
                    .any(|approval| approval.approval_id == approval_id)
                {
                    self.selected_approval_id = Some(approval_id);
                }
            }

            ApprovalAction::SetFilter(filter) => {
                self.filter = filter;
                self.selected_approval_id = None;
            }

            ApprovalAction::Resolve {
                approval_id,
                decision: _,
            } => {
                self.pending_approvals
                    .retain(|a| a.approval_id != approval_id);
                if self.selected_approval_id.as_deref() == Some(approval_id.as_str()) {
                    self.selected_approval_id = self
                        .pending_approvals
                        .first()
                        .map(|approval| approval.approval_id.clone());
                }
            }

            ApprovalAction::AllowSession(pattern) => {
                self.session_allowlist.insert(pattern);
            }

            ApprovalAction::ClearResolved(approval_id) => {
                self.pending_approvals
                    .retain(|a| a.approval_id != approval_id);
                if self.selected_approval_id.as_deref() == Some(approval_id.as_str()) {
                    self.selected_approval_id = self
                        .pending_approvals
                        .first()
                        .map(|approval| approval.approval_id.clone());
                }
            }
        }
    }
}

impl Default for ApprovalState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
    fn allow_session_adds_to_allowlist() {
        let mut state = ApprovalState::new();
        assert!(!state.is_allowed("git push"));

        state.reduce(ApprovalAction::AllowSession("git push".into()));
        assert!(state.is_allowed("git push"));
        assert!(!state.is_allowed("rm -rf"));
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
}
