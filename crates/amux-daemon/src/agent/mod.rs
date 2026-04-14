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

pub(crate) mod agent_identity;
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
mod critique;
mod debate;
mod dispatcher;
mod emergent_protocol;
mod engine;
mod engine_runtime;
mod explainability;
mod explanation;
mod external_messaging;
mod forge;
mod gateway_health;
mod gateway_loop;
mod goal_llm;
mod goal_parsing;
mod goal_planner;
mod heartbeat;
mod heartbeat_checks;
mod honcho;
mod memory;
mod memory_context;
mod memory_distillation;
mod memory_flush;
mod messaging;
mod metadata;
mod notifications;
pub(crate) mod openai_codex_auth;
mod operational_context;
mod operator_model;
mod operator_questions;
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
pub(crate) mod skill_mesh;
pub(crate) mod skill_preflight;
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
mod thread_participant_runner;
pub(crate) mod thread_participants;
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
use anticipatory::*;
use anticipatory_support::*;
pub(crate) use authorship::AuthorshipTag;
pub(crate) use autonomy::AutonomyLevel;
use behavioral_events::*;
use compaction::*;
pub(crate) use config::ConfigRuntimeProjection;
pub(crate) use explanation::*;
pub(crate) use gateway_health::GatewayConnectionStatus as RuntimeGatewayConnectionStatus;
use goal_parsing::*;
use honcho::*;
use memory::*;
use metadata::*;
use operator_model::*;
use operator_questions::*;
use provider_resolution::*;
use runtime_continuity::*;
use system_prompt::*;
use task_prompt::*;
use task_scheduler::*;
use thread_handoffs::*;
pub(crate) use thread_participants::*;

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
#[cfg(test)]
pub(crate) use aline_startup::AlineStartupShortCircuitReason;
pub(crate) use aline_startup::{
    run_aline_startup_worker_from_stdio, WatcherState, ALINE_STARTUP_WORKER_ARG,
};
pub(crate) use config::canonicalize_weles_client_update;
#[cfg(test)]
pub(crate) use config::{ConfigEffectiveRuntimeState, ConfigReconcileState};
#[allow(unused_imports)]
pub use engine::*;
#[cfg(test)]
pub(crate) use provider_auth_store::provider_auth_test_env_lock;
pub use task_prompt::load_config_from_history;
#[cfg(test)]
pub(crate) use whatsapp_link::transport::PersistedState as WhatsAppPersistedState;
pub(crate) use whatsapp_link::{
    clear_persisted_provider_state, persist_transport_session_update, WHATSAPP_LINK_PROVIDER_ID,
};
#[cfg(test)]
pub(crate) use whatsapp_link::{load_persisted_provider_state, save_persisted_provider_state};
#[allow(unused_imports)]
pub(crate) use whatsapp_native::{
    disconnect_native_whatsapp_client, send_native_whatsapp_message, start_whatsapp_link_native,
    whatsapp_native_store_path,
};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
