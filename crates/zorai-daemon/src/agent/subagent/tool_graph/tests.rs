use super::*;

fn make_node(name: &str, category: &str) -> ToolNode {
    ToolNode {
        name: name.into(),
        capabilities: vec!["test_cap".into()],
        limitations: vec!["test_lim".into()],
        category: category.into(),
    }
}

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

    assert_eq!(graph.get_conflicts("bash"), vec!["managed"]);
    assert_eq!(graph.get_conflicts("managed"), vec!["bash"]);
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

    assert_eq!(graph.get_dependencies("write"), vec!["read"]);
    assert!(graph.get_dependencies("read").is_empty());
}

#[test]
fn cache_composition_stores_entry() {
    let mut graph = ToolGraph::default();
    graph.cache_composition(vec!["read".into(), "write".into()], "edit_file", true, 100);
    assert_eq!(graph.cache_size(), 1);
}

#[test]
fn cache_eviction_when_full_evicts_non_permanent_first() {
    let mut graph = ToolGraph::new(3, 10);

    graph.cache_composition(vec!["a".into()], "type_a", true, 1);
    graph.cache_composition(vec!["b".into()], "type_b", true, 2);
    graph.cache_composition(vec!["c".into()], "type_c", true, 3);
    assert_eq!(graph.cache_size(), 3);

    for i in 0..10 {
        graph.cache_composition(vec!["b".into()], "type_b", true, 10 + i);
    }
    graph.promote_frequent();

    graph.cache_composition(vec!["d".into()], "type_d", true, 100);
    assert_eq!(graph.cache_size(), 3);

    let suggestion = graph.suggest_composition("type_b");
    assert!(suggestion.is_some());
    assert!(suggestion.unwrap().is_permanent);
}

#[test]
fn suggest_composition_picks_best_for_task_type() {
    let mut graph = ToolGraph::default();

    graph.cache_composition(vec!["read".into(), "write".into()], "edit_file", true, 1);
    graph.cache_composition(vec!["read".into(), "write".into()], "edit_file", true, 2);
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

    let bash_synergies = graph.get_synergies("bash_command");
    let synergy_names: Vec<&str> = bash_synergies.iter().map(|(name, _)| *name).collect();
    assert!(synergy_names.contains(&"read_file"));
    assert!(synergy_names.contains(&"list_files"));

    assert!(graph.get_dependencies("write_file").contains(&"read_file"));
    assert!(graph
        .get_dependencies("replace_in_file")
        .contains(&"read_file"));
    assert!(graph
        .get_conflicts("bash_command")
        .contains(&"execute_managed_command"));

    let search_synergies = graph.get_synergies("search_files");
    let search_names: Vec<&str> = search_synergies.iter().map(|(name, _)| *name).collect();
    assert!(search_names.contains(&"read_file"));

    let spawn_synergies = graph.get_synergies("spawn_subagent");
    let spawn_names: Vec<&str> = spawn_synergies.iter().map(|(name, _)| *name).collect();
    assert!(spawn_names.contains(&"list_subagents"));
}

#[test]
fn cache_updates_success_rate_on_repeated_use() {
    let mut graph = ToolGraph::default();

    graph.cache_composition(vec!["a".into()], "t", true, 1);
    graph.cache_composition(vec!["a".into()], "t", false, 2);

    let composition = &graph.composition_cache[0];
    assert_eq!(composition.uses, 2);
    assert!((composition.success_rate - 0.5).abs() < f64::EPSILON);
}
