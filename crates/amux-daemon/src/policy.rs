use amux_protocol::{ApprovalPayload, ManagedCommandRequest, SecurityLevel, WorkspaceId};
use regex::Regex;
use std::sync::LazyLock;

static RISK_PATTERNS: LazyLock<Vec<(Regex, &'static str, &'static str, &'static str)>> =
    LazyLock::new(|| {
        vec![
            (
                Regex::new(r"(^|\s)rm\s+-rf\s+\S+").unwrap(),
                "critical",
                "filesystem-wide",
                "destructive recursive delete",
            ),
            (
                Regex::new(r"(^|\s)(mkfs|fdisk|parted|dd)\b").unwrap(),
                "critical",
                "disk-level",
                "disk or block-device mutation",
            ),
            (
                Regex::new(r"(^|\s)(git\s+push\b.*(--force|-f)|git\s+reset\s+--hard\b)").unwrap(),
                "high",
                "repository-wide",
                "git history rewrite or destructive reset",
            ),
            (
                Regex::new(r"curl\b[^|\n]*\|\s*(sh|bash|zsh)\b").unwrap(),
                "high",
                "remote code execution",
                "executes a remote script directly",
            ),
            (
                Regex::new(r"(^|\s)(docker\s+system\s+prune|kubectl\s+delete|terraform\s+destroy|systemctl\s+(stop|restart|disable))\b").unwrap(),
                "high",
                "service or infrastructure",
                "mutates infrastructure or service lifecycle",
            ),
            (
                Regex::new(r"(^|\s)(remove-item|ri)\b[^\n]*\b(-recurse|-r)\b").unwrap(),
                "high",
                "workspace or subtree",
                "recursive file deletion on Windows",
            ),
            (
                Regex::new(r"(^|\s)(rd|rmdir)\s+[^\n]*\s+/s\b").unwrap(),
                "high",
                "workspace or subtree",
                "recursive directory delete via cmd.exe",
            ),
            (
                Regex::new(r"(^|\s)(del|erase)\s+[^\n]*\s+/s\b").unwrap(),
                "high",
                "workspace or subtree",
                "recursive file delete via cmd.exe",
            ),
            (
                Regex::new(r"(invoke-webrequest|iwr)\b[^|\n]*\|\s*(iex|invoke-expression)\b").unwrap(),
                "high",
                "remote code execution",
                "downloads and executes remote PowerShell content",
            ),
            (
                Regex::new(r"(^|\s)(stop-service|restart-service|set-service)\b").unwrap(),
                "high",
                "host services",
                "mutates Windows service lifecycle",
            ),
            (
                Regex::new(r"(^|\s)(format|diskpart)\b").unwrap(),
                "critical",
                "disk-level",
                "disk or volume mutation on Windows",
            ),
        ]
    });

pub enum PolicyDecision {
    Allow,
    RequireApproval(ApprovalPayload),
}

pub fn evaluate_command(
    execution_id: String,
    request: &ManagedCommandRequest,
    workspace_id: Option<WorkspaceId>,
) -> PolicyDecision {
    if matches!(request.security_level, SecurityLevel::Yolo) {
        return PolicyDecision::Allow;
    }

    let normalized = request.command.trim().to_ascii_lowercase();
    let mut risk_level = "medium".to_string();
    let mut blast_radius = "current session".to_string();
    let mut reasons = Vec::new();

    if request.allow_network {
        reasons.push("network access requested".to_string());
        blast_radius = "network and workspace".to_string();
    }

    for (pattern, level, radius, reason) in RISK_PATTERNS.iter() {
        if pattern.is_match(&normalized) {
            risk_level = (*level).to_string();
            blast_radius = (*radius).to_string();
            reasons.push((*reason).to_string());
        }
    }

    if reasons.is_empty() {
        return PolicyDecision::Allow;
    }

    PolicyDecision::RequireApproval(ApprovalPayload {
        approval_id: format!("apr_{}", uuid::Uuid::new_v4()),
        execution_id,
        command: request.command.clone(),
        rationale: request.rationale.clone(),
        risk_level,
        blast_radius,
        reasons,
        workspace_id,
        allow_network: request.allow_network,
    })
}

#[cfg(test)]
mod tests {
    use super::{evaluate_command, PolicyDecision};
    use amux_protocol::{ManagedCommandRequest, ManagedCommandSource, SecurityLevel};

    fn request(
        command: &str,
        security_level: SecurityLevel,
        allow_network: bool,
    ) -> ManagedCommandRequest {
        ManagedCommandRequest {
            command: command.to_string(),
            rationale: "test".to_string(),
            allow_network,
            sandbox_enabled: false,
            security_level,
            cwd: None,
            language_hint: None,
            source: ManagedCommandSource::Agent,
        }
    }

    #[test]
    fn yolo_bypasses_approval_even_for_risky_commands() {
        let req = request("rm -rf /", SecurityLevel::Yolo, false);
        let decision = evaluate_command("exec_1".to_string(), &req, None);
        assert!(matches!(decision, PolicyDecision::Allow));
    }

    #[test]
    fn lowest_requires_approval_for_risky_commands() {
        let req = request("rm -rf /", SecurityLevel::Lowest, false);
        let decision = evaluate_command("exec_2".to_string(), &req, None);
        match decision {
            PolicyDecision::RequireApproval(payload) => {
                assert!(payload
                    .reasons
                    .iter()
                    .any(|reason| reason.contains("destructive recursive delete")));
            }
            PolicyDecision::Allow => panic!("expected approval for risky command at lowest level"),
        }
    }

    #[test]
    fn lowest_requires_approval_for_targeted_rm_rf_paths() {
        let req = request("rm -rf /home/mkurman/to_remove", SecurityLevel::Lowest, false);
        let decision = evaluate_command("exec_2b".to_string(), &req, None);
        match decision {
            PolicyDecision::RequireApproval(payload) => {
                assert!(payload
                    .reasons
                    .iter()
                    .any(|reason| reason.contains("destructive recursive delete")));
            }
            PolicyDecision::Allow => panic!("expected approval for rm -rf on specific paths"),
        }
    }

    #[test]
    fn lowest_requires_approval_when_network_requested() {
        let req = request("echo hello", SecurityLevel::Lowest, true);
        let decision = evaluate_command("exec_3".to_string(), &req, None);
        match decision {
            PolicyDecision::RequireApproval(payload) => {
                assert!(payload
                    .reasons
                    .iter()
                    .any(|reason| reason.contains("network access requested")));
            }
            PolicyDecision::Allow => panic!("expected approval when network access is requested"),
        }
    }
}
