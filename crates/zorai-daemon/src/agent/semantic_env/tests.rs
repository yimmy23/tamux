use super::scan::{
    parse_cargo_manifest, parse_compose_services, parse_kubernetes_resources,
    parse_package_manifest, parse_script_imports, parse_terraform_resources,
    scan_workspace_semantics,
};
use super::*;
use uuid::Uuid;

fn make_temp_dir() -> Result<PathBuf> {
    let root = std::env::temp_dir().join(format!("zorai-semantic-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&root)?;
    Ok(root)
}

#[test]
fn parse_cargo_manifest_extracts_name_and_dependencies() -> Result<()> {
    let root = make_temp_dir()?;
    let manifest = root.join("Cargo.toml");
    fs::write(
        &manifest,
        r#"[package]
name = "daemon-core"

[dependencies]
serde = "1"
tokio = { version = "1" }
"#,
    )?;

    let package = parse_cargo_manifest(&manifest)?.expect("cargo package should parse");
    assert_eq!(package.name, "daemon-core");
    assert_eq!(
        package.dependencies,
        vec!["serde".to_string(), "tokio".to_string()]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn parse_package_manifest_extracts_dependencies() -> Result<()> {
    let root = make_temp_dir()?;
    let manifest = root.join("package.json");
    fs::write(
        &manifest,
        r#"{"name":"frontend","dependencies":{"react":"18"},"devDependencies":{"vite":"5"}}"#,
    )?;

    let package = parse_package_manifest(&manifest)?.expect("npm package should parse");
    assert_eq!(package.name, "frontend");
    assert_eq!(
        package.dependencies,
        vec!["react".to_string(), "vite".to_string()]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn parse_compose_services_extracts_services_and_dependencies() -> Result<()> {
    let root = make_temp_dir()?;
    let compose = root.join("docker-compose.yml");
    fs::write(
        &compose,
        r#"
services:
  api:
    depends_on:
      - db
  worker:
    depends_on:
      redis:
        condition: service_started
  db: {}
"#,
    )?;

    let services = parse_compose_services(&compose)?;
    assert_eq!(services.len(), 3);
    let api = services
        .iter()
        .find(|service| service.name == "api")
        .expect("api service should parse");
    let worker = services
        .iter()
        .find(|service| service.name == "worker")
        .expect("worker service should parse");
    assert_eq!(api.dependencies, vec!["db".to_string()]);
    assert_eq!(worker.dependencies, vec!["redis".to_string()]);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn parse_terraform_resources_extracts_resources_and_dependencies() -> Result<()> {
    let root = make_temp_dir()?;
    let manifest = root.join("main.tf");
    fs::write(
        &manifest,
        r#"
resource "aws_vpc" "core" {}

resource "aws_subnet" "app" {
    depends_on = [aws_vpc.core]
}
"#,
    )?;

    let resources = parse_terraform_resources(&manifest)?;
    assert_eq!(resources.len(), 2);
    let subnet = resources
        .iter()
        .find(|resource| resource.name == "app")
        .expect("subnet resource should parse");
    assert_eq!(subnet.system, "terraform");
    assert_eq!(subnet.kind, "aws_subnet");
    assert_eq!(subnet.dependencies, vec!["aws_vpc.core".to_string()]);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn parse_kubernetes_resources_extracts_kinds_and_service_dependencies() -> Result<()> {
    let root = make_temp_dir()?;
    let manifest = root.join("k8s.yaml");
    fs::write(
        &manifest,
        r#"
apiVersion: apps/v1
kind: Deployment
metadata:
    name: api
    namespace: default
spec:
    template:
        spec:
            serviceAccountName: api-sa
---
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
    name: public
spec:
    defaultBackend:
        service:
            name: api
            port:
                number: 80
"#,
    )?;

    let resources = parse_kubernetes_resources(&manifest)?;
    assert_eq!(resources.len(), 2);
    let ingress = resources
        .iter()
        .find(|resource| resource.kind == "Ingress")
        .expect("ingress should parse");
    assert_eq!(ingress.system, "kubernetes");
    assert_eq!(ingress.dependencies, vec!["service:api".to_string()]);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn parse_script_imports_extracts_modules() {
    let imports = parse_script_imports(
        r#"
import React from "react";
import { api } from "./lib/api";
export * from "../shared/types";
"#,
    );

    assert!(imports.iter().any(|item| item == "react"));
    assert!(imports.iter().any(|item| item == "./lib/api"));
    assert!(imports.iter().any(|item| item == "../shared/types"));
}

#[test]
fn scan_workspace_semantics_skips_hidden_worktrees_with_invalid_yaml() -> Result<()> {
    let root = make_temp_dir()?;
    let manifest = root.join("Cargo.toml");
    fs::write(
        &manifest,
        r#"[package]
name = "zorai-daemon"
"#,
    )?;

    let hidden_skill = root.join(
        ".claude/worktrees/agent-test/plugins/zorai-plugin-gmail-calendar/gmail/skills/gmail-inbox.yaml",
    );
    fs::create_dir_all(
        hidden_skill
            .parent()
            .expect("hidden skill fixture should have a parent directory"),
    )?;
    fs::write(
        &hidden_skill,
        r#"name: gmail-inbox
description: Test skill
---
# Gmail Inbox

Call `plugin_api_call`:

```json
{
  "tool": "plugin_api_call"
}
```
"#,
    )?;

    let graph = scan_workspace_semantics(&root)?;
    assert_eq!(graph.packages.len(), 1);
    assert_eq!(graph.packages[0].name, "zorai-daemon");
    assert!(graph.infra_resources.is_empty());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn render_service_dependents_lists_reverse_service_edges() {
    let graph = SemanticGraph {
        packages: Vec::new(),
        services: vec![
            SemanticService {
                name: "api".to_string(),
                compose_path: "/tmp/docker-compose.yml".to_string(),
                dependencies: vec!["db".to_string()],
            },
            SemanticService {
                name: "db".to_string(),
                compose_path: "/tmp/docker-compose.yml".to_string(),
                dependencies: vec![],
            },
        ],
        import_files: Vec::new(),
        infra_resources: Vec::new(),
    };

    let rendered = render_service_dependents(Path::new("/tmp"), &graph, Some("db")).unwrap();
    assert!(rendered.contains("api"));
}

#[test]
fn render_infra_dependents_lists_resources_pointing_at_target_service() {
    let graph = SemanticGraph {
        packages: Vec::new(),
        services: vec![SemanticService {
            name: "api".to_string(),
            compose_path: "/tmp/docker-compose.yml".to_string(),
            dependencies: vec![],
        }],
        infra_resources: vec![
            SemanticInfraResource {
                system: "kubernetes",
                kind: "Ingress".to_string(),
                name: "public".to_string(),
                source_path: "/tmp/k8s.yaml".to_string(),
                namespace: Some("default".to_string()),
                dependencies: vec!["service:api".to_string()],
            },
            SemanticInfraResource {
                system: "terraform",
                kind: "aws_lb_target_group".to_string(),
                name: "tg".to_string(),
                source_path: "/tmp/main.tf".to_string(),
                namespace: None,
                dependencies: vec!["service:worker".to_string()],
            },
        ],
        import_files: Vec::new(),
    };

    let rendered = render_infra_dependents(Path::new("/tmp"), &graph, Some("api")).unwrap();
    assert!(rendered.contains("Ingress"));
    assert!(rendered.contains("public"));
    assert!(rendered.contains("/tmp/k8s.yaml"));
    assert!(!rendered.contains("aws_lb_target_group"));
}

#[test]
fn render_infra_dependents_reports_empty_result_cleanly() {
    let graph = SemanticGraph {
        packages: Vec::new(),
        services: vec![SemanticService {
            name: "db".to_string(),
            compose_path: "/tmp/docker-compose.yml".to_string(),
            dependencies: vec![],
        }],
        infra_resources: Vec::new(),
        import_files: Vec::new(),
    };

    let rendered = render_infra_dependents(Path::new("/tmp"), &graph, Some("db")).unwrap();
    assert!(rendered.contains("No infra resources under /tmp depend on service db."));
}

#[test]
fn render_imported_by_lists_matching_files() {
    let graph = SemanticGraph {
        packages: Vec::new(),
        services: Vec::new(),
        infra_resources: Vec::new(),
        import_files: vec![
            SemanticImportFile {
                language: "typescript",
                source_path: "/tmp/src/main.ts".to_string(),
                imports: vec!["./lib/api".to_string(), "react".to_string()],
            },
            SemanticImportFile {
                language: "rust",
                source_path: "/tmp/src/lib.rs".to_string(),
                imports: vec!["crate::db".to_string()],
            },
        ],
    };

    let rendered = render_imported_by(Path::new("/tmp"), &graph, Some("api"), 10).unwrap();
    assert!(rendered.contains("/tmp/src/main.ts"));
}

#[test]
fn render_imports_rejects_ambiguous_file_matches() {
    let graph = SemanticGraph {
        packages: Vec::new(),
        services: Vec::new(),
        infra_resources: Vec::new(),
        import_files: vec![
            SemanticImportFile {
                language: "typescript",
                source_path: "/tmp/src/lib/api.ts".to_string(),
                imports: vec!["react".to_string()],
            },
            SemanticImportFile {
                language: "typescript",
                source_path: "/tmp/src/lib/auth/api.ts".to_string(),
                imports: vec!["zod".to_string()],
            },
        ],
    };

    let error = render_imports(Path::new("/tmp"), &graph, Some("api"), 10)
        .expect_err("ambiguous import file target should require a more specific query");
    assert!(error
        .to_string()
        .contains("multiple import files matched `api`; be more specific"));
}

#[test]
fn render_dependents_lists_local_reverse_edges() {
    let graph = SemanticGraph {
        packages: vec![
            SemanticPackage {
                ecosystem: "cargo",
                name: "zorai-daemon".to_string(),
                manifest_path: "/tmp/daemon/Cargo.toml".to_string(),
                dependencies: vec!["zorai-protocol".to_string()],
            },
            SemanticPackage {
                ecosystem: "cargo",
                name: "zorai-protocol".to_string(),
                manifest_path: "/tmp/protocol/Cargo.toml".to_string(),
                dependencies: vec![],
            },
        ],
        services: Vec::new(),
        infra_resources: Vec::new(),
        import_files: Vec::new(),
    };

    let rendered = render_dependents(Path::new("/tmp"), &graph, Some("zorai-protocol")).unwrap();
    assert!(rendered.contains("zorai-daemon"));
}

#[test]
fn render_infra_lists_resources() {
    let graph = SemanticGraph {
        packages: Vec::new(),
        services: Vec::new(),
        infra_resources: vec![
            SemanticInfraResource {
                system: "terraform",
                kind: "aws_vpc".to_string(),
                name: "core".to_string(),
                source_path: "/tmp/main.tf".to_string(),
                namespace: None,
                dependencies: vec![],
            },
            SemanticInfraResource {
                system: "kubernetes",
                kind: "Ingress".to_string(),
                name: "public".to_string(),
                source_path: "/tmp/k8s.yaml".to_string(),
                namespace: Some("default".to_string()),
                dependencies: vec!["service:api".to_string()],
            },
        ],
        import_files: Vec::new(),
    };

    let rendered = render_infra(Path::new("/tmp"), &graph, 10);
    assert!(rendered.contains("terraform"));
    assert!(rendered.contains("Ingress"));
    assert!(rendered.contains("service:api"));
}

#[test]
fn convention_entry_matches_fact_keys_and_content() {
    let entry = crate::history::MemoryProvenanceReportEntry {
        id: "1".to_string(),
        target: "MEMORY.md".to_string(),
        mode: "append".to_string(),
        source_kind: "goal_reflection".to_string(),
        content: "- Error types go in src/error.rs".to_string(),
        fact_keys: vec!["error".to_string(), "src/error.rs".to_string()],
        thread_id: None,
        task_id: None,
        goal_run_id: None,
        created_at: 0,
        age_days: 0.0,
        confidence: 1.0,
        status: "active".to_string(),
        entry_hash: None,
        signature: None,
        signature_scheme: None,
        hash_valid: false,
        signature_valid: false,
        relationships: Vec::new(),
    };

    assert!(convention_entry_matches(
        &entry,
        &tokenize_convention_query("error")
    ));
    assert!(convention_entry_matches(
        &entry,
        &tokenize_convention_query("src/error.rs")
    ));
    assert!(!convention_entry_matches(
        &entry,
        &tokenize_convention_query("terraform")
    ));
}

#[test]
fn collect_matching_skills_filters_by_target() -> Result<()> {
    let root = make_temp_dir()?;
    fs::create_dir_all(root.join("generated"))?;
    fs::write(
        root.join("generated/error-handling.md"),
        "# Error handling\n",
    )?;
    fs::write(root.join("generated/deploy.md"), "# Deploy\n")?;

    let matches = collect_matching_skills(&root, Some("error"), 5);
    assert_eq!(matches, vec!["generated/error-handling.md".to_string()]);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn render_temporal_summarizes_recent_workspace_history() -> Result<()> {
    let root = make_temp_dir()?;
    let store = HistoryStore::new_test_store(&root).await?;
    store
        .append_command_log(&zorai_protocol::CommandLogEntry {
            id: "cmd-1".to_string(),
            command: "deploy staging".to_string(),
            timestamp: 123,
            path: None,
            cwd: Some(root.display().to_string()),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            exit_code: Some(1),
            duration_ms: Some(50),
        })
        .await?;

    let rendered = render_temporal(&root, &store, Some("deploy"), 5).await?;
    assert!(rendered.contains("deploy staging"));
    assert!(rendered.contains("1 failure"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn render_temporal_excludes_sibling_paths() -> Result<()> {
    let root = make_temp_dir()?;
    let sibling = root.with_file_name(format!(
        "{}-other",
        root.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("workspace")
    ));
    fs::create_dir_all(&sibling)?;
    let store = HistoryStore::new_test_store(&root).await?;
    store
        .append_command_log(&zorai_protocol::CommandLogEntry {
            id: "cmd-in".to_string(),
            command: "cargo test".to_string(),
            timestamp: 1,
            path: None,
            cwd: Some(root.display().to_string()),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            exit_code: Some(0),
            duration_ms: Some(10),
        })
        .await?;
    store
        .append_command_log(&zorai_protocol::CommandLogEntry {
            id: "cmd-out".to_string(),
            command: "cargo build".to_string(),
            timestamp: 2,
            path: None,
            cwd: Some(sibling.display().to_string()),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            exit_code: Some(0),
            duration_ms: Some(10),
        })
        .await?;

    let rendered = render_temporal(&root, &store, None, 10).await?;
    assert!(rendered.contains("cargo test"));
    assert!(!rendered.contains("cargo build"));

    fs::remove_dir_all(root)?;
    fs::remove_dir_all(sibling)?;
    Ok(())
}
