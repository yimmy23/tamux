#[cfg(test)]
use super::*;

#[test]
fn detect_nested_plugins_multi() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path();

    // Create nested structure: root/gmail/plugin.json and root/calendar/plugin.json
    let gmail_dir = root.join("gmail");
    std::fs::create_dir_all(&gmail_dir).unwrap();
    std::fs::write(
        gmail_dir.join("plugin.json"),
        r#"{"name": "gmail", "version": "1.0.0", "schema_version": 1}"#,
    )
    .unwrap();

    let calendar_dir = root.join("calendar");
    std::fs::create_dir_all(&calendar_dir).unwrap();
    std::fs::write(
        calendar_dir.join("plugin.json"),
        r#"{"name": "calendar", "version": "1.0.0", "schema_version": 1}"#,
    )
    .unwrap();

    // Also create a non-plugin subdir (should be ignored)
    let docs_dir = root.join("docs");
    std::fs::create_dir_all(&docs_dir).unwrap();
    std::fs::write(docs_dir.join("README.md"), "docs").unwrap();

    let results = detect_nested_plugins(root).unwrap();
    assert_eq!(results.len(), 2);
    let names: Vec<&str> = results.iter().map(|(_path, name)| name.as_str()).collect();
    assert!(names.contains(&"gmail"));
    assert!(names.contains(&"calendar"));
}

#[test]
fn detect_nested_plugins_none_when_root_has_manifest() {
    // If root has plugin.json, detect_nested_plugins should return empty
    // (caller handles root plugin.json separately)
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path();

    std::fs::write(
        root.join("plugin.json"),
        r#"{"name": "test-plugin", "version": "1.0.0", "schema_version": 1}"#,
    )
    .unwrap();

    // Even with nested dirs, if called on a dir that has root plugin.json,
    // caller should not call detect_nested_plugins. But the function itself
    // just scans subdirs -- no root plugin.json in subdirs means empty.
    // Actually, detect_nested_plugins scans immediate children only.
    let results = detect_nested_plugins(root).unwrap();
    assert!(results.is_empty());
}

#[test]
fn detect_nested_plugins_ignores_non_plugin_dirs() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path();

    // Only non-plugin subdirs
    let docs_dir = root.join("docs");
    std::fs::create_dir_all(&docs_dir).unwrap();
    std::fs::write(docs_dir.join("README.md"), "docs").unwrap();

    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(src_dir.join("index.js"), "// code").unwrap();

    let results = detect_nested_plugins(root).unwrap();
    assert!(results.is_empty());
}

#[test]
fn detect_nested_plugins_reads_name_from_manifest() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path();

    let subdir = root.join("my-plugin-dir");
    std::fs::create_dir_all(&subdir).unwrap();
    std::fs::write(
        subdir.join("plugin.json"),
        r#"{"name": "custom-name", "version": "2.0.0", "schema_version": 1}"#,
    )
    .unwrap();

    let results = detect_nested_plugins(root).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1, "custom-name");
}

#[test]
fn detect_npm_package() {
    assert_eq!(
        detect_source_type("zorai-plugin-gmail"),
        PluginSource::Npm("zorai-plugin-gmail".to_string())
    );
    assert_eq!(
        detect_source_type("@zorai/plugin-gmail"),
        PluginSource::Npm("@zorai/plugin-gmail".to_string())
    );
}

#[test]
fn detect_github_https() {
    match detect_source_type("https://github.com/user/zorai-plugin-test") {
        PluginSource::GitHub { owner, repo, .. } => {
            assert_eq!(owner, "user");
            assert_eq!(repo, "zorai-plugin-test");
        }
        other => panic!("expected GitHub, got {:?}", other),
    }
}

#[test]
fn detect_github_ssh() {
    match detect_source_type("git@github.com:user/plugin.git") {
        PluginSource::GitHub { owner, repo, .. } => {
            assert_eq!(owner, "user");
            assert_eq!(repo, "plugin");
        }
        other => panic!("expected GitHub, got {:?}", other),
    }
}

#[test]
fn detect_github_shorthand() {
    match detect_source_type("github:user/my-plugin") {
        PluginSource::GitHub { owner, repo, .. } => {
            assert_eq!(owner, "user");
            assert_eq!(repo, "my-plugin");
        }
        other => panic!("expected GitHub, got {:?}", other),
    }
}

#[test]
fn parse_github_urls() {
    assert_eq!(
        parse_github_url("https://github.com/foo/bar"),
        Some(("foo".into(), "bar".into()))
    );
    assert_eq!(
        parse_github_url("https://github.com/foo/bar.git"),
        Some(("foo".into(), "bar".into()))
    );
    assert_eq!(
        parse_github_url("git@github.com:foo/bar.git"),
        Some(("foo".into(), "bar".into()))
    );
    assert_eq!(
        parse_github_url("github:foo/bar"),
        Some(("foo".into(), "bar".into()))
    );
}
