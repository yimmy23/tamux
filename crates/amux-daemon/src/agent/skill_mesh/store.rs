use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::{Mutex, OnceLock};

use anyhow::Result;

use super::index::{LanceDbSkillMeshIndex, SkillMeshIndex};
use super::types::{SkillMeshDocument, SkillMeshDocumentKey};
use super::watcher::{debounce_skill_events, SkillMeshRecompileJob, SkillMeshWatchEvent};

#[derive(Debug)]
pub struct SkillMeshStore {
    index: LanceDbSkillMeshIndex,
    pending_jobs: tokio::sync::Mutex<Vec<SkillMeshRecompileJob>>,
}

impl SkillMeshStore {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            index: LanceDbSkillMeshIndex::open(path).await?,
            pending_jobs: tokio::sync::Mutex::new(Vec::new()),
        })
    }

    pub async fn upsert_document(&self, document: SkillMeshDocument) -> Result<()> {
        self.index.upsert_document(document).await
    }

    pub async fn get_document(
        &self,
        key: &SkillMeshDocumentKey,
    ) -> Result<Option<SkillMeshDocument>> {
        self.index.get_document(key).await
    }

    pub async fn enqueue_jobs(&self, jobs: Vec<SkillMeshRecompileJob>) {
        let mut pending = self.pending_jobs.lock().await;
        pending.extend(jobs);
        dedupe_jobs(&mut pending);
    }

    pub fn pending_recompile_jobs(&self) -> Vec<SkillMeshRecompileJob> {
        self.pending_jobs.blocking_lock().iter().cloned().collect()
    }

    pub fn bump_compile_version_for_tests(&self) {
        self.pending_jobs
            .blocking_lock()
            .push(SkillMeshRecompileJob {
                path: PathBuf::from("compile-version-change"),
                kind: super::watcher::SkillMeshRecompileJobKind::Invalidate,
            });
    }

    pub fn update_trust_inputs_for_tests(&self) {
        self.pending_jobs
            .blocking_lock()
            .push(SkillMeshRecompileJob {
                path: PathBuf::from("trust-input-change"),
                kind: super::watcher::SkillMeshRecompileJobKind::Invalidate,
            });
    }
}

pub async fn apply_watch_event(store: &SkillMeshStore, event: SkillMeshWatchEvent) -> Result<()> {
    store
        .enqueue_jobs(debounce_skill_events(
            vec![event],
            std::time::Duration::from_millis(0),
        ))
        .await;
    Ok(())
}

pub fn sample_rename_event() -> SkillMeshWatchEvent {
    SkillMeshWatchEvent::rename(
        PathBuf::from("skills/development/old-debug/SKILL.md"),
        PathBuf::from("skills/development/new-debug/SKILL.md"),
    )
}

pub fn sample_delete_event() -> SkillMeshWatchEvent {
    SkillMeshWatchEvent::delete(PathBuf::from("skills/development/old-debug/SKILL.md"))
}

#[cfg(test)]
pub async fn sample_persistent_mesh_store() -> SkillMeshStore {
    sample_persistent_mesh_store_named("default").await
}

#[cfg(test)]
pub async fn sample_persistent_mesh_store_named(name: &str) -> SkillMeshStore {
    let path = test_store_root(name);
    SkillMeshStore::open(path)
        .await
        .expect("sample persistent mesh store should open")
}

fn dedupe_jobs(jobs: &mut Vec<SkillMeshRecompileJob>) {
    let mut seen = BTreeSet::new();
    jobs.retain(|job| seen.insert((job.path.clone(), job.kind.clone())));
}

#[cfg(test)]
fn test_store_root(name: &str) -> PathBuf {
    static INITIALIZED: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();

    let sanitized = name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>();
    let root = std::env::temp_dir()
        .join("tamux-skill-mesh-tests")
        .join(&sanitized);

    let initialized = INITIALIZED.get_or_init(|| Mutex::new(BTreeSet::new()));
    let mut seen = initialized.lock().expect("test store init mutex poisoned");
    if seen.insert(sanitized) {
        let _ = std::fs::remove_dir_all(&root);
    }
    let _ = std::fs::create_dir_all(&root);
    root
}
