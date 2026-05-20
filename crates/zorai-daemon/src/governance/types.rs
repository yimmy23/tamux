#![allow(dead_code)]

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Parser};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    RunAdmission,
    LaneAdmission,
    StageAdvance,
    LaneRetry,
    ResumeFromBlocked,
    CompensationEntry,
    FinalDisposition,
    ManagedCommandDispatch,
    ApprovalReuseCheck,
}

impl TransitionKind {
    /// Canonical snake-case string form used in audit rows and log payloads.
    /// One source of truth so the duplicate `transition_kind_str` helpers
    /// scattered across policy.rs, policy_external.rs, and session_ops.rs can
    /// be retired in a future refactor.
    pub fn as_str(&self) -> &'static str {
        match self {
            TransitionKind::RunAdmission => "run_admission",
            TransitionKind::LaneAdmission => "lane_admission",
            TransitionKind::StageAdvance => "stage_advance",
            TransitionKind::LaneRetry => "lane_retry",
            TransitionKind::ResumeFromBlocked => "resume_from_blocked",
            TransitionKind::CompensationEntry => "compensation_entry",
            TransitionKind::FinalDisposition => "final_disposition",
            TransitionKind::ManagedCommandDispatch => "managed_command_dispatch",
            TransitionKind::ApprovalReuseCheck => "approval_reuse_check",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerdictClass {
    Allow,
    AllowWithConstraints,
    RequireApproval,
    Defer,
    Deny,
    HaltAndIsolate,
    AllowOnlyWithCompensationPlan,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKind {
    SandboxRequired,
    NetworkDenied,
    NetworkRestricted,
    FilesystemScopeNarrowed,
    TargetScopeCapped,
    SerialOnlyExecution,
    RetriesDisabled,
    RetriesRequireFreshCheckpoint,
    ArtifactRetentionElevated,
    ManualResumeRequiredAfterCompletion,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceConstraint {
    pub kind: ConstraintKind,
    pub value: Option<String>,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkRestrictionSpec {
    pub allowed_hosts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TargetScopeCapSpec {
    pub allowed_prefixes: Vec<String>,
    pub max_targets: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryCheckpointSpec {
    pub max_age_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactRetentionSpec {
    pub level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BooleanConstraintSpec {
    pub enabled: bool,
}

impl GovernanceConstraint {
    pub fn serial_only(rationale: impl Into<String>) -> Self {
        Self {
            kind: ConstraintKind::SerialOnlyExecution,
            value: None,
            rationale: Some(rationale.into()),
        }
    }

    pub fn sandbox_required(rationale: impl Into<String>) -> Self {
        Self {
            kind: ConstraintKind::SandboxRequired,
            value: None,
            rationale: Some(rationale.into()),
        }
    }

    pub fn network_denied(rationale: impl Into<String>) -> Self {
        Self {
            kind: ConstraintKind::NetworkDenied,
            value: None,
            rationale: Some(rationale.into()),
        }
    }

    pub fn network_restricted(allowed_hosts: Vec<String>, rationale: impl Into<String>) -> Self {
        Self::with_value(
            ConstraintKind::NetworkRestricted,
            &NetworkRestrictionSpec { allowed_hosts },
            rationale,
        )
    }

    /// Bind an approval to a specific filesystem subtree. `prefixes` is the
    /// allowlist of absolute path prefixes the dispatched command may touch;
    /// dispatch enforcement (in `governance::constraints`) rejects any request
    /// whose `cwd` or absolute-path arguments fall outside this list.
    pub fn filesystem_scope_narrowed(prefixes: Vec<String>, rationale: impl Into<String>) -> Self {
        Self::with_value(
            ConstraintKind::FilesystemScopeNarrowed,
            &prefixes,
            rationale,
        )
    }

    pub fn target_scope_capped(
        allowed_prefixes: Vec<String>,
        max_targets: Option<usize>,
        rationale: impl Into<String>,
    ) -> Self {
        Self::with_value(
            ConstraintKind::TargetScopeCapped,
            &TargetScopeCapSpec {
                allowed_prefixes,
                max_targets,
            },
            rationale,
        )
    }

    pub fn retries_disabled(rationale: impl Into<String>) -> Self {
        Self::with_value(
            ConstraintKind::RetriesDisabled,
            &BooleanConstraintSpec { enabled: true },
            rationale,
        )
    }

    pub fn retries_require_fresh_checkpoint(
        max_age_secs: Option<u64>,
        rationale: impl Into<String>,
    ) -> Self {
        Self::with_value(
            ConstraintKind::RetriesRequireFreshCheckpoint,
            &RetryCheckpointSpec { max_age_secs },
            rationale,
        )
    }

    pub fn artifact_retention_elevated(
        level: impl Into<String>,
        rationale: impl Into<String>,
    ) -> Self {
        Self::with_value(
            ConstraintKind::ArtifactRetentionElevated,
            &ArtifactRetentionSpec {
                level: level.into(),
            },
            rationale,
        )
    }

    pub fn manual_resume_required_after_completion(rationale: impl Into<String>) -> Self {
        Self::with_value(
            ConstraintKind::ManualResumeRequiredAfterCompletion,
            &BooleanConstraintSpec { enabled: true },
            rationale,
        )
    }

    /// Parse a `FilesystemScopeNarrowed` constraint's `value` back into the
    /// prefix allowlist. Returns `None` for any other constraint kind or when
    /// the encoded value is missing/malformed.
    pub fn filesystem_scope_prefixes(&self) -> Option<Vec<String>> {
        if !matches!(self.kind, ConstraintKind::FilesystemScopeNarrowed) {
            return None;
        }
        let raw = self.value.as_deref()?;
        serde_json::from_str(raw).ok()
    }

    pub fn network_restriction_spec(&self) -> Option<NetworkRestrictionSpec> {
        self.decode_value_for(ConstraintKind::NetworkRestricted)
    }

    pub fn target_scope_cap_spec(&self) -> Option<TargetScopeCapSpec> {
        self.decode_value_for(ConstraintKind::TargetScopeCapped)
    }

    pub fn retries_disabled_spec(&self) -> Option<BooleanConstraintSpec> {
        self.decode_value_for(ConstraintKind::RetriesDisabled)
    }

    pub fn retry_checkpoint_spec(&self) -> Option<RetryCheckpointSpec> {
        self.decode_value_for(ConstraintKind::RetriesRequireFreshCheckpoint)
    }

    pub fn artifact_retention_spec(&self) -> Option<ArtifactRetentionSpec> {
        self.decode_value_for(ConstraintKind::ArtifactRetentionElevated)
    }

    pub fn manual_resume_spec(&self) -> Option<BooleanConstraintSpec> {
        self.decode_value_for(ConstraintKind::ManualResumeRequiredAfterCompletion)
    }

    fn with_value<T: Serialize>(
        kind: ConstraintKind,
        value: &T,
        rationale: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            value: Some(
                serde_json::to_string(value)
                    .expect("governance constraint value serializes to JSON"),
            ),
            rationale: Some(rationale.into()),
        }
    }

    fn decode_value_for<T: DeserializeOwned>(&self, kind: ConstraintKind) -> Option<T> {
        if self.kind != kind {
            return None;
        }
        let raw = self.value.as_deref()?;
        serde_json::from_str(raw).ok()
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RiskDimensions {
    pub destructiveness: u8,
    pub scope: u8,
    pub reversibility: u8,
    pub privilege: u8,
    pub externality: u8,
    pub concurrency: u8,
}

impl RiskDimensions {
    pub fn max_score(&self) -> u8 {
        [
            self.destructiveness,
            self.scope,
            self.reversibility,
            self.privilege,
            self.externality,
            self.concurrency,
        ]
        .into_iter()
        .max()
        .unwrap_or(0)
    }

    pub fn from_managed_command(request: &zorai_protocol::ManagedCommandRequest) -> Self {
        let analysis = ShellAnalysis::from_command(&request.command);

        let destructive = analysis.destructiveness();
        let scope = if request.allow_network { 6 } else { 2 };
        let reversibility = if destructive >= 8 { 9 } else { 3 };
        let privilege = if analysis.has_privilege_escalation() {
            8
        } else {
            2
        };
        let externality = if request.allow_network { 7 } else { 1 };
        let concurrency = if analysis.has_command_chaining() {
            5
        } else {
            1
        };

        Self {
            destructiveness: destructive,
            scope,
            reversibility,
            privilege,
            externality,
            concurrency,
        }
    }
}

/// Structural analysis of a shell command. Valid bash is parsed with
/// tree-sitter-bash and scored from command/operator nodes, including nested
/// command substitutions. The lightweight lexer is retained only as a
/// conservative fallback for parser failures.
struct ShellAnalysis {
    /// Sequence of argv vectors extracted from shell command nodes. When parsing
    /// fails, `commands` is empty and `parse_failed` forces a conservative
    /// high-risk fallback.
    commands: Vec<Vec<String>>,
    heredoc_payloads: Vec<String>,
    parse_failed: bool,
    chain_operator_present: bool,
}

impl ShellAnalysis {
    fn from_command(command: &str) -> Self {
        if let Some(analysis) = shell_ast_analysis(command) {
            return analysis;
        }

        let Some(raw_tokens) = lex_shell_words_and_operators(command) else {
            return Self {
                commands: Vec::new(),
                heredoc_payloads: Vec::new(),
                parse_failed: true,
                chain_operator_present: contains_chain_operator(command),
            };
        };

        let mut commands: Vec<Vec<String>> = Vec::new();
        let mut current: Vec<String> = Vec::new();
        let mut chain_operator_present = false;
        for token in raw_tokens {
            if let ShellToken::Operator(_) = token {
                chain_operator_present = true;
                if !current.is_empty() {
                    commands.push(std::mem::take(&mut current));
                }
            } else if let ShellToken::Word(word) = token {
                current.push(word);
            }
        }
        if !current.is_empty() {
            commands.push(current);
        }

        Self {
            commands,
            heredoc_payloads: Vec::new(),
            parse_failed: false,
            chain_operator_present,
        }
    }

    fn destructiveness(&self) -> u8 {
        if self.parse_failed {
            // We can't see argv-shape; assume the worst rather than silently
            // scoring as low-risk.
            return 9;
        }
        let mut score: u8 = 1;
        for argv in &self.commands {
            score = score.max(destructiveness_for(argv));
        }
        if self.commands.iter().any(shell_interpreter_without_inline) {
            for payload in &self.heredoc_payloads {
                score = score.max(
                    ShellAnalysis::from_command(payload)
                        .destructiveness()
                        .max(4),
                );
            }
        }
        score
    }

    fn has_privilege_escalation(&self) -> bool {
        self.commands.iter().any(|argv| {
            matches!(
                argv.first().map(|s| program_basename(s)),
                Some("sudo") | Some("doas") | Some("systemctl") | Some("pkexec"),
            )
        })
    }

    fn has_command_chaining(&self) -> bool {
        self.chain_operator_present
    }
}

fn shell_ast_analysis(command: &str) -> Option<ShellAnalysis> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_bash::language()).ok()?;
    let tree = parser.parse(command, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }

    let mut commands = Vec::new();
    let mut heredoc_payloads = Vec::new();
    let mut chain_operator_present = false;
    collect_shell_nodes(
        root,
        command.as_bytes(),
        &mut commands,
        &mut heredoc_payloads,
        &mut chain_operator_present,
    );

    Some(ShellAnalysis {
        commands,
        heredoc_payloads,
        parse_failed: false,
        chain_operator_present,
    })
}

fn collect_shell_nodes(
    node: Node<'_>,
    source: &[u8],
    commands: &mut Vec<Vec<String>>,
    heredoc_payloads: &mut Vec<String>,
    chain_operator_present: &mut bool,
) {
    if node.kind() == "command" {
        if let Some(argv) = command_node_argv(node, source) {
            commands.push(argv);
        }
    } else if node.kind() == "heredoc_body" {
        if let Ok(payload) = node.utf8_text(source) {
            heredoc_payloads.push(normalize_heredoc_payload(payload));
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if shell_operator_is_chain(child.kind()) {
            *chain_operator_present = true;
        }
        collect_shell_nodes(
            child,
            source,
            commands,
            heredoc_payloads,
            chain_operator_present,
        );
    }
}

fn normalize_heredoc_payload(payload: &str) -> String {
    let mut lines = payload.lines().collect::<Vec<_>>();
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    if lines.len() > 1
        && lines
            .last()
            .is_some_and(|line| !line.contains(char::is_whitespace))
    {
        lines.pop();
    }
    lines.join("\n")
}

fn command_node_argv(node: Node<'_>, source: &[u8]) -> Option<Vec<String>> {
    let name = node
        .child_by_field_name("name")
        .and_then(|name| shell_node_single_word(name, source))?;
    let mut argv = vec![name];
    let mut cursor = node.walk();
    for argument in node.children_by_field_name("argument", &mut cursor) {
        if let Some(value) = shell_node_single_word(argument, source) {
            argv.push(value);
        }
    }
    Some(argv)
}

fn shell_node_single_word(node: Node<'_>, source: &[u8]) -> Option<String> {
    let text = node.utf8_text(source).ok()?.trim();
    if text.is_empty() {
        return None;
    }
    shlex::split(text).and_then(|parts| {
        if parts.len() == 1 {
            parts.into_iter().next()
        } else {
            None
        }
    })
}

fn shell_operator_is_chain(kind: &str) -> bool {
    matches!(kind, "&&" | "||" | ";" | "|" | "&" | ";;")
}

enum ShellToken {
    Word(String),
    Operator(String),
}

fn lex_shell_words_and_operators(command: &str) -> Option<Vec<ShellToken>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = command.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;

    while let Some(ch) = chars.next() {
        if in_single {
            if ch == '\'' {
                in_single = false;
            } else {
                current.push(ch);
            }
            continue;
        }

        if in_double {
            match ch {
                '"' => in_double = false,
                '\\' => {
                    let next = chars.next()?;
                    if next != '\n' {
                        if matches!(next, '$' | '`' | '"' | '\\') {
                            current.push(next);
                        } else {
                            current.push('\\');
                            current.push(next);
                        }
                    }
                }
                _ => current.push(ch),
            }
            continue;
        }

        match ch {
            '\'' => in_single = true,
            '"' => in_double = true,
            '\\' => {
                let next = chars.next()?;
                if next != '\n' {
                    current.push(next);
                }
            }
            ' ' | '\t' | '\n' => {
                flush_shell_word(&mut tokens, &mut current);
            }
            '#' if current.is_empty() => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        break;
                    }
                }
            }
            '&' | '|' | ';' => {
                flush_shell_word(&mut tokens, &mut current);
                let op = match (ch, chars.peek().copied()) {
                    ('&', Some('&')) | ('|', Some('|')) | (';', Some(';')) => {
                        chars.next();
                        format!("{ch}{ch}")
                    }
                    _ => ch.to_string(),
                };
                tokens.push(ShellToken::Operator(op));
            }
            _ => current.push(ch),
        }
    }

    if in_single || in_double {
        return None;
    }

    flush_shell_word(&mut tokens, &mut current);
    Some(tokens)
}

fn flush_shell_word(tokens: &mut Vec<ShellToken>, current: &mut String) {
    if !current.is_empty() {
        tokens.push(ShellToken::Word(std::mem::take(current)));
    }
}

fn contains_chain_operator(command: &str) -> bool {
    // Only used in the tokenization-failed fallback. Substring is acceptable
    // here because we're already pessimistic about the input.
    command.contains("&&")
        || command.contains("||")
        || command.contains(';')
        || command.contains('|')
}

fn program_basename(program: &str) -> &str {
    program.rsplit('/').next().unwrap_or(program)
}

fn destructiveness_for(argv: &[String]) -> u8 {
    let Some(program) = argv.first() else {
        return 1;
    };
    let (name, rest_start) = unwrap_privileged_payload(argv, program);
    let rest: Vec<&str> = argv.iter().skip(rest_start).map(|s| s.as_str()).collect();

    match name.as_str() {
        "rm" => {
            if rm_has_recursive_force(&rest) {
                9
            } else {
                4
            }
        }
        "mv" | "cp" => 4,
        "dd" | "mkfs" | "fdisk" | "parted" | "shred" | "wipefs" => 9,
        "bash" | "sh" | "zsh" => shell_inline_payload(&rest)
            .map(|payload| {
                ShellAnalysis::from_command(payload)
                    .destructiveness()
                    .max(4)
            })
            .unwrap_or(1),
        "git" => {
            if rest.first().copied() == Some("reset") && rest.iter().any(|arg| *arg == "--hard") {
                9
            } else if rest.first().copied() == Some("push")
                && rest.iter().any(|arg| *arg == "--force" || *arg == "-f")
            {
                7
            } else {
                1
            }
        }
        "terraform" => {
            if rest.first().copied() == Some("destroy") {
                9
            } else {
                1
            }
        }
        "kubectl" => {
            if rest.first().copied() == Some("delete") {
                9
            } else {
                1
            }
        }
        "docker" => {
            if rest.first().copied() == Some("system") && rest.get(1).copied() == Some("prune") {
                7
            } else {
                1
            }
        }
        _ => 1,
    }
}

fn shell_inline_payload<'a>(args: &'a [&str]) -> Option<&'a str> {
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if *arg == "-c" {
            return args.get(index + 1).copied();
        }
        if *arg == "--" {
            return None;
        }
        if !arg.starts_with('-') || *arg == "-" {
            return None;
        }
        index += 1;
    }
    None
}

fn shell_interpreter_without_inline(argv: &Vec<String>) -> bool {
    let Some(program) = argv.first() else {
        return false;
    };
    let name = program_basename(program).to_ascii_lowercase();
    if !matches!(name.as_str(), "bash" | "sh" | "zsh") {
        return false;
    }
    let rest = argv
        .iter()
        .skip(1)
        .map(|arg| arg.as_str())
        .collect::<Vec<_>>();
    shell_inline_payload(&rest).is_none()
}

fn unwrap_privileged_payload(argv: &[String], program: &str) -> (String, usize) {
    let mut name = program_basename(program).to_ascii_lowercase();
    if !matches!(name.as_str(), "sudo" | "doas" | "pkexec") {
        return (name, 1);
    }

    let mut index = 1;
    while let Some(arg) = argv.get(index) {
        if arg == "--" {
            index += 1;
            break;
        }
        if !arg.starts_with('-') || arg == "-" {
            break;
        }
        let consumes_value = matches!(
            arg.as_str(),
            "-A" | "-a"
                | "-C"
                | "-c"
                | "-D"
                | "-g"
                | "-h"
                | "-p"
                | "-R"
                | "-r"
                | "-T"
                | "-t"
                | "-U"
                | "-u"
                | "--askpass"
                | "--auth-type"
                | "--chdir"
                | "--close-from"
                | "--group"
                | "--host"
                | "--login-class"
                | "--prompt"
                | "--role"
                | "--type"
                | "--user"
        );
        index += 1;
        if consumes_value {
            index += 1;
        }
    }

    let Some(payload) = argv.get(index) else {
        return (name, 1);
    };
    name = program_basename(payload).to_ascii_lowercase();
    (name, index + 1)
}

fn rm_has_recursive_force(args: &[&str]) -> bool {
    let mut recursive = false;
    let mut force = false;
    for arg in args {
        if let Some((r, f)) = rm_flag_bits(arg) {
            recursive |= r;
            force |= f;
        }
    }
    recursive && force
}

fn rm_flag_bits(arg: &str) -> Option<(bool, bool)> {
    if let Some(flags) = arg.strip_prefix('-') {
        if flags.starts_with('-') {
            return match flags {
                "-recursive" | "-dir" => Some((true, false)),
                "-force" => Some((false, true)),
                _ => Some((false, false)),
            };
        }
        let chars: std::collections::HashSet<char> = flags.chars().collect();
        return Some((
            chars.contains(&'r') || chars.contains(&'R'),
            chars.contains(&'f'),
        ));
    }
    None
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlastRadiusEstimate {
    pub lane_scope: String,
    pub stage_scope: String,
    pub run_scope: String,
}

impl BlastRadiusEstimate {
    pub fn for_managed_command(
        request: &zorai_protocol::ManagedCommandRequest,
        workspace_id: Option<String>,
    ) -> Self {
        let run_scope = if request.allow_network {
            "network and workspace".to_string()
        } else if workspace_id.is_some() {
            "workspace".to_string()
        } else {
            "current session".to_string()
        };

        Self {
            lane_scope: "current terminal lane".to_string(),
            stage_scope: "managed command dispatch".to_string(),
            run_scope,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentFacts {
    pub sandbox_available: bool,
    pub sandbox_enabled: bool,
    pub network_allowed: bool,
    pub filesystem_scope: Option<String>,
    pub workspace_id: Option<String>,
    pub host_type: Option<String>,
    pub privilege_posture: Option<String>,
}

impl EnvironmentFacts {
    pub fn for_managed_command(
        request: &zorai_protocol::ManagedCommandRequest,
        workspace_id: Option<String>,
    ) -> Self {
        Self {
            sandbox_available: true,
            sandbox_enabled: request.sandbox_enabled,
            network_allowed: request.allow_network,
            filesystem_scope: request.cwd.clone(),
            workspace_id,
            host_type: None,
            privilege_posture: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalContext {
    pub prior_approval_ids: Vec<String>,
    pub approval_fresh: bool,
    pub conditions_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceCompleteness {
    Complete,
    Partial,
    Insufficient,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProvenanceStatus {
    pub completeness: ProvenanceCompleteness,
    pub missing_evidence: Vec<String>,
}

impl ProvenanceStatus {
    pub fn complete() -> Self {
        Self {
            completeness: ProvenanceCompleteness::Complete,
            missing_evidence: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompensationHints {
    pub rollback_feasible: bool,
    pub compensation_feasible: bool,
    pub hints: Vec<String>,
}

impl CompensationHints {
    pub fn unknown() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceInitiator {
    Operator,
    Agent,
    GoalRunner,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceInput {
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub transition_kind: TransitionKind,
    pub stage_id: Option<String>,
    pub lane_ids: Vec<String>,
    pub target_ids: Vec<String>,
    pub requested_action_summary: String,
    pub intent_summary: String,
    pub risk_dimensions: RiskDimensions,
    pub blast_radius: BlastRadiusEstimate,
    pub environment_facts: EnvironmentFacts,
    pub approval_context: ApprovalContext,
    pub retry_or_rebind_history: Vec<String>,
    pub provenance_status: ProvenanceStatus,
    pub rollback_or_compensation_hints: CompensationHints,
    pub initiator: GovernanceInitiator,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalRequirement {
    pub scope_summary: String,
    pub expires_at: Option<u64>,
    pub policy_fingerprint: String,
    pub constraints: Vec<GovernanceConstraint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContainmentScope {
    Lane,
    Stage,
    Run,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompensationRequirement {
    pub required: bool,
    pub reason: String,
    pub plan_reference: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceVerdict {
    pub verdict_class: VerdictClass,
    pub risk_class: RiskClass,
    pub rationale: Vec<String>,
    pub constraints: Vec<GovernanceConstraint>,
    pub approval_requirement: Option<ApprovalRequirement>,
    pub containment_scope: Option<ContainmentScope>,
    pub compensation_requirement: Option<CompensationRequirement>,
    pub freshness_window_secs: Option<u64>,
    pub policy_fingerprint: String,
}

impl GovernanceVerdict {
    pub fn allow(policy_fingerprint: String, risk_class: RiskClass) -> Self {
        Self {
            verdict_class: VerdictClass::Allow,
            risk_class,
            rationale: Vec::new(),
            constraints: Vec::new(),
            approval_requirement: None,
            containment_scope: None,
            compensation_requirement: None,
            freshness_window_secs: None,
            policy_fingerprint,
        }
    }

    pub fn allow_with_optional_constraints(
        policy_fingerprint: String,
        risk_class: RiskClass,
        rationale: Vec<String>,
        constraints: Vec<GovernanceConstraint>,
    ) -> Self {
        let verdict_class = if constraints.is_empty() {
            VerdictClass::Allow
        } else {
            VerdictClass::AllowWithConstraints
        };

        Self {
            verdict_class,
            risk_class,
            rationale,
            constraints,
            approval_requirement: None,
            containment_scope: None,
            compensation_requirement: None,
            freshness_window_secs: None,
            policy_fingerprint,
        }
    }

    pub fn require_approval(
        policy_fingerprint: String,
        risk_class: RiskClass,
        rationale: Vec<String>,
        constraints: Vec<GovernanceConstraint>,
    ) -> Self {
        Self {
            verdict_class: VerdictClass::RequireApproval,
            risk_class,
            rationale: rationale.clone(),
            constraints: constraints.clone(),
            approval_requirement: Some(ApprovalRequirement {
                scope_summary: "managed transition".to_string(),
                expires_at: None,
                policy_fingerprint: policy_fingerprint.clone(),
                constraints,
            }),
            containment_scope: None,
            compensation_requirement: None,
            freshness_window_secs: Some(900),
            policy_fingerprint,
        }
    }

    pub fn defer(policy_fingerprint: String, rationale: Vec<String>) -> Self {
        Self {
            verdict_class: VerdictClass::Defer,
            risk_class: RiskClass::High,
            rationale,
            constraints: Vec::new(),
            approval_requirement: None,
            containment_scope: None,
            compensation_requirement: None,
            freshness_window_secs: None,
            policy_fingerprint,
        }
    }

    /// Refuse the transition outright. Use when the environment cannot satisfy
    /// the constraints that *would* be required for an approval — there is no
    /// approval that can rescue this transition, so the operator shouldn't be
    /// prompted for one.
    pub fn deny(
        policy_fingerprint: String,
        risk_class: RiskClass,
        rationale: Vec<String>,
    ) -> Self {
        Self {
            verdict_class: VerdictClass::Deny,
            risk_class,
            rationale,
            constraints: Vec::new(),
            approval_requirement: None,
            containment_scope: None,
            compensation_requirement: None,
            freshness_window_secs: None,
            policy_fingerprint,
        }
    }

    /// Stop the transition and quarantine the work for operator review. Used
    /// when repeated retries of a risky transition keep failing — continuing
    /// to thrash is worse than freezing the run and surfacing the loop.
    pub fn halt_and_isolate(
        policy_fingerprint: String,
        risk_class: RiskClass,
        rationale: Vec<String>,
        containment_scope: ContainmentScope,
    ) -> Self {
        Self {
            verdict_class: VerdictClass::HaltAndIsolate,
            risk_class,
            rationale,
            constraints: Vec::new(),
            approval_requirement: None,
            containment_scope: Some(containment_scope),
            compensation_requirement: None,
            freshness_window_secs: None,
            policy_fingerprint,
        }
    }

    /// Allow the transition only if an explicit compensation/rollback plan is
    /// attached. Triggered when destructive work has no automatic rollback
    /// path available and no compensation feasibility — the operator must
    /// commit to a recovery plan before the work runs.
    pub fn allow_only_with_compensation_plan(
        policy_fingerprint: String,
        risk_class: RiskClass,
        rationale: Vec<String>,
        compensation_requirement: CompensationRequirement,
    ) -> Self {
        Self {
            verdict_class: VerdictClass::AllowOnlyWithCompensationPlan,
            risk_class,
            rationale,
            constraints: Vec::new(),
            approval_requirement: None,
            containment_scope: None,
            compensation_requirement: Some(compensation_requirement),
            freshness_window_secs: None,
            policy_fingerprint,
        }
    }
}

pub(crate) fn effective_constraints(verdict: &GovernanceVerdict) -> Vec<GovernanceConstraint> {
    let mut constraints = verdict.constraints.clone();

    if let Some(requirement) = &verdict.approval_requirement {
        for constraint in &requirement.constraints {
            if !constraints.contains(constraint) {
                constraints.push(constraint.clone());
            }
        }
    }

    constraints
}
