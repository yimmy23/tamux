#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkillMeshRecompileJobKind {
    Compile,
    Invalidate,
}

impl SkillMeshRecompileJobKind {
    pub fn is_invalidation(&self) -> bool {
        matches!(self, Self::Invalidate)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillMeshRecompileJob {
    pub path: PathBuf,
    pub kind: SkillMeshRecompileJobKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillMeshWatchEvent {
    Write(PathBuf),
    Rename { from: PathBuf, to: PathBuf },
    Delete(PathBuf),
}

impl SkillMeshWatchEvent {
    pub fn write(path: PathBuf) -> Self {
        Self::Write(path)
    }

    pub fn rename(from: PathBuf, to: PathBuf) -> Self {
        Self::Rename { from, to }
    }

    pub fn delete(path: PathBuf) -> Self {
        Self::Delete(path)
    }
}

pub fn debounce_skill_events(
    events: Vec<SkillMeshWatchEvent>,
    _debounce_window: Duration,
) -> Vec<SkillMeshRecompileJob> {
    let mut jobs = BTreeMap::<PathBuf, SkillMeshRecompileJobKind>::new();

    for event in events {
        match event {
            SkillMeshWatchEvent::Write(path) => {
                jobs.insert(path, SkillMeshRecompileJobKind::Compile);
            }
            SkillMeshWatchEvent::Rename { from, to } => {
                jobs.insert(from, SkillMeshRecompileJobKind::Invalidate);
                jobs.insert(to, SkillMeshRecompileJobKind::Compile);
            }
            SkillMeshWatchEvent::Delete(path) => {
                jobs.insert(path, SkillMeshRecompileJobKind::Invalidate);
            }
        }
    }

    jobs.into_iter()
        .map(|(path, kind)| SkillMeshRecompileJob { path, kind })
        .collect()
}
