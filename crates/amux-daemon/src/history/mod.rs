use crate::agent::liveness::state_layers::CheckpointType;
use crate::agent::types::{
    AgentTask, AgentTaskLogEntry, GoalRun, GoalRunEvent, GoalRunStatus, GoalRunStep,
    GoalRunStepKind, GoalRunStepStatus, TaskLogLevel, TaskPriority, TaskStatus,
};
use amux_protocol::{
    AgentDbMessage, AgentDbThread, AgentEventRow, CommandLogEntry, GatewayHealthState,
    HistorySearchHit, SnapshotIndexEntry, TranscriptIndexEntry, WormChainTip,
};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_rusqlite;

/// Helper trait to convert any error into `tokio_rusqlite::Error` inside `.call()` closures.
trait IntoCallError<T> {
    fn call_err(self) -> std::result::Result<T, tokio_rusqlite::Error>;
}

impl<T, E: std::error::Error + Send + Sync + 'static> IntoCallError<T>
    for std::result::Result<T, E>
{
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

#[derive(Debug, Clone)]
pub struct ManagedCommandFinishedRecord {
    pub command: String,
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

#[derive(Debug, Clone)]
pub struct OperatorProfileSessionRow {
    pub session_id: String,
    pub kind: String,
    pub session_json: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone)]
pub struct ProviderAuthStateRow {
    pub provider_id: String,
    pub auth_mode: String,
    pub state_json: serde_json::Value,
    pub updated_at: i64,
}

/// A single learned or answered profile field for the operator.
#[derive(Debug, Clone)]
pub struct OperatorProfileFieldRow {
    pub field_key: String,
    pub field_value_json: String,
    pub confidence: f64,
    pub source: String,
    pub updated_at: i64,
}

/// Consent record for a specific data-collection or behaviour consent key.
#[derive(Debug, Clone)]
pub struct OperatorProfileConsentRow {
    pub consent_key: String,
    pub granted: bool,
    pub updated_at: i64,
}

/// An event in the operator-profile event log (answer, inference, prompt, skip, deferral).
#[derive(Debug, Clone)]
pub struct OperatorProfileEventRow {
    pub id: String,
    pub event_type: String,
    pub field_key: Option<String>,
    pub value_json: Option<String>,
    pub source: String,
    pub metadata_json: Option<String>,
    pub created_at: i64,
}

/// A scheduled check-in for the operator profile questionnaire flow.
#[derive(Debug, Clone)]
pub struct OperatorProfileCheckinRow {
    pub id: String,
    pub kind: String,
    pub scheduled_at: i64,
    pub shown_at: Option<i64>,
    pub status: String,
    pub response_json: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
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

#[derive(Debug, Clone)]
pub struct WhatsAppProviderStateRow {
    pub provider_id: String,
    pub linked_phone: Option<String>,
    pub auth_json: Option<String>,
    pub metadata_json: Option<String>,
    pub last_reset_at: Option<u64>,
    pub last_linked_at: Option<u64>,
    pub updated_at: u64,
}

#[derive(Debug, Clone)]
pub struct GatewayReplayCursorRow {
    pub platform: String,
    pub channel_id: String,
    pub cursor_value: String,
    pub cursor_type: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone)]
pub struct GatewayHealthSnapshotRow {
    pub platform: String,
    pub state_json: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalRecordRow {
    pub approval_id: String,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub thread_id: Option<String>,
    pub transition_kind: String,
    pub stage_id: Option<String>,
    pub scope_summary: Option<String>,
    pub target_scope_json: String,
    pub constraints_json: String,
    pub risk_class: String,
    pub rationale_json: String,
    pub policy_fingerprint: String,
    pub requested_at: u64,
    pub resolved_at: Option<u64>,
    pub expires_at: Option<u64>,
    pub resolution: Option<String>,
    pub invalidated_at: Option<u64>,
    pub invalidation_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceEvaluationRow {
    pub id: String,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub thread_id: Option<String>,
    pub transition_kind: String,
    pub input_json: String,
    pub verdict_json: String,
    pub policy_fingerprint: String,
    pub created_at: u64,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillVariantInspection {
    pub record: SkillVariantRecord,
    pub lifecycle_summary: String,
    pub selection_summary: String,
    pub selected_for_context: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillVariantPage {
    pub variants: Vec<SkillVariantRecord>,
    pub next_cursor: Option<String>,
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
pub struct MemoryProvenanceRelationship {
    pub related_entry_id: String,
    pub relation_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact_key: Option<String>,
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
    pub relationships: Vec<MemoryProvenanceRelationship>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_scheme: Option<String>,
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
    pub signature_scheme: Option<String>,
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

mod audit;
mod causal_traces;
mod checkpoints;
mod command_log;
mod consolidation;
mod context_archive;
mod core;
mod gateway_state;
mod goal_runs;
mod governance;
mod integrity_helpers;
mod offloaded_payloads;
mod operator_profile;
mod provenance;
mod row_mapping;
mod schema;
mod schema_helpers;
mod schema_migrations;
mod schema_sql;
mod schema_sql_extra;
mod skill_generation;
pub(crate) use skill_generation::page_skill_variants;
mod skill_metadata;
pub(crate) use skill_metadata::{derive_skill_metadata, DerivedSkillMetadata};
mod skill_tagging;
mod skill_variants;
mod task_enums;
mod tasks;
mod thread_structural_memory;
mod threads;

use integrity_helpers::*;
pub use offloaded_payloads::OffloadedPayloadMetadataRow;
use row_mapping::*;
use schema_helpers::*;
use skill_metadata::*;
use skill_tagging::*;
use task_enums::*;
pub use thread_structural_memory::ThreadStructuralMemoryRow;

pub(crate) fn now_ts() -> u64 {
    integrity_helpers::now_ts()
}

#[cfg(test)]
mod tests;
