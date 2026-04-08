use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

pub(super) const WATCHER_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const IMPORT_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const TRACKED_POLL_INTERVAL: Duration = Duration::from_millis(250);
pub(super) const TRACKED_POLL_MAX_ATTEMPTS: usize = 8;
pub(super) const RECONCILIATION_BUDGET: Duration = Duration::from_secs(30);
const DEFAULT_MAX_CANDIDATES: usize = 3;
const DEFAULT_MAX_PAGES: usize = 3;
const DEFAULT_RECENCY_WINDOW: Duration = Duration::from_secs(72 * 60 * 60);
const STARTUP_STATE_FILE: &str = "aline-startup-state.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WatcherState {
    Running,
    Stopped,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WatcherStatus {
    pub(super) state: WatcherState,
    pub(super) mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(super) struct SessionListJson {
    #[serde(default)]
    pub(super) has_more: Option<bool>,
    #[serde(default)]
    pub(super) page: Option<usize>,
    #[serde(default)]
    pub(super) per_page: Option<usize>,
    #[serde(default)]
    pub(super) total_pages: Option<usize>,
    pub(super) sessions: Vec<AlineDiscoveredSession>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(super) struct AlineDiscoveredSession {
    pub(super) status: String,
    pub(super) source: String,
    pub(super) project_name: String,
    #[serde(default)]
    pub(super) project_path: Option<String>,
    pub(super) session_id: String,
    pub(super) created_at: String,
    pub(super) last_activity: String,
    pub(super) session_file: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StartupSelectionPolicy {
    pub(super) recency_window: Duration,
    pub(super) max_candidates: usize,
    pub(super) max_pages: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AlineStartupShortCircuitReason {
    AlineUnavailable,
    NoRepoRoots,
    MultipleRepoRoots,
    NoSelectedSessions,
    ImportNotConfirmed,
    BudgetExhausted,
    CommandFailed,
}

impl AlineStartupShortCircuitReason {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::AlineUnavailable => "aline_unavailable",
            Self::NoRepoRoots => "no_repo_roots",
            Self::MultipleRepoRoots => "multiple_repo_roots",
            Self::NoSelectedSessions => "no_selected_sessions",
            Self::ImportNotConfirmed => "import_not_confirmed",
            Self::BudgetExhausted => "budget_exhausted",
            Self::CommandFailed => "command_failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub(super) struct PersistedAlineStartupState {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) recently_imported_session_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AlineStartupSummary {
    pub(super) aline_available: bool,
    pub(super) watcher_initial_state: Option<WatcherState>,
    pub(super) watcher_started: bool,
    pub(super) discovered_count: usize,
    pub(super) selected_count: usize,
    pub(super) imported_count: usize,
    pub(super) generated_count: usize,
    pub(super) short_circuit_reason: Option<AlineStartupShortCircuitReason>,
    pub(super) skipped_recently_imported_count: usize,
    pub(super) budget_exhausted: bool,
    pub(super) failure_stage: Option<String>,
    pub(super) failure_message: Option<String>,
    pub(super) recently_imported_session_ids: Vec<String>,
}

impl AlineStartupSummary {
    pub(super) fn unavailable() -> Self {
        Self {
            aline_available: false,
            watcher_initial_state: None,
            watcher_started: false,
            discovered_count: 0,
            selected_count: 0,
            imported_count: 0,
            generated_count: 0,
            short_circuit_reason: Some(AlineStartupShortCircuitReason::AlineUnavailable),
            skipped_recently_imported_count: 0,
            budget_exhausted: false,
            failure_stage: None,
            failure_message: None,
            recently_imported_session_ids: Vec::new(),
        }
    }

    pub(super) fn skipped(reason: AlineStartupShortCircuitReason) -> Self {
        let mut summary = Self {
            aline_available: false,
            watcher_initial_state: None,
            watcher_started: false,
            discovered_count: 0,
            selected_count: 0,
            imported_count: 0,
            generated_count: 0,
            short_circuit_reason: None,
            skipped_recently_imported_count: 0,
            budget_exhausted: false,
            failure_stage: None,
            failure_message: None,
            recently_imported_session_ids: Vec::new(),
        };
        summary.short_circuit_reason = Some(reason);
        summary
    }

    fn available() -> Self {
        Self {
            aline_available: true,
            watcher_initial_state: None,
            watcher_started: false,
            discovered_count: 0,
            selected_count: 0,
            imported_count: 0,
            generated_count: 0,
            short_circuit_reason: None,
            skipped_recently_imported_count: 0,
            budget_exhausted: false,
            failure_stage: None,
            failure_message: None,
            recently_imported_session_ids: Vec::new(),
        }
    }

    fn mark_command_failure(&mut self, stage: &str, error: &anyhow::Error) {
        self.short_circuit_reason = Some(AlineStartupShortCircuitReason::CommandFailed);
        self.failure_stage = Some(stage.to_string());
        self.failure_message = Some(error.to_string());
    }
}

impl PersistedAlineStartupState {
    pub(super) fn recent_session_ids(
        &self,
        now: DateTime<Utc>,
        max_age: Duration,
    ) -> HashSet<String> {
        let Some(updated_at) = self
            .updated_at
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|timestamp| timestamp.with_timezone(&Utc))
        else {
            return HashSet::new();
        };

        let max_age = chrono::Duration::from_std(max_age)
            .expect("startup dedupe max age should fit into chrono duration");
        if now.signed_duration_since(updated_at) > max_age {
            return HashSet::new();
        }

        self.recently_imported_session_ids
            .iter()
            .map(|session_id| session_id.trim())
            .filter(|session_id| !session_id.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }

    pub(super) fn record_recently_imported(
        &mut self,
        session_ids: &[String],
        updated_at: DateTime<Utc>,
    ) {
        let mut deduped = HashSet::new();
        deduped.extend(
            session_ids
                .iter()
                .map(|session_id| session_id.trim())
                .filter(|session_id| !session_id.is_empty())
                .map(ToOwned::to_owned),
        );
        let mut ordered = deduped.into_iter().collect::<Vec<_>>();
        ordered.sort();
        self.recently_imported_session_ids = ordered;
        self.updated_at = Some(updated_at.to_rfc3339());
    }
}

impl Default for StartupSelectionPolicy {
    fn default() -> Self {
        Self {
            recency_window: DEFAULT_RECENCY_WINDOW,
            max_candidates: DEFAULT_MAX_CANDIDATES,
            max_pages: DEFAULT_MAX_PAGES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StartupCommandSpec {
    pub(super) program: String,
    pub(super) args: Vec<String>,
    pub(super) timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StartupCommandOutput {
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) exit_code: i32,
}

#[async_trait]
pub(super) trait StartupCommandRunner: Send + Sync {
    async fn run(&self, spec: StartupCommandSpec) -> Result<StartupCommandOutput>;
}

#[derive(Debug, Default)]
pub(super) struct TokioStartupCommandRunner;

#[async_trait]
impl StartupCommandRunner for TokioStartupCommandRunner {
    async fn run(&self, spec: StartupCommandSpec) -> Result<StartupCommandOutput> {
        let program = spec.program.clone();
        let output = tokio::time::timeout(
            spec.timeout,
            tokio::process::Command::new(&spec.program)
                .args(&spec.args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .output(),
        )
        .await
        .with_context(|| format!("aline startup command timed out: {program}"))?
        .with_context(|| format!("failed to run aline startup command: {program}"))?;

        Ok(StartupCommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
}

pub(super) async fn list_recent_project_sessions<R>(
    runner: &R,
    repo_root: &Path,
    now: DateTime<Utc>,
    policy: StartupSelectionPolicy,
) -> Result<Vec<AlineDiscoveredSession>>
where
    R: StartupCommandRunner + ?Sized,
{
    let collected = collect_discovered_sessions(runner, policy, Instant::now()).await?;
    Ok(select_import_candidates(repo_root, &collected, now, policy))
}

pub(super) async fn reconcile_startup_sessions<R>(
    runner: &R,
    repo_root: &Path,
    now: DateTime<Utc>,
    policy: StartupSelectionPolicy,
    recently_imported_session_ids: &HashSet<String>,
) -> Result<AlineStartupSummary>
where
    R: StartupCommandRunner + ?Sized,
{
    let mut summary = AlineStartupSummary::available();
    let started_at = Instant::now();

    let status_output = match runner.run(build_watcher_status_command()).await {
        Ok(output) => output,
        Err(error) => {
            summary.mark_command_failure("watcher_status", &error);
            return Ok(summary);
        }
    };
    if let Err(error) = ensure_success_exit(&status_output, "aline watcher status") {
        summary.mark_command_failure("watcher_status", &error);
        return Ok(summary);
    }
    let watcher_status = parse_watcher_status(&status_output.stdout);
    summary.watcher_initial_state = Some(watcher_status.state.clone());

    if watcher_status.state == WatcherState::Stopped {
        let start_output = match runner.run(build_watcher_start_command()).await {
            Ok(output) => output,
            Err(error) => {
                summary.mark_command_failure("watcher_start", &error);
                return Ok(summary);
            }
        };
        if let Err(error) = ensure_success_exit(&start_output, "aline watcher start") {
            summary.mark_command_failure("watcher_start", &error);
            return Ok(summary);
        }
        summary.watcher_started = true;
    }

    let discovered = match collect_discovered_sessions(runner, policy, started_at).await {
        Ok(discovered) => discovered,
        Err(error) => {
            if error.to_string().contains("budget exhausted") {
                summary.short_circuit_reason =
                    Some(AlineStartupShortCircuitReason::BudgetExhausted);
                summary.budget_exhausted = true;
            } else {
                summary.mark_command_failure("session_list", &error);
            }
            return Ok(summary);
        }
    };
    summary.discovered_count = discovered.len();
    let mut selected = select_import_candidates(repo_root, &discovered, now, policy);
    let pre_dedupe_selected_count = selected.len();
    selected.retain(|session| !recently_imported_session_ids.contains(&session.session_id));
    summary.skipped_recently_imported_count =
        pre_dedupe_selected_count.saturating_sub(selected.len());
    summary.selected_count = selected.len();

    if selected.is_empty() {
        summary.short_circuit_reason = Some(AlineStartupShortCircuitReason::NoSelectedSessions);
        return Ok(summary);
    }

    for session in selected {
        if !has_budget_for_next_import(started_at) {
            summary.short_circuit_reason = Some(AlineStartupShortCircuitReason::BudgetExhausted);
            summary.budget_exhausted = true;
            break;
        }

        let import_output = match runner.run(build_import_command(&session)).await {
            Ok(output) => output,
            Err(error) => {
                summary.mark_command_failure("session_import", &error);
                return Ok(summary);
            }
        };
        if let Err(error) = ensure_success_exit(&import_output, "aline watcher session import") {
            summary.mark_command_failure("session_import", &error);
            return Ok(summary);
        }
        summary.imported_count += 1;

        let import_confirmed =
            match confirm_import_ready(runner, &session.session_id, policy, started_at).await {
                Ok(import_confirmed) => import_confirmed,
                Err(error) => {
                    if error.to_string().contains("budget exhausted") {
                        summary.short_circuit_reason =
                            Some(AlineStartupShortCircuitReason::BudgetExhausted);
                        summary.budget_exhausted = true;
                    } else {
                        summary.mark_command_failure("import_confirm", &error);
                    }
                    return Ok(summary);
                }
            };
        if !import_confirmed {
            summary.short_circuit_reason = Some(AlineStartupShortCircuitReason::ImportNotConfirmed);
            return Ok(summary);
        }

        summary
            .recently_imported_session_ids
            .push(session.session_id.clone());

        let generate_output = match runner
            .run(build_event_generate_command(&session.session_id))
            .await
        {
            Ok(output) => output,
            Err(error) => {
                summary.mark_command_failure("event_generate", &error);
                return Ok(summary);
            }
        };
        if let Err(error) = ensure_success_exit(&generate_output, "aline watcher event generate") {
            summary.mark_command_failure("event_generate", &error);
            return Ok(summary);
        }
        summary.generated_count += 1;
    }

    Ok(summary)
}

pub(super) fn aline_startup_state_path(data_dir: &Path) -> PathBuf {
    data_dir.join(STARTUP_STATE_FILE)
}

pub(super) async fn load_persisted_aline_startup_state(
    data_dir: &Path,
) -> PersistedAlineStartupState {
    let path = aline_startup_state_path(data_dir);
    let contents = match tokio::fs::read_to_string(&path).await {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return PersistedAlineStartupState::default();
        }
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "failed to read Aline startup state");
            return PersistedAlineStartupState::default();
        }
    };

    match serde_json::from_str(&contents) {
        Ok(state) => state,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "failed to parse Aline startup state");
            PersistedAlineStartupState::default()
        }
    }
}

pub(super) async fn persist_aline_startup_state(
    data_dir: &Path,
    state: &PersistedAlineStartupState,
) -> Result<()> {
    let path = aline_startup_state_path(data_dir);
    let payload = serde_json::to_string(state).context("serialize Aline startup state")?;
    tokio::fs::write(&path, payload)
        .await
        .with_context(|| format!("write Aline startup state to {}", path.display()))
}

pub(super) fn build_session_list_command(page: usize) -> StartupCommandSpec {
    StartupCommandSpec {
        program: "aline".to_string(),
        args: vec![
            "watcher".to_string(),
            "session".to_string(),
            "list".to_string(),
            "--json".to_string(),
            "--page".to_string(),
            page.to_string(),
            "--per-page".to_string(),
            "30".to_string(),
        ],
        timeout: WATCHER_COMMAND_TIMEOUT,
    }
}

pub(super) fn build_watcher_status_command() -> StartupCommandSpec {
    StartupCommandSpec {
        program: "aline".to_string(),
        args: vec!["watcher".to_string(), "status".to_string()],
        timeout: WATCHER_COMMAND_TIMEOUT,
    }
}

pub(super) fn build_watcher_start_command() -> StartupCommandSpec {
    StartupCommandSpec {
        program: "aline".to_string(),
        args: vec!["watcher".to_string(), "start".to_string()],
        timeout: WATCHER_COMMAND_TIMEOUT,
    }
}

pub(super) fn build_import_command(session: &AlineDiscoveredSession) -> StartupCommandSpec {
    StartupCommandSpec {
        program: "aline".to_string(),
        args: vec![
            "watcher".to_string(),
            "session".to_string(),
            "import".to_string(),
            session.session_id.clone(),
            "--sync".to_string(),
        ],
        timeout: IMPORT_TIMEOUT,
    }
}

pub(super) fn build_event_generate_command(session_id: &str) -> StartupCommandSpec {
    StartupCommandSpec {
        program: "aline".to_string(),
        args: vec![
            "watcher".to_string(),
            "event".to_string(),
            "generate".to_string(),
            session_id.to_string(),
        ],
        timeout: WATCHER_COMMAND_TIMEOUT,
    }
}

pub(super) fn build_session_show_command(session_id: &str) -> StartupCommandSpec {
    StartupCommandSpec {
        program: "aline".to_string(),
        args: vec![
            "watcher".to_string(),
            "session".to_string(),
            "show".to_string(),
            session_id.to_string(),
            "--json".to_string(),
        ],
        timeout: WATCHER_COMMAND_TIMEOUT,
    }
}

pub(super) fn parse_watcher_status(output: &str) -> WatcherStatus {
    let mut state = WatcherState::Unknown;
    let mut mode = None;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Some((label, value)) = trimmed.split_once(':') else {
            continue;
        };
        let label = label.trim().to_ascii_lowercase();
        let value = value.trim();
        let value_lower = value.to_ascii_lowercase();

        match label.as_str() {
            "watcher status" => {
                state = match value_lower.split_whitespace().next() {
                    Some("running") => WatcherState::Running,
                    Some("stopped") => WatcherState::Stopped,
                    _ => WatcherState::Unknown,
                };
            }
            "mode" => mode = Some(value.to_string()),
            _ => {}
        }
    }

    WatcherStatus { state, mode }
}

pub(super) fn parse_session_list_json(json: &str) -> Result<SessionListJson> {
    let parsed: SessionListJson =
        serde_json::from_str(json).context("failed to parse aline session list json")?;

    for (index, session) in parsed.sessions.iter().enumerate() {
        require_non_empty_field(index, "source", &session.source)?;
        require_non_empty_field(index, "project_name", &session.project_name)?;
        require_non_empty_field(index, "session_id", &session.session_id)?;
        require_non_empty_field(index, "created_at", &session.created_at)?;
        require_non_empty_field(index, "last_activity", &session.last_activity)?;
        require_non_empty_field(index, "session_file", &session.session_file)?;
        parse_timestamp_field("created_at", index, &session.created_at)?;
        parse_last_activity(&session.last_activity).ok_or_else(|| {
            anyhow!(
                "aline session row {index} has invalid last_activity {}",
                session.last_activity
            )
        })?;
    }

    Ok(parsed)
}

pub(super) fn repo_root_basename(repo_root: &Path) -> Option<&str> {
    let mut components = repo_root.components().peekable();
    let mut previous_normal = None::<&std::ffi::OsStr>;
    while let Some(component) = components.next() {
        let std::path::Component::Normal(value) = component else {
            continue;
        };
        if value == std::ffi::OsStr::new(".worktrees") {
            return previous_normal.and_then(|name| name.to_str());
        }
        previous_normal = Some(value);
    }
    repo_root.file_name()?.to_str()
}

pub(super) fn repo_root_matches_project_name(repo_root: &Path, project_name: &str) -> bool {
    matches!(repo_root_basename(repo_root), Some(name) if name == project_name)
}

pub(super) fn session_matches_repo(repo_root: &Path, session: &AlineDiscoveredSession) -> bool {
    let Some(project_path) = session
        .project_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return repo_root_matches_project_name(repo_root, &session.project_name);
    };

    if let (Some(current_identity), Some(session_identity)) = (
        canonical_repo_identity(repo_root),
        canonical_repo_identity(Path::new(project_path)),
    ) {
        return current_identity == session_identity;
    }

    let normalized_repo_root = normalize_repo_path_hint(repo_root);
    let normalized_project_path = normalize_repo_path_hint(Path::new(project_path));
    if normalized_repo_root == normalized_project_path {
        return true;
    }

    repo_root_matches_project_name(repo_root, &session.project_name)
}

pub(super) fn select_import_candidates(
    repo_root: &Path,
    sessions: &[AlineDiscoveredSession],
    now: DateTime<Utc>,
    policy: StartupSelectionPolicy,
) -> Vec<AlineDiscoveredSession> {
    let max_age = chrono::Duration::from_std(policy.recency_window)
        .expect("selection recency window should fit into chrono duration");
    let mut candidates: Vec<(DateTime<Utc>, AlineDiscoveredSession)> = sessions
        .iter()
        .filter_map(|session| {
            if session.status != "new" {
                return None;
            }

            if !session_matches_repo(repo_root, session) {
                return None;
            }
            if session.session_id.trim().is_empty() || session.session_file.trim().is_empty() {
                return None;
            }

            let last_activity = parse_last_activity(&session.last_activity)?;
            let age = now.signed_duration_since(last_activity);
            if age < chrono::Duration::zero() || age > max_age {
                return None;
            }

            Some((last_activity, session.clone()))
        })
        .collect();

    candidates.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.session_id.cmp(&right.1.session_id))
    });
    candidates.truncate(policy.max_candidates);
    candidates.into_iter().map(|(_, session)| session).collect()
}

async fn collect_discovered_sessions<R>(
    runner: &R,
    policy: StartupSelectionPolicy,
    started_at: Instant,
) -> Result<Vec<AlineDiscoveredSession>>
where
    R: StartupCommandRunner + ?Sized,
{
    let mut collected: Vec<AlineDiscoveredSession> = Vec::new();
    let mut session_indices: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for page in 1..=policy.max_pages {
        if !has_budget_for_command(started_at, WATCHER_COMMAND_TIMEOUT) {
            return Err(anyhow!(
                "Aline startup reconciliation budget exhausted before session list"
            ));
        }
        let output = runner.run(build_session_list_command(page)).await?;
        ensure_success_exit(&output, "aline watcher session list")?;
        let parsed = parse_session_list_json(&output.stdout)?;
        let is_empty = parsed.sessions.is_empty();
        for session in parsed.sessions {
            if let Some(existing_index) = session_indices.get(&session.session_id).copied() {
                let existing_last_activity =
                    parse_last_activity(&collected[existing_index].last_activity)
                        .expect("stored session last_activity should already be valid");
                let candidate_last_activity = parse_last_activity(&session.last_activity)
                    .expect("parsed session last_activity should already be valid");
                if candidate_last_activity > existing_last_activity {
                    collected[existing_index] = session;
                }
            } else {
                session_indices.insert(session.session_id.clone(), collected.len());
                collected.push(session);
            }
        }
        let reached_last_page = parsed
            .page
            .zip(parsed.total_pages)
            .is_some_and(|(page, total_pages)| page >= total_pages);
        if is_empty || parsed.has_more == Some(false) || reached_last_page {
            break;
        }
    }

    Ok(collected)
}

async fn confirm_import_ready<R>(
    runner: &R,
    session_id: &str,
    policy: StartupSelectionPolicy,
    started_at: Instant,
) -> Result<bool>
where
    R: StartupCommandRunner + ?Sized,
{
    for attempt in 0..TRACKED_POLL_MAX_ATTEMPTS {
        if !has_budget_for_command(started_at, WATCHER_COMMAND_TIMEOUT) {
            return Err(anyhow!(
                "Aline startup reconciliation budget exhausted before import confirmation"
            ));
        }
        let output = runner.run(build_session_show_command(session_id)).await?;
        if output.exit_code == 0 {
            return Ok(true);
        }

        if attempt + 1 < TRACKED_POLL_MAX_ATTEMPTS {
            if !has_budget_for_command(started_at, TRACKED_POLL_INTERVAL) {
                return Err(anyhow!(
                    "Aline startup reconciliation budget exhausted before import confirmation"
                ));
            }
            wait_for_next_confirmation_poll().await;
        }
    }

    Ok(false)
}

#[cfg(test)]
async fn wait_for_next_confirmation_poll() {
    tokio::task::yield_now().await;
}

#[cfg(not(test))]
async fn wait_for_next_confirmation_poll() {
    tokio::time::sleep(TRACKED_POLL_INTERVAL).await;
}

fn ensure_success_exit(output: &StartupCommandOutput, command_name: &str) -> Result<()> {
    if output.exit_code == 0 {
        return Ok(());
    }

    let stderr = output.stderr.trim();
    let stdout = output.stdout.trim();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        "no output"
    };
    Err(anyhow!(
        "{command_name} failed with exit code {}: {detail}",
        output.exit_code
    ))
}

fn require_non_empty_field(index: usize, field_name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(anyhow!(
            "aline session row {index} missing required {field_name}"
        ));
    }
    Ok(())
}

fn parse_last_activity(value: &str) -> Option<DateTime<Utc>> {
    parse_timestamp_value(value)
}

fn parse_timestamp_field(field_name: &str, index: usize, value: &str) -> Result<DateTime<Utc>> {
    parse_timestamp_value(value)
        .ok_or_else(|| anyhow!("aline session row {index} has invalid {field_name} {value}"))
}

fn canonical_repo_identity(path: &Path) -> Option<String> {
    let show_toplevel = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(path)
        .output()
        .ok()?;
    if !show_toplevel.status.success() {
        return None;
    }
    let normalized_root = String::from_utf8_lossy(&show_toplevel.stdout)
        .trim()
        .to_string();
    if normalized_root.is_empty() {
        return None;
    }

    let common_dir_output = std::process::Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(&normalized_root)
        .output()
        .ok()?;
    if !common_dir_output.status.success() {
        return Some(normalized_root);
    }

    let common_dir = String::from_utf8_lossy(&common_dir_output.stdout)
        .trim()
        .to_string();
    if common_dir.is_empty() {
        return Some(normalized_root);
    }

    let resolved_common_dir = Path::new(&normalized_root).join(common_dir);
    if let Ok(canonical_common_dir) = std::fs::canonicalize(&resolved_common_dir) {
        if canonical_common_dir
            .file_name()
            .is_some_and(|name| name == std::ffi::OsStr::new(".git"))
        {
            if let Some(parent) = canonical_common_dir.parent() {
                return Some(parent.to_string_lossy().to_string());
            }
        }
        return Some(canonical_common_dir.to_string_lossy().to_string());
    }

    Some(normalized_root)
}

fn normalize_repo_path_hint(path: &Path) -> String {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        if let std::path::Component::Normal(value) = component {
            if value == std::ffi::OsStr::new(".worktrees") {
                break;
            }
        }
        normalized.push(component.as_os_str());
    }
    normalized.to_string_lossy().to_string()
}

fn parse_timestamp_value(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(|timestamp| DateTime::<Utc>::from_naive_utc_and_offset(timestamp, Utc))
        })
}

fn has_budget_for_next_import(started_at: Instant) -> bool {
    let required_budget = IMPORT_TIMEOUT
        .saturating_add(WATCHER_COMMAND_TIMEOUT)
        .saturating_add(WATCHER_COMMAND_TIMEOUT);
    RECONCILIATION_BUDGET.saturating_sub(started_at.elapsed()) >= required_budget
}

fn has_budget_for_command(started_at: Instant, next_command_budget: Duration) -> bool {
    RECONCILIATION_BUDGET.saturating_sub(started_at.elapsed()) >= next_command_budget
}

#[cfg(test)]
#[path = "tests/aline_startup/mod.rs"]
mod tests;
