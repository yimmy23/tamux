use crate::agent::liveness::state_layers::CheckpointType;
use crate::agent::types::{
    AgentTask, AgentTaskLogEntry, GoalRun, GoalRunEvent, GoalRunStatus, GoalRunStep,
    GoalRunStepKind, GoalRunStepStatus, TaskLogLevel, TaskPriority, TaskStatus,
};
use amux_protocol::{
    AgentDbMessage, AgentDbThread, AgentEventRow, CommandLogEntry, HistorySearchHit,
    SnapshotIndexEntry, TranscriptIndexEntry, WormChainTip,
};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use tokio_rusqlite;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Helper trait to convert any error into `tokio_rusqlite::Error` inside `.call()` closures.
trait IntoCallError<T> {
    fn call_err(self) -> std::result::Result<T, tokio_rusqlite::Error>;
}

impl<T, E: std::error::Error + Send + Sync + 'static> IntoCallError<T> for std::result::Result<T, E> {
    fn call_err(self) -> std::result::Result<T, tokio_rusqlite::Error> {
        self.map_err(|e| tokio_rusqlite::Error::Other(Box::new(e)))
    }
}
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use sysinfo::System;

/// Result of verifying a single WORM ledger file's hash-chain integrity.
pub struct WormIntegrityResult {
    pub kind: String,
    pub total_entries: usize,
    pub valid: bool,
    pub first_invalid_seq: Option<usize>,
    pub message: String,
}

#[derive(Clone)]
pub struct HistoryStore {
    pub(crate) conn: tokio_rusqlite::Connection,
    skill_dir: PathBuf,
    telemetry_dir: PathBuf,
    worm_dir: PathBuf,
}

pub struct ManagedHistoryRecord {
    pub execution_id: String,
    pub session_id: String,
    pub workspace_id: Option<String>,
    pub command: String,
    pub rationale: String,
    pub source: String,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub snapshot_path: Option<String>,
}

pub struct MemoryProvenanceRecord<'a> {
    pub id: &'a str,
    pub target: &'a str,
    pub mode: &'a str,
    pub source_kind: &'a str,
    pub content: &'a str,
    pub fact_keys: &'a [String],
    pub thread_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub goal_run_id: Option<&'a str>,
    pub created_at: u64,
}

#[derive(Debug, Clone)]
pub struct CollaborationSessionRow {
    pub parent_task_id: String,
    pub session_json: String,
    pub updated_at: u64,
}

/// Row type for heartbeat_history table. Per D-12.
#[derive(Debug, Clone)]
pub struct HeartbeatHistoryRow {
    pub id: String,
    pub cycle_timestamp: i64,
    pub checks_json: String,
    pub synthesis_json: Option<String>,
    pub actionable: bool,
    pub digest_text: Option<String>,
    pub llm_tokens_used: i64,
    pub duration_ms: i64,
    pub status: String,
}

/// Row type for action_audit table. Per D-06/TRNS-03.
#[derive(Debug, Clone)]
pub struct AuditEntryRow {
    pub id: String,
    pub timestamp: i64,
    pub action_type: String,
    pub summary: String,
    pub explanation: Option<String>,
    pub confidence: Option<f64>,
    pub confidence_band: Option<String>,
    pub causal_trace_id: Option<String>,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub raw_data_json: Option<String>,
}

/// Row type for memory_tombstones table (Phase 5 — consolidation).
#[derive(Debug, Clone)]
pub struct MemoryTombstoneRow {
    pub id: String,
    pub target: String,
    pub original_content: String,
    pub fact_key: Option<String>,
    pub replaced_by: Option<String>,
    pub replaced_at: i64,
    pub source_kind: String,
    pub provenance_id: Option<String>,
    pub created_at: i64,
}

/// Row type for execution_traces query results (Phase 5 — consolidation).
#[derive(Debug, Clone)]
pub struct ExecutionTraceRow {
    pub id: String,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub task_type: Option<String>,
    pub outcome: Option<String>,
    pub quality_score: Option<f64>,
    pub tool_sequence_json: Option<String>,
    pub metrics_json: Option<String>,
    pub duration_ms: Option<i64>,
    pub tokens_used: Option<i64>,
    pub created_at: i64,
}

/// Row type for context_archive query results (Phase 5 — cross-session continuity).
#[derive(Debug, Clone)]
pub struct ContextArchiveRow {
    pub id: String,
    pub thread_id: String,
    pub original_role: Option<String>,
    pub compressed_content: String,
    pub summary: Option<String>,
    pub relevance_score: f64,
    pub token_count_original: i64,
    pub token_count_compressed: i64,
    pub metadata_json: Option<String>,
    pub archived_at: i64,
    pub last_accessed_at: Option<i64>,
}

pub struct ProvenanceEventRecord<'a> {
    pub event_type: &'a str,
    pub summary: &'a str,
    pub details: &'a serde_json::Value,
    pub agent_id: &'a str,
    pub goal_run_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub thread_id: Option<&'a str>,
    pub approval_id: Option<&'a str>,
    pub causal_trace_id: Option<&'a str>,
    pub compliance_mode: &'a str,
    pub sign: bool,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillVariantRecord {
    pub variant_id: String,
    pub skill_name: String,
    pub variant_name: String,
    pub relative_path: String,
    pub parent_variant_id: Option<String>,
    pub version: String,
    pub context_tags: Vec<String>,
    pub use_count: u32,
    pub success_count: u32,
    pub failure_count: u32,
    pub status: String,
    pub last_used_at: Option<u64>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl SkillVariantRecord {
    pub fn success_rate(&self) -> f64 {
        let attempts = self.success_count + self.failure_count;
        if attempts == 0 {
            0.5
        } else {
            self.success_count as f64 / attempts as f64
        }
    }

    pub fn is_canonical(&self) -> bool {
        self.variant_name == "canonical"
    }
}

const SKILL_ARCHIVE_MIN_USES: u32 = 3;
const SKILL_ARCHIVE_MAX_IDLE_SECS: u64 = 90 * 24 * 60 * 60;
const SKILL_ARCHIVE_SUCCESS_RATE_THRESHOLD: f64 = 0.30;
const SKILL_PROMOTION_MIN_USES: u32 = 3;
const SKILL_PROMOTION_MIN_SUCCESS_COUNT: u32 = 2;
const SKILL_PROMOTION_SUCCESS_RATE_THRESHOLD: f64 = 0.80;
const SKILL_PROMOTION_MARGIN: f64 = 0.15;
const SKILL_MERGE_MIN_USES: u32 = 4;
const SKILL_MERGE_SUCCESS_RATE_THRESHOLD: f64 = 0.85;
const SKILL_MERGE_SIMILARITY_THRESHOLD: f64 = 0.40;

pub struct SkillVariantConsultationRecord<'a> {
    pub usage_id: &'a str,
    pub variant_id: &'a str,
    pub thread_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub goal_run_id: Option<&'a str>,
    pub context_tags: &'a [String],
    pub consulted_at: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryProvenanceReportEntry {
    pub id: String,
    pub target: String,
    pub mode: String,
    pub source_kind: String,
    pub content: String,
    pub fact_keys: Vec<String>,
    pub thread_id: Option<String>,
    pub task_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub created_at: u64,
    pub age_days: f64,
    pub confidence: f64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryProvenanceReport {
    pub total_entries: usize,
    pub target_filter: Option<String>,
    pub summary_by_target: BTreeMap<String, usize>,
    pub summary_by_source: BTreeMap<String, usize>,
    pub summary_by_status: BTreeMap<String, usize>,
    pub entries: Vec<MemoryProvenanceReportEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceLogEntry {
    pub sequence: u64,
    pub timestamp: u64,
    pub event_type: String,
    pub summary: String,
    pub details: serde_json::Value,
    pub prev_hash: String,
    pub entry_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    pub agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causal_trace_id: Option<String>,
    pub compliance_mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProvenanceReportEntry {
    pub sequence: u64,
    pub timestamp: u64,
    pub event_type: String,
    pub summary: String,
    pub agent_id: String,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub thread_id: Option<String>,
    pub approval_id: Option<String>,
    pub causal_trace_id: Option<String>,
    pub compliance_mode: String,
    pub hash_valid: bool,
    pub signature_valid: bool,
    pub chain_valid: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProvenanceReport {
    pub total_entries: usize,
    pub signed_entries: usize,
    pub valid_hash_entries: usize,
    pub valid_signature_entries: usize,
    pub valid_chain_entries: usize,
    pub summary_by_event: BTreeMap<String, usize>,
    pub entries: Vec<ProvenanceReportEntry>,
}

#[derive(Debug, Clone)]
pub struct SubagentMetrics {
    pub task_id: String,
    pub parent_task_id: Option<String>,
    pub thread_id: Option<String>,
    pub tool_calls_total: i64,
    pub tool_calls_succeeded: i64,
    pub tool_calls_failed: i64,
    pub tokens_consumed: i64,
    pub context_budget_tokens: Option<i64>,
    pub progress_rate: f64,
    pub last_progress_at: Option<u64>,
    pub stuck_score: f64,
    pub health_state: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Default, Clone)]
pub struct AgentMessagePatch {
    pub content: Option<String>,
    pub reasoning: Option<Option<String>>,
    pub tool_calls_json: Option<Option<String>>,
    pub metadata_json: Option<Option<String>>,
    pub provider: Option<Option<String>>,
    pub model: Option<Option<String>>,
    pub input_tokens: Option<Option<i64>>,
    pub output_tokens: Option<Option<i64>>,
    pub total_tokens: Option<Option<i64>>,
}

impl HistoryStore {
    pub fn data_dir(&self) -> &Path {
        self.skill_dir
            .parent()
            .unwrap_or(self.skill_dir.as_path())
    }

    pub async fn new() -> Result<Self> {
        let base = amux_protocol::ensure_amux_data_dir()?;
        let history_dir = base.join("history");
        let skill_dir = base.join("skills").join("generated");
        let telemetry_dir = base.join("semantic-logs");
        let worm_dir = telemetry_dir.join("worm");

        std::fs::create_dir_all(&history_dir)?;
        std::fs::create_dir_all(&skill_dir)?;
        std::fs::create_dir_all(&telemetry_dir)?;
        std::fs::create_dir_all(&worm_dir)?;

        let db_path = history_dir.join("command-history.db");
        let conn = tokio_rusqlite::Connection::open(&db_path)
            .await
            .context("failed to open SQLite connection via tokio-rusqlite")?;

        // Apply WAL pragmas on the connection's background thread (FOUN-01, FOUN-06, per D-03)
        // busy_timeout=5000 also satisfies D-13: SQLite waits up to 5s before SQLITE_BUSY
        conn.call(|conn| {
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "synchronous", "NORMAL")?;
            conn.pragma_update(None, "foreign_keys", "ON")?;
            conn.pragma_update(None, "wal_autocheckpoint", "1000")?;
            conn.pragma_update(None, "busy_timeout", "5000")?;
            Ok(())
        })
        .await
        .context("failed to apply WAL pragmas")?;

        let store = Self {
            conn,
            skill_dir,
            telemetry_dir,
            worm_dir,
        };
        store.init_schema().await?;
        Ok(store)
    }

    pub async fn list_agent_config_items(&self) -> Result<Vec<(String, serde_json::Value)>> {
        self.conn.call(|conn| {
            let mut stmt = conn.prepare(
                "SELECT key_path, value_json FROM agent_config_items ORDER BY length(key_path) ASC, key_path ASC",
            )?;
            let rows = stmt.query_map([], |row| {
                let key_path = row.get::<_, String>(0)?;
                let value_json = row.get::<_, String>(1)?;
                Ok((key_path, value_json))
            })?;
            let mut items = Vec::new();
            for row in rows {
                let (key_path, value_json) = row?;
                let value = serde_json::from_str::<serde_json::Value>(&value_json)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                items.push((key_path, value));
            }
            Ok(items)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn replace_agent_config_items(&self, items: &[(String, serde_json::Value)]) -> Result<()> {
        let items: Vec<(String, String)> = items
            .iter()
            .map(|(k, v)| Ok((k.clone(), serde_json::to_string(v)?)))
            .collect::<Result<Vec<_>>>()?;
        let items = items.clone();
        self.conn.call(move |conn| {
            let transaction = conn.unchecked_transaction()?;
            transaction.execute("DELETE FROM agent_config_items", [])?;
            let now = now_ts() as i64;
            for (key_path, value_json) in &items {
                transaction.execute(
                    "INSERT OR REPLACE INTO agent_config_items (key_path, value_json, updated_at) VALUES (?1, ?2, ?3)",
                    params![key_path, value_json, now],
                )?;
            }
            transaction.commit()?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_agent_config_item(
        &self,
        key_path: &str,
        value: &serde_json::Value,
    ) -> Result<()> {
        let key_path = key_path.to_string();
        let value_json = serde_json::to_string(value)?;
        let value = value.clone();
        self.conn.call(move |conn| {
            let transaction = conn.unchecked_transaction()?;
            let prefix = format!("{key_path}/%");
            let now = now_ts() as i64;
            transaction.execute(
                "DELETE FROM agent_config_items \
                 WHERE key_path = ?1 OR key_path LIKE ?2 OR ?1 LIKE key_path || '/%'",
                params![key_path, prefix],
            )?;
            transaction.execute(
                "INSERT OR REPLACE INTO agent_config_items (key_path, value_json, updated_at) VALUES (?1, ?2, ?3)",
                params![key_path, value_json.clone(), now],
            )?;
            transaction.execute(
                "INSERT INTO agent_config_updates (id, key_path, value_json, updated_at) VALUES (?1, ?2, ?3, ?4)",
                params![uuid::Uuid::new_v4().to_string(), key_path, value_json, now],
            )?;
            transaction.commit()?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    #[cfg(test)]
    pub(crate) async fn new_test_store(root: &Path) -> Result<Self> {
        let history_dir = root.join("history");
        let skill_dir = root.join("skills").join("generated");
        let telemetry_dir = root.join("semantic-logs");
        let worm_dir = telemetry_dir.join("worm");

        std::fs::create_dir_all(&history_dir)?;
        std::fs::create_dir_all(&skill_dir)?;
        std::fs::create_dir_all(&telemetry_dir)?;
        std::fs::create_dir_all(&worm_dir)?;

        let db_path = history_dir.join("command-history.db");
        let conn = tokio_rusqlite::Connection::open(&db_path).await?;
        conn.call(|conn| {
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "synchronous", "NORMAL")?;
            conn.pragma_update(None, "foreign_keys", "ON")?;
            conn.pragma_update(None, "wal_autocheckpoint", "1000")?;
            conn.pragma_update(None, "busy_timeout", "5000")?;
            Ok(())
        })
        .await?;

        let store = Self {
            conn,
            skill_dir,
            telemetry_dir,
            worm_dir,
        };
        store.init_schema().await?;
        Ok(store)
    }

    pub async fn record_managed_finish(&self, record: &ManagedHistoryRecord) -> Result<()> {
        let timestamp = now_ts() as i64;
        let excerpt = format!(
            "exit={:?} duration_ms={:?} snapshot={} rationale={}",
            record.exit_code,
            record.duration_ms,
            record.snapshot_path.as_deref().unwrap_or("none"),
            record.rationale
        );

        let execution_id = record.execution_id.clone();
        let command = record.command.clone();
        let rationale = record.rationale.clone();
        let snapshot_path = record.snapshot_path.clone();
        let excerpt_clone = excerpt.clone();
        let record = record.clone();
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO history_entries (id, kind, title, excerpt, content, path, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    execution_id,
                    "managed-command",
                    command,
                    excerpt_clone,
                    format!("{}\n{}", command, rationale),
                    snapshot_path,
                    timestamp,
                ],
            )?;
            conn.execute(
                "INSERT OR REPLACE INTO history_fts (id, title, excerpt, content) VALUES (?1, ?2, ?3, ?4)",
                params![execution_id, command, excerpt_clone, rationale],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

        self.append_telemetry(
            "operational",
            json!({
                "timestamp": timestamp,
                "execution_id": record.execution_id,
                "session_id": record.session_id,
                "workspace_id": record.workspace_id,
                "command": record.command,
                "exit_code": record.exit_code,
                "duration_ms": record.duration_ms,
                "snapshot": record.snapshot_path,
            }),
        ).await?;
        self.append_telemetry(
            "cognitive",
            json!({
                "timestamp": timestamp,
                "execution_id": record.execution_id,
                "source": record.source,
                "rationale": record.rationale,
            }),
        ).await?;

        let mut system = System::new_all();
        system.refresh_memory();
        system.refresh_cpu();
        self.append_telemetry(
            "contextual",
            json!({
                "timestamp": timestamp,
                "execution_id": record.execution_id,
                "total_memory": system.total_memory(),
                "used_memory": system.used_memory(),
                "cpu_usage": system.global_cpu_info().cpu_usage(),
            }),
        ).await?;

        Ok(())
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<(String, Vec<HistorySearchHit>)> {
        let query = query.to_string();
        let query = query.to_string();
        self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT history_entries.id, kind, title, excerpt, path, timestamp, bm25(history_fts) \
                 FROM history_fts JOIN history_entries ON history_entries.id = history_fts.id \
                 WHERE history_fts MATCH ?1 ORDER BY bm25(history_fts) LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![query, limit as i64], |row| {
                Ok(HistorySearchHit {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    title: row.get(2)?,
                    excerpt: row.get(3)?,
                    path: row.get(4)?,
                    timestamp: row.get::<_, i64>(5)? as u64,
                    score: row.get(6)?,
                })
            })?;

            let hits = rows.filter_map(|row| row.ok()).collect::<Vec<_>>();
            let summary = if hits.is_empty() {
                format!("No prior runs matched '{query}'.")
            } else {
                format!("Found {} historical matches for '{query}'.", hits.len())
            };
            Ok((summary, hits))
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn generate_skill(
        &self,
        query: Option<&str>,
        title: Option<&str>,
    ) -> Result<(String, String)> {
        let title = title.unwrap_or("Recovered Workflow").trim();
        let (summary, hits) = self
            .search(query.unwrap_or("*"), 8)
            .await
            .unwrap_or_else(|_| ("No history available.".to_string(), Vec::new()));
        let safe_name = title
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_ascii_lowercase();
        let path = self.skill_dir.join(format!(
            "{}.md",
            if safe_name.is_empty() {
                "recovered-workflow"
            } else {
                &safe_name
            }
        ));
        let mut body = format!(
            "# {}\n\n## Summary\n{}\n\n## Retrieved Steps\n",
            title, summary
        );
        for hit in &hits {
            body.push_str(&format!("- {}\n", hit.title));
            body.push_str(&format!("  {}\n", hit.excerpt));
        }
        if hits.is_empty() {
            body.push_str("- No matching executions were available.\n");
        }
        std::fs::write(&path, body)
            .with_context(|| format!("failed to write {}", path.display()))?;
        self.register_skill_document(&path).await?;
        Ok((title.to_string(), path.to_string_lossy().into_owned()))
    }

    pub async fn register_skill_document(&self, path: &Path) -> Result<SkillVariantRecord> {
        let skills_root = self.skills_root();
        let canonical = std::fs::canonicalize(path)
            .with_context(|| format!("failed to access skill document {}", path.display()))?;
        if !canonical.starts_with(&skills_root) {
            anyhow::bail!(
                "skill document {} must stay inside {}",
                canonical.display(),
                skills_root.display()
            );
        }

        let relative_path = canonical
            .strip_prefix(&skills_root)
            .unwrap_or(canonical.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let content = std::fs::read_to_string(&canonical)
            .with_context(|| format!("failed to read skill document {}", canonical.display()))?;
        let metadata = derive_skill_metadata(&relative_path, &content);
        let now = now_ts();
        let context_tags_json = serde_json::to_string(&metadata.context_tags)?;
        let skill_name = metadata.skill_name.clone();
        let variant_name = metadata.variant_name.clone();

        let path = path.clone();
        let variant_id = self.conn.call(move |conn| {
            let existing: Option<SkillVariantRecord> = conn
                .query_row(
                    "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at \
                     FROM skill_variants WHERE relative_path = ?1",
                    params![relative_path],
                    map_skill_variant_row,
                )
                .optional()?;

            let variant_id = existing
                .as_ref()
                .map(|record| record.variant_id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let version = existing
                .as_ref()
                .map(|record| record.version.clone())
                .unwrap_or_else(|| {
                    let version_num: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM skill_variants WHERE skill_name = ?1",
                            params![skill_name.as_str()],
                            |row| row.get(0),
                        )
                        .unwrap_or(0);
                    format!("v{}.0", version_num + 1)
                });
            let parent_variant_id: Option<String> = if variant_name == "canonical" {
                None
            } else {
                conn.query_row(
                    "SELECT variant_id FROM skill_variants WHERE skill_name = ?1 AND variant_name = 'canonical' LIMIT 1",
                    params![skill_name.as_str()],
                    |row| row.get(0),
                )
                .optional()?
            };
            let created_at = existing
                .as_ref()
                .map(|record| record.created_at)
                .unwrap_or(now);
            let last_used_at = existing.as_ref().and_then(|record| record.last_used_at);
            let use_count = existing
                .as_ref()
                .map(|record| record.use_count)
                .unwrap_or(0);
            let success_count = existing
                .as_ref()
                .map(|record| record.success_count)
                .unwrap_or(0);
            let failure_count = existing
                .as_ref()
                .map(|record| record.failure_count)
                .unwrap_or(0);
            let status = existing
                .as_ref()
                .map(|record| record.status.clone())
                .unwrap_or_else(|| "active".to_string());

            conn.execute(
                "INSERT OR REPLACE INTO skill_variants \
                 (variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    variant_id,
                    skill_name,
                    variant_name,
                    relative_path,
                    parent_variant_id,
                    version,
                    context_tags_json,
                    use_count as i64,
                    success_count as i64,
                    failure_count as i64,
                    status,
                    last_used_at.map(|value| value as i64),
                    created_at as i64,
                    now as i64,
                ],
            )?;

            Ok(variant_id)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

        self.rebalance_skill_variants(&metadata.skill_name).await?;

        let vid = variant_id.clone();
        self.conn.call(move |conn| {
            conn.query_row(
                "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at \
                 FROM skill_variants WHERE variant_id = ?1",
                params![vid],
                map_skill_variant_row,
            )
            .map_err(Into::into)
        }).await.context("failed to load rebalanced skill variant")
    }

    pub async fn list_skill_variants(
        &self,
        query: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SkillVariantRecord>> {
        let normalized_query = query
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());
        let limit = limit.clamp(1, 200) as i64;

        let query = query.map(str::to_string);
        self.conn.call(move |conn| {
            let mut variants = if let Some(query) = normalized_query.as_deref() {
                let like = format!("%{query}%");
                let mut stmt = conn.prepare(
                    "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at \
                     FROM skill_variants \
                     WHERE lower(skill_name) LIKE ?1 OR lower(variant_name) LIKE ?1 OR lower(relative_path) LIKE ?1 OR lower(context_tags_json) LIKE ?1 \
                     ORDER BY updated_at DESC LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![like, limit], map_skill_variant_row)?;
                rows.filter_map(|row| row.ok()).collect::<Vec<_>>()
            } else {
                let mut stmt = conn.prepare(
                    "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at \
                     FROM skill_variants \
                     ORDER BY updated_at DESC LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit], map_skill_variant_row)?;
                rows.filter_map(|row| row.ok()).collect::<Vec<_>>()
            };

            variants.sort_by(|left, right| compare_skill_variants(left, right, &[]));
            Ok(variants)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Update the maturity status of a skill variant and bump `updated_at`.
    pub async fn update_skill_variant_status(
        &self,
        variant_id: &str,
        new_status: &str,
    ) -> Result<()> {
        let variant_id = variant_id.to_string();
        let new_status = new_status.to_string();
        self.conn
            .call(move |conn| {
                let now = now_ts() as i64;
                conn.execute(
                    "UPDATE skill_variants SET status = ?2, updated_at = ?3 WHERE variant_id = ?1",
                    params![variant_id, new_status, now],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Retrieve a single skill variant by its `variant_id`.
    pub async fn get_skill_variant(
        &self,
        variant_id: &str,
    ) -> Result<Option<SkillVariantRecord>> {
        let variant_id = variant_id.to_string();
        self.conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at \
                     FROM skill_variants WHERE variant_id = ?1",
                    params![variant_id],
                    map_skill_variant_row,
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn resolve_skill_variant(
        &self,
        skill: &str,
        context_tags: &[String],
    ) -> Result<Option<SkillVariantRecord>> {
        let normalized = normalize_skill_lookup(skill);
        if normalized.is_empty() {
            return Ok(None);
        }
        let context_tags = context_tags.to_vec();

        let skill = skill.to_string();
        self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at \
                 FROM skill_variants",
            )?;
            let rows = stmt.query_map([], map_skill_variant_row)?;
            let mut candidates = rows
                .filter_map(|row| row.ok())
                .filter(|record| skill_variant_matches(record, &normalized))
                .filter(|record| record.status != "archived" && record.status != "merged")
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                return Ok(None);
            }
            candidates.sort_by(|left, right| compare_skill_variants(left, right, &context_tags));
            Ok(candidates.into_iter().next())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn record_skill_variant_use(&self, variant_id: &str, outcome: Option<bool>) -> Result<()> {
        let variant_id = variant_id.to_string();
        let variant_id = variant_id.to_string();
        self.conn.call(move |conn| {
            let now = now_ts() as i64;
            match outcome {
                Some(true) => {
                    conn.execute(
                        "UPDATE skill_variants \
                         SET use_count = use_count + 1, success_count = success_count + 1, last_used_at = ?2, updated_at = ?2 \
                         WHERE variant_id = ?1",
                        params![variant_id, now],
                    )?;
                }
                Some(false) => {
                    conn.execute(
                        "UPDATE skill_variants \
                         SET use_count = use_count + 1, failure_count = failure_count + 1, last_used_at = ?2, updated_at = ?2 \
                         WHERE variant_id = ?1",
                        params![variant_id, now],
                    )?;
                }
                None => {
                    conn.execute(
                        "UPDATE skill_variants \
                         SET use_count = use_count + 1, last_used_at = ?2, updated_at = ?2 \
                         WHERE variant_id = ?1",
                        params![variant_id, now],
                    )?;
                }
            }
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn record_skill_variant_consultation(
        &self,
        record: &SkillVariantConsultationRecord<'_>,
    ) -> Result<()> {
        let now = record.consulted_at as i64;
        let context_tags_json = serde_json::to_string(record.context_tags)?;
        let usage_id = record.usage_id.to_string();
        let variant_id = record.variant_id.to_string();
        let thread_id = record.thread_id.map(str::to_string);
        let task_id = record.task_id.map(str::to_string);
        let goal_run_id = record.goal_run_id.map(str::to_string);
        let context_tags_owned: Vec<String> = record.context_tags.to_vec();

        let record = record.clone();
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO skill_variant_usage \
                 (usage_id, variant_id, thread_id, task_id, goal_run_id, context_tags_json, consulted_at, resolved_at, outcome) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL)",
                params![
                    usage_id,
                    variant_id,
                    thread_id,
                    task_id,
                    goal_run_id,
                    context_tags_json,
                    now,
                ],
            )?;
            conn.execute(
                "UPDATE skill_variants \
                 SET use_count = use_count + 1, last_used_at = ?2, updated_at = ?2 \
                 WHERE variant_id = ?1",
                params![variant_id, now],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        self.append_telemetry(
            "cognitive",
            json!({
                "timestamp": now,
                "kind": "skill_variant_consulted",
                "variant_id": record.variant_id,
                "thread_id": record.thread_id,
                "task_id": record.task_id,
                "goal_run_id": record.goal_run_id,
                "context_tags": context_tags_owned,
            }),
        ).await?;
        Ok(())
    }

    pub async fn settle_skill_variant_usage(
        &self,
        thread_id: Option<&str>,
        task_id: Option<&str>,
        goal_run_id: Option<&str>,
        outcome: &str,
    ) -> Result<usize> {
        let normalized_outcome = outcome.trim().to_ascii_lowercase();
        if !matches!(
            normalized_outcome.as_str(),
            "success" | "failure" | "cancelled"
        ) {
            anyhow::bail!("invalid skill usage outcome '{outcome}'");
        }

        let thread_id_owned = thread_id.map(str::to_string);
        let task_id_owned = task_id.map(str::to_string);
        let goal_run_id_owned = goal_run_id.map(str::to_string);
        let outcome_clone = normalized_outcome.clone();

        let thread_id = thread_id.map(str::to_string);
        let task_id = task_id.map(str::to_string);
        let goal_run_id = goal_run_id.map(str::to_string);
        let outcome = outcome.to_string();
        let (pending_len, skill_names) = self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT usage_id, variant_id FROM skill_variant_usage \
                 WHERE resolved_at IS NULL AND ( \
                    (?1 IS NOT NULL AND task_id = ?1) OR \
                    (?2 IS NOT NULL AND goal_run_id = ?2) OR \
                    (?3 IS NOT NULL AND task_id IS NULL AND goal_run_id IS NULL AND thread_id = ?3) \
                 )",
            )?;
            let rows = stmt.query_map(params![task_id_owned, goal_run_id_owned, thread_id_owned], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let pending = rows.filter_map(|row| row.ok()).collect::<Vec<_>>();
            if pending.is_empty() {
                return Ok((0usize, BTreeSet::<String>::new()));
            }

            let resolved_at = now_ts() as i64;
            let variant_ids = pending
                .iter()
                .map(|(_, variant_id)| variant_id.clone())
                .collect::<BTreeSet<_>>();
            let mut success_counts = BTreeMap::<String, usize>::new();
            let mut failure_counts = BTreeMap::<String, usize>::new();
            for (usage_id, variant_id) in &pending {
                conn.execute(
                    "UPDATE skill_variant_usage SET resolved_at = ?2, outcome = ?3 WHERE usage_id = ?1",
                    params![usage_id, resolved_at, outcome_clone.as_str()],
                )?;
                if outcome_clone == "success" {
                    *success_counts.entry(variant_id.clone()).or_default() += 1;
                } else {
                    *failure_counts.entry(variant_id.clone()).or_default() += 1;
                }
            }

            for (variant_id, count) in success_counts {
                conn.execute(
                    "UPDATE skill_variants \
                     SET success_count = success_count + ?2, updated_at = ?3 \
                     WHERE variant_id = ?1",
                    params![variant_id, count as i64, resolved_at],
                )?;
            }
            for (variant_id, count) in failure_counts {
                conn.execute(
                    "UPDATE skill_variants \
                     SET failure_count = failure_count + ?2, updated_at = ?3 \
                     WHERE variant_id = ?1",
                    params![variant_id, count as i64, resolved_at],
                )?;
            }

            let skill_names = variant_ids
                .into_iter()
                .filter_map(|variant_id| {
                    conn.query_row(
                        "SELECT skill_name FROM skill_variants WHERE variant_id = ?1",
                        params![variant_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .ok()
                    .flatten()
                })
                .collect::<BTreeSet<_>>();

            Ok((pending.len(), skill_names))
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

        if pending_len == 0 {
            return Ok(0);
        }

        self.append_telemetry(
            "cognitive",
            json!({
                "timestamp": now_ts() as i64,
                "kind": "skill_variant_settled",
                "thread_id": thread_id,
                "task_id": task_id,
                "goal_run_id": goal_run_id,
                "outcome": normalized_outcome,
                "count": pending_len,
            }),
        ).await?;

        for skill_name in skill_names {
            let _ = self.rebalance_skill_variants(&skill_name).await;
            if normalized_outcome == "success" {
                let _ = self.maybe_branch_skill_variants(&skill_name).await;
                let _ = self.maybe_merge_skill_variants(&skill_name).await;
            }
        }
        Ok(pending_len)
    }

    pub async fn rebalance_skill_variants(&self, skill_name: &str) -> Result<Vec<SkillVariantRecord>> {
        let skill_name = skill_name.to_string();
        let skill_name = skill_name.to_string();
        self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at \
                 FROM skill_variants WHERE skill_name = ?1",
            )?;
            let rows = stmt.query_map(params![skill_name], map_skill_variant_row)?;
            let mut variants = rows.filter_map(|row| row.ok()).collect::<Vec<_>>();
            if variants.is_empty() {
                return Ok(Vec::new());
            }

            let now = now_ts();
            let canonical = variants
                .iter()
                .find(|variant| variant.is_canonical())
                .cloned();
            let canonical_success_rate = canonical
                .as_ref()
                .map(SkillVariantRecord::success_rate)
                .unwrap_or(0.0);
            let promoted_variant_id = variants
                .iter()
                .filter(|variant| !variant.is_canonical())
                .filter(|variant| {
                    variant.use_count >= SKILL_PROMOTION_MIN_USES
                        && variant.success_count >= SKILL_PROMOTION_MIN_SUCCESS_COUNT
                        && variant.success_rate() >= SKILL_PROMOTION_SUCCESS_RATE_THRESHOLD
                        && variant.success_rate() > canonical_success_rate + SKILL_PROMOTION_MARGIN
                })
                .max_by(|left, right| compare_skill_variants(left, right, &[]))
                .map(|variant| variant.variant_id.clone());

            for variant in &mut variants {
                let next_status =
                    rebalance_skill_variant_status(variant, promoted_variant_id.as_deref(), now);
                if next_status != variant.status {
                    conn.execute(
                        "UPDATE skill_variants SET status = ?2, updated_at = ?3 WHERE variant_id = ?1",
                        params![variant.variant_id, next_status, now as i64],
                    )?;
                    variant.status = next_status.to_string();
                    variant.updated_at = now;
                }
            }

            variants.sort_by(|left, right| compare_skill_variants(left, right, &[]));
            Ok(variants)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn maybe_branch_skill_variants(&self, skill_name: &str) -> Result<Vec<SkillVariantRecord>> {
        let skill_name_owned = skill_name.to_string();
        let skill_name = skill_name.to_string();
        let candidates = self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT skill_variants.variant_id, skill_variants.relative_path, skill_variants.context_tags_json, skill_variant_usage.context_tags_json \
                 FROM skill_variant_usage \
                 JOIN skill_variants ON skill_variants.variant_id = skill_variant_usage.variant_id \
                 WHERE skill_variants.skill_name = ?1 AND skill_variant_usage.outcome = 'success'",
            )?;
            let rows = stmt.query_map(params![skill_name_owned], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?;

            let mut candidates = BTreeMap::<(String, String), BranchCandidate>::new();
            for row in rows.filter_map(|row| row.ok()) {
                let (variant_id, relative_path, variant_tags_json, usage_tags_json) = row;
                let variant_tags =
                    serde_json::from_str::<Vec<String>>(&variant_tags_json).unwrap_or_default();
                let usage_tags =
                    serde_json::from_str::<Vec<String>>(&usage_tags_json).unwrap_or_default();
                let mismatch = usage_tags
                    .into_iter()
                    .map(|value| value.to_ascii_lowercase())
                    .filter(|tag| {
                        !variant_tags
                            .iter()
                            .any(|existing| existing.eq_ignore_ascii_case(tag))
                    })
                    .collect::<BTreeSet<_>>();
                if mismatch.len() < 2 {
                    continue;
                }
                let branch_tags = mismatch.into_iter().take(3).collect::<Vec<_>>();
                let branch_key = branch_tags.join("-");
                let entry = candidates
                    .entry((variant_id.clone(), branch_key.clone()))
                    .or_insert_with(|| BranchCandidate {
                        source_variant_id: variant_id.clone(),
                        source_relative_path: relative_path.clone(),
                        branch_tags: branch_tags.clone(),
                        success_count: 0,
                    });
                entry.success_count += 1;
            }
            Ok(candidates)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

        let existing = self.list_skill_variants(Some(&skill_name), 200).await?;
        let mut created = Vec::new();
        for candidate in candidates.into_values() {
            if candidate.success_count < 3 {
                continue;
            }
            if existing.iter().any(|variant| {
                variant.status != "archived"
                    && skill_variant_covers_branch_tags(variant, &candidate.branch_tags)
            }) {
                continue;
            }
            if let Some(record) = self.create_branched_skill_variant(&skill_name, &candidate).await? {
                created.push(record);
            }
        }

        if !created.is_empty() {
            let _ = self.rebalance_skill_variants(&skill_name).await;
        }
        Ok(created)
    }

    pub async fn maybe_merge_skill_variants(&self, skill_name: &str) -> Result<Vec<SkillVariantRecord>> {
        let skill_name_owned = skill_name.to_string();
        let skill_name = skill_name.to_string();
        let variants = self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at \
                 FROM skill_variants WHERE skill_name = ?1",
            )?;
            let rows = stmt.query_map(params![skill_name_owned], map_skill_variant_row)?;
            Ok(rows.filter_map(|row| row.ok()).collect::<Vec<_>>())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

        if variants.is_empty() {
            return Ok(Vec::new());
        }

        let Some(canonical) = variants
            .iter()
            .find(|variant| variant.is_canonical())
            .cloned()
        else {
            return Ok(Vec::new());
        };
        let canonical_path = self.skills_root().join(&canonical.relative_path);
        if !canonical_path.exists() {
            return Ok(Vec::new());
        }

        let canonical_content = std::fs::read_to_string(&canonical_path).with_context(|| {
            format!(
                "failed to read canonical skill {}",
                canonical_path.display()
            )
        })?;
        let canonical_merge_body = extract_mergeable_variant_body(&canonical_content);
        let now = now_ts();
        let mut merged_ids = Vec::new();
        let mut merged_notes = Vec::new();
        let mut merged_sections = Vec::new();
        for variant in variants.iter().filter(|variant| {
            !variant.is_canonical()
                && variant.status != "archived"
                && variant.status != "merged"
                && variant.use_count >= SKILL_MERGE_MIN_USES
                && variant.success_rate() >= SKILL_MERGE_SUCCESS_RATE_THRESHOLD
                && variant.parent_variant_id.as_deref() == Some(canonical.variant_id.as_str())
        }) {
            let variant_path = self.skills_root().join(&variant.relative_path);
            if !variant_path.exists() {
                continue;
            }
            let variant_content = std::fs::read_to_string(&variant_path).with_context(|| {
                format!("failed to read skill variant {}", variant_path.display())
            })?;
            let variant_merge_body = extract_mergeable_variant_body(&variant_content);
            let similarity = skill_content_similarity(&canonical_merge_body, &variant_merge_body);
            if similarity < SKILL_MERGE_SIMILARITY_THRESHOLD {
                continue;
            }
            merged_ids.push(variant.variant_id.clone());
            merged_notes.push(skill_merge_note(variant, similarity));
            merged_sections.push(skill_merge_section(variant, &variant_content, similarity));
        }

        if merged_ids.is_empty() {
            return Ok(Vec::new());
        }

        let merged_content = append_skill_merge_sections(
            &append_skill_merge_notes(&canonical_content, &merged_notes),
            &merged_sections,
        );
        if merged_content != canonical_content {
            std::fs::write(&canonical_path, merged_content).with_context(|| {
                format!(
                    "failed to update canonical skill with merged contexts {}",
                    canonical_path.display()
                )
            })?;
            let _ = self.register_skill_document(&canonical_path).await?;
        }

        let merged_ids_clone = merged_ids.clone();
        self.conn.call(move |conn| {
            for variant_id in &merged_ids_clone {
                conn.execute(
                    "UPDATE skill_variants SET status = 'merged', updated_at = ?2 WHERE variant_id = ?1",
                    params![variant_id, now as i64],
                )?;
            }
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

        let merged_notes_len = merged_notes.len();
        self.append_telemetry(
            "cognitive",
            json!({
                "timestamp": now,
                "kind": "skill_variant_merged",
                "skill_name": skill_name,
                "variant_ids": merged_ids,
                "merged_count": merged_notes_len,
            }),
        ).await?;

        self.list_skill_variants(Some(&skill_name), 200).await
    }

    async fn create_branched_skill_variant(
        &self,
        skill_name: &str,
        candidate: &BranchCandidate,
    ) -> Result<Option<SkillVariantRecord>> {
        let source_path = self.skills_root().join(&candidate.source_relative_path);
        if !source_path.exists() {
            return Ok(None);
        }

        let source_content = std::fs::read_to_string(&source_path)
            .with_context(|| format!("failed to read source skill {}", source_path.display()))?;
        let title = extract_markdown_title(&source_content).unwrap_or_else(|| {
            skill_name
                .split('-')
                .map(|part| {
                    let mut chars = part.chars();
                    match chars.next() {
                        Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        });
        let branch_slug = candidate.branch_tags.join("-");
        let branch_path = self
            .skills_root()
            .join("generated")
            .join(format!("{skill_name}--{branch_slug}.md"));
        if branch_path.exists() {
            return self.register_skill_document(&branch_path).await.map(Some);
        }

        let mut body = format!(
            "# {} ({})\n\n> Auto-branched from `{}` (variant `{}`) after {} successful consultations in contexts: {}.\n\n## When To Use\nUse this variant when the workspace context includes: {}.\n\n",
            title,
            candidate.branch_tags.join(", "),
            candidate.source_relative_path,
            candidate.source_variant_id,
            candidate.success_count,
            candidate.branch_tags.join(", "),
            candidate.branch_tags.join(", "),
        );
        body.push_str(&source_content);

        if let Some(parent) = branch_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&branch_path, body)
            .with_context(|| format!("failed to write branched skill {}", branch_path.display()))?;
        self.register_skill_document(&branch_path).await.map(Some)
    }

    pub async fn append_command_log(&self, entry: &CommandLogEntry) -> Result<()> {
        let entry = entry.clone();
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO command_log \
             (id, command, timestamp, path, cwd, workspace_id, surface_id, pane_id, exit_code, duration_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                entry.id,
                entry.command,
                entry.timestamp,
                entry.path,
                entry.cwd,
                entry.workspace_id,
                entry.surface_id,
                entry.pane_id,
                entry.exit_code,
                entry.duration_ms,
            ],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn complete_command_log(
        &self,
        id: &str,
        exit_code: Option<i32>,
        duration_ms: Option<i64>,
    ) -> Result<()> {
        let id = id.to_string();
        self.conn.call(move |conn| {
        conn.execute(
            "UPDATE command_log SET exit_code = ?2, duration_ms = ?3 WHERE id = ?1",
            params![id, exit_code, duration_ms],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn query_command_log(
        &self,
        workspace_id: Option<&str>,
        pane_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<CommandLogEntry>> {
        let workspace_id = workspace_id.map(str::to_string);
        let pane_id = pane_id.map(str::to_string);
        self.conn.call(move |conn| {
        let limit = limit.unwrap_or(200).max(1) as i64;

        let sql = match (workspace_id.is_some(), pane_id.is_some()) {
            (true, true) => {
                "SELECT id, command, timestamp, path, cwd, workspace_id, surface_id, pane_id, exit_code, duration_ms \
                 FROM command_log WHERE workspace_id = ?1 AND pane_id = ?2 \
                 ORDER BY timestamp DESC LIMIT ?3"
            }
            (true, false) => {
                "SELECT id, command, timestamp, path, cwd, workspace_id, surface_id, pane_id, exit_code, duration_ms \
                 FROM command_log WHERE workspace_id = ?1 \
                 ORDER BY timestamp DESC LIMIT ?2"
            }
            (false, true) => {
                "SELECT id, command, timestamp, path, cwd, workspace_id, surface_id, pane_id, exit_code, duration_ms \
                 FROM command_log WHERE pane_id = ?1 \
                 ORDER BY timestamp DESC LIMIT ?2"
            }
            (false, false) => {
                "SELECT id, command, timestamp, path, cwd, workspace_id, surface_id, pane_id, exit_code, duration_ms \
                 FROM command_log ORDER BY timestamp DESC LIMIT ?1"
            }
        };

        let mut stmt = conn.prepare(sql)?;
        let rows = match (workspace_id, pane_id) {
            (Some(workspace_id), Some(pane_id)) => {
                stmt.query_map(params![workspace_id, pane_id, limit], map_command_log_entry)?
            }
            (Some(workspace_id), None) => {
                stmt.query_map(params![workspace_id, limit], map_command_log_entry)?
            }
            (None, Some(pane_id)) => {
                stmt.query_map(params![pane_id, limit], map_command_log_entry)?
            }
            (None, None) => stmt.query_map(params![limit], map_command_log_entry)?,
        };

        Ok(rows.filter_map(|row| row.ok()).collect())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn clear_command_log(&self) -> Result<()> {
        self.conn.call(move |conn| {
        conn.execute("DELETE FROM command_log", [])?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn create_thread(&self, thread: &AgentDbThread) -> Result<()> {
        let thread = thread.clone();
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO agent_threads \
             (id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview, metadata_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                thread.id,
                thread.workspace_id,
                thread.surface_id,
                thread.pane_id,
                thread.agent_name,
                thread.title,
                thread.created_at,
                thread.updated_at,
                thread.message_count,
                thread.total_tokens,
                thread.last_preview,
                thread.metadata_json,
            ],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_thread(&self, id: &str) -> Result<()> {
        let id = id.to_string();
        self.conn.call(move |conn| {
        conn.execute("DELETE FROM agent_threads WHERE id = ?1", params![id])?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_threads(&self) -> Result<Vec<AgentDbThread>> {
        self.conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview, metadata_json \
             FROM agent_threads ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], map_agent_thread)?;
        Ok(rows.filter_map(|row| row.ok()).collect())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_thread(&self, id: &str) -> Result<Option<AgentDbThread>> {
        let id = id.to_string();
        self.conn.call(move |conn| {
        conn
            .query_row(
                "SELECT id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview, metadata_json \
                 FROM agent_threads WHERE id = ?1",
                params![id],
                map_agent_thread,
            )
            .optional()
            .map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn add_message(&self, message: &AgentDbMessage) -> Result<()> {
        let message = message.clone();
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO agent_messages \
             (id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, reasoning, tool_calls_json, metadata_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                message.id,
                message.thread_id,
                message.created_at,
                message.role,
                message.content,
                message.provider,
                message.model,
                message.input_tokens,
                message.output_tokens,
                message.total_tokens,
                message.reasoning,
                message.tool_calls_json,
                message.metadata_json,
            ],
        )?;
        refresh_thread_stats(conn, &message.thread_id)?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn update_message(&self, id: &str, patch: &AgentMessagePatch) -> Result<()> {
        let id = id.to_string();
        let patch = patch.clone();
        self.conn.call(move |conn| {
        let thread_id: Option<String> = conn
            .query_row(
                "SELECT thread_id FROM agent_messages WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;

        if thread_id.is_none() {
            return Ok(());
        }

        conn.execute(
            "UPDATE agent_messages SET
                content = COALESCE(?2, content),
                provider = COALESCE(?3, provider),
                model = COALESCE(?4, model),
                input_tokens = COALESCE(?5, input_tokens),
                output_tokens = COALESCE(?6, output_tokens),
                total_tokens = COALESCE(?7, total_tokens),
                reasoning = COALESCE(?8, reasoning),
                tool_calls_json = COALESCE(?9, tool_calls_json),
                metadata_json = COALESCE(?10, metadata_json)
             WHERE id = ?1",
            params![
                id,
                patch.content.as_deref(),
                flatten_option_str(&patch.provider),
                flatten_option_str(&patch.model),
                flatten_option_i64(&patch.input_tokens),
                flatten_option_i64(&patch.output_tokens),
                flatten_option_i64(&patch.total_tokens),
                flatten_option_str(&patch.reasoning),
                flatten_option_str(&patch.tool_calls_json),
                flatten_option_str(&patch.metadata_json),
            ],
        )?;

        if let Some(thread_id) = thread_id {
            refresh_thread_stats(conn, &thread_id)?;
        }
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Delete specific messages from a thread by their IDs.
    pub async fn delete_messages(&self, thread_id: &str, message_ids: &[&str]) -> Result<usize> {
        if message_ids.is_empty() {
            return Ok(0);
        }
        let thread_id = thread_id.to_string();
        let message_ids: Vec<String> = message_ids.iter().map(|s| s.to_string()).collect();
        self.conn.call(move |conn| {
        let placeholders: Vec<String> = message_ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "DELETE FROM agent_messages WHERE thread_id = ? AND id IN ({})",
            placeholders.join(", ")
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        params.push(Box::new(thread_id.to_string()));
        for id in message_ids {
            params.push(Box::new(id.to_string()));
        }
        let refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let count = conn.execute(&sql, refs.as_slice())?;
        refresh_thread_stats(conn, &thread_id)?;
        Ok(count)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_messages(
        &self,
        thread_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<AgentDbMessage>> {
        let thread_id = thread_id.to_string();
        self.conn.call(move |conn| {
        let limit = limit.unwrap_or(500).max(1) as i64;
        let mut stmt = conn.prepare(
            "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, reasoning, tool_calls_json, metadata_json \
             FROM agent_messages WHERE thread_id = ?1 ORDER BY created_at ASC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![thread_id, limit], map_agent_message)?;
        Ok(rows.filter_map(|row| row.ok()).collect())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_worm_chain_tip(&self, kind: &str) -> Result<Option<WormChainTip>> {
        let kind = kind.to_string();
        let kind = kind.to_string();
        self.conn.call(move |conn| {
            conn.query_row(
                "SELECT kind, seq, hash FROM worm_chain_tip WHERE kind = ?1",
                params![kind],
                |row| {
                    Ok(WormChainTip {
                        kind: row.get(0)?,
                        seq: row.get(1)?,
                        hash: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn set_worm_chain_tip(&self, kind: &str, seq: i64, hash: &str) -> Result<()> {
        let kind = kind.to_string();
        let hash = hash.to_string();
        let kind = kind.to_string();
        let hash = hash.to_string();
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT INTO worm_chain_tip (kind, seq, hash) VALUES (?1, ?2, ?3) \
                 ON CONFLICT(kind) DO UPDATE SET seq = excluded.seq, hash = excluded.hash",
                params![kind, seq, hash],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_transcript_index(&self, entry: &TranscriptIndexEntry) -> Result<()> {
        let entry = entry.clone();
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO transcript_index \
             (id, pane_id, workspace_id, surface_id, filename, reason, captured_at, size_bytes, preview) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                entry.id,
                entry.pane_id,
                entry.workspace_id,
                entry.surface_id,
                entry.filename,
                entry.reason,
                entry.captured_at,
                entry.size_bytes,
                entry.preview,
            ],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_transcript_index(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<Vec<TranscriptIndexEntry>> {
        let workspace_id = workspace_id.map(str::to_string);
        self.conn.call(move |conn| {
        let sql = if workspace_id.is_some() {
            "SELECT id, pane_id, workspace_id, surface_id, filename, reason, captured_at, size_bytes, preview \
             FROM transcript_index WHERE workspace_id = ?1 ORDER BY captured_at DESC"
        } else {
            "SELECT id, pane_id, workspace_id, surface_id, filename, reason, captured_at, size_bytes, preview \
             FROM transcript_index ORDER BY captured_at DESC"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = if let Some(workspace_id) = workspace_id {
            stmt.query_map(params![workspace_id], map_transcript_index_entry)?
        } else {
            stmt.query_map([], map_transcript_index_entry)?
        };
        Ok(rows.filter_map(|row| row.ok()).collect())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_snapshot_index(&self, entry: &SnapshotIndexEntry) -> Result<()> {
        let entry = entry.clone();
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO snapshot_index \
             (snapshot_id, workspace_id, session_id, kind, label, path, created_at, details_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                entry.snapshot_id,
                entry.workspace_id,
                entry.session_id,
                entry.kind,
                entry.label,
                entry.path,
                entry.created_at,
                entry.details_json,
            ],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_snapshot_index(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<Vec<SnapshotIndexEntry>> {
        let workspace_id = workspace_id.map(str::to_string);
        self.conn.call(move |conn| {
        let sql = if workspace_id.is_some() {
            "SELECT snapshot_id, workspace_id, session_id, kind, label, path, created_at, details_json \
             FROM snapshot_index WHERE workspace_id = ?1 OR workspace_id IS NULL ORDER BY created_at DESC"
        } else {
            "SELECT snapshot_id, workspace_id, session_id, kind, label, path, created_at, details_json \
             FROM snapshot_index ORDER BY created_at DESC"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = if let Some(workspace_id) = workspace_id {
            stmt.query_map(params![workspace_id], map_snapshot_index_entry)?
        } else {
            stmt.query_map([], map_snapshot_index_entry)?
        };
        Ok(rows.filter_map(|row| row.ok()).collect())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_snapshot_index(&self, snapshot_id: &str) -> Result<Option<SnapshotIndexEntry>> {
        let snapshot_id = snapshot_id.to_string();
        self.conn.call(move |conn| {
        conn
            .query_row(
                "SELECT snapshot_id, workspace_id, session_id, kind, label, path, created_at, details_json \
                 FROM snapshot_index WHERE snapshot_id = ?1",
                params![snapshot_id],
                map_snapshot_index_entry,
            )
            .optional()
            .map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_snapshot_index(&self, snapshot_id: &str) -> Result<bool> {
        let snapshot_id = snapshot_id.to_string();
        self.conn.call(move |conn| {
        let affected = conn.execute(
            "DELETE FROM snapshot_index WHERE snapshot_id = ?1",
            params![snapshot_id],
        )?;
        Ok(affected > 0)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_agent_event(&self, entry: &AgentEventRow) -> Result<()> {
        let entry = entry.clone();
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO agent_events \
             (id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                entry.id,
                entry.category,
                entry.kind,
                entry.pane_id,
                entry.workspace_id,
                entry.surface_id,
                entry.session_id,
                entry.payload_json,
                entry.timestamp,
            ],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_agent_events(
        &self,
        category: Option<&str>,
        pane_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<AgentEventRow>> {
        let category = category.map(str::to_string);
        let pane_id = pane_id.map(str::to_string);
        self.conn.call(move |conn| {
        let limit = limit.unwrap_or(500).max(1) as i64;
        let sql = match (category.is_some(), pane_id.is_some()) {
            (true, true) => {
                "SELECT id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp \
                 FROM agent_events WHERE category = ?1 AND pane_id = ?2 ORDER BY timestamp DESC LIMIT ?3"
            }
            (true, false) => {
                "SELECT id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp \
                 FROM agent_events WHERE category = ?1 ORDER BY timestamp DESC LIMIT ?2"
            }
            (false, true) => {
                "SELECT id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp \
                 FROM agent_events WHERE pane_id = ?1 ORDER BY timestamp DESC LIMIT ?2"
            }
            (false, false) => {
                "SELECT id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp \
                 FROM agent_events ORDER BY timestamp DESC LIMIT ?1"
            }
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = match (category, pane_id) {
            (Some(category), Some(pane_id)) => {
                stmt.query_map(params![category, pane_id, limit], map_agent_event_row)?
            }
            (Some(category), None) => {
                stmt.query_map(params![category, limit], map_agent_event_row)?
            }
            (None, Some(pane_id)) => {
                stmt.query_map(params![pane_id, limit], map_agent_event_row)?
            }
            (None, None) => stmt.query_map(params![limit], map_agent_event_row)?,
        };
        Ok(rows.filter_map(|row| row.ok()).collect())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_agent_task(&self, task: &AgentTask) -> Result<()> {
        let task = task.clone();
        self.conn.call(move |conn| {
        let transaction = conn.transaction()?;
        let notify_channels_json = serde_json::to_string(&task.notify_channels).call_err()?;

        transaction.execute(
            "INSERT OR REPLACE INTO agent_tasks \
             (id, title, description, status, priority, progress, created_at, started_at, completed_at, error, result, thread_id, source, notify_on_complete, notify_channels_json, command, session_id, goal_run_id, goal_run_title, goal_step_id, goal_step_title, parent_task_id, parent_thread_id, runtime, retry_count, max_retries, next_retry_at, scheduled_at, blocked_reason, awaiting_approval_id, lane_id, last_error) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32)",
            params![
                &task.id,
                &task.title,
                &task.description,
                task_status_to_str(task.status),
                task_priority_to_str(task.priority),
                task.progress as i64,
                task.created_at as i64,
                task.started_at.map(|value| value as i64),
                task.completed_at.map(|value| value as i64),
                &task.error,
                &task.result,
                &task.thread_id,
                &task.source,
                if task.notify_on_complete { 1 } else { 0 },
                notify_channels_json,
                &task.command,
                &task.session_id,
                &task.goal_run_id,
                &task.goal_run_title,
                &task.goal_step_id,
                &task.goal_step_title,
                &task.parent_task_id,
                &task.parent_thread_id,
                &task.runtime,
                task.retry_count as i64,
                task.max_retries as i64,
                task.next_retry_at.map(|value| value as i64),
                task.scheduled_at.map(|value| value as i64),
                &task.blocked_reason,
                &task.awaiting_approval_id,
                &task.lane_id,
                &task.last_error,
            ],
        )?;

        transaction.execute(
            "DELETE FROM agent_task_dependencies WHERE task_id = ?1",
            params![&task.id],
        )?;
        for (ordinal, dependency) in task.dependencies.iter().enumerate() {
            transaction.execute(
                "INSERT INTO agent_task_dependencies (task_id, depends_on_task_id, ordinal) VALUES (?1, ?2, ?3)",
                params![&task.id, dependency, ordinal as i64],
            )?;
        }

        transaction.execute(
            "DELETE FROM agent_task_logs WHERE task_id = ?1",
            params![&task.id],
        )?;
        for log in &task.logs {
            transaction.execute(
                "INSERT OR REPLACE INTO agent_task_logs (id, task_id, timestamp, level, phase, message, details, attempt) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    &log.id,
                    &task.id,
                    log.timestamp as i64,
                    task_log_level_to_str(log.level),
                    &log.phase,
                    &log.message,
                    &log.details,
                    log.attempt as i64,
                ],
            )?;
        }

        transaction.commit()?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_agent_task(&self, task_id: &str) -> Result<()> {
        let task_id = task_id.to_string();
        self.conn.call(move |conn| {
        conn.execute("DELETE FROM agent_tasks WHERE id = ?1", params![task_id])?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_agent_tasks(&self) -> Result<Vec<AgentTask>> {
        self.conn.call(move |conn| {
        let mut dependency_stmt = conn.prepare(
            "SELECT task_id, depends_on_task_id FROM agent_task_dependencies ORDER BY ordinal ASC",
        )?;
        let dependency_rows = dependency_stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut dependency_map = std::collections::HashMap::<String, Vec<String>>::new();
        for row in dependency_rows {
            let (task_id, dependency) = row?;
            dependency_map.entry(task_id).or_default().push(dependency);
        }

        let mut log_stmt = conn.prepare(
            "SELECT id, task_id, timestamp, level, phase, message, details, attempt FROM agent_task_logs ORDER BY timestamp ASC",
        )?;
        let log_rows = log_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                AgentTaskLogEntry {
                    id: row.get(0)?,
                    timestamp: row.get::<_, i64>(2)? as u64,
                    level: parse_task_log_level(&row.get::<_, String>(3)?),
                    phase: row.get(4)?,
                    message: row.get(5)?,
                    details: row.get(6)?,
                    attempt: row.get::<_, i64>(7)? as u32,
                },
            ))
        })?;
        let mut log_map = std::collections::HashMap::<String, Vec<AgentTaskLogEntry>>::new();
        for row in log_rows {
            let (task_id, log) = row?;
            log_map.entry(task_id).or_default().push(log);
        }

        let mut stmt = conn.prepare(
            "SELECT id, title, description, status, priority, progress, created_at, started_at, completed_at, error, result, thread_id, source, notify_on_complete, notify_channels_json, command, session_id, goal_run_id, goal_run_title, goal_step_id, goal_step_title, parent_task_id, parent_thread_id, runtime, retry_count, max_retries, next_retry_at, scheduled_at, blocked_reason, awaiting_approval_id, lane_id, last_error \
             FROM agent_tasks \
             ORDER BY CASE status \
                 WHEN 'in_progress' THEN 0 \
                 WHEN 'awaiting_approval' THEN 1 \
                 WHEN 'failed_analyzing' THEN 2 \
                 WHEN 'blocked' THEN 3 \
                 WHEN 'queued' THEN 4 \
                 WHEN 'failed' THEN 5 \
                 WHEN 'completed' THEN 6 \
                 ELSE 7 END, \
                 CASE priority \
                 WHEN 'urgent' THEN 0 \
                 WHEN 'high' THEN 1 \
                 WHEN 'normal' THEN 2 \
                 ELSE 3 END, \
                 created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let task_id: String = row.get(0)?;
            let notify_channels_json: String = row.get(14)?;
            Ok(AgentTask {
                id: task_id,
                title: row.get(1)?,
                description: row.get(2)?,
                status: parse_task_status(&row.get::<_, String>(3)?),
                priority: parse_task_priority(&row.get::<_, String>(4)?),
                progress: row.get::<_, i64>(5)? as u8,
                created_at: row.get::<_, i64>(6)? as u64,
                started_at: row.get::<_, Option<i64>>(7)?.map(|value| value as u64),
                completed_at: row.get::<_, Option<i64>>(8)?.map(|value| value as u64),
                error: row.get(9)?,
                result: row.get(10)?,
                thread_id: row.get(11)?,
                source: row.get(12)?,
                notify_on_complete: row.get::<_, i64>(13)? != 0,
                notify_channels: serde_json::from_str(&notify_channels_json).unwrap_or_default(),
                dependencies: Vec::new(),
                command: row.get(15)?,
                session_id: row.get(16)?,
                goal_run_id: row.get(17)?,
                goal_run_title: row.get(18)?,
                goal_step_id: row.get(19)?,
                goal_step_title: row.get(20)?,
                parent_task_id: row.get(21)?,
                parent_thread_id: row.get(22)?,
                runtime: row
                    .get::<_, Option<String>>(23)?
                    .unwrap_or_else(|| "daemon".to_string()),
                retry_count: row.get::<_, i64>(24)? as u32,
                max_retries: row.get::<_, i64>(25)? as u32,
                next_retry_at: row.get::<_, Option<i64>>(26)?.map(|value| value as u64),
                scheduled_at: row.get::<_, Option<i64>>(27)?.map(|value| value as u64),
                blocked_reason: row.get(28)?,
                awaiting_approval_id: row.get(29)?,
                lane_id: row.get(30)?,
                last_error: row.get(31)?,
                logs: Vec::new(),
                tool_whitelist: None,
                tool_blacklist: None,
                context_budget_tokens: None,
                context_overflow_action: None,
                termination_conditions: None,
                success_criteria: None,
                max_duration_secs: None,
                supervisor_config: None,
                override_provider: None,
                override_model: None,
                override_system_prompt: None,
                sub_agent_def_id: None,
            })
        })?;

        let mut tasks = Vec::new();
        for row in rows {
            let mut task = row?;
            task.dependencies = dependency_map.remove(&task.id).unwrap_or_default();
            task.logs = log_map.remove(&task.id).unwrap_or_default();
            tasks.push(task);
        }
        Ok(tasks)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_goal_run(&self, goal_run: &GoalRun) -> Result<()> {
        let goal_run = goal_run.clone();
        self.conn.call(move |conn| {
        let transaction = conn.transaction()?;
        let memory_updates_json = serde_json::to_string(&goal_run.memory_updates).call_err()?;
        let child_task_ids_json = serde_json::to_string(&goal_run.child_task_ids).call_err()?;

        transaction.execute(
            "INSERT OR REPLACE INTO goal_runs \
             (id, title, goal, client_request_id, status, priority, created_at, updated_at, started_at, completed_at, thread_id, session_id, current_step_index, replan_count, max_replans, plan_summary, reflection_summary, memory_updates_json, generated_skill_path, last_error, child_task_ids_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
            params![
                &goal_run.id,
                &goal_run.title,
                &goal_run.goal,
                &goal_run.client_request_id,
                goal_run_status_to_str(goal_run.status),
                task_priority_to_str(goal_run.priority),
                goal_run.created_at as i64,
                goal_run.updated_at as i64,
                goal_run.started_at.map(|value| value as i64),
                goal_run.completed_at.map(|value| value as i64),
                &goal_run.thread_id,
                &goal_run.session_id,
                goal_run.current_step_index as i64,
                goal_run.replan_count as i64,
                goal_run.max_replans as i64,
                &goal_run.plan_summary,
                &goal_run.reflection_summary,
                memory_updates_json,
                &goal_run.generated_skill_path,
                &goal_run.last_error,
                child_task_ids_json,
            ],
        )?;

        transaction.execute(
            "DELETE FROM goal_run_steps WHERE goal_run_id = ?1",
            params![&goal_run.id],
        )?;
        for step in &goal_run.steps {
            transaction.execute(
                "INSERT OR REPLACE INTO goal_run_steps \
                 (id, goal_run_id, ordinal, title, instructions, kind, success_criteria, session_id, status, task_id, summary, error, started_at, completed_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    &step.id,
                    &goal_run.id,
                    step.position as i64,
                    &step.title,
                    &step.instructions,
                    goal_run_step_kind_to_str(step.kind),
                    &step.success_criteria,
                    &step.session_id,
                    goal_run_step_status_to_str(step.status),
                    &step.task_id,
                    &step.summary,
                    &step.error,
                    step.started_at.map(|value| value as i64),
                    step.completed_at.map(|value| value as i64),
                ],
            )?;
        }

        transaction.execute(
            "DELETE FROM goal_run_events WHERE goal_run_id = ?1",
            params![&goal_run.id],
        )?;
        for event in &goal_run.events {
            let todo_snapshot_json = serde_json::to_string(&event.todo_snapshot).call_err()?;
            transaction.execute(
                "INSERT OR REPLACE INTO goal_run_events (id, goal_run_id, timestamp, phase, message, details, step_index, todo_snapshot_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    &event.id,
                    &goal_run.id,
                    event.timestamp as i64,
                    &event.phase,
                    &event.message,
                    &event.details,
                    event.step_index.map(|value| value as i64),
                    todo_snapshot_json,
                ],
            )?;
        }

        transaction.commit()?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_goal_runs(&self) -> Result<Vec<GoalRun>> {
        self.conn.call(move |conn| {
        let mut step_stmt = conn.prepare(
            "SELECT id, goal_run_id, ordinal, title, instructions, kind, success_criteria, session_id, status, task_id, summary, error, started_at, completed_at \
             FROM goal_run_steps ORDER BY goal_run_id ASC, ordinal ASC",
        )?;
        let step_rows = step_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                GoalRunStep {
                    id: row.get(0)?,
                    position: row.get::<_, i64>(2)? as usize,
                    title: row.get(3)?,
                    instructions: row.get(4)?,
                    kind: parse_goal_run_step_kind(&row.get::<_, String>(5)?),
                    success_criteria: row.get(6)?,
                    session_id: row.get(7)?,
                    status: parse_goal_run_step_status(&row.get::<_, String>(8)?),
                    task_id: row.get(9)?,
                    summary: row.get(10)?,
                    error: row.get(11)?,
                    started_at: row.get::<_, Option<i64>>(12)?.map(|value| value as u64),
                    completed_at: row.get::<_, Option<i64>>(13)?.map(|value| value as u64),
                },
            ))
        })?;
        let mut step_map = std::collections::HashMap::<String, Vec<GoalRunStep>>::new();
        for row in step_rows {
            let (goal_run_id, step) = row?;
            step_map.entry(goal_run_id).or_default().push(step);
        }

        let mut event_stmt = conn.prepare(
            "SELECT id, goal_run_id, timestamp, phase, message, details, step_index, todo_snapshot_json FROM goal_run_events ORDER BY timestamp ASC",
        )?;
        let event_rows = event_stmt.query_map([], |row| {
            let todo_snapshot_json: Option<String> = row.get(7)?;
            Ok((
                row.get::<_, String>(1)?,
                GoalRunEvent {
                    id: row.get(0)?,
                    timestamp: row.get::<_, i64>(2)? as u64,
                    phase: row.get(3)?,
                    message: row.get(4)?,
                    details: row.get(5)?,
                    step_index: row.get::<_, Option<i64>>(6)?.map(|value| value as usize),
                    todo_snapshot: todo_snapshot_json
                        .as_deref()
                        .and_then(|json| serde_json::from_str(json).ok())
                        .unwrap_or_default(),
                },
            ))
        })?;
        let mut event_map = std::collections::HashMap::<String, Vec<GoalRunEvent>>::new();
        for row in event_rows {
            let (goal_run_id, event) = row?;
            event_map.entry(goal_run_id).or_default().push(event);
        }

        let mut stmt = conn.prepare(
            "SELECT id, title, goal, client_request_id, status, priority, created_at, updated_at, started_at, completed_at, thread_id, session_id, current_step_index, replan_count, max_replans, plan_summary, reflection_summary, memory_updates_json, generated_skill_path, last_error, child_task_ids_json \
             FROM goal_runs ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let memory_updates_json: String = row.get(17)?;
            let child_task_ids_json: String = row.get(20)?;
            Ok(GoalRun {
                id,
                title: row.get(1)?,
                goal: row.get(2)?,
                client_request_id: row.get(3)?,
                status: parse_goal_run_status(&row.get::<_, String>(4)?),
                priority: parse_task_priority(&row.get::<_, String>(5)?),
                created_at: row.get::<_, i64>(6)? as u64,
                updated_at: row.get::<_, i64>(7)? as u64,
                started_at: row.get::<_, Option<i64>>(8)?.map(|value| value as u64),
                completed_at: row.get::<_, Option<i64>>(9)?.map(|value| value as u64),
                thread_id: row.get(10)?,
                session_id: row.get(11)?,
                current_step_index: row.get::<_, i64>(12)? as usize,
                current_step_title: None,
                current_step_kind: None,
                replan_count: row.get::<_, i64>(13)? as u32,
                max_replans: row.get::<_, i64>(14)? as u32,
                plan_summary: row.get(15)?,
                reflection_summary: row.get(16)?,
                memory_updates: serde_json::from_str(&memory_updates_json).unwrap_or_default(),
                generated_skill_path: row.get(18)?,
                last_error: row.get(19)?,
                failure_cause: None,
                awaiting_approval_id: None,
                active_task_id: None,
                duration_ms: None,
                child_task_ids: serde_json::from_str(&child_task_ids_json).unwrap_or_default(),
                child_task_count: 0,
                approval_count: 0,
                steps: Vec::new(),
                events: Vec::new(),
            })
        })?;

        let mut goal_runs = Vec::new();
        for row in rows {
            let mut goal_run = row?;
            goal_run.steps = step_map.remove(&goal_run.id).unwrap_or_default();
            goal_run.events = event_map.remove(&goal_run.id).unwrap_or_default();
            goal_runs.push(goal_run);
        }
        Ok(goal_runs)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_goal_run(&self, goal_run_id: &str) -> Result<Option<GoalRun>> {
        Ok(self
            .list_goal_runs()
            .await?
            .into_iter()
            .find(|goal_run| goal_run.id == goal_run_id))
    }

    pub async fn upsert_collaboration_session(
        &self,
        parent_task_id: &str,
        session_json: &str,
        updated_at: u64,
    ) -> Result<()> {
        let parent_task_id = parent_task_id.to_string();
        let session_json = session_json.to_string();
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO collaboration_sessions (parent_task_id, session_json, updated_at) VALUES (?1, ?2, ?3)",
            params![parent_task_id, session_json, updated_at as i64],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_collaboration_sessions(&self) -> Result<Vec<CollaborationSessionRow>> {
        self.conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT parent_task_id, session_json, updated_at FROM collaboration_sessions ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CollaborationSessionRow {
                parent_task_id: row.get(0)?,
                session_json: row.get(1)?,
                updated_at: row.get::<_, i64>(2)? as u64,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn init_schema(&self) -> Result<()> {
        self.conn.call(|connection| {
        connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS history_entries (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                title TEXT NOT NULL,
                excerpt TEXT NOT NULL,
                content TEXT NOT NULL,
                path TEXT,
                timestamp INTEGER NOT NULL
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS history_fts USING fts5(
                id UNINDEXED,
                title,
                excerpt,
                content
            );
            CREATE TABLE IF NOT EXISTS command_log (
                id           TEXT PRIMARY KEY,
                command      TEXT NOT NULL,
                timestamp    INTEGER NOT NULL,
                path         TEXT,
                cwd          TEXT,
                workspace_id TEXT,
                surface_id   TEXT,
                pane_id      TEXT,
                exit_code    INTEGER,
                duration_ms  INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_command_log_ts ON command_log(timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_command_log_pane ON command_log(pane_id);
            CREATE TABLE IF NOT EXISTS agent_threads (
                id             TEXT PRIMARY KEY,
                workspace_id   TEXT,
                surface_id     TEXT,
                pane_id        TEXT,
                agent_name     TEXT,
                title          TEXT NOT NULL DEFAULT '',
                created_at     INTEGER NOT NULL,
                updated_at     INTEGER NOT NULL,
                message_count  INTEGER NOT NULL DEFAULT 0,
                total_tokens   INTEGER NOT NULL DEFAULT 0,
                last_preview   TEXT NOT NULL DEFAULT '',
                metadata_json  TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_threads_updated ON agent_threads(updated_at DESC);
            CREATE TABLE IF NOT EXISTS agent_messages (
                id              TEXT PRIMARY KEY,
                thread_id       TEXT NOT NULL REFERENCES agent_threads(id) ON DELETE CASCADE,
                created_at      INTEGER NOT NULL,
                role            TEXT NOT NULL,
                content         TEXT NOT NULL DEFAULT '',
                provider        TEXT,
                model           TEXT,
                input_tokens    INTEGER,
                output_tokens   INTEGER,
                total_tokens    INTEGER,
                reasoning       TEXT,
                tool_calls_json TEXT,
                metadata_json   TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_messages_thread ON agent_messages(thread_id, created_at);
            CREATE TABLE IF NOT EXISTS worm_chain_tip (
                kind      TEXT PRIMARY KEY,
                seq       INTEGER NOT NULL,
                hash      TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS agent_events (
                id           TEXT PRIMARY KEY,
                category     TEXT NOT NULL,
                kind         TEXT NOT NULL,
                pane_id      TEXT,
                workspace_id TEXT,
                surface_id   TEXT,
                session_id   TEXT,
                payload_json TEXT NOT NULL,
                timestamp    INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_agent_events_cat ON agent_events(category, timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_agent_events_pane ON agent_events(pane_id, timestamp DESC);
            CREATE TABLE IF NOT EXISTS agent_tasks (
                id                   TEXT PRIMARY KEY,
                title                TEXT NOT NULL,
                description          TEXT NOT NULL,
                status               TEXT NOT NULL,
                priority             TEXT NOT NULL,
                progress             INTEGER NOT NULL DEFAULT 0,
                created_at           INTEGER NOT NULL,
                started_at           INTEGER,
                completed_at         INTEGER,
                error                TEXT,
                result               TEXT,
                thread_id            TEXT,
                source               TEXT NOT NULL DEFAULT 'user',
                notify_on_complete   INTEGER NOT NULL DEFAULT 0,
                notify_channels_json TEXT NOT NULL DEFAULT '[]',
                command              TEXT,
                session_id           TEXT,
                goal_run_id          TEXT,
                goal_run_title       TEXT,
                goal_step_id         TEXT,
                goal_step_title      TEXT,
                parent_task_id       TEXT,
                parent_thread_id     TEXT,
                runtime              TEXT NOT NULL DEFAULT 'daemon',
                retry_count          INTEGER NOT NULL DEFAULT 0,
                max_retries          INTEGER NOT NULL DEFAULT 3,
                next_retry_at        INTEGER,
                scheduled_at         INTEGER,
                blocked_reason       TEXT,
                awaiting_approval_id TEXT,
                lane_id              TEXT,
                last_error           TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_agent_tasks_status ON agent_tasks(status, priority, created_at DESC);
            CREATE TABLE IF NOT EXISTS agent_task_dependencies (
                task_id             TEXT NOT NULL REFERENCES agent_tasks(id) ON DELETE CASCADE,
                depends_on_task_id  TEXT NOT NULL,
                ordinal             INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (task_id, depends_on_task_id)
            );
            CREATE INDEX IF NOT EXISTS idx_agent_task_deps_parent ON agent_task_dependencies(depends_on_task_id);
            CREATE TABLE IF NOT EXISTS agent_task_logs (
                id         TEXT PRIMARY KEY,
                task_id    TEXT NOT NULL REFERENCES agent_tasks(id) ON DELETE CASCADE,
                timestamp  INTEGER NOT NULL,
                level      TEXT NOT NULL,
                phase      TEXT NOT NULL,
                message    TEXT NOT NULL,
                details    TEXT,
                attempt    INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_agent_task_logs_task_ts ON agent_task_logs(task_id, timestamp ASC);
            CREATE TABLE IF NOT EXISTS transcript_index (
                id           TEXT PRIMARY KEY,
                pane_id      TEXT,
                workspace_id TEXT,
                surface_id   TEXT,
                filename     TEXT NOT NULL,
                reason       TEXT,
                captured_at  INTEGER NOT NULL,
                size_bytes   INTEGER,
                preview      TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_transcript_ts ON transcript_index(captured_at DESC);
            CREATE TABLE IF NOT EXISTS snapshot_index (
                snapshot_id  TEXT PRIMARY KEY,
                workspace_id TEXT,
                session_id   TEXT,
                kind         TEXT NOT NULL,
                label        TEXT,
                path         TEXT NOT NULL,
                created_at   INTEGER NOT NULL,
                details_json TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_snapshot_ts ON snapshot_index(created_at DESC);
            CREATE TABLE IF NOT EXISTS goal_runs (
                id                  TEXT PRIMARY KEY,
                title               TEXT NOT NULL,
                goal                TEXT NOT NULL,
                client_request_id   TEXT,
                status              TEXT NOT NULL,
                priority            TEXT NOT NULL,
                created_at          INTEGER NOT NULL,
                updated_at          INTEGER NOT NULL,
                started_at          INTEGER,
                completed_at        INTEGER,
                thread_id           TEXT,
                session_id          TEXT,
                current_step_index  INTEGER NOT NULL DEFAULT 0,
                replan_count        INTEGER NOT NULL DEFAULT 0,
                max_replans         INTEGER NOT NULL DEFAULT 2,
                plan_summary        TEXT,
                reflection_summary  TEXT,
                memory_updates_json TEXT NOT NULL DEFAULT '[]',
                generated_skill_path TEXT,
                last_error          TEXT,
                child_task_ids_json TEXT NOT NULL DEFAULT '[]'
            );
            CREATE INDEX IF NOT EXISTS idx_goal_runs_status ON goal_runs(status, updated_at DESC);
            CREATE TABLE IF NOT EXISTS goal_run_steps (
                id                TEXT PRIMARY KEY,
                goal_run_id       TEXT NOT NULL REFERENCES goal_runs(id) ON DELETE CASCADE,
                ordinal           INTEGER NOT NULL,
                title             TEXT NOT NULL,
                instructions      TEXT NOT NULL,
                kind              TEXT NOT NULL,
                success_criteria  TEXT NOT NULL,
                session_id        TEXT,
                status            TEXT NOT NULL,
                task_id           TEXT,
                summary           TEXT,
                error             TEXT,
                started_at        INTEGER,
                completed_at      INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_goal_run_steps_goal_run ON goal_run_steps(goal_run_id, ordinal ASC);
            CREATE TABLE IF NOT EXISTS goal_run_events (
                id          TEXT PRIMARY KEY,
                goal_run_id TEXT NOT NULL REFERENCES goal_runs(id) ON DELETE CASCADE,
                timestamp   INTEGER NOT NULL,
                phase       TEXT NOT NULL,
                message     TEXT NOT NULL,
                details     TEXT,
                step_index  INTEGER,
                todo_snapshot_json TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_goal_run_events_goal_run_ts ON goal_run_events(goal_run_id, timestamp ASC);
            CREATE TABLE IF NOT EXISTS subagent_metrics (
                task_id              TEXT PRIMARY KEY,
                parent_task_id       TEXT,
                thread_id            TEXT,
                tool_calls_total     INTEGER DEFAULT 0,
                tool_calls_succeeded INTEGER DEFAULT 0,
                tool_calls_failed    INTEGER DEFAULT 0,
                tokens_consumed      INTEGER DEFAULT 0,
                context_budget_tokens INTEGER,
                progress_rate        REAL DEFAULT 0.0,
                last_progress_at     INTEGER,
                stuck_score          REAL DEFAULT 0.0,
                health_state         TEXT DEFAULT 'healthy',
                created_at           INTEGER NOT NULL,
                updated_at           INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS agent_checkpoints (
                id                TEXT PRIMARY KEY,
                goal_run_id       TEXT NOT NULL,
                thread_id         TEXT,
                task_id           TEXT,
                checkpoint_type   TEXT NOT NULL,
                state_json        TEXT NOT NULL,
                context_summary   TEXT,
                created_at        INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_agent_checkpoints_goal_run ON agent_checkpoints(goal_run_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS agent_health_log (
                id            TEXT PRIMARY KEY,
                entity_type   TEXT NOT NULL,
                entity_id     TEXT NOT NULL,
                health_state  TEXT NOT NULL,
                indicators_json TEXT,
                intervention  TEXT,
                created_at    INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_agent_health_log_entity ON agent_health_log(entity_type, entity_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS context_archive (
                id                     TEXT PRIMARY KEY,
                thread_id              TEXT NOT NULL,
                original_role          TEXT,
                compressed_content     TEXT NOT NULL,
                summary                TEXT,
                relevance_score        REAL DEFAULT 0.0,
                token_count_original   INTEGER,
                token_count_compressed INTEGER,
                metadata_json          TEXT,
                archived_at            INTEGER NOT NULL,
                last_accessed_at       INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_context_archive_thread ON context_archive(thread_id, archived_at DESC);

            CREATE TABLE IF NOT EXISTS execution_traces (
                id               TEXT PRIMARY KEY,
                goal_run_id      TEXT,
                task_id          TEXT,
                task_type        TEXT,
                outcome          TEXT,
                quality_score    REAL,
                tool_sequence_json TEXT,
                metrics_json     TEXT,
                duration_ms      INTEGER,
                tokens_used      INTEGER,
                created_at       INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_execution_traces_task_type ON execution_traces(task_type, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_execution_traces_goal_run ON execution_traces(goal_run_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS causal_traces (
                id                    TEXT PRIMARY KEY,
                thread_id             TEXT,
                goal_run_id           TEXT,
                task_id               TEXT,
                decision_type         TEXT NOT NULL,
                selected_json         TEXT NOT NULL,
                rejected_options_json TEXT,
                context_hash          TEXT,
                causal_factors_json   TEXT,
                outcome_json          TEXT NOT NULL,
                model_used            TEXT,
                created_at            INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_causal_traces_decision_type ON causal_traces(decision_type, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_causal_traces_task_id ON causal_traces(task_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS memory_provenance (
                id            TEXT PRIMARY KEY,
                target        TEXT NOT NULL,
                mode          TEXT NOT NULL,
                source_kind   TEXT NOT NULL,
                content       TEXT NOT NULL,
                fact_keys_json TEXT NOT NULL DEFAULT '[]',
                thread_id     TEXT,
                task_id       TEXT,
                goal_run_id   TEXT,
                created_at    INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_memory_provenance_target_ts ON memory_provenance(target, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_memory_provenance_goal_run ON memory_provenance(goal_run_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS skill_variants (
                variant_id         TEXT PRIMARY KEY,
                skill_name         TEXT NOT NULL,
                variant_name       TEXT NOT NULL,
                relative_path      TEXT NOT NULL UNIQUE,
                parent_variant_id  TEXT,
                version            TEXT NOT NULL,
                context_tags_json  TEXT NOT NULL DEFAULT '[]',
                use_count          INTEGER NOT NULL DEFAULT 0,
                success_count      INTEGER NOT NULL DEFAULT 0,
                failure_count      INTEGER NOT NULL DEFAULT 0,
                status             TEXT NOT NULL DEFAULT 'active',
                last_used_at       INTEGER,
                created_at         INTEGER NOT NULL,
                updated_at         INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_skill_variants_name ON skill_variants(skill_name, status, updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_skill_variants_path ON skill_variants(relative_path);

            CREATE TABLE IF NOT EXISTS skill_variant_usage (
                usage_id           TEXT PRIMARY KEY,
                variant_id         TEXT NOT NULL,
                thread_id          TEXT,
                task_id            TEXT,
                goal_run_id        TEXT,
                context_tags_json  TEXT NOT NULL DEFAULT '[]',
                consulted_at       INTEGER NOT NULL,
                resolved_at        INTEGER,
                outcome            TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_skill_variant_usage_variant ON skill_variant_usage(variant_id, consulted_at DESC);
            CREATE INDEX IF NOT EXISTS idx_skill_variant_usage_resolution ON skill_variant_usage(task_id, goal_run_id, thread_id, resolved_at);

            CREATE TABLE IF NOT EXISTS collaboration_sessions (
                parent_task_id TEXT PRIMARY KEY,
                session_json   TEXT NOT NULL,
                updated_at     INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_collaboration_sessions_updated ON collaboration_sessions(updated_at DESC);

            CREATE TABLE IF NOT EXISTS agent_config_items (
                key_path   TEXT PRIMARY KEY,
                value_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_agent_config_items_updated ON agent_config_items(updated_at DESC);

            CREATE TABLE IF NOT EXISTS agent_config_updates (
                id         TEXT PRIMARY KEY,
                key_path   TEXT NOT NULL,
                value_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_agent_config_updates_key_ts ON agent_config_updates(key_path, updated_at DESC);

            CREATE TABLE IF NOT EXISTS heartbeat_history (
                id              TEXT PRIMARY KEY,
                cycle_timestamp INTEGER NOT NULL,
                checks_json     TEXT NOT NULL,
                synthesis_json  TEXT,
                actionable      INTEGER NOT NULL DEFAULT 0,
                digest_text     TEXT,
                llm_tokens_used INTEGER NOT NULL DEFAULT 0,
                duration_ms     INTEGER NOT NULL DEFAULT 0,
                status          TEXT NOT NULL DEFAULT 'completed'
            );
            CREATE INDEX IF NOT EXISTS idx_heartbeat_history_ts ON heartbeat_history(cycle_timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_heartbeat_history_actionable ON heartbeat_history(actionable, cycle_timestamp DESC);

            CREATE TABLE IF NOT EXISTS action_audit (
                id                TEXT PRIMARY KEY,
                timestamp         INTEGER NOT NULL,
                action_type       TEXT NOT NULL,
                summary           TEXT NOT NULL,
                explanation       TEXT,
                confidence        REAL,
                confidence_band   TEXT,
                causal_trace_id   TEXT,
                thread_id         TEXT,
                goal_run_id       TEXT,
                task_id           TEXT,
                raw_data_json     TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_action_audit_ts ON action_audit(timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_action_audit_type_ts ON action_audit(action_type, timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_action_audit_thread ON action_audit(thread_id, timestamp DESC);

            CREATE TABLE IF NOT EXISTS memory_tombstones (
                id TEXT PRIMARY KEY,
                target TEXT NOT NULL,
                original_content TEXT NOT NULL,
                fact_key TEXT,
                replaced_by TEXT,
                replaced_at INTEGER NOT NULL,
                source_kind TEXT NOT NULL,
                provenance_id TEXT,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_tombstones_created ON memory_tombstones(created_at);
            CREATE INDEX IF NOT EXISTS idx_tombstones_target ON memory_tombstones(target, created_at DESC);

            CREATE TABLE IF NOT EXISTS consolidation_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS plugins (
                name            TEXT PRIMARY KEY,
                version         TEXT NOT NULL,
                description     TEXT,
                author          TEXT,
                manifest_json   TEXT NOT NULL,
                install_source  TEXT NOT NULL DEFAULT 'local',
                enabled         INTEGER NOT NULL DEFAULT 1,
                installed_at    TEXT NOT NULL,
                updated_at      TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS plugin_settings (
                plugin_name     TEXT NOT NULL,
                key             TEXT NOT NULL,
                value           TEXT NOT NULL,
                is_secret       INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (plugin_name, key),
                FOREIGN KEY (plugin_name) REFERENCES plugins(name) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS plugin_credentials (
                plugin_name      TEXT NOT NULL,
                credential_type  TEXT NOT NULL,
                encrypted_value  BLOB,
                expires_at       TEXT,
                created_at       TEXT NOT NULL,
                updated_at       TEXT NOT NULL,
                PRIMARY KEY (plugin_name, credential_type),
                FOREIGN KEY (plugin_name) REFERENCES plugins(name) ON DELETE CASCADE
            );
            ",
        )?;
        // FTS5 virtual table for context archive full-text search.
        // Created separately since virtual tables need individual statements.
        connection.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS context_archive_fts USING fts5(summary, compressed_content, content=context_archive, content_rowid=rowid);",
        ).ok(); // .ok() — ignore if FTS5 not available in this SQLite build

        ensure_column(connection, "agent_tasks", "session_id", "TEXT")?;
        ensure_column(connection, "agent_threads", "metadata_json", "TEXT")?;
        ensure_column(connection, "agent_tasks", "scheduled_at", "INTEGER")?;
        ensure_column(connection, "agent_tasks", "goal_run_id", "TEXT")?;
        ensure_column(connection, "agent_tasks", "goal_run_title", "TEXT")?;
        ensure_column(connection, "agent_tasks", "goal_step_id", "TEXT")?;
        ensure_column(connection, "agent_tasks", "goal_step_title", "TEXT")?;
        ensure_column(connection, "agent_tasks", "parent_task_id", "TEXT")?;
        ensure_column(connection, "agent_tasks", "parent_thread_id", "TEXT")?;
        ensure_column(
            connection,
            "agent_tasks",
            "runtime",
            "TEXT NOT NULL DEFAULT 'daemon'",
        )?;
        ensure_column(connection, "goal_runs", "client_request_id", "TEXT")?;
        ensure_column(connection, "goal_run_events", "step_index", "INTEGER")?;
        ensure_column(connection, "goal_run_events", "todo_snapshot_json", "TEXT")?;
        // BEAT-09: user_action column for dismissal tracking in action_audit.
        ensure_column(connection, "action_audit", "user_action", "TEXT")?;
        connection.execute(
            "CREATE INDEX IF NOT EXISTS idx_agent_tasks_goal_run ON agent_tasks(goal_run_id, created_at DESC)",
            [],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    // ── Memory tombstone CRUD (Phase 5) ────────────────────────────────

    pub async fn insert_memory_tombstone(
        &self,
        id: &str,
        target: &str,
        original_content: &str,
        fact_key: Option<&str>,
        replaced_by: Option<&str>,
        source_kind: &str,
        provenance_id: Option<&str>,
        created_at: u64,
    ) -> Result<()> {
        let id = id.to_string();
        let target = target.to_string();
        let original_content = original_content.to_string();
        let fact_key = fact_key.map(str::to_string);
        let replaced_by = replaced_by.map(str::to_string);
        let source_kind = source_kind.to_string();
        let provenance_id = provenance_id.map(str::to_string);
        let now = created_at as i64;
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO memory_tombstones (id, target, original_content, fact_key, replaced_by, replaced_at, source_kind, provenance_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![id, target, original_content, fact_key, replaced_by, now, source_kind, provenance_id, now],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_memory_tombstones(
        &self,
        target: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MemoryTombstoneRow>> {
        let target = target.map(str::to_string);
        self.conn.call(move |conn| {
            if let Some(target) = target {
                let mut stmt = conn.prepare(
                    "SELECT id, target, original_content, fact_key, replaced_by, replaced_at, source_kind, provenance_id, created_at FROM memory_tombstones WHERE target = ?1 ORDER BY created_at DESC LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![target, limit as i64], |row| {
                    Ok(MemoryTombstoneRow {
                        id: row.get(0)?,
                        target: row.get(1)?,
                        original_content: row.get(2)?,
                        fact_key: row.get(3)?,
                        replaced_by: row.get(4)?,
                        replaced_at: row.get(5)?,
                        source_kind: row.get(6)?,
                        provenance_id: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, target, original_content, fact_key, replaced_by, replaced_at, source_kind, provenance_id, created_at FROM memory_tombstones ORDER BY created_at DESC LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit as i64], |row| {
                    Ok(MemoryTombstoneRow {
                        id: row.get(0)?,
                        target: row.get(1)?,
                        original_content: row.get(2)?,
                        fact_key: row.get(3)?,
                        replaced_by: row.get(4)?,
                        replaced_at: row.get(5)?,
                        source_kind: row.get(6)?,
                        provenance_id: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
            }
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_expired_tombstones(&self, max_age_ms: u64, now: u64) -> Result<usize> {
        let cutoff = (now as i64) - (max_age_ms as i64);
        self.conn.call(move |conn| {
            let count = conn.execute(
                "DELETE FROM memory_tombstones WHERE created_at < ?1",
                params![cutoff],
            )?;
            Ok(count)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn restore_tombstone(&self, tombstone_id: &str) -> Result<Option<MemoryTombstoneRow>> {
        let tombstone_id = tombstone_id.to_string();
        self.conn.call(move |conn| {
            let row: Option<MemoryTombstoneRow> = conn.query_row(
                "SELECT id, target, original_content, fact_key, replaced_by, replaced_at, source_kind, provenance_id, created_at FROM memory_tombstones WHERE id = ?1",
                params![tombstone_id],
                |row| {
                    Ok(MemoryTombstoneRow {
                        id: row.get(0)?,
                        target: row.get(1)?,
                        original_content: row.get(2)?,
                        fact_key: row.get(3)?,
                        replaced_by: row.get(4)?,
                        replaced_at: row.get(5)?,
                        source_kind: row.get(6)?,
                        provenance_id: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                },
            ).optional()?;
            if row.is_some() {
                conn.execute("DELETE FROM memory_tombstones WHERE id = ?1", params![tombstone_id])?;
            }
            Ok(row)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    // ── Consolidation state CRUD (Phase 5) ───────────────────────────────

    pub async fn get_consolidation_state(&self, key: &str) -> Result<Option<String>> {
        let key = key.to_string();
        self.conn.call(move |conn| {
            let value: Option<String> = conn.query_row(
                "SELECT value FROM consolidation_state WHERE key = ?1",
                params![key],
                |row| row.get(0),
            ).optional()?;
            Ok(value)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn set_consolidation_state(&self, key: &str, value: &str, now: u64) -> Result<()> {
        let key = key.to_string();
        let value = value.to_string();
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO consolidation_state (key, value, updated_at) VALUES (?1, ?2, ?3)",
                params![key, value, now as i64],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// List consolidation_state entries whose key starts with `prefix`.
    pub async fn list_consolidation_state_by_prefix(
        &self,
        prefix: &str,
    ) -> Result<Vec<(String, String)>> {
        let like = format!("{}%", prefix);
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT key, value FROM consolidation_state WHERE key LIKE ?1",
                )?;
                let rows = stmt.query_map(params![like], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
                Ok(rows.filter_map(|r| r.ok()).collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// List skill variants matching a given status string, up to `limit` rows.
    pub async fn list_skill_variants_by_status(
        &self,
        status: &str,
        limit: usize,
    ) -> Result<Vec<SkillVariantRecord>> {
        let status = status.to_string();
        let limit = limit.clamp(1, 200) as i64;
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at \
                     FROM skill_variants WHERE status = ?1 ORDER BY updated_at ASC LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![status, limit], map_skill_variant_row)?;
                Ok(rows.filter_map(|r| r.ok()).collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    // ── Successful trace queries (Phase 5) ───────────────────────────────

    pub async fn list_recent_successful_traces(
        &self,
        after_timestamp: u64,
        limit: usize,
    ) -> Result<Vec<ExecutionTraceRow>> {
        self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, goal_run_id, task_id, task_type, outcome, quality_score, tool_sequence_json, metrics_json, duration_ms, tokens_used, created_at FROM execution_traces WHERE outcome = 'success' AND created_at > ?1 ORDER BY created_at ASC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![after_timestamp as i64, limit as i64], |row| {
                Ok(ExecutionTraceRow {
                    id: row.get(0)?,
                    goal_run_id: row.get(1)?,
                    task_id: row.get(2)?,
                    task_type: row.get(3)?,
                    outcome: row.get(4)?,
                    quality_score: row.get(5)?,
                    tool_sequence_json: row.get(6)?,
                    metrics_json: row.get(7)?,
                    duration_ms: row.get(8)?,
                    tokens_used: row.get(9)?,
                    created_at: row.get(10)?,
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn append_telemetry(&self, kind: &str, payload: serde_json::Value) -> Result<()> {
        let line = serde_json::to_string(&payload)?;
        let telemetry_dir = self.telemetry_dir.clone();
        let worm_dir = self.worm_dir.clone();
        let log_path = telemetry_dir.join(format!("{}.jsonl", kind));
        let worm_path = worm_dir.join(format!("{}-ledger.jsonl", kind));

        append_line(&log_path, &line)?;

        let (prev_hash, seq) = match self.get_worm_chain_tip(kind).await? {
            Some(tip) => (tip.hash, tip.seq + 1),
            None => {
                let (prev_hash, seq) = read_last_worm_entry(&worm_path);
                (prev_hash, seq as i64)
            }
        };

        let payload_json = serde_json::to_string(&payload)?;
        let hash = hex_hash(&format!("{}{}", prev_hash, payload_json));
        let worm_line = serde_json::to_string(&json!({
            "seq": seq,
            "prev_hash": prev_hash,
            "hash": hash,
            "payload": payload,
        }))?;
        append_line(&worm_path, &worm_line)?;
        self.set_worm_chain_tip(kind, seq, &hash).await?;
        Ok(())
    }

    /// Detect sequences of 3+ consecutive successful managed commands
    /// that completed within a 5-minute window.
    pub async fn detect_skill_candidates(&self) -> Result<Vec<(String, Vec<HistorySearchHit>)>> {
        self.conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, kind, title, excerpt, path, timestamp FROM history_entries \
             WHERE kind = 'managed-command' \
             ORDER BY timestamp DESC LIMIT 20",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(HistorySearchHit {
                id: row.get(0)?,
                kind: row.get(1)?,
                title: row.get(2)?,
                excerpt: row.get(3)?,
                path: row.get(4)?,
                timestamp: row.get::<_, i64>(5)? as u64,
                score: 0.0,
            })
        })?;

        let hits: Vec<_> = rows.filter_map(|r| r.ok()).collect();
        let mut candidates = Vec::new();

        // Find runs of 3+ successful commands within 5-minute windows
        let mut run: Vec<HistorySearchHit> = Vec::new();
        for hit in &hits {
            // Check if excerpt indicates success (exit=Some(0))
            if hit.excerpt.contains("exit=Some(0)") {
                if run.is_empty() || (run.last().unwrap().timestamp.abs_diff(hit.timestamp) < 300) {
                    run.push(hit.clone());
                } else {
                    if run.len() >= 3 {
                        let title = format!("Workflow: {}", run.first().unwrap().title);
                        candidates.push((title, run.clone()));
                    }
                    run = vec![hit.clone()];
                }
            } else {
                if run.len() >= 3 {
                    let title = format!("Workflow: {}", run.first().unwrap().title);
                    candidates.push((title, run.clone()));
                }
                run.clear();
            }
        }
        if run.len() >= 3 {
            let title = format!("Workflow: {}", run.first().unwrap().title);
            candidates.push((title, run));
        }

        Ok(candidates)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Verify the hash-chain integrity of all WORM telemetry ledger files.
    pub fn verify_worm_integrity(&self) -> Result<Vec<WormIntegrityResult>> {
        let ledger_kinds = ["operational", "cognitive", "contextual", "provenance"];
        let mut results = Vec::with_capacity(ledger_kinds.len());

        for kind in &ledger_kinds {
            let worm_path = self.worm_dir.join(format!("{}-ledger.jsonl", kind));
            results.push(verify_ledger_file(kind, &worm_path));
        }

        Ok(results)
    }

    fn skills_root(&self) -> PathBuf {
        self.skill_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.skill_dir.clone())
    }

    fn provenance_signing_key(&self) -> Result<String> {
        let path = self
            .telemetry_dir
            .parent()
            .unwrap_or(&self.telemetry_dir)
            .join("provenance-signing.key");
        if path.exists() {
            return Ok(std::fs::read_to_string(&path)?.trim().to_string());
        }
        let key = format!(
            "tamux-prov-{}-{}",
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4()
        );
        std::fs::write(&path, &key)?;
        Ok(key)
    }

    pub async fn upsert_subagent_metrics(
        &self,
        task_id: &str,
        parent_task_id: Option<&str>,
        thread_id: Option<&str>,
        tool_calls_total: i64,
        tool_calls_succeeded: i64,
        tool_calls_failed: i64,
        tokens_consumed: i64,
        context_budget_tokens: Option<i64>,
        progress_rate: f64,
        last_progress_at: Option<u64>,
        stuck_score: f64,
        health_state: &str,
        created_at: u64,
        updated_at: u64,
    ) -> Result<()> {
        let task_id = task_id.to_string();
        let parent_task_id = parent_task_id.map(str::to_string);
        let thread_id = thread_id.map(str::to_string);
        let health_state = health_state.to_string();
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO subagent_metrics \
             (task_id, parent_task_id, thread_id, tool_calls_total, tool_calls_succeeded, tool_calls_failed, tokens_consumed, context_budget_tokens, progress_rate, last_progress_at, stuck_score, health_state, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                task_id,
                parent_task_id,
                thread_id,
                tool_calls_total,
                tool_calls_succeeded,
                tool_calls_failed,
                tokens_consumed,
                context_budget_tokens,
                progress_rate,
                last_progress_at.map(|v| v as i64),
                stuck_score,
                health_state,
                created_at as i64,
                updated_at as i64,
            ],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn record_memory_provenance(&self, record: &MemoryProvenanceRecord<'_>) -> Result<()> {
        let fact_keys_json = serde_json::to_string(record.fact_keys)?;
        let id = record.id.to_string();
        let target = record.target.to_string();
        let mode = record.mode.to_string();
        let source_kind = record.source_kind.to_string();
        let content = record.content.to_string();
        let thread_id = record.thread_id.map(str::to_string);
        let task_id = record.task_id.map(str::to_string);
        let goal_run_id = record.goal_run_id.map(str::to_string);
        let created_at = record.created_at;
        let fact_keys_owned: Vec<String> = record.fact_keys.to_vec();

        let record = record.clone();
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO memory_provenance \
                 (id, target, mode, source_kind, content, fact_keys_json, thread_id, task_id, goal_run_id, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    id,
                    target,
                    mode,
                    source_kind,
                    content,
                    fact_keys_json,
                    thread_id,
                    task_id,
                    goal_run_id,
                    created_at as i64,
                ],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        self.append_telemetry(
            "cognitive",
            json!({
                "timestamp": record.created_at as i64,
                "kind": "memory_write",
                "target": record.target,
                "mode": record.mode,
                "source_kind": record.source_kind,
                "thread_id": record.thread_id,
                "task_id": record.task_id,
                "goal_run_id": record.goal_run_id,
                "fact_keys": fact_keys_owned,
            }),
        ).await?;
        Ok(())
    }

    pub async fn memory_provenance_report(
        &self,
        target: Option<&str>,
        limit: usize,
    ) -> Result<MemoryProvenanceReport> {
        let target = target.map(str::to_string);
        self.conn.call(move |conn| {
        let limit = limit.clamp(1, 200);
        let normalized_target = target
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let mut entries = Vec::new();
        let mut summary_by_target = BTreeMap::new();
        let mut summary_by_source = BTreeMap::new();
        let mut summary_by_status = BTreeMap::new();

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        match normalized_target.as_deref() {
            Some(target) => {
                let mut stmt = conn.prepare(
                    "SELECT id, target, mode, source_kind, content, fact_keys_json, thread_id, task_id, goal_run_id, created_at \
                     FROM memory_provenance WHERE target = ?1 ORDER BY created_at DESC LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![target, limit as i64], |row| {
                    Ok(memory_provenance_entry_from_row(row, now_ms))
                })?;
                for row in rows {
                    let entry = row?;
                    *summary_by_target.entry(entry.target.clone()).or_insert(0) += 1;
                    *summary_by_source
                        .entry(entry.source_kind.clone())
                        .or_insert(0) += 1;
                    *summary_by_status.entry(entry.status.clone()).or_insert(0) += 1;
                    entries.push(entry);
                }
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT id, target, mode, source_kind, content, fact_keys_json, thread_id, task_id, goal_run_id, created_at \
                     FROM memory_provenance ORDER BY created_at DESC LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit as i64], |row| {
                    Ok(memory_provenance_entry_from_row(row, now_ms))
                })?;
                for row in rows {
                    let entry = row?;
                    *summary_by_target.entry(entry.target.clone()).or_insert(0) += 1;
                    *summary_by_source
                        .entry(entry.source_kind.clone())
                        .or_insert(0) += 1;
                    *summary_by_status.entry(entry.status.clone()).or_insert(0) += 1;
                    entries.push(entry);
                }
            }
        }

        Ok(MemoryProvenanceReport {
            total_entries: entries.len(),
            target_filter: normalized_target,
            summary_by_target,
            summary_by_source,
            summary_by_status,
            entries,
        })
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn record_provenance_event(&self, record: &ProvenanceEventRecord<'_>) -> Result<()> {
        let telemetry_path = self.telemetry_dir.join("provenance.jsonl");
        let previous_entry = read_last_provenance_entry(&telemetry_path);
        let sequence = previous_entry
            .as_ref()
            .map(|entry| entry.sequence.saturating_add(1))
            .unwrap_or(0);
        let prev_hash = previous_entry
            .as_ref()
            .map(|entry| entry.entry_hash.clone())
            .unwrap_or_else(|| "genesis".to_string());
        let entry_hash = compute_provenance_hash(
            sequence,
            record.created_at,
            record.event_type,
            record.summary,
            record.details,
            &prev_hash,
            record.agent_id,
            record.goal_run_id,
            record.task_id,
            record.thread_id,
            record.approval_id,
            record.causal_trace_id,
            record.compliance_mode,
        );
        let signature = if record.sign {
            Some(sign_provenance_hash(
                &self.provenance_signing_key()?,
                &entry_hash,
            ))
        } else {
            None
        };
        let entry = ProvenanceLogEntry {
            sequence,
            timestamp: record.created_at,
            event_type: record.event_type.to_string(),
            summary: record.summary.to_string(),
            details: record.details.clone(),
            prev_hash,
            entry_hash,
            signature,
            agent_id: record.agent_id.to_string(),
            goal_run_id: record.goal_run_id.map(str::to_string),
            task_id: record.task_id.map(str::to_string),
            thread_id: record.thread_id.map(str::to_string),
            approval_id: record.approval_id.map(str::to_string),
            causal_trace_id: record.causal_trace_id.map(str::to_string),
            compliance_mode: record.compliance_mode.to_string(),
        };

        self.append_telemetry("provenance", serde_json::to_value(entry)?).await?;
        Ok(())
    }

    pub fn provenance_report(&self, limit: usize) -> Result<ProvenanceReport> {
        let entries = read_provenance_entries(&self.telemetry_dir.join("provenance.jsonl"))?;
        let signing_key = self.provenance_signing_key().ok();
        let mut summary_by_event = BTreeMap::new();
        let mut valid_hash_entries = 0usize;
        let mut valid_signature_entries = 0usize;
        let mut valid_chain_entries = 0usize;
        let mut signed_entries = 0usize;
        let mut previous_hash = "genesis".to_string();
        let mut report_entries = Vec::new();

        for entry in entries.iter() {
            let expected_hash = compute_provenance_hash(
                entry.sequence,
                entry.timestamp,
                &entry.event_type,
                &entry.summary,
                &entry.details,
                &entry.prev_hash,
                &entry.agent_id,
                entry.goal_run_id.as_deref(),
                entry.task_id.as_deref(),
                entry.thread_id.as_deref(),
                entry.approval_id.as_deref(),
                entry.causal_trace_id.as_deref(),
                &entry.compliance_mode,
            );
            let hash_valid = entry.entry_hash == expected_hash;
            let chain_valid = entry.prev_hash == previous_hash;
            let signature_valid = match (&entry.signature, signing_key.as_deref()) {
                (Some(signature), Some(key)) => {
                    signed_entries += 1;
                    *signature == sign_provenance_hash(key, &entry.entry_hash)
                }
                (Some(_), None) => {
                    signed_entries += 1;
                    false
                }
                (None, _) => true,
            };
            if hash_valid {
                valid_hash_entries += 1;
            }
            if chain_valid {
                valid_chain_entries += 1;
            }
            if signature_valid {
                valid_signature_entries += 1;
            }
            *summary_by_event
                .entry(entry.event_type.clone())
                .or_insert(0) += 1;
            report_entries.push(ProvenanceReportEntry {
                sequence: entry.sequence,
                timestamp: entry.timestamp,
                event_type: entry.event_type.clone(),
                summary: entry.summary.clone(),
                agent_id: entry.agent_id.clone(),
                goal_run_id: entry.goal_run_id.clone(),
                task_id: entry.task_id.clone(),
                thread_id: entry.thread_id.clone(),
                approval_id: entry.approval_id.clone(),
                causal_trace_id: entry.causal_trace_id.clone(),
                compliance_mode: entry.compliance_mode.clone(),
                hash_valid,
                signature_valid,
                chain_valid,
            });
            previous_hash = entry.entry_hash.clone();
        }

        report_entries.reverse();
        report_entries.truncate(limit.clamp(1, 500));

        Ok(ProvenanceReport {
            total_entries: entries.len(),
            signed_entries,
            valid_hash_entries,
            valid_signature_entries,
            valid_chain_entries,
            summary_by_event,
            entries: report_entries,
        })
    }

    pub fn generate_soc2_artifact(&self, period_days: u32) -> Result<PathBuf> {
        let entries = read_provenance_entries(&self.telemetry_dir.join("provenance.jsonl"))?;
        let cutoff = now_ts()
            .saturating_sub((period_days as u64).saturating_mul(86_400))
            .saturating_mul(1000);
        let recent = entries
            .into_iter()
            .filter(|entry| entry.timestamp >= cutoff)
            .collect::<Vec<_>>();
        let integrity = self.verify_worm_integrity()?;
        let artifact = json!({
            "generated_at": now_ts() * 1000,
            "period_days": period_days,
            "change_management": recent.iter().filter(|entry| matches!(entry.event_type.as_str(), "goal_created" | "plan_generated" | "step_started" | "step_completed" | "step_failed" | "replan_triggered" | "recovery_triggered" | "tool_call")).collect::<Vec<_>>(),
            "system_access": recent.iter().filter(|entry| matches!(entry.event_type.as_str(), "approval_requested" | "approval_granted" | "approval_denied" | "escalation_triggered")).collect::<Vec<_>>(),
            "data_integrity": integrity.iter().map(|item| json!({
                "kind": item.kind,
                "valid": item.valid,
                "total_entries": item.total_entries,
                "message": item.message,
            })).collect::<Vec<_>>(),
            "incident_log": recent.iter().filter(|entry| matches!(entry.event_type.as_str(), "step_failed" | "recovery_triggered" | "escalation_triggered")).collect::<Vec<_>>(),
        });

        let audit_dir = self
            .telemetry_dir
            .parent()
            .unwrap_or(&self.telemetry_dir)
            .join("audit")
            .join("soc2");
        std::fs::create_dir_all(&audit_dir)?;
        let path = audit_dir.join(format!("soc2-artifact-{}.json", now_ts()));
        std::fs::write(&path, serde_json::to_string_pretty(&artifact)?)?;
        Ok(path)
    }

    pub async fn get_subagent_metrics(&self, task_id: &str) -> Result<Option<SubagentMetrics>> {
        let task_id = task_id.to_string();
        self.conn.call(move |conn| {
        conn
            .query_row(
                "SELECT task_id, parent_task_id, thread_id, tool_calls_total, tool_calls_succeeded, tool_calls_failed, tokens_consumed, context_budget_tokens, progress_rate, last_progress_at, stuck_score, health_state, created_at, updated_at \
                 FROM subagent_metrics WHERE task_id = ?1",
                params![task_id],
                |row| {
                    Ok(SubagentMetrics {
                        task_id: row.get(0)?,
                        parent_task_id: row.get(1)?,
                        thread_id: row.get(2)?,
                        tool_calls_total: row.get(3)?,
                        tool_calls_succeeded: row.get(4)?,
                        tool_calls_failed: row.get(5)?,
                        tokens_consumed: row.get(6)?,
                        context_budget_tokens: row.get(7)?,
                        progress_rate: row.get(8)?,
                        last_progress_at: row.get::<_, Option<i64>>(9)?.map(|v| v as u64),
                        stuck_score: row.get(10)?,
                        health_state: row.get(11)?,
                        created_at: row.get::<_, i64>(12)? as u64,
                        updated_at: row.get::<_, i64>(13)? as u64,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Insert or replace a checkpoint row in the `agent_checkpoints` table.
    pub async fn upsert_checkpoint(
        &self,
        id: &str,
        goal_run_id: &str,
        thread_id: Option<&str>,
        task_id: Option<&str>,
        checkpoint_type: CheckpointType,
        state_json: &str,
        context_summary: Option<&str>,
        created_at: u64,
    ) -> Result<()> {
        let id = id.to_string();
        let goal_run_id = goal_run_id.to_string();
        let thread_id = thread_id.map(str::to_string);
        let task_id = task_id.map(str::to_string);
        let state_json = state_json.to_string();
        let context_summary = context_summary.map(str::to_string);
        self.conn.call(move |conn| {
        let type_str = match checkpoint_type {
            CheckpointType::PreStep => "pre_step",
            CheckpointType::PostStep => "post_step",
            CheckpointType::Manual => "manual",
            CheckpointType::PreRecovery => "pre_recovery",
            CheckpointType::Periodic => "periodic",
        };
        conn.execute(
            "INSERT OR REPLACE INTO agent_checkpoints \
             (id, goal_run_id, thread_id, task_id, checkpoint_type, state_json, context_summary, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                goal_run_id,
                thread_id,
                task_id,
                type_str,
                state_json,
                context_summary,
                created_at as i64,
            ],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Load all checkpoint JSON blobs for a given goal run, ordered by
    /// `created_at` descending.
    pub async fn list_checkpoints_for_goal_run(&self, goal_run_id: &str) -> Result<Vec<String>> {
        let goal_run_id = goal_run_id.to_string();
        self.conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT state_json FROM agent_checkpoints WHERE goal_run_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt
            .query_map(params![goal_run_id], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Load a single checkpoint by ID.
    pub async fn get_checkpoint(&self, id: &str) -> Result<Option<String>> {
        let id = id.to_string();
        self.conn.call(move |conn| {
        conn
            .query_row(
                "SELECT state_json FROM agent_checkpoints WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Delete checkpoints by their IDs.
    pub async fn delete_checkpoints(&self, ids: &[&str]) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
        self.conn.call(move |conn| {
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
        let sql = format!(
            "DELETE FROM agent_checkpoints WHERE id IN ({})",
            placeholders.join(", ")
        );
        let params: Vec<&dyn rusqlite::types::ToSql> = ids
            .iter()
            .map(|id| id as &dyn rusqlite::types::ToSql)
            .collect();
        let deleted = conn.execute(&sql, params.as_slice())?;
        Ok(deleted)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    // -- Health log --

    pub async fn insert_health_log(
        &self,
        id: &str,
        entity_type: &str,
        entity_id: &str,
        health_state: &str,
        indicators_json: Option<&str>,
        intervention: Option<&str>,
        created_at: u64,
    ) -> Result<()> {
        let id = id.to_string();
        let entity_type = entity_type.to_string();
        let entity_id = entity_id.to_string();
        let health_state = health_state.to_string();
        let indicators_json = indicators_json.map(str::to_string);
        let intervention = intervention.map(str::to_string);
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT INTO agent_health_log (id, entity_type, entity_id, health_state, indicators_json, intervention, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, entity_type, entity_id, health_state, indicators_json, intervention, created_at as i64],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_health_log(
        &self,
        limit: u32,
    ) -> Result<
        Vec<(
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            u64,
        )>,
    > {
        self.conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, entity_type, entity_id, health_state, indicators_json, intervention, created_at FROM agent_health_log ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, i64>(6)? as u64,
            ))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    // -- Context archive --

    pub async fn insert_context_archive(
        &self,
        id: &str,
        thread_id: &str,
        original_role: Option<&str>,
        compressed_content: &str,
        summary: Option<&str>,
        relevance_score: f64,
        token_count_original: u32,
        token_count_compressed: u32,
        metadata_json: Option<&str>,
        archived_at: u64,
    ) -> Result<()> {
        let id = id.to_string();
        let thread_id = thread_id.to_string();
        let original_role = original_role.map(str::to_string);
        let compressed_content = compressed_content.to_string();
        let summary = summary.map(str::to_string);
        let metadata_json = metadata_json.map(str::to_string);
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO context_archive (id, thread_id, original_role, compressed_content, summary, relevance_score, token_count_original, token_count_compressed, metadata_json, archived_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![id, thread_id, original_role, compressed_content, summary, relevance_score, token_count_original as i64, token_count_compressed as i64, metadata_json, archived_at as i64],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn search_context_archive(
        &self,
        thread_id: &str,
        query: &str,
        limit: u32,
    ) -> Result<Vec<String>> {
        let thread_id = thread_id.to_string();
        let query = query.to_string();
        self.conn.call(move |conn| {
        // Try FTS5 first, fall back to LIKE search
        let fts_result: Result<Vec<String>> = (|| {
            let mut stmt = conn.prepare(
                "SELECT ca.id FROM context_archive ca JOIN context_archive_fts fts ON ca.rowid = fts.rowid WHERE ca.thread_id = ?1 AND context_archive_fts MATCH ?2 ORDER BY rank LIMIT ?3",
            )?;
            let rows = stmt.query_map(params![thread_id, query, limit], |row| row.get(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(Into::into)
        })();

        match fts_result {
            Ok(ids) => Ok(ids),
            Err(_) => {
                // Fallback: simple LIKE search
                let like_pattern = format!("%{query}%");
                let mut stmt = conn.prepare(
                    "SELECT id FROM context_archive WHERE thread_id = ?1 AND (compressed_content LIKE ?2 OR summary LIKE ?2) ORDER BY archived_at DESC LIMIT ?3",
                )?;
                let rows =
                    stmt.query_map(params![thread_id, like_pattern, limit], |row| row.get(0))?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            }
        }
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// List the most recent context archive entries for a thread.
    pub async fn list_context_archive_entries(
        &self,
        thread_id: &str,
        limit: usize,
    ) -> Result<Vec<ContextArchiveRow>> {
        let thread_id = thread_id.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, thread_id, original_role, compressed_content, summary, \
                     relevance_score, token_count_original, token_count_compressed, \
                     metadata_json, archived_at, last_accessed_at \
                     FROM context_archive WHERE thread_id = ?1 \
                     ORDER BY archived_at DESC LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![thread_id, limit as i64], |row| {
                    Ok(ContextArchiveRow {
                        id: row.get(0)?,
                        thread_id: row.get(1)?,
                        original_role: row.get(2)?,
                        compressed_content: row.get(3)?,
                        summary: row.get(4)?,
                        relevance_score: row.get::<_, f64>(5).unwrap_or(0.0),
                        token_count_original: row.get::<_, i64>(6).unwrap_or(0),
                        token_count_compressed: row.get::<_, i64>(7).unwrap_or(0),
                        metadata_json: row.get(8)?,
                        archived_at: row.get(9)?,
                        last_accessed_at: row.get(10)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    // -- Execution traces --

    pub async fn insert_execution_trace(
        &self,
        id: &str,
        goal_run_id: Option<&str>,
        task_id: Option<&str>,
        task_type: &str,
        outcome: &str,
        quality_score: Option<f64>,
        tool_sequence_json: &str,
        metrics_json: &str,
        duration_ms: u64,
        tokens_used: u32,
        created_at: u64,
    ) -> Result<()> {
        let id = id.to_string();
        let goal_run_id = goal_run_id.map(str::to_string);
        let task_id = task_id.map(str::to_string);
        let task_type = task_type.to_string();
        let outcome = outcome.to_string();
        let tool_sequence_json = tool_sequence_json.to_string();
        let metrics_json = metrics_json.to_string();
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO execution_traces (id, goal_run_id, task_id, task_type, outcome, quality_score, tool_sequence_json, metrics_json, duration_ms, tokens_used, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![id, goal_run_id, task_id, task_type, outcome, quality_score, tool_sequence_json, metrics_json, duration_ms as i64, tokens_used as i64, created_at as i64],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_execution_traces(
        &self,
        task_type: Option<&str>,
        limit: u32,
    ) -> Result<Vec<String>> {
        let task_type = task_type.map(str::to_string);
        self.conn.call(move |conn| {
        if let Some(task_type) = task_type {
            let mut stmt = conn.prepare(
                "SELECT metrics_json FROM execution_traces WHERE task_type = ?1 ORDER BY created_at DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![task_type, limit], |row| row.get(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(Into::into)
        } else {
            let mut stmt = conn.prepare(
                "SELECT metrics_json FROM execution_traces ORDER BY created_at DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], |row| row.get(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(Into::into)
        }
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn insert_causal_trace(
        &self,
        id: &str,
        thread_id: Option<&str>,
        goal_run_id: Option<&str>,
        task_id: Option<&str>,
        decision_type: &str,
        selected_json: &str,
        rejected_options_json: &str,
        context_hash: &str,
        causal_factors_json: &str,
        outcome_json: &str,
        model_used: Option<&str>,
        created_at: u64,
    ) -> Result<()> {
        let id = id.to_string();
        let thread_id = thread_id.map(str::to_string);
        let goal_run_id = goal_run_id.map(str::to_string);
        let task_id = task_id.map(str::to_string);
        let decision_type = decision_type.to_string();
        let selected_json = selected_json.to_string();
        let rejected_options_json = rejected_options_json.to_string();
        let context_hash = context_hash.to_string();
        let causal_factors_json = causal_factors_json.to_string();
        let outcome_json = outcome_json.to_string();
        let model_used = model_used.map(str::to_string);
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO causal_traces (id, thread_id, goal_run_id, task_id, decision_type, selected_json, rejected_options_json, context_hash, causal_factors_json, outcome_json, model_used, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                id,
                thread_id,
                goal_run_id,
                task_id,
                decision_type,
                selected_json,
                rejected_options_json,
                context_hash,
                causal_factors_json,
                outcome_json,
                model_used,
                created_at as i64
            ],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_causal_traces_for_option(
        &self,
        option_type: &str,
        limit: u32,
    ) -> Result<Vec<String>> {
        let option_type = option_type.to_string();
        self.conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT outcome_json
             FROM causal_traces
             WHERE json_extract(selected_json, '$.option_type') = ?1
             ORDER BY created_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![option_type, limit], |row| row.get(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_recent_causal_trace_records(
        &self,
        option_type: &str,
        limit: u32,
    ) -> Result<Vec<CausalTraceRecord>> {
        let option_type = option_type.to_string();
        self.conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT selected_json, causal_factors_json, outcome_json, created_at
             FROM causal_traces
             WHERE json_extract(selected_json, '$.option_type') = ?1
             ORDER BY created_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![option_type, limit], |row| {
            Ok(CausalTraceRecord {
                selected_json: row.get(0)?,
                causal_factors_json: row.get(1)?,
                outcome_json: row.get(2)?,
                created_at: row.get::<_, i64>(3)? as u64,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn settle_skill_selection_causal_traces(
        &self,
        thread_id: Option<&str>,
        task_id: Option<&str>,
        goal_run_id: Option<&str>,
        outcome_json: &str,
    ) -> Result<usize> {
        let thread_id = thread_id.map(str::to_string);
        let task_id = task_id.map(str::to_string);
        let goal_run_id = goal_run_id.map(str::to_string);
        let outcome_json = outcome_json.to_string();
        self.conn.call(move |conn| {
        let updated = conn.execute(
            "UPDATE causal_traces
             SET outcome_json = ?4
             WHERE decision_type = 'skill_selection'
               AND json_extract(outcome_json, '$.type') = 'unresolved'
               AND (
                    (?1 IS NOT NULL AND task_id = ?1) OR
                    (?2 IS NOT NULL AND goal_run_id = ?2) OR
                    (?3 IS NOT NULL AND task_id IS NULL AND goal_run_id IS NULL AND thread_id = ?3)
               )",
            params![task_id, goal_run_id, thread_id, outcome_json],
        )?;
        Ok(updated)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Persist a heartbeat cycle result to SQLite. Per D-12.
    pub async fn insert_heartbeat_history(
        &self,
        id: &str,
        cycle_timestamp: i64,
        checks_json: &str,
        synthesis_json: Option<&str>,
        actionable: bool,
        digest_text: Option<&str>,
        llm_tokens_used: i64,
        duration_ms: i64,
        status: &str,
    ) -> Result<()> {
        let id = id.to_string();
        let checks_json = checks_json.to_string();
        let synthesis_json = synthesis_json.map(|s| s.to_string());
        let digest_text = digest_text.map(|s| s.to_string());
        let status = status.to_string();
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO heartbeat_history \
                 (id, cycle_timestamp, checks_json, synthesis_json, actionable, digest_text, llm_tokens_used, duration_ms, status) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id, cycle_timestamp, checks_json, synthesis_json,
                    actionable as i32, digest_text, llm_tokens_used, duration_ms, status
                ],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("insert_heartbeat_history: {e}"))
    }

    /// List recent heartbeat history entries. Per D-12.
    pub async fn list_heartbeat_history(&self, limit: usize) -> Result<Vec<HeartbeatHistoryRow>> {
        self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, cycle_timestamp, checks_json, synthesis_json, actionable, \
                 digest_text, llm_tokens_used, duration_ms, status \
                 FROM heartbeat_history ORDER BY cycle_timestamp DESC LIMIT ?1"
            )?;
            let rows = stmt.query_map([limit as i64], |row| {
                Ok(HeartbeatHistoryRow {
                    id: row.get(0)?,
                    cycle_timestamp: row.get(1)?,
                    checks_json: row.get(2)?,
                    synthesis_json: row.get(3)?,
                    actionable: row.get::<_, i32>(4)? != 0,
                    digest_text: row.get(5)?,
                    llm_tokens_used: row.get(6)?,
                    duration_ms: row.get(7)?,
                    status: row.get(8)?,
                })
            })?.collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows)
        }).await.map_err(|e| anyhow::anyhow!("list_heartbeat_history: {e}"))
    }

    // -----------------------------------------------------------------------
    // Action audit CRUD (per D-06/TRNS-03)
    // -----------------------------------------------------------------------

    /// Insert or replace an action audit entry.
    pub async fn insert_action_audit(&self, entry: &AuditEntryRow) -> Result<()> {
        let entry = entry.clone();
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO action_audit \
                 (id, timestamp, action_type, summary, explanation, confidence, \
                  confidence_band, causal_trace_id, thread_id, goal_run_id, task_id, raw_data_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                rusqlite::params![
                    entry.id,
                    entry.timestamp,
                    entry.action_type,
                    entry.summary,
                    entry.explanation,
                    entry.confidence,
                    entry.confidence_band,
                    entry.causal_trace_id,
                    entry.thread_id,
                    entry.goal_run_id,
                    entry.task_id,
                    entry.raw_data_json,
                ],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("insert_action_audit: {e}"))
    }

    /// List action audit entries with optional filters.
    pub async fn list_action_audit(
        &self,
        action_types: Option<&[String]>,
        since: Option<i64>,
        limit: usize,
    ) -> Result<Vec<AuditEntryRow>> {
        let action_types = action_types.map(|a| a.to_vec());
        self.conn.call(move |conn| {
            let mut sql = String::from(
                "SELECT id, timestamp, action_type, summary, explanation, confidence, \
                 confidence_band, causal_trace_id, thread_id, goal_run_id, task_id, raw_data_json \
                 FROM action_audit"
            );
            let mut conditions: Vec<String> = Vec::new();

            if let Some(ref types) = action_types {
                if !types.is_empty() {
                    let placeholders: Vec<String> = types.iter().enumerate()
                        .map(|(i, _)| format!("?{}", i + 100))
                        .collect();
                    conditions.push(format!("action_type IN ({})", placeholders.join(",")));
                }
            }
            if since.is_some() {
                conditions.push("timestamp >= ?50".to_string());
            }
            if !conditions.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&conditions.join(" AND "));
            }
            sql.push_str(" ORDER BY timestamp DESC LIMIT ?99");

            let mut stmt = conn.prepare(&sql)?;

            // Bind parameters dynamically
            let mut param_idx = 1;
            let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

            // Rewrite: use a simpler approach with raw SQL and named params
            drop(stmt);
            drop(params);

            // Build the query more simply
            let mut where_parts: Vec<String> = Vec::new();
            let mut bind_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

            if let Some(ref types) = action_types {
                if !types.is_empty() {
                    let placeholders: Vec<String> = (0..types.len())
                        .map(|i| format!("?{}", i + 1))
                        .collect();
                    where_parts.push(format!("action_type IN ({})", placeholders.join(",")));
                    for t in types {
                        bind_values.push(Box::new(t.clone()));
                    }
                }
            }
            let since_idx = bind_values.len() + 1;
            if let Some(ts) = since {
                where_parts.push(format!("timestamp >= ?{}", since_idx));
                bind_values.push(Box::new(ts));
            }
            let limit_idx = bind_values.len() + 1;
            bind_values.push(Box::new(limit as i64));

            let mut final_sql = String::from(
                "SELECT id, timestamp, action_type, summary, explanation, confidence, \
                 confidence_band, causal_trace_id, thread_id, goal_run_id, task_id, raw_data_json \
                 FROM action_audit"
            );
            if !where_parts.is_empty() {
                final_sql.push_str(" WHERE ");
                final_sql.push_str(&where_parts.join(" AND "));
            }
            final_sql.push_str(&format!(" ORDER BY timestamp DESC LIMIT ?{}", limit_idx));

            let mut stmt = conn.prepare(&final_sql)?;
            let params_ref: Vec<&dyn rusqlite::types::ToSql> = bind_values.iter().map(|b| b.as_ref()).collect();
            let rows = stmt.query_map(params_ref.as_slice(), |row| {
                Ok(AuditEntryRow {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    action_type: row.get(2)?,
                    summary: row.get(3)?,
                    explanation: row.get(4)?,
                    confidence: row.get(5)?,
                    confidence_band: row.get(6)?,
                    causal_trace_id: row.get(7)?,
                    thread_id: row.get(8)?,
                    goal_run_id: row.get(9)?,
                    task_id: row.get(10)?,
                    raw_data_json: row.get(11)?,
                })
            })?.collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows)
        }).await.map_err(|e| anyhow::anyhow!("list_action_audit: {e}"))
    }

    /// Delete oldest audit entries exceeding retention limits. Returns count deleted.
    pub async fn cleanup_action_audit(&self, max_entries: usize, max_age_days: u32) -> Result<usize> {
        self.conn.call(move |conn| {
            let mut deleted = 0usize;
            // Delete by age
            if max_age_days > 0 {
                let cutoff = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64
                    - (max_age_days as i64 * 86_400 * 1000);
                deleted += conn.execute(
                    "DELETE FROM action_audit WHERE timestamp < ?1",
                    [cutoff],
                )? as usize;
            }
            // Delete excess entries (keep newest max_entries)
            if max_entries > 0 {
                deleted += conn.execute(
                    "DELETE FROM action_audit WHERE id NOT IN \
                     (SELECT id FROM action_audit ORDER BY timestamp DESC LIMIT ?1)",
                    [max_entries as i64],
                )? as usize;
            }
            Ok(deleted)
        }).await.map_err(|e| anyhow::anyhow!("cleanup_action_audit: {e}"))
    }

    /// Mark an audit entry as dismissed by the user. Per BEAT-09/D-04.
    pub async fn dismiss_audit_entry(&self, entry_id: &str) -> Result<()> {
        let entry_id = entry_id.to_string();
        self.conn.call(move |conn| {
            conn.execute(
                "UPDATE action_audit SET user_action = 'dismissed' WHERE id = ?1",
                [&entry_id],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("dismiss_audit_entry: {e}"))
    }

    /// Count dismissed audit entries per action_type since a given timestamp (ms).
    pub async fn count_dismissals_by_type(&self, since_timestamp: i64) -> Result<std::collections::HashMap<String, u64>> {
        let since = since_timestamp;
        self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT action_type, COUNT(*) FROM action_audit \
                 WHERE user_action = 'dismissed' AND timestamp >= ?1 \
                 GROUP BY action_type"
            )?;
            let rows = stmt.query_map([since], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
            })?.collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows.into_iter().collect())
        }).await.map_err(|e| anyhow::anyhow!("count_dismissals_by_type: {e}"))
    }

    /// Count total audit entries per action_type since a given timestamp (ms).
    pub async fn count_shown_by_type(&self, since_timestamp: i64) -> Result<std::collections::HashMap<String, u64>> {
        let since = since_timestamp;
        self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT action_type, COUNT(*) FROM action_audit \
                 WHERE timestamp >= ?1 \
                 GROUP BY action_type"
            )?;
            let rows = stmt.query_map([since], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
            })?.collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows.into_iter().collect())
        }).await.map_err(|e| anyhow::anyhow!("count_shown_by_type: {e}"))
    }

    /// Count audit entries where user acted on them, per action_type since a given timestamp (ms).
    pub async fn count_acted_on_by_type(&self, since_timestamp: i64) -> Result<std::collections::HashMap<String, u64>> {
        let since = since_timestamp;
        self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT action_type, COUNT(*) FROM action_audit \
                 WHERE user_action = 'acted_on' AND timestamp >= ?1 \
                 GROUP BY action_type"
            )?;
            let rows = stmt.query_map([since], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
            })?.collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows.into_iter().collect())
        }).await.map_err(|e| anyhow::anyhow!("count_acted_on_by_type: {e}"))
    }
}

fn refresh_thread_stats(connection: &Connection, thread_id: &str) -> std::result::Result<(), rusqlite::Error> {
    let (message_count, total_tokens, last_preview, updated_at): (i64, i64, String, i64) = connection.query_row(
        "SELECT
            COUNT(*),
            COALESCE(SUM(total_tokens), 0),
            COALESCE((SELECT substr(content, 1, 100) FROM agent_messages WHERE thread_id = ?1 ORDER BY created_at DESC LIMIT 1), ''),
            COALESCE((SELECT created_at FROM agent_messages WHERE thread_id = ?1 ORDER BY created_at DESC LIMIT 1), strftime('%s','now') * 1000)
         FROM agent_messages WHERE thread_id = ?1",
        params![thread_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;

    connection.execute(
        "UPDATE agent_threads SET message_count = ?2, total_tokens = ?3, last_preview = ?4, updated_at = ?5 WHERE id = ?1",
        params![thread_id, message_count, total_tokens, last_preview, updated_at],
    )?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct CausalTraceRecord {
    pub selected_json: String,
    pub causal_factors_json: String,
    pub outcome_json: String,
    pub created_at: u64,
}

fn map_command_log_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<CommandLogEntry> {
    Ok(CommandLogEntry {
        id: row.get(0)?,
        command: row.get(1)?,
        timestamp: row.get(2)?,
        path: row.get(3)?,
        cwd: row.get(4)?,
        workspace_id: row.get(5)?,
        surface_id: row.get(6)?,
        pane_id: row.get(7)?,
        exit_code: row.get(8)?,
        duration_ms: row.get(9)?,
    })
}

fn map_agent_thread(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentDbThread> {
    Ok(AgentDbThread {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        surface_id: row.get(2)?,
        pane_id: row.get(3)?,
        agent_name: row.get(4)?,
        title: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        message_count: row.get(8)?,
        total_tokens: row.get(9)?,
        last_preview: row.get(10)?,
        metadata_json: row.get(11)?,
    })
}

fn map_agent_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentDbMessage> {
    Ok(AgentDbMessage {
        id: row.get(0)?,
        thread_id: row.get(1)?,
        created_at: row.get(2)?,
        role: row.get(3)?,
        content: row.get(4)?,
        provider: row.get(5)?,
        model: row.get(6)?,
        input_tokens: row.get(7)?,
        output_tokens: row.get(8)?,
        total_tokens: row.get(9)?,
        reasoning: row.get(10)?,
        tool_calls_json: row.get(11)?,
        metadata_json: row.get(12)?,
    })
}

fn map_transcript_index_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<TranscriptIndexEntry> {
    Ok(TranscriptIndexEntry {
        id: row.get(0)?,
        pane_id: row.get(1)?,
        workspace_id: row.get(2)?,
        surface_id: row.get(3)?,
        filename: row.get(4)?,
        reason: row.get(5)?,
        captured_at: row.get(6)?,
        size_bytes: row.get(7)?,
        preview: row.get(8)?,
    })
}

fn map_snapshot_index_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<SnapshotIndexEntry> {
    Ok(SnapshotIndexEntry {
        snapshot_id: row.get(0)?,
        workspace_id: row.get(1)?,
        session_id: row.get(2)?,
        kind: row.get(3)?,
        label: row.get(4)?,
        path: row.get(5)?,
        created_at: row.get(6)?,
        details_json: row.get(7)?,
    })
}

fn map_agent_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentEventRow> {
    Ok(AgentEventRow {
        id: row.get(0)?,
        category: row.get(1)?,
        kind: row.get(2)?,
        pane_id: row.get(3)?,
        workspace_id: row.get(4)?,
        surface_id: row.get(5)?,
        session_id: row.get(6)?,
        payload_json: row.get(7)?,
        timestamp: row.get(8)?,
    })
}

fn flatten_option_str(value: &Option<Option<String>>) -> Option<&str> {
    value.as_ref().and_then(|inner| inner.as_deref())
}

fn flatten_option_i64(value: &Option<Option<i64>>) -> Option<i64> {
    value.as_ref().copied().flatten()
}

fn task_status_to_str(value: TaskStatus) -> &'static str {
    match value {
        TaskStatus::Queued => "queued",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::AwaitingApproval => "awaiting_approval",
        TaskStatus::Blocked => "blocked",
        TaskStatus::FailedAnalyzing => "failed_analyzing",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Cancelled => "cancelled",
    }
}

fn parse_task_status(value: &str) -> TaskStatus {
    match value {
        "in_progress" => TaskStatus::InProgress,
        "awaiting_approval" => TaskStatus::AwaitingApproval,
        "blocked" => TaskStatus::Blocked,
        "failed_analyzing" => TaskStatus::FailedAnalyzing,
        "completed" => TaskStatus::Completed,
        "failed" => TaskStatus::Failed,
        "cancelled" => TaskStatus::Cancelled,
        _ => TaskStatus::Queued,
    }
}

fn task_priority_to_str(value: TaskPriority) -> &'static str {
    match value {
        TaskPriority::Low => "low",
        TaskPriority::Normal => "normal",
        TaskPriority::High => "high",
        TaskPriority::Urgent => "urgent",
    }
}

fn parse_task_priority(value: &str) -> TaskPriority {
    match value {
        "low" => TaskPriority::Low,
        "high" => TaskPriority::High,
        "urgent" => TaskPriority::Urgent,
        _ => TaskPriority::Normal,
    }
}

fn task_log_level_to_str(value: TaskLogLevel) -> &'static str {
    match value {
        TaskLogLevel::Info => "info",
        TaskLogLevel::Warn => "warn",
        TaskLogLevel::Error => "error",
    }
}

fn parse_task_log_level(value: &str) -> TaskLogLevel {
    match value {
        "warn" => TaskLogLevel::Warn,
        "error" => TaskLogLevel::Error,
        _ => TaskLogLevel::Info,
    }
}

fn goal_run_status_to_str(value: GoalRunStatus) -> &'static str {
    match value {
        GoalRunStatus::Queued => "queued",
        GoalRunStatus::Planning => "planning",
        GoalRunStatus::Running => "running",
        GoalRunStatus::AwaitingApproval => "awaiting_approval",
        GoalRunStatus::Paused => "paused",
        GoalRunStatus::Completed => "completed",
        GoalRunStatus::Failed => "failed",
        GoalRunStatus::Cancelled => "cancelled",
    }
}

fn parse_goal_run_status(value: &str) -> GoalRunStatus {
    match value {
        "planning" => GoalRunStatus::Planning,
        "running" => GoalRunStatus::Running,
        "awaiting_approval" => GoalRunStatus::AwaitingApproval,
        "paused" => GoalRunStatus::Paused,
        "completed" => GoalRunStatus::Completed,
        "failed" => GoalRunStatus::Failed,
        "cancelled" => GoalRunStatus::Cancelled,
        _ => GoalRunStatus::Queued,
    }
}

fn goal_run_step_kind_to_str(value: GoalRunStepKind) -> &'static str {
    match value {
        GoalRunStepKind::Reason => "reason",
        GoalRunStepKind::Command => "command",
        GoalRunStepKind::Research => "research",
        GoalRunStepKind::Memory => "memory",
        GoalRunStepKind::Skill => "skill",
        GoalRunStepKind::Unknown => "reason",
    }
}

fn parse_goal_run_step_kind(value: &str) -> GoalRunStepKind {
    match value {
        "reason" => GoalRunStepKind::Reason,
        "command" => GoalRunStepKind::Command,
        "memory" => GoalRunStepKind::Memory,
        "skill" => GoalRunStepKind::Skill,
        _ => GoalRunStepKind::Research,
    }
}

fn goal_run_step_status_to_str(value: GoalRunStepStatus) -> &'static str {
    match value {
        GoalRunStepStatus::Pending => "pending",
        GoalRunStepStatus::InProgress => "in_progress",
        GoalRunStepStatus::Completed => "completed",
        GoalRunStepStatus::Failed => "failed",
        GoalRunStepStatus::Skipped => "skipped",
    }
}

fn parse_goal_run_step_status(value: &str) -> GoalRunStepStatus {
    match value {
        "in_progress" => GoalRunStepStatus::InProgress,
        "completed" => GoalRunStepStatus::Completed,
        "failed" => GoalRunStepStatus::Failed,
        "skipped" => GoalRunStepStatus::Skipped,
        _ => GoalRunStepStatus::Pending,
    }
}

fn append_line(path: &PathBuf, line: &str) -> Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn memory_provenance_entry_from_row(
    row: &rusqlite::Row<'_>,
    now_ms: u64,
) -> MemoryProvenanceReportEntry {
    let created_at = row.get::<_, i64>(9).unwrap_or_default().max(0) as u64;
    let fact_keys_json: String = row.get(5).unwrap_or_else(|_| "[]".to_string());
    let fact_keys = serde_json::from_str::<Vec<String>>(&fact_keys_json).unwrap_or_default();
    let age_days = now_ms.saturating_sub(created_at) as f64 / 86_400_000.0;
    let confidence = memory_provenance_confidence(age_days);
    let mode: String = row.get(2).unwrap_or_default();
    let status = if mode == "remove" {
        "retracted"
    } else if confidence < 0.55 {
        "uncertain"
    } else {
        "active"
    };

    MemoryProvenanceReportEntry {
        id: row.get(0).unwrap_or_default(),
        target: row.get(1).unwrap_or_default(),
        mode,
        source_kind: row.get(3).unwrap_or_default(),
        content: row.get(4).unwrap_or_default(),
        fact_keys,
        thread_id: row.get(6).ok(),
        task_id: row.get(7).ok(),
        goal_run_id: row.get(8).ok(),
        created_at,
        age_days,
        confidence,
        status: status.to_string(),
    }
}

fn memory_provenance_confidence(age_days: f64) -> f64 {
    let raw = 1.0 - (age_days * 0.02);
    raw.clamp(0.15, 1.0)
}

fn compute_provenance_hash(
    sequence: u64,
    timestamp: u64,
    event_type: &str,
    summary: &str,
    details: &serde_json::Value,
    prev_hash: &str,
    agent_id: &str,
    goal_run_id: Option<&str>,
    task_id: Option<&str>,
    thread_id: Option<&str>,
    approval_id: Option<&str>,
    causal_trace_id: Option<&str>,
    compliance_mode: &str,
) -> String {
    hex_hash(
        &serde_json::json!({
            "sequence": sequence,
            "timestamp": timestamp,
            "event_type": event_type,
            "summary": summary,
            "details": details,
            "prev_hash": prev_hash,
            "agent_id": agent_id,
            "goal_run_id": goal_run_id,
            "task_id": task_id,
            "thread_id": thread_id,
            "approval_id": approval_id,
            "causal_trace_id": causal_trace_id,
            "compliance_mode": compliance_mode,
        })
        .to_string(),
    )
}

fn sign_provenance_hash(key: &str, entry_hash: &str) -> String {
    hex_hash(&format!("{key}:{entry_hash}"))
}

fn hex_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Read the last line of a WORM ledger file and extract (prev_hash, next_seq).
/// Returns ("genesis", 0) if the file does not exist or is empty.
fn read_last_worm_entry(path: &PathBuf) -> (String, usize) {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return ("genesis".to_string(), 0),
    };

    let reader = std::io::BufReader::new(file);
    let mut last_line: Option<String> = None;
    for line in reader.lines() {
        if let Ok(l) = line {
            if !l.trim().is_empty() {
                last_line = Some(l);
            }
        }
    }

    match last_line {
        None => ("genesis".to_string(), 0),
        Some(line) => {
            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) {
                let hash = entry
                    .get("hash")
                    .and_then(|v| v.as_str())
                    .unwrap_or("genesis")
                    .to_string();
                let seq = entry
                    .get("seq")
                    .and_then(|v| v.as_u64())
                    .map(|s| s as usize + 1)
                    .unwrap_or(0);
                (hash, seq)
            } else {
                // Could not parse last line (possibly old format); start fresh chain.
                ("genesis".to_string(), 0)
            }
        }
    }
}

fn read_last_provenance_entry(path: &PathBuf) -> Option<ProvenanceLogEntry> {
    read_provenance_entries(path).ok()?.into_iter().last()
}

fn read_provenance_entries(path: &PathBuf) -> Result<Vec<ProvenanceLogEntry>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let reader = std::io::BufReader::new(file);
    let mut entries = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<ProvenanceLogEntry>(trimmed) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

/// Verify an individual WORM ledger file's hash-chain integrity.
fn verify_ledger_file(kind: &str, path: &PathBuf) -> WormIntegrityResult {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => {
            return WormIntegrityResult {
                kind: kind.to_string(),
                total_entries: 0,
                valid: true,
                first_invalid_seq: None,
                message: "Ledger file not found; no entries to verify.".to_string(),
            };
        }
    };

    let reader = std::io::BufReader::new(file);
    let mut prev_hash = "genesis".to_string();
    let mut total: usize = 0;
    let mut expected_seq: usize = 0;
    let mut first_invalid_seq: Option<usize> = None;
    let mut failure_message: Option<String> = None;

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                if first_invalid_seq.is_none() {
                    first_invalid_seq = Some(expected_seq);
                    failure_message = Some(format!(
                        "IO error reading line at seq {}: {}",
                        expected_seq, e
                    ));
                }
                break;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        total += 1;

        let entry: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                if first_invalid_seq.is_none() {
                    first_invalid_seq = Some(expected_seq);
                    failure_message =
                        Some(format!("JSON parse error at seq {}: {}", expected_seq, e));
                }
                break;
            }
        };

        // Detect old-format entries (no seq/prev_hash fields) and handle gracefully.
        let has_seq = entry.get("seq").is_some();
        let has_prev_hash = entry.get("prev_hash").is_some();

        if !has_seq || !has_prev_hash {
            // Old-format entry: verify standalone hash only.
            let payload = &entry["payload"];
            let recorded_hash = entry
                .get("hash")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let payload_json = serde_json::to_string(payload).unwrap_or_default();
            let computed = hex_hash(&payload_json);

            if recorded_hash != computed {
                if first_invalid_seq.is_none() {
                    first_invalid_seq = Some(expected_seq);
                    failure_message = Some(format!(
                        "Old-format entry at position {} has invalid standalone hash.",
                        expected_seq
                    ));
                }
                break;
            }

            // For chain continuity, treat old entries' hash as the prev_hash for the next entry.
            prev_hash = recorded_hash.to_string();
            expected_seq += 1;
            continue;
        }

        // New-format entry: full hash-chain verification.
        let entry_seq = entry["seq"].as_u64().unwrap_or(0) as usize;
        let entry_prev_hash = entry
            .get("prev_hash")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let recorded_hash = entry
            .get("hash")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let payload = &entry["payload"];
        let payload_json = serde_json::to_string(payload).unwrap_or_default();

        // Verify sequence number.
        if entry_seq != expected_seq {
            if first_invalid_seq.is_none() {
                first_invalid_seq = Some(entry_seq);
                failure_message = Some(format!(
                    "Sequence number mismatch: expected {}, found {} at entry {}.",
                    expected_seq, entry_seq, total
                ));
            }
            break;
        }

        // Verify prev_hash matches previous entry's hash.
        if entry_prev_hash != prev_hash {
            if first_invalid_seq.is_none() {
                first_invalid_seq = Some(entry_seq);
                failure_message = Some(format!(
                    "prev_hash mismatch at seq {}: expected '{}', found '{}'.",
                    entry_seq,
                    &prev_hash[..prev_hash.len().min(16)],
                    &entry_prev_hash[..entry_prev_hash.len().min(16)]
                ));
            }
            break;
        }

        // Verify hash = sha256(prev_hash + payload_json).
        let computed_hash = hex_hash(&format!("{}{}", entry_prev_hash, payload_json));
        if recorded_hash != computed_hash {
            if first_invalid_seq.is_none() {
                first_invalid_seq = Some(entry_seq);
                failure_message = Some(format!(
                    "Hash mismatch at seq {}: recorded '{}...', computed '{}...'.",
                    entry_seq,
                    &recorded_hash[..recorded_hash.len().min(16)],
                    &computed_hash[..computed_hash.len().min(16)]
                ));
            }
            break;
        }

        prev_hash = recorded_hash.to_string();
        expected_seq += 1;
    }

    let valid = first_invalid_seq.is_none();
    let message = if valid {
        format!(
            "{} ledger: all {} entries verified successfully.",
            kind, total
        )
    } else {
        failure_message.unwrap_or_else(|| format!("{} ledger: integrity check failed.", kind))
    };

    WormIntegrityResult {
        kind: kind.to_string(),
        total_entries: total,
        valid,
        first_invalid_seq,
        message,
    }
}

fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug)]
struct DerivedSkillMetadata {
    skill_name: String,
    variant_name: String,
    context_tags: Vec<String>,
}

#[derive(Debug, Clone)]
struct BranchCandidate {
    source_variant_id: String,
    source_relative_path: String,
    branch_tags: Vec<String>,
    success_count: u32,
}

fn derive_skill_metadata(relative_path: &str, content: &str) -> DerivedSkillMetadata {
    let normalized_path = relative_path.replace('\\', "/");
    let path = Path::new(&normalized_path);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let file_name_is_skill = file_name.eq_ignore_ascii_case("skill.md");
    let base_skill_name = if file_name_is_skill {
        path.parent()
            .and_then(|parent| parent.file_name())
            .and_then(|value| value.to_str())
            .unwrap_or(stem)
            .to_string()
    } else {
        stem.to_string()
    };
    let mut skill_name = base_skill_name.clone();
    let mut variant_name = "canonical".to_string();

    if let Some((name, variant)) = base_skill_name.split_once("--") {
        if !name.trim().is_empty() && !variant.trim().is_empty() {
            skill_name = name.to_string();
            variant_name = normalize_skill_lookup(variant);
        }
    } else if !file_name_is_skill && normalized_path.contains("/generated/") {
        variant_name = "canonical".to_string();
    }

    let skill_name = normalize_skill_lookup(&skill_name);
    let mut tags = BTreeSet::new();
    infer_skill_tags(&normalized_path, &content.to_ascii_lowercase(), &mut tags);

    DerivedSkillMetadata {
        skill_name: if skill_name.is_empty() {
            "skill".to_string()
        } else {
            skill_name
        },
        variant_name,
        context_tags: tags.into_iter().collect(),
    }
}

fn infer_skill_tags(path: &str, content: &str, out: &mut BTreeSet<String>) {
    let haystack = format!("{path}\n{}", excerpt_on_char_boundary(content, 4000));
    for (needle, tag) in [
        ("rust", "rust"),
        ("cargo", "rust"),
        ("crate", "rust"),
        ("tokio", "async"),
        ("async-std", "async"),
        ("async ", "async"),
        ("wasm", "wasm32"),
        ("wasm32", "wasm32"),
        ("typescript", "typescript"),
        ("javascript", "javascript"),
        ("node", "node"),
        ("npm", "node"),
        ("react", "frontend"),
        ("frontend", "frontend"),
        ("electron", "desktop"),
        ("tauri", "desktop"),
        ("terminal", "terminal"),
        ("tui", "terminal"),
        ("docker", "docker"),
        ("kubernetes", "kubernetes"),
        ("k8s", "kubernetes"),
        ("terraform", "terraform"),
        ("postgres", "database"),
        ("sqlx", "database"),
        ("diesel", "database"),
        ("database", "database"),
        ("slack", "messaging"),
        ("discord", "messaging"),
        ("telegram", "messaging"),
        ("python", "python"),
    ] {
        if haystack.contains(needle) {
            out.insert(tag.to_string());
        }
    }
}

fn excerpt_on_char_boundary(input: &str, max_bytes: usize) -> &str {
    if input.len() <= max_bytes {
        return input;
    }

    let mut end = max_bytes;
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    &input[..end]
}

fn extract_markdown_title(content: &str) -> Option<String> {
    content
        .lines()
        .find_map(|line| line.trim().strip_prefix("# "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_skill_lookup(value: &str) -> String {
    value
        .trim()
        .trim_matches('/')
        .trim_end_matches(".md")
        .trim_end_matches("/skill")
        .trim_end_matches("/SKILL")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else if matches!(ch, '/' | '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn skill_variant_matches(record: &SkillVariantRecord, normalized: &str) -> bool {
    let relative = record.relative_path.to_ascii_lowercase();
    let skill_name = record.skill_name.to_ascii_lowercase();
    let variant_name = record.variant_name.to_ascii_lowercase();
    let stem = Path::new(&relative)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    skill_name == normalized
        || stem == normalized
        || relative == normalized
        || relative.ends_with(&format!("/{normalized}.md"))
        || format!("{skill_name}--{variant_name}") == normalized
        || relative.contains(normalized)
}

fn compare_skill_variants(
    left: &SkillVariantRecord,
    right: &SkillVariantRecord,
    context_tags: &[String],
) -> std::cmp::Ordering {
    let left_overlap = skill_context_overlap(left, context_tags);
    let right_overlap = skill_context_overlap(right, context_tags);
    let left_status_rank = skill_status_rank(&left.status);
    let right_status_rank = skill_status_rank(&right.status);

    right_overlap
        .cmp(&left_overlap)
        .then_with(|| right_status_rank.cmp(&left_status_rank))
        .then_with(|| {
            right
                .success_rate()
                .partial_cmp(&left.success_rate())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| right.use_count.cmp(&left.use_count))
        .then_with(|| right.is_canonical().cmp(&left.is_canonical()))
        .then_with(|| right.updated_at.cmp(&left.updated_at))
        .then_with(|| left.relative_path.cmp(&right.relative_path))
}

fn skill_status_rank(status: &str) -> u8 {
    match status {
        "promoted-to-canonical" => 4,
        "active" => 3,
        "deprecated" => 2,
        "archived" => 1,
        "merged" => 0,
        _ => 0,
    }
}

fn skill_variant_covers_branch_tags(variant: &SkillVariantRecord, branch_tags: &[String]) -> bool {
    branch_tags.iter().all(|tag| {
        variant
            .context_tags
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(tag))
    })
}

fn rebalance_skill_variant_status<'a>(
    variant: &'a SkillVariantRecord,
    promoted_variant_id: Option<&str>,
    now: u64,
) -> &'a str {
    if variant.status == "merged" {
        return "merged";
    }
    if !variant.is_canonical() {
        let idle_secs = variant
            .last_used_at
            .map(|value| now.saturating_sub(value))
            .unwrap_or_else(|| now.saturating_sub(variant.created_at));
        let is_stale =
            variant.use_count >= SKILL_ARCHIVE_MIN_USES && idle_secs >= SKILL_ARCHIVE_MAX_IDLE_SECS;
        let is_low_value = variant.use_count >= SKILL_ARCHIVE_MIN_USES
            && variant.success_rate() < SKILL_ARCHIVE_SUCCESS_RATE_THRESHOLD;
        if is_stale || is_low_value {
            return "archived";
        }
    }

    if Some(variant.variant_id.as_str()) == promoted_variant_id {
        "promoted-to-canonical"
    } else if variant.is_canonical() && promoted_variant_id.is_some() {
        "deprecated"
    } else {
        "active"
    }
}

fn skill_context_overlap(record: &SkillVariantRecord, context_tags: &[String]) -> usize {
    if context_tags.is_empty() {
        return 0;
    }
    let tags = record
        .context_tags
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    context_tags
        .iter()
        .filter(|tag| tags.contains(&tag.to_ascii_lowercase()))
        .count()
}

fn skill_content_similarity(left: &str, right: &str) -> f64 {
    let left_tokens = tokenize_skill_similarity(left);
    let right_tokens = tokenize_skill_similarity(right);
    if left_tokens.is_empty() || right_tokens.is_empty() {
        return 0.0;
    }
    let overlap = left_tokens.intersection(&right_tokens).count() as f64;
    let union = left_tokens.union(&right_tokens).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        overlap / union
    }
}

fn tokenize_skill_similarity(content: &str) -> BTreeSet<String> {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with("> Auto-branched from")
                && !trimmed.starts_with("## Learned Variant Contexts")
                && !trimmed.starts_with("- Merged ")
        })
        .flat_map(|line| {
            line.split(|ch: char| !ch.is_ascii_alphanumeric() && !matches!(ch, '-' | '_'))
                .map(str::trim)
                .filter(|token| token.len() >= 3)
                .map(|token| token.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn skill_merge_note(variant: &SkillVariantRecord, similarity: f64) -> String {
    format!(
        "- Merged `{}` back into canonical after {} uses, {:.0}% success, {:.0}% content overlap. Use when context includes: {}.",
        variant.variant_name,
        variant.use_count,
        variant.success_rate() * 100.0,
        similarity * 100.0,
        if variant.context_tags.is_empty() {
            "the learned variant context".to_string()
        } else {
            variant.context_tags.join(", ")
        }
    )
}

fn append_skill_merge_notes(canonical_content: &str, notes: &[String]) -> String {
    if notes.is_empty() {
        return canonical_content.to_string();
    }
    let mut next = canonical_content.trim_end().to_string();
    if !next.contains("## Learned Variant Contexts") {
        next.push_str("\n\n## Learned Variant Contexts\n");
    }
    for note in notes {
        if !next.contains(note) {
            next.push('\n');
            next.push_str(note);
        }
    }
    next.push('\n');
    next
}

fn skill_merge_section(
    variant: &SkillVariantRecord,
    variant_content: &str,
    similarity: f64,
) -> String {
    let body = extract_mergeable_variant_body(variant_content);
    format!(
        "### Variant `{}`\n\nSuccess rate: {:.0}% across {} uses with {:.0}% overlap to canonical.\n\n{}\n",
        variant.variant_name,
        variant.success_rate() * 100.0,
        variant.use_count,
        similarity * 100.0,
        if body.is_empty() {
            format!(
                "Use when context includes: {}.",
                if variant.context_tags.is_empty() {
                    "the learned variant context".to_string()
                } else {
                    variant.context_tags.join(", ")
                }
            )
        } else {
            body
        }
    )
}

fn extract_mergeable_variant_body(content: &str) -> String {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with("# ") && !trimmed.starts_with("> Auto-branched from")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn append_skill_merge_sections(canonical_content: &str, sections: &[String]) -> String {
    if sections.is_empty() {
        return canonical_content.to_string();
    }
    let mut next = canonical_content.trim_end().to_string();
    if !next.contains("## Merged Variant Playbooks") {
        next.push_str("\n\n## Merged Variant Playbooks\n");
    }
    for section in sections {
        let marker = section
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        if !marker.is_empty() && next.contains(&marker) {
            continue;
        }
        next.push('\n');
        next.push_str(section.trim());
        next.push('\n');
    }
    next
}

fn map_skill_variant_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkillVariantRecord> {
    let context_tags_json: String = row.get(6)?;
    let context_tags =
        serde_json::from_str::<Vec<String>>(&context_tags_json).unwrap_or_else(|_| Vec::new());
    Ok(SkillVariantRecord {
        variant_id: row.get(0)?,
        skill_name: row.get(1)?,
        variant_name: row.get(2)?,
        relative_path: row.get(3)?,
        parent_variant_id: row.get(4)?,
        version: row.get(5)?,
        context_tags,
        use_count: row.get::<_, i64>(7)? as u32,
        success_count: row.get::<_, i64>(8)? as u32,
        failure_count: row.get::<_, i64>(9)? as u32,
        status: row.get(10)?,
        last_used_at: row.get::<_, Option<i64>>(11)?.map(|value| value as u64),
        created_at: row.get::<_, i64>(12)? as u64,
        updated_at: row.get::<_, i64>(13)? as u64,
    })
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> std::result::Result<(), rusqlite::Error> {
    if table_has_column(connection, table, column)? {
        return Ok(());
    }

    connection.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )?;
    Ok(())
}

fn table_has_column(connection: &Connection, table: &str, column: &str) -> std::result::Result<bool, rusqlite::Error> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = connection.prepare(&pragma)?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    async fn make_test_store() -> Result<(HistoryStore, PathBuf)> {
        let root = std::env::temp_dir().join(format!("tamux-history-test-{}", Uuid::new_v4()));
        let store = HistoryStore::new_test_store(&root).await?;
        Ok((store, root))
    }

    #[tokio::test]
    async fn init_schema_migrates_legacy_agent_tasks_before_goal_run_index() -> Result<()> {
        let (store, root) = make_test_store().await?;
        // Drop existing tables and recreate with legacy schema (missing columns)
        store.conn.call(|conn| {
            conn.execute_batch("DROP TABLE IF EXISTS agent_tasks")?;
            conn.execute_batch(
            "
            CREATE TABLE agent_tasks (
                id                   TEXT PRIMARY KEY,
                title                TEXT NOT NULL,
                description          TEXT NOT NULL,
                status               TEXT NOT NULL,
                priority             TEXT NOT NULL,
                progress             INTEGER NOT NULL DEFAULT 0,
                created_at           INTEGER NOT NULL,
                started_at           INTEGER,
                completed_at         INTEGER,
                error                TEXT,
                result               TEXT,
                thread_id            TEXT,
                source               TEXT NOT NULL DEFAULT 'user',
                notify_on_complete   INTEGER NOT NULL DEFAULT 0,
                notify_channels_json TEXT NOT NULL DEFAULT '[]',
                command              TEXT,
                retry_count          INTEGER NOT NULL DEFAULT 0,
                max_retries          INTEGER NOT NULL DEFAULT 3,
                next_retry_at        INTEGER,
                blocked_reason       TEXT,
                awaiting_approval_id TEXT,
                lane_id              TEXT,
                last_error           TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_agent_tasks_status ON agent_tasks(status, priority, created_at DESC);
            ",
        )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

        store.init_schema().await?;

        let has_cols = store.conn.call(|conn| {
            let has_session = table_has_column(conn, "agent_tasks", "session_id")?;
            let has_scheduled = table_has_column(conn, "agent_tasks", "scheduled_at")?;
            let has_goal_run = table_has_column(conn, "agent_tasks", "goal_run_id")?;
            let index_name: Option<String> = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_agent_tasks_goal_run'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            Ok((has_session, has_scheduled, has_goal_run, index_name))
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

        assert!(has_cols.0);
        assert!(has_cols.1);
        assert!(has_cols.2);
        assert_eq!(has_cols.3.as_deref(), Some("idx_agent_tasks_goal_run"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn goal_run_event_todo_snapshot_round_trips() -> Result<()> {
        let (store, root) = make_test_store().await?;

        let goal_run = GoalRun {
            id: "goal-1".to_string(),
            title: "Goal".to_string(),
            goal: "Do the thing".to_string(),
            client_request_id: None,
            status: GoalRunStatus::Running,
            priority: TaskPriority::Normal,
            created_at: 1,
            updated_at: 2,
            started_at: Some(1),
            completed_at: None,
            thread_id: Some("thread-1".to_string()),
            session_id: None,
            current_step_index: 0,
            current_step_title: Some("Inspect".to_string()),
            current_step_kind: Some(GoalRunStepKind::Research),
            replan_count: 0,
            max_replans: 2,
            plan_summary: Some("Plan".to_string()),
            reflection_summary: None,
            memory_updates: Vec::new(),
            generated_skill_path: None,
            last_error: None,
            failure_cause: None,
            child_task_ids: Vec::new(),
            child_task_count: 0,
            approval_count: 0,
            awaiting_approval_id: None,
            active_task_id: None,
            duration_ms: None,
            steps: vec![GoalRunStep {
                id: "step-1".to_string(),
                position: 0,
                title: "Inspect".to_string(),
                instructions: "Inspect state".to_string(),
                kind: GoalRunStepKind::Research,
                success_criteria: "Know state".to_string(),
                session_id: None,
                status: GoalRunStepStatus::InProgress,
                task_id: None,
                summary: None,
                error: None,
                started_at: Some(1),
                completed_at: None,
            }],
            events: vec![GoalRunEvent {
                id: "event-1".to_string(),
                timestamp: 3,
                phase: "todo".to_string(),
                message: "goal todo updated".to_string(),
                details: None,
                step_index: Some(0),
                todo_snapshot: vec![crate::agent::types::TodoItem {
                    id: "todo-1".to_string(),
                    content: "Inspect state".to_string(),
                    status: crate::agent::types::TodoStatus::InProgress,
                    position: 0,
                    step_index: Some(0),
                    created_at: 3,
                    updated_at: 3,
                }],
            }],
        };

        store.upsert_goal_run(&goal_run).await?;
        let loaded = store
            .get_goal_run("goal-1").await?
            .expect("goal run should exist after upsert");

        assert_eq!(loaded.events.len(), 1);
        assert_eq!(loaded.events[0].step_index, Some(0));
        assert_eq!(loaded.events[0].todo_snapshot.len(), 1);
        assert_eq!(loaded.events[0].todo_snapshot[0].content, "Inspect state");

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn memory_provenance_write_round_trips() -> Result<()> {
        let (store, root) = make_test_store().await?;
        store.init_schema().await?;

        let fact_keys = vec!["shell".to_string(), "editor".to_string()];
        store.record_memory_provenance(&MemoryProvenanceRecord {
            id: "mem-1",
            target: "USER.md",
            mode: "append",
            source_kind: "tool",
            content: "- shell: bash",
            fact_keys: &fact_keys,
            thread_id: Some("thread-1"),
            task_id: Some("task-1"),
            goal_run_id: None,
            created_at: 42,
        }).await?;

        let row = store.conn.call(|conn| {
            conn.query_row(
                "SELECT target, mode, source_kind, content, fact_keys_json FROM memory_provenance WHERE id = 'mem-1'",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?)),
            ).map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        assert_eq!(row.0, "USER.md");
        assert_eq!(row.1, "append");
        assert_eq!(row.2, "tool");
        assert_eq!(row.3, "- shell: bash");
        assert_eq!(
            serde_json::from_str::<Vec<String>>(&row.4)?,
            vec!["shell".to_string(), "editor".to_string()]
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn memory_provenance_report_marks_old_entries_uncertain() -> Result<()> {
        let (store, root) = make_test_store().await?;
        store.init_schema().await?;
        let recent_keys = vec!["shell".to_string()];
        let old_keys = vec!["editor".to_string()];
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        store.record_memory_provenance(&MemoryProvenanceRecord {
            id: "recent",
            target: "USER.md",
            mode: "append",
            source_kind: "tool",
            content: "- shell: bash",
            fact_keys: &recent_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms,
        }).await?;
        store.record_memory_provenance(&MemoryProvenanceRecord {
            id: "old",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "goal_reflection",
            content: "- editor: helix",
            fact_keys: &old_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms.saturating_sub(40 * 86_400_000),
        }).await?;

        let report = store.memory_provenance_report(None, 10).await?;
        assert_eq!(report.total_entries, 2);
        assert_eq!(report.summary_by_status.get("active").copied(), Some(1));
        assert_eq!(report.summary_by_status.get("uncertain").copied(), Some(1));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn collaboration_session_round_trips() -> Result<()> {
        let (store, root) = make_test_store().await?;
        store.init_schema().await?;
        store.upsert_collaboration_session(
            "task-parent",
            r#"{"id":"c1","parent_task_id":"task-parent"}"#,
            42,
        ).await?;

        let rows = store.list_collaboration_sessions().await?;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].parent_task_id, "task-parent");
        assert_eq!(rows[0].updated_at, 42);
        assert!(rows[0].session_json.contains("\"id\":\"c1\""));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn provenance_report_validates_hash_and_signature() -> Result<()> {
        let (store, root) = make_test_store().await?;
        store.init_schema().await?;
        let first = serde_json::json!({"step": 1});
        let second = serde_json::json!({"step": 2});
        store.record_provenance_event(&ProvenanceEventRecord {
            event_type: "goal_created",
            summary: "goal created",
            details: &first,
            agent_id: "test-agent",
            goal_run_id: Some("goal-1"),
            task_id: None,
            thread_id: Some("thread-1"),
            approval_id: None,
            causal_trace_id: None,
            compliance_mode: "soc2",
            sign: true,
            created_at: 1_000,
        }).await?;
        store.record_provenance_event(&ProvenanceEventRecord {
            event_type: "step_completed",
            summary: "step completed",
            details: &second,
            agent_id: "test-agent",
            goal_run_id: Some("goal-1"),
            task_id: Some("task-1"),
            thread_id: Some("thread-1"),
            approval_id: None,
            causal_trace_id: None,
            compliance_mode: "soc2",
            sign: true,
            created_at: 2_000,
        }).await?;

        let report = store.provenance_report(10)?;
        assert_eq!(report.total_entries, 2);
        assert_eq!(report.valid_hash_entries, 2);
        assert_eq!(report.valid_chain_entries, 2);
        assert_eq!(report.valid_signature_entries, 2);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn register_skill_document_infers_variant_metadata() -> Result<()> {
        let (store, root) = make_test_store().await?;
        store.init_schema().await?;
        let skill_path = root.join("skills/generated/debug-rust-stack-overflow--async-runtime.md");
        fs::write(
            &skill_path,
            "# Rust async debugging\nUse tokio console for async stack inspection.\n",
        )?;

        let record = store.register_skill_document(&skill_path).await?;

        assert_eq!(record.skill_name, "debug-rust-stack-overflow");
        assert_eq!(record.variant_name, "async-runtime");
        assert!(record.context_tags.iter().any(|tag| tag == "rust"));
        assert!(record.context_tags.iter().any(|tag| tag == "async"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn resolve_skill_variant_prefers_context_overlap_and_tracks_usage() -> Result<()> {
        let (store, root) = make_test_store().await?;
        store.init_schema().await?;
        let canonical = root.join("skills/generated/build-pipeline.md");
        let frontend = root.join("skills/generated/build-pipeline--frontend.md");
        fs::write(&canonical, "# Build pipeline\nRun cargo build.\n")?;
        fs::write(
            &frontend,
            "# Frontend build pipeline\nUse react build checks.\n",
        )?;

        let canonical_record = store.register_skill_document(&canonical).await?;
        let frontend_record = store.register_skill_document(&frontend).await?;
        let resolved = store
            .resolve_skill_variant("build-pipeline", &["frontend".to_string()]).await?
            .expect("variant should resolve");
        assert_eq!(resolved.variant_id, frontend_record.variant_id);

        store.record_skill_variant_use(&frontend_record.variant_id, Some(true)).await?;
        let refreshed = store
            .resolve_skill_variant("build-pipeline", &["frontend".to_string()]).await?
            .expect("variant should still resolve");
        assert_eq!(refreshed.use_count, 1);
        assert_eq!(refreshed.success_count, 1);
        assert_eq!(canonical_record.use_count, 0);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn skill_variant_consultation_settlement_updates_outcomes_once() -> Result<()> {
        let (store, root) = make_test_store().await?;
        store.init_schema().await?;
        let frontend = root.join("skills/generated/build-pipeline--frontend.md");
        fs::write(
            &frontend,
            "# Frontend build pipeline\nUse react build checks.\n",
        )?;

        let frontend_record = store.register_skill_document(&frontend).await?;
        let tags = vec!["frontend".to_string()];
        store.record_skill_variant_consultation(&SkillVariantConsultationRecord {
            usage_id: "usage-1",
            variant_id: &frontend_record.variant_id,
            thread_id: Some("thread-1"),
            task_id: Some("task-1"),
            goal_run_id: Some("goal-1"),
            context_tags: &tags,
            consulted_at: 100,
        }).await?;

        let pending = store.settle_skill_variant_usage(
            Some("thread-1"),
            Some("task-1"),
            Some("goal-1"),
            "success",
        ).await?;
        assert_eq!(pending, 1);
        assert_eq!(
            store.settle_skill_variant_usage(
                Some("thread-1"),
                Some("task-1"),
                Some("goal-1"),
                "success",
            ).await?,
            0
        );

        let refreshed = store
            .resolve_skill_variant("build-pipeline", &["frontend".to_string()]).await?
            .expect("variant should resolve");
        assert_eq!(refreshed.use_count, 1);
        assert_eq!(refreshed.success_count, 1);
        assert_eq!(refreshed.failure_count, 0);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn rebalance_skill_variants_archives_weak_variant() -> Result<()> {
        let (store, root) = make_test_store().await?;
        store.init_schema().await?;
        let canonical = root.join("skills/generated/build-pipeline.md");
        let weak = root.join("skills/generated/build-pipeline--legacy.md");
        fs::write(&canonical, "# Build pipeline\nRun cargo build.\n")?;
        fs::write(&weak, "# Legacy build pipeline\nOld slow workflow.\n")?;

        let canonical_record = store.register_skill_document(&canonical).await?;
        let weak_record = store.register_skill_document(&weak).await?;
        let wv = weak_record.variant_id.clone();
        let cv = canonical_record.variant_id.clone();
        store.conn.call(move |conn| {
            conn.execute(
                "UPDATE skill_variants SET use_count = 4, success_count = 0, failure_count = 4, last_used_at = ?2 WHERE variant_id = ?1",
                params![wv, now_ts() as i64],
            )?;
            conn.execute(
                "UPDATE skill_variants SET use_count = 4, success_count = 3, failure_count = 1, last_used_at = ?2 WHERE variant_id = ?1",
                params![cv, now_ts() as i64],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

        let variants = store.rebalance_skill_variants("build-pipeline").await?;
        let weak_variant = variants
            .iter()
            .find(|variant| variant.variant_id == weak_record.variant_id)
            .expect("weak variant should exist");
        let resolved = store
            .resolve_skill_variant("build-pipeline", &["legacy".to_string()]).await?
            .expect("canonical should still resolve");

        assert_eq!(weak_variant.status, "archived");
        assert_eq!(resolved.variant_id, canonical_record.variant_id);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn rebalance_skill_variants_promotes_strong_variant() -> Result<()> {
        let (store, root) = make_test_store().await?;
        store.init_schema().await?;
        let canonical = root.join("skills/generated/build-pipeline.md");
        let frontend = root.join("skills/generated/build-pipeline--frontend.md");
        fs::write(&canonical, "# Build pipeline\nRun cargo build.\n")?;
        fs::write(
            &frontend,
            "# Frontend build pipeline\nUse react build checks.\n",
        )?;

        let canonical_record = store.register_skill_document(&canonical).await?;
        let frontend_record = store.register_skill_document(&frontend).await?;
        let cv = canonical_record.variant_id.clone();
        let fv = frontend_record.variant_id.clone();
        store.conn.call(move |conn| {
            conn.execute(
                "UPDATE skill_variants SET use_count = 5, success_count = 2, failure_count = 3, last_used_at = ?2 WHERE variant_id = ?1",
                params![cv, now_ts() as i64],
            )?;
            conn.execute(
                "UPDATE skill_variants SET use_count = 5, success_count = 5, failure_count = 0, last_used_at = ?2 WHERE variant_id = ?1",
                params![fv, now_ts() as i64],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

        let variants = store.rebalance_skill_variants("build-pipeline").await?;
        let promoted = variants
            .iter()
            .find(|variant| variant.variant_id == frontend_record.variant_id)
            .expect("frontend variant should exist");
        let canonical_variant = variants
            .iter()
            .find(|variant| variant.variant_id == canonical_record.variant_id)
            .expect("canonical variant should exist");
        let resolved = store
            .resolve_skill_variant("build-pipeline", &[]).await?
            .expect("promoted variant should resolve");

        assert_eq!(promoted.status, "promoted-to-canonical");
        assert_eq!(canonical_variant.status, "deprecated");
        assert_eq!(resolved.variant_id, frontend_record.variant_id);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn successful_context_mismatch_branches_new_variant() -> Result<()> {
        let (store, root) = make_test_store().await?;
        store.init_schema().await?;
        let canonical = root.join("skills/generated/build-pipeline.md");
        fs::write(&canonical, "# Build pipeline\nRun cargo build.\n")?;

        let canonical_record = store.register_skill_document(&canonical).await?;
        let branch_tags = vec![
            "rust".to_string(),
            "frontend".to_string(),
            "database".to_string(),
        ];
        for index in 0..3 {
            let usage_id = format!("usage-{index}");
            let task_id = format!("task-{index}");
            store.record_skill_variant_consultation(&SkillVariantConsultationRecord {
                usage_id: &usage_id,
                variant_id: &canonical_record.variant_id,
                thread_id: Some("thread-1"),
                task_id: Some(&task_id),
                goal_run_id: Some("goal-1"),
                context_tags: &branch_tags,
                consulted_at: 100 + index,
            }).await?;
            store.settle_skill_variant_usage(
                Some("thread-1"),
                Some(&task_id),
                Some("goal-1"),
                "success",
            ).await?;
        }

        let variants = store.list_skill_variants(Some("build-pipeline"), 10).await?;
        let branched = variants
            .iter()
            .find(|variant| {
                variant.variant_name.contains("database")
                    && variant.variant_name.contains("frontend")
            })
            .expect("branched variant should exist");
        let branched_path = root.join("skills").join(&branched.relative_path);

        assert!(branched_path.exists());
        assert!(branched.context_tags.iter().any(|tag| tag == "frontend"));
        assert!(branched.context_tags.iter().any(|tag| tag == "database"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn stable_variant_merges_back_into_canonical() -> Result<()> {
        let (store, root) = make_test_store().await?;
        store.init_schema().await?;
        let canonical = root.join("skills/generated/build-pipeline.md");
        let frontend = root.join("skills/generated/build-pipeline--frontend.md");
        fs::write(
            &canonical,
            "# Build pipeline\n\n## When To Use\nUse this for standard builds.\n\n## How\nRun cargo build.\n",
        )?;
        fs::write(
            &frontend,
            "# Build pipeline (frontend)\n\n> Auto-branched from `generated/build-pipeline.md` (variant `canonical`) after 4 successful consultations in contexts: frontend, rust.\n\n## When To Use\nUse this variant when the workspace context includes: frontend, rust.\n\n## How\nRun cargo build.\n",
        )?;

        let canonical_record = store.register_skill_document(&canonical).await?;
        let frontend_record = store.register_skill_document(&frontend).await?;
        let fv = frontend_record.variant_id.clone();
        let cv = canonical_record.variant_id.clone();
        store.conn.call(move |conn| {
            conn.execute(
                "UPDATE skill_variants SET use_count = 5, success_count = 5, failure_count = 0, parent_variant_id = ?2, last_used_at = ?3 WHERE variant_id = ?1",
                params![fv, cv, now_ts() as i64],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

        let variants = store.maybe_merge_skill_variants("build-pipeline").await?;
        let merged = variants
            .iter()
            .find(|variant| variant.variant_id == frontend_record.variant_id)
            .expect("frontend variant should still exist after merge");
        let canonical_content = fs::read_to_string(&canonical)?;
        let resolved = store
            .resolve_skill_variant("build-pipeline", &["frontend".to_string()]).await?
            .expect("canonical should resolve once branch is merged");

        assert_eq!(merged.status, "merged");
        assert!(canonical_content.contains("## Learned Variant Contexts"));
        assert!(canonical_content.contains("frontend"));
        assert_eq!(resolved.variant_id, canonical_record.variant_id);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn skill_tag_excerpt_respects_utf8_boundaries() {
        let content = format!("{}\n{}", "a".repeat(3998), "│architecture");
        let excerpt = excerpt_on_char_boundary(&content, 4000);

        assert!(excerpt.is_char_boundary(excerpt.len()));
        assert!(excerpt.len() <= 4000);
        assert!(!excerpt.ends_with('\u{FFFD}'));

        let mut tags = BTreeSet::new();
        infer_skill_tags("skills/terminal-architecture.md", &content, &mut tags);
        assert!(tags.contains("terminal"));
    }

    /// FOUN-01: WAL journal mode is active after HistoryStore construction.
    #[tokio::test]
    async fn wal_mode_enabled() -> Result<()> {
        let (store, root) = make_test_store().await?;
        let mode: String = store.conn.call(|conn| {
            conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        assert_eq!(mode.to_lowercase(), "wal");
        fs::remove_dir_all(root)?;
        Ok(())
    }

    /// FOUN-06: All 5 WAL pragmas are applied on connection open.
    #[tokio::test]
    async fn wal_pragmas_applied() -> Result<()> {
        let (store, root) = make_test_store().await?;
        let pragmas = store.conn.call(|conn| {
            let journal_mode: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
            let synchronous: i64 = conn.query_row("PRAGMA synchronous", [], |row| row.get(0))?;
            let foreign_keys: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
            let wal_autocheckpoint: i64 = conn.query_row("PRAGMA wal_autocheckpoint", [], |row| row.get(0))?;
            let busy_timeout: i64 = conn.query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
            Ok((journal_mode, synchronous, foreign_keys, wal_autocheckpoint, busy_timeout))
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        assert_eq!(pragmas.0.to_lowercase(), "wal");
        assert_eq!(pragmas.1, 1); // NORMAL
        assert_eq!(pragmas.2, 1); // ON
        assert_eq!(pragmas.3, 1000);
        assert_eq!(pragmas.4, 5000);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    /// FOUN-02: HistoryStore can perform a basic async roundtrip through .call().
    #[tokio::test]
    async fn async_connection_roundtrip() -> Result<()> {
        let (store, root) = make_test_store().await?;
        let thread = AgentDbThread {
            id: "test-thread-1".to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("test-agent".to_string()),
            title: "Test Thread".to_string(),
            created_at: 1000,
            updated_at: 1000,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        };
        store.create_thread(&thread).await?;
        let loaded = store.get_thread("test-thread-1").await?;
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.title, "Test Thread");
        assert_eq!(loaded.agent_name, Some("test-agent".to_string()));
        fs::remove_dir_all(root)?;
        Ok(())
    }

    /// FOUN-01 + FOUN-02: Concurrent reads and writes do not produce "database is locked" errors.
    #[tokio::test]
    async fn concurrent_read_write() -> Result<()> {
        let (store, root) = make_test_store().await?;
        let mut handles = Vec::new();
        for i in 0..8 {
            let store_clone = store.clone();
            handles.push(tokio::spawn(async move {
                let thread = AgentDbThread {
                    id: format!("concurrent-thread-{i}"),
                    workspace_id: None,
                    surface_id: None,
                    pane_id: None,
                    agent_name: Some("test-agent".to_string()),
                    title: format!("Concurrent Thread {i}"),
                    created_at: 1000 + i as i64,
                    updated_at: 1000 + i as i64,
                    message_count: 0,
                    total_tokens: 0,
                    last_preview: String::new(),
                    metadata_json: None,
                };
                store_clone.create_thread(&thread).await?;
                let loaded = store_clone.list_threads().await?;
                assert!(!loaded.is_empty());
                Ok::<(), anyhow::Error>(())
            }));
        }
        for handle in handles {
            handle.await??;
        }
        let all_threads = store.list_threads().await?;
        assert_eq!(all_threads.len(), 8);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    // ── action_audit user_action column tests (BEAT-09/D-04) ────────────

    #[tokio::test]
    async fn ensure_column_adds_user_action_to_action_audit() -> Result<()> {
        let (store, root) = make_test_store().await?;
        // Verify the column exists by inserting and querying
        let has = store.conn.call(|conn| {
            Ok(table_has_column(conn, "action_audit", "user_action")?)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        assert!(has, "user_action column should exist after init_schema");
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn dismiss_audit_entry_sets_user_action() -> Result<()> {
        let (store, root) = make_test_store().await?;
        let entry = AuditEntryRow {
            id: "test-dismiss-1".to_string(),
            timestamp: 1000,
            action_type: "heartbeat".to_string(),
            summary: "Test entry".to_string(),
            explanation: None,
            confidence: None,
            confidence_band: None,
            causal_trace_id: None,
            thread_id: None,
            goal_run_id: None,
            task_id: None,
            raw_data_json: None,
        };
        store.insert_action_audit(&entry).await?;
        store.dismiss_audit_entry("test-dismiss-1").await?;

        let user_action: Option<String> = store.conn.call(|conn| {
            conn.query_row(
                "SELECT user_action FROM action_audit WHERE id = ?1",
                ["test-dismiss-1"],
                |row| row.get(0),
            ).map_err(|e| e.into())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        assert_eq!(user_action.as_deref(), Some("dismissed"));
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn count_dismissals_by_type_returns_correct_counts() -> Result<()> {
        let (store, root) = make_test_store().await?;
        // Insert 3 heartbeat entries, dismiss 2
        for i in 0..3 {
            let entry = AuditEntryRow {
                id: format!("hb-{}", i),
                timestamp: 1000 + i,
                action_type: "heartbeat".to_string(),
                summary: format!("HB entry {}", i),
                explanation: None,
                confidence: None,
                confidence_band: None,
                causal_trace_id: None,
                thread_id: None,
                goal_run_id: None,
                task_id: None,
                raw_data_json: None,
            };
            store.insert_action_audit(&entry).await?;
        }
        store.dismiss_audit_entry("hb-0").await?;
        store.dismiss_audit_entry("hb-1").await?;

        // Insert 1 escalation entry, dismiss it
        let esc_entry = AuditEntryRow {
            id: "esc-0".to_string(),
            timestamp: 2000,
            action_type: "escalation".to_string(),
            summary: "Escalation".to_string(),
            explanation: None,
            confidence: None,
            confidence_band: None,
            causal_trace_id: None,
            thread_id: None,
            goal_run_id: None,
            task_id: None,
            raw_data_json: None,
        };
        store.insert_action_audit(&esc_entry).await?;
        store.dismiss_audit_entry("esc-0").await?;

        let counts = store.count_dismissals_by_type(0).await?;
        assert_eq!(counts.get("heartbeat").copied(), Some(2));
        assert_eq!(counts.get("escalation").copied(), Some(1));

        let shown = store.count_shown_by_type(0).await?;
        assert_eq!(shown.get("heartbeat").copied(), Some(3));
        assert_eq!(shown.get("escalation").copied(), Some(1));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    // ── Memory tombstone tests (Phase 5) ─────────────────────────────────

    #[tokio::test]
    async fn memory_tombstone_insert_and_list_round_trips() -> Result<()> {
        let (store, root) = make_test_store().await?;

        store.insert_memory_tombstone(
            "t-1", "soul", "old fact about rust", Some("rust_version"),
            Some("Rust 1.80"), "consolidation", None, 1000,
        ).await?;
        store.insert_memory_tombstone(
            "t-2", "memory", "stale project note", None,
            None, "decay", Some("prov-1"), 2000,
        ).await?;

        // List all
        let all = store.list_memory_tombstones(None, 10).await?;
        assert_eq!(all.len(), 2);
        // Ordered by created_at DESC
        assert_eq!(all[0].id, "t-2");
        assert_eq!(all[1].id, "t-1");

        // List by target
        let soul_only = store.list_memory_tombstones(Some("soul"), 10).await?;
        assert_eq!(soul_only.len(), 1);
        assert_eq!(soul_only[0].id, "t-1");
        assert_eq!(soul_only[0].original_content, "old fact about rust");
        assert_eq!(soul_only[0].fact_key.as_deref(), Some("rust_version"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn memory_tombstone_delete_expired() -> Result<()> {
        let (store, root) = make_test_store().await?;

        store.insert_memory_tombstone(
            "old", "soul", "ancient", None, None, "decay", None, 100,
        ).await?;
        store.insert_memory_tombstone(
            "new", "soul", "recent", None, None, "decay", None, 5000,
        ).await?;

        // Delete tombstones older than 4000ms as of now=6000
        let deleted = store.delete_expired_tombstones(4000, 6000).await?;
        assert_eq!(deleted, 1);

        let remaining = store.list_memory_tombstones(None, 10).await?;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, "new");

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn memory_tombstone_restore() -> Result<()> {
        let (store, root) = make_test_store().await?;

        store.insert_memory_tombstone(
            "t-restore", "memory", "fact to restore", None,
            None, "decay", None, 3000,
        ).await?;

        let restored = store.restore_tombstone("t-restore").await?;
        assert!(restored.is_some());
        let row = restored.unwrap();
        assert_eq!(row.original_content, "fact to restore");

        // Should be deleted after restore
        let remaining = store.list_memory_tombstones(None, 10).await?;
        assert_eq!(remaining.len(), 0);

        // Restoring non-existent returns None
        let none = store.restore_tombstone("t-restore").await?;
        assert!(none.is_none());

        fs::remove_dir_all(root)?;
        Ok(())
    }

    // ── Consolidation state tests (Phase 5) ──────────────────────────────

    #[tokio::test]
    async fn consolidation_state_set_get_round_trips() -> Result<()> {
        let (store, root) = make_test_store().await?;

        // Initially empty
        let val = store.get_consolidation_state("last_watermark").await?;
        assert!(val.is_none());

        // Set and get
        store.set_consolidation_state("last_watermark", "12345", 1000).await?;
        let val = store.get_consolidation_state("last_watermark").await?;
        assert_eq!(val.as_deref(), Some("12345"));

        // Overwrite
        store.set_consolidation_state("last_watermark", "99999", 2000).await?;
        let val = store.get_consolidation_state("last_watermark").await?;
        assert_eq!(val.as_deref(), Some("99999"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    // ── Successful trace query test (Phase 5) ────────────────────────────

    #[tokio::test]
    async fn list_recent_successful_traces_with_watermark() -> Result<()> {
        let (store, root) = make_test_store().await?;

        // Insert traces with different outcomes
        store.insert_execution_trace(
            "tr-1", None, Some("task-1"), "research", "success",
            Some(0.9), "[]", "{}", 100, 50, 1000,
        ).await?;
        store.insert_execution_trace(
            "tr-2", None, Some("task-2"), "coding", "failure",
            Some(0.3), "[]", "{}", 200, 80, 2000,
        ).await?;
        store.insert_execution_trace(
            "tr-3", None, Some("task-3"), "research", "success",
            Some(0.8), "[]", "{}", 150, 60, 3000,
        ).await?;

        // Query after watermark=1500 should only return tr-3 (success after watermark)
        let traces = store.list_recent_successful_traces(1500, 100).await?;
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].id, "tr-3");
        assert_eq!(traces[0].outcome.as_deref(), Some("success"));

        // Query after watermark=0 should return both successful traces
        let traces = store.list_recent_successful_traces(0, 100).await?;
        assert_eq!(traces.len(), 2);
        // ASC order
        assert_eq!(traces[0].id, "tr-1");
        assert_eq!(traces[1].id, "tr-3");

        fs::remove_dir_all(root)?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Skill variant status and retrieval tests (SKIL-01, SKIL-02)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn update_skill_variant_status_changes_status_and_updated_at() -> Result<()> {
        let (store, root) = make_test_store().await?;
        store.init_schema().await?;

        // Register a skill to create a variant
        let skill_dir = root.join("skills").join("generated").join("test-skill");
        fs::create_dir_all(&skill_dir)?;
        let skill_path = skill_dir.join("canonical.md");
        fs::write(&skill_path, "# Test Skill\nA test skill document.")?;
        let record = store.register_skill_document(&skill_path).await?;
        assert_eq!(record.status, "active");

        // Update status to "testing"
        store.update_skill_variant_status(&record.variant_id, "testing").await?;

        // Verify the status changed
        let updated = store.get_skill_variant(&record.variant_id).await?;
        assert!(updated.is_some());
        let updated = updated.unwrap();
        assert_eq!(updated.status, "testing");
        assert!(updated.updated_at >= record.updated_at);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn get_skill_variant_returns_none_for_missing() -> Result<()> {
        let (store, root) = make_test_store().await?;
        store.init_schema().await?;

        let result = store.get_skill_variant("nonexistent-variant-id").await?;
        assert!(result.is_none());

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn get_skill_variant_returns_some_for_existing() -> Result<()> {
        let (store, root) = make_test_store().await?;
        store.init_schema().await?;

        let skill_dir = root.join("skills").join("generated").join("retrieval-skill");
        fs::create_dir_all(&skill_dir)?;
        let skill_path = skill_dir.join("canonical.md");
        fs::write(&skill_path, "# Retrieval Skill\nA skill for retrieval test.")?;
        let record = store.register_skill_document(&skill_path).await?;

        let fetched = store.get_skill_variant(&record.variant_id).await?;
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.variant_id, record.variant_id);
        assert_eq!(fetched.skill_name, record.skill_name);

        fs::remove_dir_all(root)?;
        Ok(())
    }
}
