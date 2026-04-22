#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::agent::background_workers::domain_memory::build_memory_snapshot;
    use crate::agent::context::structural_memory::{
        ObservedFileNode, StructuralEdge, ThreadStructuralMemory, WorkspaceSeed,
    };
    use crate::agent::engine::AgentEngine;
    use crate::agent::semantic_env::SemanticPackageSummary;
    use crate::agent::types::AgentConfig;
    use crate::session_manager::SessionManager;

    #[test]
    fn memory_worker_builds_global_graph_from_semantic_and_structural_inputs() {
        let structural_memory = ThreadStructuralMemory {
            workspace_seed_scan_complete: true,
            language_hints: vec!["rust".to_string()],
            workspace_seeds: vec![WorkspaceSeed {
                node_id: "workspace:Cargo.toml".to_string(),
                relative_path: "Cargo.toml".to_string(),
                kind: "manifest".to_string(),
            }],
            observed_files: vec![ObservedFileNode {
                node_id: "node:file:src/auth.rs".to_string(),
                relative_path: "src/auth.rs".to_string(),
            }],
            edges: vec![StructuralEdge {
                from: "node:file:src/auth.rs".to_string(),
                to: "workspace:Cargo.toml".to_string(),
                kind: "imports_file".to_string(),
            }],
        };
        let semantic_packages = vec![SemanticPackageSummary {
            ecosystem: "cargo".to_string(),
            name: "amux-daemon".to_string(),
            manifest_path: "Cargo.toml".to_string(),
        }];

        let snapshot = build_memory_snapshot(
            Some("thread-1"),
            Some("task-1"),
            Some(&structural_memory),
            &semantic_packages,
            1_000,
        );

        assert!(snapshot
            .update_batch
            .nodes
            .iter()
            .any(|node| node.id == "node:file:src/auth.rs" && node.node_type == "file"));
        assert!(snapshot
            .update_batch
            .nodes
            .iter()
            .any(|node| node.id == "node:package:cargo:amux-daemon"));
        assert!(snapshot.update_batch.edges.iter().any(|edge| {
            edge.source_node_id == "node:file:Cargo.toml"
                && edge.target_node_id == "node:package:cargo:amux-daemon"
                && edge.relation_type == "manifest_declares_package"
        }));
        assert!(snapshot.clusters.is_empty());
    }

    #[test]
    fn memory_worker_materializes_structural_edge_endpoints_before_persisting_edges() {
        let structural_memory = ThreadStructuralMemory {
            workspace_seed_scan_complete: true,
            language_hints: vec!["rust".to_string()],
            workspace_seeds: vec![WorkspaceSeed {
                node_id: "node:file:crates/amux-cli/Cargo.toml".to_string(),
                relative_path: "crates/amux-cli/Cargo.toml".to_string(),
                kind: "cargo_manifest".to_string(),
            }],
            observed_files: Vec::new(),
            edges: vec![StructuralEdge {
                from: "node:file:crates/amux-cli/Cargo.toml".to_string(),
                to: "node:file:crates/amux-cli".to_string(),
                kind: "crate_path".to_string(),
            }],
        };

        let snapshot =
            build_memory_snapshot(Some("thread-1"), None, Some(&structural_memory), &[], 1_000);

        assert!(snapshot
            .update_batch
            .nodes
            .iter()
            .any(|node| { node.id == "node:file:crates/amux-cli/Cargo.toml" }));
        assert!(snapshot
            .update_batch
            .nodes
            .iter()
            .any(|node| { node.id == "node:file:crates/amux-cli" }));
        assert!(snapshot.update_batch.edges.iter().any(|edge| {
            edge.source_node_id == "node:file:crates/amux-cli/Cargo.toml"
                && edge.target_node_id == "node:file:crates/amux-cli"
                && edge.relation_type == "crate_path"
        }));
    }

    #[tokio::test]
    async fn memory_palace_query_returns_related_cross_thread_context() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        engine
            .history
            .upsert_memory_node(
                "node:file:src/auth.rs",
                "src/auth.rs",
                "file",
                Some("authentication entrypoint"),
                1_000,
            )
            .await
            .expect("auth node");
        engine
            .history
            .upsert_memory_node(
                "node:error:LoginError",
                "LoginError",
                "error",
                Some("login failed due to token parsing"),
                1_000,
            )
            .await
            .expect("error node");
        engine
            .history
            .upsert_memory_node(
                "node:file:src/tokens.rs",
                "src/tokens.rs",
                "file",
                Some("token parsing logic"),
                1_000,
            )
            .await
            .expect("token node");
        engine
            .history
            .upsert_memory_edge(
                "node:file:src/auth.rs",
                "node:error:LoginError",
                "file_hit_error",
                2.0,
                1_000,
            )
            .await
            .expect("auth->error");
        engine
            .history
            .upsert_memory_edge(
                "node:error:LoginError",
                "node:file:src/tokens.rs",
                "caused_by",
                2.0,
                1_000,
            )
            .await
            .expect("error->tokens");

        let context = engine
            .memory_palace_query("node:file:src/auth.rs", 2, 4)
            .await
            .expect("query should succeed");

        assert_eq!(context.center_node_id, "node:file:src/auth.rs");
        assert!(
            context.summary.contains("src/tokens.rs"),
            "expected cross-thread second hop in summary: {}",
            context.summary
        );
        assert!(context
            .subgraph_edges
            .iter()
            .any(|edge| edge.relation == "caused_by"));
    }

    #[tokio::test]
    async fn memory_palace_query_surfaces_cluster_summaries_for_pruned_low_signal_edges() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        engine
            .history
            .upsert_memory_node("node:file:src/lib.rs", "src/lib.rs", "file", None, 1_000)
            .await
            .expect("center node");
        engine
            .history
            .upsert_memory_node(
                "node:file:src/legacy.rs",
                "src/legacy.rs",
                "file",
                None,
                1_000,
            )
            .await
            .expect("member node");
        engine
            .history
            .upsert_memory_cluster(
                "cluster:node:file:src/lib.rs",
                "summarized low-signal relations fanout from node:file:src/lib.rs",
                Some("node:file:src/lib.rs"),
                &[
                    "node:file:src/lib.rs".to_string(),
                    "node:file:src/legacy.rs".to_string(),
                ],
                1_000,
            )
            .await
            .expect("cluster");

        let context = engine
            .memory_palace_query("node:file:src/lib.rs", 1, 4)
            .await
            .expect("query should succeed");

        assert!(context
            .cluster_summaries
            .iter()
            .any(|summary| summary.contains("low-signal relations")));
        assert!(context.summary.contains("low-signal relations"));
    }
}
