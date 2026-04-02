//! Tool composition engine — graph-based tool relationships and cached tool sequences.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A node in the tool graph representing a single tool and its metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolNode {
    pub name: String,
    pub capabilities: Vec<String>,
    pub limitations: Vec<String>,
    pub category: String,
}

/// The kind of relationship between two tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolRelation {
    /// Tool A needs tool B to run first.
    DependsOn,
    /// Tool A and tool B work well together.
    SynergizesWith,
    /// Tool A and tool B should not be used together.
    ConflictsWith,
}

/// A directed edge connecting two tools with a typed, weighted relationship.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEdge {
    pub from: String,
    pub to: String,
    pub relation: ToolRelation,
    pub weight: f64,
}

/// A cached tool composition sequence with usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedComposition {
    pub sequence: Vec<String>,
    pub task_type: String,
    pub uses: u32,
    pub success_rate: f64,
    pub last_used_at: u64,
    pub is_permanent: bool,
}

/// Graph of tool nodes and relationship edges with an LRU composition cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolGraph {
    nodes: HashMap<String, ToolNode>,
    edges: Vec<ToolEdge>,
    composition_cache: Vec<CachedComposition>,
    max_cache_size: usize,
    promotion_threshold: u32,
}

impl Default for ToolGraph {
    fn default() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            composition_cache: Vec::new(),
            max_cache_size: 50,
            promotion_threshold: 10,
        }
    }
}

impl ToolGraph {
    /// Create a new empty tool graph with the given cache and promotion settings.
    pub fn new(max_cache: usize, promotion_threshold: u32) -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            composition_cache: Vec::new(),
            max_cache_size: max_cache,
            promotion_threshold,
        }
    }

    /// Register a tool node in the graph.
    pub fn add_node(&mut self, node: ToolNode) {
        self.nodes.insert(node.name.clone(), node);
    }

    /// Add a relationship edge between two tools.
    pub fn add_edge(&mut self, edge: ToolEdge) {
        self.edges.push(edge);
    }

    /// Returns tools that synergize with the given tool, sorted by weight descending.
    pub fn get_synergies(&self, tool_name: &str) -> Vec<(&str, f64)> {
        let mut results: Vec<(&str, f64)> = self
            .edges
            .iter()
            .filter_map(|edge| match edge.relation {
                ToolRelation::SynergizesWith => {
                    if edge.from == tool_name {
                        Some((edge.to.as_str(), edge.weight))
                    } else if edge.to == tool_name {
                        Some((edge.from.as_str(), edge.weight))
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Returns tools that conflict with the given tool.
    pub fn get_conflicts(&self, tool_name: &str) -> Vec<&str> {
        self.edges
            .iter()
            .filter_map(|edge| match edge.relation {
                ToolRelation::ConflictsWith => {
                    if edge.from == tool_name {
                        Some(edge.to.as_str())
                    } else if edge.to == tool_name {
                        Some(edge.from.as_str())
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect()
    }

    /// Returns tools that must run before the given tool (its dependencies).
    pub fn get_dependencies(&self, tool_name: &str) -> Vec<&str> {
        self.edges
            .iter()
            .filter_map(|edge| match edge.relation {
                ToolRelation::DependsOn => {
                    if edge.from == tool_name {
                        Some(edge.to.as_str())
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect()
    }

    /// Add or update a cached composition. Applies LRU eviction when the cache
    /// is full, evicting the lowest-use non-permanent entry first.
    pub fn cache_composition(
        &mut self,
        sequence: Vec<String>,
        task_type: &str,
        succeeded: bool,
        now: u64,
    ) {
        // Check if this exact sequence + task_type already exists.
        if let Some(existing) = self
            .composition_cache
            .iter_mut()
            .find(|c| c.sequence == sequence && c.task_type == task_type)
        {
            let total = existing.success_rate * existing.uses as f64;
            existing.uses += 1;
            let success_add = if succeeded { 1.0 } else { 0.0 };
            existing.success_rate = (total + success_add) / existing.uses as f64;
            existing.last_used_at = now;
            return;
        }

        // Evict if cache is full.
        if self.composition_cache.len() >= self.max_cache_size {
            // Find the non-permanent entry with the lowest uses.
            if let Some(evict_idx) = self
                .composition_cache
                .iter()
                .enumerate()
                .filter(|(_, c)| !c.is_permanent)
                .min_by_key(|(_, c)| c.uses)
                .map(|(i, _)| i)
            {
                self.composition_cache.remove(evict_idx);
            } else {
                // All entries are permanent; cannot evict.
                return;
            }
        }

        self.composition_cache.push(CachedComposition {
            sequence,
            task_type: task_type.to_string(),
            uses: 1,
            success_rate: if succeeded { 1.0 } else { 0.0 },
            last_used_at: now,
            is_permanent: false,
        });
    }

    /// Return the best cached composition for a task type, ranked by
    /// `success_rate * uses`.
    pub fn suggest_composition(&self, task_type: &str) -> Option<&CachedComposition> {
        self.composition_cache
            .iter()
            .filter(|c| c.task_type == task_type)
            .max_by(|a, b| {
                let score_a = a.success_rate * a.uses as f64;
                let score_b = b.success_rate * b.uses as f64;
                score_a
                    .partial_cmp(&score_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// Promote compositions with uses >= promotion_threshold to permanent.
    pub fn promote_frequent(&mut self) {
        for comp in &mut self.composition_cache {
            if comp.uses >= self.promotion_threshold {
                comp.is_permanent = true;
            }
        }
    }

    /// Number of tool nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of relationship edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Number of entries in the composition cache.
    pub fn cache_size(&self) -> usize {
        self.composition_cache.len()
    }
}

/// Build a pre-populated tool graph with common tamux tools and their
/// known relationships.
pub fn build_default_graph() -> ToolGraph {
    let mut graph = ToolGraph::default();

    // -- Nodes --

    graph.add_node(ToolNode {
        name: "bash_command".into(),
        capabilities: vec!["shell_execution".into(), "process_management".into()],
        limitations: vec!["no_interactive".into()],
        category: "execution".into(),
    });

    graph.add_node(ToolNode {
        name: "read_file".into(),
        capabilities: vec!["file_read".into(), "code_analysis".into()],
        limitations: vec!["read_only".into()],
        category: "file_operations".into(),
    });

    graph.add_node(ToolNode {
        name: "write_file".into(),
        capabilities: vec!["file_write".into(), "file_create".into()],
        limitations: vec!["overwrites_existing".into()],
        category: "file_operations".into(),
    });

    graph.add_node(ToolNode {
        name: "replace_in_file".into(),
        capabilities: vec!["file_edit".into(), "partial_update".into()],
        limitations: vec!["requires_existing_content".into()],
        category: "file_operations".into(),
    });

    graph.add_node(ToolNode {
        name: "list_files".into(),
        capabilities: vec!["directory_listing".into(), "file_discovery".into()],
        limitations: vec!["read_only".into()],
        category: "file_operations".into(),
    });

    graph.add_node(ToolNode {
        name: "search_files".into(),
        capabilities: vec!["content_search".into(), "pattern_matching".into()],
        limitations: vec!["read_only".into()],
        category: "file_operations".into(),
    });

    graph.add_node(ToolNode {
        name: "execute_managed_command".into(),
        capabilities: vec!["managed_execution".into(), "lifecycle_tracking".into()],
        limitations: vec!["no_interactive".into()],
        category: "execution".into(),
    });

    graph.add_node(ToolNode {
        name: "spawn_subagent".into(),
        capabilities: vec!["delegation".into(), "parallel_work".into()],
        limitations: vec!["resource_intensive".into()],
        category: "agent_management".into(),
    });

    graph.add_node(ToolNode {
        name: "list_subagents".into(),
        capabilities: vec!["agent_monitoring".into(), "status_check".into()],
        limitations: vec!["read_only".into()],
        category: "agent_management".into(),
    });

    // -- Edges --

    // bash_command synergizes with read_file
    graph.add_edge(ToolEdge {
        from: "bash_command".into(),
        to: "read_file".into(),
        relation: ToolRelation::SynergizesWith,
        weight: 0.8,
    });

    // bash_command synergizes with list_files
    graph.add_edge(ToolEdge {
        from: "bash_command".into(),
        to: "list_files".into(),
        relation: ToolRelation::SynergizesWith,
        weight: 0.7,
    });

    // write_file depends on read_file (read before write)
    graph.add_edge(ToolEdge {
        from: "write_file".into(),
        to: "read_file".into(),
        relation: ToolRelation::DependsOn,
        weight: 0.9,
    });

    // replace_in_file depends on read_file
    graph.add_edge(ToolEdge {
        from: "replace_in_file".into(),
        to: "read_file".into(),
        relation: ToolRelation::DependsOn,
        weight: 0.95,
    });

    // bash_command conflicts with execute_managed_command
    graph.add_edge(ToolEdge {
        from: "bash_command".into(),
        to: "execute_managed_command".into(),
        relation: ToolRelation::ConflictsWith,
        weight: 0.6,
    });

    // search_files synergizes with read_file
    graph.add_edge(ToolEdge {
        from: "search_files".into(),
        to: "read_file".into(),
        relation: ToolRelation::SynergizesWith,
        weight: 0.85,
    });

    // spawn_subagent synergizes with list_subagents
    graph.add_edge(ToolEdge {
        from: "spawn_subagent".into(),
        to: "list_subagents".into(),
        relation: ToolRelation::SynergizesWith,
        weight: 0.75,
    });

    graph
}

#[cfg(test)]
#[path = "tool_graph/tests.rs"]
mod tests;
