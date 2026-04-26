use super::*;
use std::fs;
use uuid::Uuid;

async fn make_test_store() -> Result<(HistoryStore, PathBuf)> {
    let root = std::env::temp_dir().join(format!("tamux-history-test-{}", Uuid::new_v4()));
    let store = HistoryStore::new_test_store(&root).await?;
    Ok((store, root))
}

mod cognitive_resonance;
mod dream_state;
mod elastic_context;
mod emergent_protocol;
mod event_log;
mod event_triggers;
mod goal_runs;
mod governance;
mod harness;
mod metacognition;
mod misc;
mod notifications;
mod provider_auth;
mod skill_variants;
mod sqlite_audit;
mod statistics;
mod temporal_foresight;
mod tombstones_gateway;
mod workspaces;
