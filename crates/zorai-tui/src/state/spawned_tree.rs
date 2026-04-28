#![allow(dead_code)]

use super::task::TaskStatus;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnedAgentTreeNode<T> {
    pub item: T,
    pub children: Vec<SpawnedAgentTreeNode<T>>,
    pub openable: bool,
    pub live: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnedAgentTree<T> {
    pub active_thread_id: String,
    pub anchor: Option<SpawnedAgentTreeNode<T>>,
    pub roots: Vec<SpawnedAgentTreeNode<T>>,
}

pub trait SpawnedAgentTreeSource {
    fn spawned_tree_identity(&self) -> &str;
    fn spawned_tree_created_at(&self) -> u64;
    fn spawned_tree_thread_id(&self) -> Option<&str>;
    fn spawned_tree_parent_task_id(&self) -> Option<&str>;
    fn spawned_tree_parent_thread_id(&self) -> Option<&str>;
    fn spawned_tree_status(&self) -> Option<TaskStatus>;
}

fn compare_spawned_tree_items<T: SpawnedAgentTreeSource>(left: &T, right: &T) -> Ordering {
    right
        .spawned_tree_created_at()
        .cmp(&left.spawned_tree_created_at())
        .then_with(|| {
            left.spawned_tree_identity()
                .cmp(right.spawned_tree_identity())
        })
}

fn unique_by_identity<T: SpawnedAgentTreeSource + Clone>(items: &[T]) -> Vec<T> {
    let mut by_identity: HashMap<String, T> = HashMap::new();

    for item in items {
        let identity = item.spawned_tree_identity().to_string();
        match by_identity.get(&identity) {
            Some(current) if compare_spawned_tree_items(item, current) >= Ordering::Equal => {}
            _ => {
                by_identity.insert(identity, item.clone());
            }
        }
    }

    let mut deduped: Vec<T> = by_identity.into_values().collect();
    deduped.sort_by(compare_spawned_tree_items);
    deduped
}

fn build_indexes<T: SpawnedAgentTreeSource + Clone>(
    items: &[T],
) -> (
    HashMap<String, Vec<T>>,
    HashMap<String, Vec<T>>,
    HashMap<String, Vec<T>>,
) {
    let mut by_thread_id: HashMap<String, Vec<T>> = HashMap::new();
    let mut by_parent_task_id: HashMap<String, Vec<T>> = HashMap::new();
    let mut by_parent_thread_id: HashMap<String, Vec<T>> = HashMap::new();

    let push = |map: &mut HashMap<String, Vec<T>>, key: &str, item: &T| {
        map.entry(key.to_string()).or_default().push(item.clone());
    };

    for item in items {
        if let Some(thread_id) = item.spawned_tree_thread_id() {
            push(&mut by_thread_id, thread_id, item);
        }
        if let Some(parent_task_id) = item.spawned_tree_parent_task_id() {
            push(&mut by_parent_task_id, parent_task_id, item);
        }
        if let Some(parent_thread_id) = item.spawned_tree_parent_thread_id() {
            push(&mut by_parent_thread_id, parent_thread_id, item);
        }
    }

    for map in [
        &mut by_thread_id,
        &mut by_parent_task_id,
        &mut by_parent_thread_id,
    ] {
        for bucket in map.values_mut() {
            bucket.sort_by(compare_spawned_tree_items);
        }
    }

    (by_thread_id, by_parent_task_id, by_parent_thread_id)
}

fn is_terminal(status: Option<TaskStatus>) -> bool {
    matches!(
        status,
        Some(TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled)
            | Some(TaskStatus::BudgetExceeded)
    )
}

fn has_resolved_parent<T: SpawnedAgentTreeSource>(
    item: &T,
    identity_lookup: &HashSet<String>,
) -> bool {
    item.spawned_tree_parent_task_id()
        .is_some_and(|parent| identity_lookup.contains(parent))
}

fn build_children<T: SpawnedAgentTreeSource + Clone>(
    item: &T,
    by_parent_task_id: &HashMap<String, Vec<T>>,
    by_parent_thread_id: &HashMap<String, Vec<T>>,
    root_identity_lookup: &HashSet<String>,
    ancestry: &HashSet<String>,
) -> Vec<SpawnedAgentTreeNode<T>> {
    let current_identity = item.spawned_tree_identity().to_string();

    let mut candidates = Vec::new();
    if let Some(direct_children) = by_parent_task_id.get(&current_identity) {
        candidates.extend(direct_children.iter().cloned());
    }
    if let Some(thread_id) = item.spawned_tree_thread_id() {
        if let Some(thread_children) = by_parent_thread_id.get(thread_id) {
            candidates.extend(thread_children.iter().cloned());
        }
    }

    let mut children = unique_by_identity(&candidates);
    children.retain(|candidate| {
        let identity = candidate.spawned_tree_identity();
        !root_identity_lookup.contains(identity)
            && identity != current_identity.as_str()
            && !ancestry.contains(identity)
    });

    children
        .into_iter()
        .map(|child| {
            let mut next_ancestry = ancestry.clone();
            next_ancestry.insert(child.spawned_tree_identity().to_string());
            SpawnedAgentTreeNode {
                openable: child.spawned_tree_thread_id().is_some(),
                live: !is_terminal(child.spawned_tree_status()),
                children: build_children(
                    &child,
                    by_parent_task_id,
                    by_parent_thread_id,
                    root_identity_lookup,
                    &next_ancestry,
                ),
                item: child,
            }
        })
        .collect()
}

fn build_node<T: SpawnedAgentTreeSource + Clone>(
    item: T,
    by_parent_task_id: &HashMap<String, Vec<T>>,
    by_parent_thread_id: &HashMap<String, Vec<T>>,
    root_identity_lookup: &HashSet<String>,
) -> SpawnedAgentTreeNode<T> {
    let mut ancestry = HashSet::new();
    ancestry.insert(item.spawned_tree_identity().to_string());
    SpawnedAgentTreeNode {
        openable: item.spawned_tree_thread_id().is_some(),
        live: !is_terminal(item.spawned_tree_status()),
        children: build_children(
            &item,
            by_parent_task_id,
            by_parent_thread_id,
            root_identity_lookup,
            &ancestry,
        ),
        item,
    }
}

pub fn derive_spawned_agent_tree<T: SpawnedAgentTreeSource + Clone>(
    items: &[T],
    active_thread_id: Option<&str>,
) -> Option<SpawnedAgentTree<T>> {
    let active_thread_id = active_thread_id.filter(|thread_id| !thread_id.is_empty())?;
    if items.is_empty() {
        return None;
    }

    let canonical_items = unique_by_identity(items);
    let (by_thread_id, by_parent_task_id, by_parent_thread_id) = build_indexes(&canonical_items);
    let identity_lookup: HashSet<String> = canonical_items
        .iter()
        .map(|item| item.spawned_tree_identity().to_string())
        .collect();

    let active_thread_items = by_thread_id
        .get(active_thread_id)
        .cloned()
        .unwrap_or_default();
    let anchor_candidate = active_thread_items
        .iter()
        .find(|item| !has_resolved_parent(*item, &identity_lookup))
        .cloned()
        .or_else(|| active_thread_items.first().cloned());

    let mut visible_root_candidates: Vec<T> = anchor_candidate
        .iter()
        .cloned()
        .chain(
            by_parent_thread_id
                .get(active_thread_id)
                .into_iter()
                .flatten()
                .filter(|item| !has_resolved_parent(*item, &identity_lookup))
                .cloned(),
        )
        .collect();
    visible_root_candidates.sort_by(compare_spawned_tree_items);
    visible_root_candidates = unique_by_identity(&visible_root_candidates);

    if visible_root_candidates.is_empty() {
        return None;
    }

    let root_identity_lookup: HashSet<String> = visible_root_candidates
        .iter()
        .map(|item| item.spawned_tree_identity().to_string())
        .collect();

    let roots = visible_root_candidates
        .into_iter()
        .filter(|item| {
            anchor_candidate
                .as_ref()
                .is_none_or(|anchor| item.spawned_tree_identity() != anchor.spawned_tree_identity())
        })
        .map(|item| {
            build_node(
                item,
                &by_parent_task_id,
                &by_parent_thread_id,
                &root_identity_lookup,
            )
        })
        .collect();

    Some(SpawnedAgentTree {
        active_thread_id: active_thread_id.to_string(),
        anchor: anchor_candidate.map(|item| {
            build_node(
                item,
                &by_parent_task_id,
                &by_parent_thread_id,
                &root_identity_lookup,
            )
        }),
        roots,
    })
}
