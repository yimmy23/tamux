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
mod tests {
    use super::*;

    fn make_node(name: &str, category: &str) -> ToolNode {
        ToolNode {
            name: name.into(),
            capabilities: vec!["test_cap".into()],
            limitations: vec!["test_lim".into()],
            category: category.into(),
        }
    }

    // --- Basic graph operations ---

    #[test]
    fn default_graph_is_empty() {
        let graph = ToolGraph::default();
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
        assert_eq!(graph.cache_size(), 0);
    }

    #[test]
    fn add_node_increases_count() {
        let mut graph = ToolGraph::default();
        graph.add_node(make_node("tool_a", "testing"));
        assert_eq!(graph.node_count(), 1);
        graph.add_node(make_node("tool_b", "testing"));
        assert_eq!(graph.node_count(), 2);
    }

    #[test]
    fn add_edge_increases_count() {
        let mut graph = ToolGraph::default();
        graph.add_edge(ToolEdge {
            from: "a".into(),
            to: "b".into(),
            relation: ToolRelation::SynergizesWith,
            weight: 0.5,
        });
        assert_eq!(graph.edge_count(), 1);
        graph.add_edge(ToolEdge {
            from: "b".into(),
            to: "c".into(),
            relation: ToolRelation::DependsOn,
            weight: 0.9,
        });
        assert_eq!(graph.edge_count(), 2);
    }

    // --- Relationship queries ---

    #[test]
    fn synergies_returns_correct_tools_sorted_by_weight() {
        let mut graph = ToolGraph::default();
        graph.add_edge(ToolEdge {
            from: "bash".into(),
            to: "read".into(),
            relation: ToolRelation::SynergizesWith,
            weight: 0.8,
        });
        graph.add_edge(ToolEdge {
            from: "bash".into(),
            to: "list".into(),
            relation: ToolRelation::SynergizesWith,
            weight: 0.6,
        });
        graph.add_edge(ToolEdge {
            from: "bash".into(),
            to: "other".into(),
            relation: ToolRelation::DependsOn,
            weight: 0.9,
        });

        let synergies = graph.get_synergies("bash");
        assert_eq!(synergies.len(), 2);
        assert_eq!(synergies[0], ("read", 0.8));
        assert_eq!(synergies[1], ("list", 0.6));
    }

    #[test]
    fn conflicts_detected() {
        let mut graph = ToolGraph::default();
        graph.add_edge(ToolEdge {
            from: "bash".into(),
            to: "managed".into(),
            relation: ToolRelation::ConflictsWith,
            weight: 0.7,
        });

        let conflicts = graph.get_conflicts("bash");
        assert_eq!(conflicts, vec!["managed"]);

        // Bidirectional: looking from the other side should also find the conflict.
        let conflicts_rev = graph.get_conflicts("managed");
        assert_eq!(conflicts_rev, vec!["bash"]);
    }

    #[test]
    fn dependencies_found() {
        let mut graph = ToolGraph::default();
        graph.add_edge(ToolEdge {
            from: "write".into(),
            to: "read".into(),
            relation: ToolRelation::DependsOn,
            weight: 0.9,
        });

        let deps = graph.get_dependencies("write");
        assert_eq!(deps, vec!["read"]);

        // "read" does not depend on anything.
        let deps_rev = graph.get_dependencies("read");
        assert!(deps_rev.is_empty());
    }

    // --- Composition cache ---

    #[test]
    fn cache_composition_stores_entry() {
        let mut graph = ToolGraph::default();
        graph.cache_composition(vec!["read".into(), "write".into()], "edit_file", true, 100);
        assert_eq!(graph.cache_size(), 1);
    }

    #[test]
    fn cache_eviction_when_full_evicts_non_permanent_first() {
        let mut graph = ToolGraph::new(3, 10);

        // Fill the cache.
        graph.cache_composition(vec!["a".into()], "type_a", true, 1);
        graph.cache_composition(vec!["b".into()], "type_b", true, 2);
        graph.cache_composition(vec!["c".into()], "type_c", true, 3);
        assert_eq!(graph.cache_size(), 3);

        // Bump uses on "b" a lot and promote it.
        for i in 0..10 {
            graph.cache_composition(vec!["b".into()], "type_b", true, 10 + i);
        }
        graph.promote_frequent();

        // Now add a 4th — should evict lowest-use non-permanent entry ("a" or "c").
        graph.cache_composition(vec!["d".into()], "type_d", true, 100);
        assert_eq!(graph.cache_size(), 3);

        // "b" should still be there (permanent).
        let suggestion = graph.suggest_composition("type_b");
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().is_permanent);
    }

    #[test]
    fn suggest_composition_picks_best_for_task_type() {
        let mut graph = ToolGraph::default();

        // Entry with 2 uses, 100% success => score = 2.0
        graph.cache_composition(vec!["read".into(), "write".into()], "edit_file", true, 1);
        graph.cache_composition(vec!["read".into(), "write".into()], "edit_file", true, 2);

        // Entry with 1 use, 100% success => score = 1.0
        graph.cache_composition(vec!["search".into(), "read".into()], "edit_file", true, 3);

        let best = graph.suggest_composition("edit_file").unwrap();
        assert_eq!(best.sequence, vec!["read", "write"]);
        assert_eq!(best.uses, 2);
    }

    #[test]
    fn promotion_marks_as_permanent() {
        let mut graph = ToolGraph::new(50, 3);

        graph.cache_composition(vec!["a".into()], "t", true, 1);
        graph.cache_composition(vec!["a".into()], "t", true, 2);
        graph.cache_composition(vec!["a".into()], "t", true, 3);

        assert!(!graph.composition_cache[0].is_permanent);
        graph.promote_frequent();
        assert!(graph.composition_cache[0].is_permanent);
    }

    #[test]
    fn suggest_returns_none_for_unknown_task_type() {
        let mut graph = ToolGraph::default();
        graph.cache_composition(vec!["a".into()], "known", true, 1);
        assert!(graph.suggest_composition("unknown").is_none());
    }

    // --- Default graph ---

    #[test]
    fn default_graph_has_expected_nodes() {
        let graph = build_default_graph();
        assert!(graph.node_count() >= 9);
        assert!(graph.nodes.contains_key("bash_command"));
        assert!(graph.nodes.contains_key("read_file"));
        assert!(graph.nodes.contains_key("write_file"));
        assert!(graph.nodes.contains_key("replace_in_file"));
        assert!(graph.nodes.contains_key("search_files"));
        assert!(graph.nodes.contains_key("spawn_subagent"));
        assert!(graph.nodes.contains_key("list_subagents"));
    }

    #[test]
    fn default_graph_has_expected_relationships() {
        let graph = build_default_graph();
        assert!(graph.edge_count() >= 7);

        // bash_command synergizes with read_file
        let bash_synergies = graph.get_synergies("bash_command");
        let synergy_names: Vec<&str> = bash_synergies.iter().map(|(n, _)| *n).collect();
        assert!(synergy_names.contains(&"read_file"));
        assert!(synergy_names.contains(&"list_files"));

        // write_file depends on read_file
        let write_deps = graph.get_dependencies("write_file");
        assert!(write_deps.contains(&"read_file"));

        // replace_in_file depends on read_file
        let replace_deps = graph.get_dependencies("replace_in_file");
        assert!(replace_deps.contains(&"read_file"));

        // bash_command conflicts with execute_managed_command
        let bash_conflicts = graph.get_conflicts("bash_command");
        assert!(bash_conflicts.contains(&"execute_managed_command"));

        // search_files synergizes with read_file
        let search_synergies = graph.get_synergies("search_files");
        let search_syn_names: Vec<&str> = search_synergies.iter().map(|(n, _)| *n).collect();
        assert!(search_syn_names.contains(&"read_file"));

        // spawn_subagent synergizes with list_subagents
        let spawn_synergies = graph.get_synergies("spawn_subagent");
        let spawn_syn_names: Vec<&str> = spawn_synergies.iter().map(|(n, _)| *n).collect();
        assert!(spawn_syn_names.contains(&"list_subagents"));
    }

    #[test]
    fn cache_updates_success_rate_on_repeated_use() {
        let mut graph = ToolGraph::default();

        graph.cache_composition(vec!["a".into()], "t", true, 1);
        graph.cache_composition(vec!["a".into()], "t", false, 2);

        let comp = &graph.composition_cache[0];
        assert_eq!(comp.uses, 2);
        // 1 success out of 2 uses = 0.5
        assert!((comp.success_rate - 0.5).abs() < f64::EPSILON);
    }
}
