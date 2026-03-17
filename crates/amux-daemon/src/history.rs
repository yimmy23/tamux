use anyhow::{Context, Result};
use amux_protocol::{
    AgentDbMessage, AgentDbThread, AgentEventRow, CommandLogEntry, HistorySearchHit,
    SnapshotIndexEntry, TranscriptIndexEntry, WormChainTip,
};
use crate::agent::types::{
    AgentTask, AgentTaskLogEntry, TaskLogLevel, TaskPriority, TaskStatus,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::{BufRead, Write};
use std::path::PathBuf;
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
    db_path: PathBuf,
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
    pub fn new() -> Result<Self> {
        let base = amux_protocol::ensure_amux_data_dir()?;
        let history_dir = base.join("history");
        let skill_dir = base.join("skills").join("generated");
        let telemetry_dir = base.join("semantic-logs");
        let worm_dir = telemetry_dir.join("worm");

        std::fs::create_dir_all(&history_dir)?;
        std::fs::create_dir_all(&skill_dir)?;
        std::fs::create_dir_all(&telemetry_dir)?;
        std::fs::create_dir_all(&worm_dir)?;

        let store = Self {
            db_path: history_dir.join("command-history.db"),
            skill_dir,
            telemetry_dir,
            worm_dir,
        };
        store.init_schema()?;
        Ok(store)
    }

    pub fn record_managed_finish(&self, record: &ManagedHistoryRecord) -> Result<()> {
        let connection = self.open_connection()?;
        let timestamp = now_ts() as i64;
        let excerpt = format!(
            "exit={:?} duration_ms={:?} snapshot={} rationale={}",
            record.exit_code,
            record.duration_ms,
            record.snapshot_path.as_deref().unwrap_or("none"),
            record.rationale
        );

        connection.execute(
            "INSERT OR REPLACE INTO history_entries (id, kind, title, excerpt, content, path, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                record.execution_id,
                "managed-command",
                record.command,
                excerpt,
                format!("{}\n{}", record.command, record.rationale),
                record.snapshot_path,
                timestamp,
            ],
        )?;
        connection.execute(
            "INSERT OR REPLACE INTO history_fts (id, title, excerpt, content) VALUES (?1, ?2, ?3, ?4)",
            params![record.execution_id, record.command, excerpt, record.rationale],
        )?;

        self.append_telemetry("operational", json!({
            "timestamp": timestamp,
            "execution_id": record.execution_id,
            "session_id": record.session_id,
            "workspace_id": record.workspace_id,
            "command": record.command,
            "exit_code": record.exit_code,
            "duration_ms": record.duration_ms,
            "snapshot": record.snapshot_path,
        }))?;
        self.append_telemetry("cognitive", json!({
            "timestamp": timestamp,
            "execution_id": record.execution_id,
            "source": record.source,
            "rationale": record.rationale,
        }))?;

        let mut system = System::new_all();
        system.refresh_memory();
        system.refresh_cpu();
        self.append_telemetry("contextual", json!({
            "timestamp": timestamp,
            "execution_id": record.execution_id,
            "total_memory": system.total_memory(),
            "used_memory": system.used_memory(),
            "cpu_usage": system.global_cpu_info().cpu_usage(),
        }))?;

        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<(String, Vec<HistorySearchHit>)> {
        let connection = self.open_connection()?;
        let mut stmt = connection.prepare(
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
    }

    pub fn generate_skill(&self, query: Option<&str>, title: Option<&str>) -> Result<(String, String)> {
        let title = title.unwrap_or("Recovered Workflow").trim();
        let (summary, hits) = self.search(query.unwrap_or("*"), 8).unwrap_or_else(|_| ("No history available.".to_string(), Vec::new()));
        let safe_name = title
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_ascii_lowercase();
        let path = self.skill_dir.join(format!("{}.md", if safe_name.is_empty() { "recovered-workflow" } else { &safe_name }));
        let mut body = format!("# {}\n\n## Summary\n{}\n\n## Retrieved Steps\n", title, summary);
        for hit in &hits {
            body.push_str(&format!("- {}\n", hit.title));
            body.push_str(&format!("  {}\n", hit.excerpt));
        }
        if hits.is_empty() {
            body.push_str("- No matching executions were available.\n");
        }
        std::fs::write(&path, body).with_context(|| format!("failed to write {}", path.display()))?;
        Ok((title.to_string(), path.to_string_lossy().into_owned()))
    }

    pub fn append_command_log(&self, entry: &CommandLogEntry) -> Result<()> {
        let connection = self.open_connection()?;
        connection.execute(
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
    }

    pub fn complete_command_log(
        &self,
        id: &str,
        exit_code: Option<i32>,
        duration_ms: Option<i64>,
    ) -> Result<()> {
        let connection = self.open_connection()?;
        connection.execute(
            "UPDATE command_log SET exit_code = ?2, duration_ms = ?3 WHERE id = ?1",
            params![id, exit_code, duration_ms],
        )?;
        Ok(())
    }

    pub fn query_command_log(
        &self,
        workspace_id: Option<&str>,
        pane_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<CommandLogEntry>> {
        let connection = self.open_connection()?;
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

        let mut stmt = connection.prepare(sql)?;
        let rows = match (workspace_id, pane_id) {
            (Some(workspace_id), Some(pane_id)) => stmt.query_map(
                params![workspace_id, pane_id, limit],
                map_command_log_entry,
            )?,
            (Some(workspace_id), None) => {
                stmt.query_map(params![workspace_id, limit], map_command_log_entry)?
            }
            (None, Some(pane_id)) => {
                stmt.query_map(params![pane_id, limit], map_command_log_entry)?
            }
            (None, None) => stmt.query_map(params![limit], map_command_log_entry)?,
        };

        Ok(rows.filter_map(|row| row.ok()).collect())
    }

    pub fn clear_command_log(&self) -> Result<()> {
        let connection = self.open_connection()?;
        connection.execute("DELETE FROM command_log", [])?;
        Ok(())
    }

    pub fn create_thread(&self, thread: &AgentDbThread) -> Result<()> {
        let connection = self.open_connection()?;
        connection.execute(
            "INSERT OR REPLACE INTO agent_threads \
             (id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
            ],
        )?;
        Ok(())
    }

    pub fn delete_thread(&self, id: &str) -> Result<()> {
        let connection = self.open_connection()?;
        connection.execute("DELETE FROM agent_threads WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn list_threads(&self) -> Result<Vec<AgentDbThread>> {
        let connection = self.open_connection()?;
        let mut stmt = connection.prepare(
            "SELECT id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview \
             FROM agent_threads ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], map_agent_thread)?;
        Ok(rows.filter_map(|row| row.ok()).collect())
    }

    pub fn get_thread(&self, id: &str) -> Result<Option<AgentDbThread>> {
        let connection = self.open_connection()?;
        connection
            .query_row(
                "SELECT id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview \
                 FROM agent_threads WHERE id = ?1",
                params![id],
                map_agent_thread,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn add_message(&self, message: &AgentDbMessage) -> Result<()> {
        let connection = self.open_connection()?;
        connection.execute(
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
        self.refresh_thread_stats(&connection, &message.thread_id)?;
        Ok(())
    }

    pub fn update_message(&self, id: &str, patch: &AgentMessagePatch) -> Result<()> {
        let connection = self.open_connection()?;
        let thread_id: Option<String> = connection
            .query_row(
                "SELECT thread_id FROM agent_messages WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;

        if thread_id.is_none() {
            return Ok(());
        }

        connection.execute(
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
            self.refresh_thread_stats(&connection, &thread_id)?;
        }
        Ok(())
    }

    pub fn list_messages(&self, thread_id: &str, limit: Option<usize>) -> Result<Vec<AgentDbMessage>> {
        let connection = self.open_connection()?;
        let limit = limit.unwrap_or(500).max(1) as i64;
        let mut stmt = connection.prepare(
            "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, reasoning, tool_calls_json, metadata_json \
             FROM agent_messages WHERE thread_id = ?1 ORDER BY created_at ASC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![thread_id, limit], map_agent_message)?;
        Ok(rows.filter_map(|row| row.ok()).collect())
    }

    pub fn get_worm_chain_tip(&self, kind: &str) -> Result<Option<WormChainTip>> {
        let connection = self.open_connection()?;
        connection
            .query_row(
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
    }

    pub fn set_worm_chain_tip(&self, kind: &str, seq: i64, hash: &str) -> Result<()> {
        let connection = self.open_connection()?;
        connection.execute(
            "INSERT INTO worm_chain_tip (kind, seq, hash) VALUES (?1, ?2, ?3) \
             ON CONFLICT(kind) DO UPDATE SET seq = excluded.seq, hash = excluded.hash",
            params![kind, seq, hash],
        )?;
        Ok(())
    }

    pub fn upsert_transcript_index(&self, entry: &TranscriptIndexEntry) -> Result<()> {
        let connection = self.open_connection()?;
        connection.execute(
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
    }

    pub fn list_transcript_index(&self, workspace_id: Option<&str>) -> Result<Vec<TranscriptIndexEntry>> {
        let connection = self.open_connection()?;
        let sql = if workspace_id.is_some() {
            "SELECT id, pane_id, workspace_id, surface_id, filename, reason, captured_at, size_bytes, preview \
             FROM transcript_index WHERE workspace_id = ?1 ORDER BY captured_at DESC"
        } else {
            "SELECT id, pane_id, workspace_id, surface_id, filename, reason, captured_at, size_bytes, preview \
             FROM transcript_index ORDER BY captured_at DESC"
        };
        let mut stmt = connection.prepare(sql)?;
        let rows = if let Some(workspace_id) = workspace_id {
            stmt.query_map(params![workspace_id], map_transcript_index_entry)?
        } else {
            stmt.query_map([], map_transcript_index_entry)?
        };
        Ok(rows.filter_map(|row| row.ok()).collect())
    }

    pub fn upsert_snapshot_index(&self, entry: &SnapshotIndexEntry) -> Result<()> {
        let connection = self.open_connection()?;
        connection.execute(
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
    }

    pub fn list_snapshot_index(&self, workspace_id: Option<&str>) -> Result<Vec<SnapshotIndexEntry>> {
        let connection = self.open_connection()?;
        let sql = if workspace_id.is_some() {
            "SELECT snapshot_id, workspace_id, session_id, kind, label, path, created_at, details_json \
             FROM snapshot_index WHERE workspace_id = ?1 ORDER BY created_at DESC"
        } else {
            "SELECT snapshot_id, workspace_id, session_id, kind, label, path, created_at, details_json \
             FROM snapshot_index ORDER BY created_at DESC"
        };
        let mut stmt = connection.prepare(sql)?;
        let rows = if let Some(workspace_id) = workspace_id {
            stmt.query_map(params![workspace_id], map_snapshot_index_entry)?
        } else {
            stmt.query_map([], map_snapshot_index_entry)?
        };
        Ok(rows.filter_map(|row| row.ok()).collect())
    }

    pub fn get_snapshot_index(&self, snapshot_id: &str) -> Result<Option<SnapshotIndexEntry>> {
        let connection = self.open_connection()?;
        connection
            .query_row(
                "SELECT snapshot_id, workspace_id, session_id, kind, label, path, created_at, details_json \
                 FROM snapshot_index WHERE snapshot_id = ?1",
                params![snapshot_id],
                map_snapshot_index_entry,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn upsert_agent_event(&self, entry: &AgentEventRow) -> Result<()> {
        let connection = self.open_connection()?;
        connection.execute(
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
    }

    pub fn list_agent_events(
        &self,
        category: Option<&str>,
        pane_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<AgentEventRow>> {
        let connection = self.open_connection()?;
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
        let mut stmt = connection.prepare(sql)?;
        let rows = match (category, pane_id) {
            (Some(category), Some(pane_id)) => stmt.query_map(params![category, pane_id, limit], map_agent_event_row)?,
            (Some(category), None) => stmt.query_map(params![category, limit], map_agent_event_row)?,
            (None, Some(pane_id)) => stmt.query_map(params![pane_id, limit], map_agent_event_row)?,
            (None, None) => stmt.query_map(params![limit], map_agent_event_row)?,
        };
        Ok(rows.filter_map(|row| row.ok()).collect())
    }

    pub fn upsert_agent_task(&self, task: &AgentTask) -> Result<()> {
        let mut connection = self.open_connection()?;
        let transaction = connection.transaction()?;
        let notify_channels_json = serde_json::to_string(&task.notify_channels)?;

        transaction.execute(
            "INSERT OR REPLACE INTO agent_tasks \
             (id, title, description, status, priority, progress, created_at, started_at, completed_at, error, result, thread_id, source, notify_on_complete, notify_channels_json, command, session_id, retry_count, max_retries, next_retry_at, blocked_reason, awaiting_approval_id, lane_id, last_error) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
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
                task.retry_count as i64,
                task.max_retries as i64,
                task.next_retry_at.map(|value| value as i64),
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

        transaction.execute("DELETE FROM agent_task_logs WHERE task_id = ?1", params![&task.id])?;
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
    }

    pub fn delete_agent_task(&self, task_id: &str) -> Result<()> {
        let connection = self.open_connection()?;
        connection.execute("DELETE FROM agent_tasks WHERE id = ?1", params![task_id])?;
        Ok(())
    }

    pub fn list_agent_tasks(&self) -> Result<Vec<AgentTask>> {
        let connection = self.open_connection()?;
        let mut dependency_stmt = connection.prepare(
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

        let mut log_stmt = connection.prepare(
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

        let mut stmt = connection.prepare(
            "SELECT id, title, description, status, priority, progress, created_at, started_at, completed_at, error, result, thread_id, source, notify_on_complete, notify_channels_json, command, session_id, retry_count, max_retries, next_retry_at, blocked_reason, awaiting_approval_id, lane_id, last_error \
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
                retry_count: row.get::<_, i64>(17)? as u32,
                max_retries: row.get::<_, i64>(18)? as u32,
                next_retry_at: row.get::<_, Option<i64>>(19)?.map(|value| value as u64),
                blocked_reason: row.get(20)?,
                awaiting_approval_id: row.get(21)?,
                lane_id: row.get(22)?,
                last_error: row.get(23)?,
                logs: Vec::new(),
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
    }

    fn init_schema(&self) -> Result<()> {
        let connection = self.open_connection()?;
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
                last_preview   TEXT NOT NULL DEFAULT ''
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
                retry_count          INTEGER NOT NULL DEFAULT 0,
                max_retries          INTEGER NOT NULL DEFAULT 3,
                next_retry_at        INTEGER,
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
            ",
        )?;
        let _ = connection.execute(
            "ALTER TABLE agent_tasks ADD COLUMN session_id TEXT",
            [],
        );
        Ok(())
    }

    fn append_telemetry(&self, kind: &str, payload: serde_json::Value) -> Result<()> {
        let line = serde_json::to_string(&payload)?;
        let log_path = self.telemetry_dir.join(format!("{}.jsonl", kind));
        let worm_path = self.worm_dir.join(format!("{}-ledger.jsonl", kind));

        append_line(&log_path, &line)?;

        let (prev_hash, seq) = match self.get_worm_chain_tip(kind)? {
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
        self.set_worm_chain_tip(kind, seq, &hash)?;
        Ok(())
    }

    /// Detect sequences of 3+ consecutive successful managed commands
    /// that completed within a 5-minute window.
    pub fn detect_skill_candidates(&self) -> Result<Vec<(String, Vec<HistorySearchHit>)>> {
        let connection = self.open_connection()?;
        let mut stmt = connection.prepare(
            "SELECT id, kind, title, excerpt, path, timestamp FROM history_entries \
             WHERE kind = 'managed-command' \
             ORDER BY timestamp DESC LIMIT 20"
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
    }

    /// Verify the hash-chain integrity of all WORM telemetry ledger files.
    pub fn verify_worm_integrity(&self) -> Result<Vec<WormIntegrityResult>> {
        let ledger_kinds = ["operational", "cognitive", "contextual"];
        let mut results = Vec::with_capacity(ledger_kinds.len());

        for kind in &ledger_kinds {
            let worm_path = self.worm_dir.join(format!("{}-ledger.jsonl", kind));
            results.push(verify_ledger_file(kind, &worm_path));
        }

        Ok(results)
    }

    fn open_connection(&self) -> Result<Connection> {
        let connection = Connection::open(&self.db_path)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        Ok(connection)
    }

    fn refresh_thread_stats(&self, connection: &Connection, thread_id: &str) -> Result<()> {
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

fn map_transcript_index_entry(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<TranscriptIndexEntry> {
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

fn map_snapshot_index_entry(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<SnapshotIndexEntry> {
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

fn append_line(path: &PathBuf, line: &str) -> Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{line}")?;
    Ok(())
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
                    failure_message = Some(format!("IO error reading line at seq {}: {}", expected_seq, e));
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
                    failure_message = Some(format!("JSON parse error at seq {}: {}", expected_seq, e));
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
        format!("{} ledger: all {} entries verified successfully.", kind, total)
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