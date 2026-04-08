//! Always-on autonomous agent engine.
//!
//! The agent lives in the daemon process and handles:
//! - LLM inference with streaming (OpenAI-compatible + Anthropic)
//! - Tool execution via SessionManager
//! - Persistent task queue with automatic processing
//! - Heartbeat system for periodic checks
//! - Persistent identity (SOUL.md, MEMORY.md, USER.md)

pub mod external_runner;
pub mod gateway;
pub mod llm_client;
pub mod tool_executor;
pub mod types;

mod agent_identity;
mod agent_loop;
mod aline_startup;
mod anticipatory;
mod anticipatory_support;
mod authorship;
mod autonomy;
mod behavioral_events;
pub(crate) mod capability_tier;
mod causal_traces;
mod circuit_breaker;
mod collaboration;
mod compaction;
mod config;
mod consolidation;
pub(crate) mod copilot_auth;
mod dispatcher;
mod engine;
mod engine_runtime;
mod explainability;
mod explanation;
mod external_messaging;
mod gateway_health;
mod gateway_loop;
mod goal_llm;
mod goal_parsing;
mod goal_planner;
mod heartbeat;
mod heartbeat_checks;
mod honcho;
mod memory;
mod memory_flush;
mod messaging;
mod metadata;
mod notifications;
pub(crate) mod openai_codex_auth;
mod operational_context;
mod operator_model;
mod orchestrator_policy;
mod persistence;
mod prompt_inspection;
mod provenance;
mod provider_auth_store;
mod provider_resolution;
pub mod rate_limiter;
mod runtime_continuity;
mod semantic_env;
mod session_recall;
pub(crate) mod skill_community;
mod skill_discovery;
mod skill_evolution;
mod skill_preflight;
mod skill_recommendation;
pub(crate) mod skill_registry;
mod skill_security;
mod stalled_turns;
mod system_prompt;
mod task_crud;
mod task_prompt;
mod task_scheduler;
mod thread_crud;
mod thread_handoffs;
mod tool_synthesis;
pub(crate) mod weles_governance;
mod weles_health;
mod whatsapp_link;
mod whatsapp_native;
mod work_context;

pub mod awareness;
pub mod concierge;
pub mod context;
pub mod cost;
pub mod embodied;
pub mod episodic;
pub mod handoff;
pub mod learning;
pub mod liveness;
pub mod metacognitive;
pub mod operator_profile;
pub mod subagent;
pub mod uncertainty;

// Re-exports from extracted modules — keeps everything accessible across
// sibling submodules via `use super::*;`.
use agent_identity::*;
use aline_startup::{
    parse_session_list_json, parse_watcher_status, repo_root_basename,
    repo_root_matches_project_name, select_import_candidates, AlineDiscoveredSession,
    AlineStartupShortCircuitReason, AlineStartupSummary, SessionListJson, StartupCommandOutput,
    StartupCommandRunner, StartupCommandSpec, StartupSelectionPolicy, TokioStartupCommandRunner,
    WatcherState, WatcherStatus, IMPORT_TIMEOUT, RECONCILIATION_BUDGET, TRACKED_POLL_INTERVAL,
    TRACKED_POLL_MAX_ATTEMPTS, WATCHER_COMMAND_TIMEOUT,
};
use anticipatory::*;
use anticipatory_support::*;
use behavioral_events::*;
use capability_tier::*;
use compaction::*;
pub(crate) use config::{
    ConfigEffectiveRuntimeState, ConfigReconcileState, ConfigRuntimeProjection,
};
pub(crate) use explanation::*;
pub(crate) use gateway_health::GatewayConnectionStatus as RuntimeGatewayConnectionStatus;
use goal_parsing::*;
use honcho::*;
use memory::*;
use metadata::*;
use notifications::*;
use operator_model::*;
use provider_resolution::*;
use runtime_continuity::*;
use system_prompt::*;
use task_prompt::*;
use task_scheduler::*;
use thread_handoffs::*;

// Imports needed by child modules via `use super::*;`.
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use futures::StreamExt;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::history::HistoryStore;
use crate::session_manager::SessionManager;

use self::llm_client::{send_completion_request, ApiContent, ApiMessage, RetryStrategy};
use self::tool_executor::{execute_tool, get_available_tools};
use self::types::*;

// Public re-exports consumed by sibling modules in this bin crate.
pub(crate) use config::canonicalize_weles_client_update;
#[allow(unused_imports)]
pub use engine::*;
#[cfg(test)]
pub(crate) use provider_auth_store::provider_auth_test_env_lock;
pub use task_prompt::load_config_from_history;
pub(crate) use whatsapp_link::{
    clear_persisted_provider_state, load_persisted_provider_state, merge_persisted_state_update,
    persist_transport_session_update, save_persisted_provider_state,
    transport::PersistedState as WhatsAppPersistedState, WHATSAPP_LINK_PROVIDER_ID,
};
#[allow(unused_imports)]
pub(crate) use whatsapp_native::{
    disconnect_native_whatsapp_client, send_native_whatsapp_message, start_whatsapp_link_native,
    whatsapp_native_store_path,
};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
