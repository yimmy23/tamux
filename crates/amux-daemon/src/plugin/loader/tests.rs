use super::*;

fn make_validator() -> jsonschema::Validator {
    super::super::schema::compile_schema_v1()
}

fn minimal_manifest_json() -> String {
    r#"{"name":"test","version":"1.0.0","schema_version":1}"#.to_string()
}

#[test]
fn validate_manifest_rejects_oversized() {
    let validator = make_validator();
    let big = vec![b'{'; MAX_MANIFEST_SIZE + 1];
    let err = validate_manifest(&big, &validator).unwrap_err();
    assert!(
        err.to_string().contains("100KB"),
        "error should mention 100KB limit: {}",
        err
    );
}

#[test]
fn validate_manifest_rejects_missing_name() {
    let validator = make_validator();
    let json = r#"{"version":"1.0.0","schema_version":1}"#;
    let err = validate_manifest(json.as_bytes(), &validator).unwrap_err();
    assert!(
        err.to_string().contains("name"),
        "error should mention 'name': {}",
        err
    );
}

#[test]
fn validate_manifest_rejects_invalid_name_pattern() {
    let validator = make_validator();
    let json = r#"{"name":"../escape","version":"1.0.0","schema_version":1}"#;
    let err = validate_manifest(json.as_bytes(), &validator).unwrap_err();
    assert!(
        err.to_string().contains("escape")
            || err.to_string().contains("..")
            || err.to_string().contains("pattern")
            || err.to_string().contains("validation failed"),
        "error should mention invalid name: {}",
        err
    );
}

#[test]
fn validate_manifest_accepts_minimal() {
    let validator = make_validator();
    let json = minimal_manifest_json();
    let (manifest, raw) = validate_manifest(json.as_bytes(), &validator).unwrap();
    assert_eq!(manifest.name, "test");
    assert_eq!(manifest.version, "1.0.0");
    assert_eq!(manifest.schema_version, 1);
    assert!(!raw.is_empty());
}

#[test]
fn validate_manifest_rejects_too_many_endpoints() {
    let validator = make_validator();
    let mut endpoints = serde_json::Map::new();
    for i in 0..=MAX_ENDPOINTS {
        endpoints.insert(
            format!("ep_{i}"),
            serde_json::json!({"method":"GET","path":"/test"}),
        );
    }
    let manifest = serde_json::json!({
        "name": "test",
        "version": "1.0.0",
        "schema_version": 1,
        "api": { "endpoints": endpoints }
    });
    let raw = serde_json::to_vec(&manifest).unwrap();
    let err = validate_manifest(&raw, &validator).unwrap_err();
    assert!(
        err.to_string().contains("endpoints"),
        "error should mention endpoints: {}",
        err
    );
}

#[test]
fn validate_manifest_rejects_too_many_settings() {
    let validator = make_validator();
    let mut settings = serde_json::Map::new();
    for i in 0..=MAX_SETTINGS {
        settings.insert(
            format!("setting_{i}"),
            serde_json::json!({"type":"string","label":"Test"}),
        );
    }
    let manifest = serde_json::json!({
        "name": "test",
        "version": "1.0.0",
        "schema_version": 1,
        "settings": settings
    });
    let raw = serde_json::to_vec(&manifest).unwrap();
    let err = validate_manifest(&raw, &validator).unwrap_err();
    assert!(
        err.to_string().contains("settings"),
        "error should mention settings: {}",
        err
    );
}

#[test]
fn validate_manifest_accepts_max_endpoints_and_settings() {
    let validator = make_validator();
    let mut endpoints = serde_json::Map::new();
    for i in 0..MAX_ENDPOINTS {
        endpoints.insert(
            format!("ep_{i}"),
            serde_json::json!({"method":"GET","path":"/test"}),
        );
    }
    let mut settings = serde_json::Map::new();
    for i in 0..MAX_SETTINGS {
        settings.insert(
            format!("setting_{i}"),
            serde_json::json!({"type":"string","label":"Test"}),
        );
    }
    let manifest = serde_json::json!({
        "name": "test",
        "version": "1.0.0",
        "schema_version": 1,
        "api": { "endpoints": endpoints },
        "settings": settings
    });
    let raw = serde_json::to_vec(&manifest).unwrap();
    let (manifest, _) = validate_manifest(&raw, &validator).unwrap();
    assert_eq!(
        manifest.api.as_ref().unwrap().endpoints.len(),
        MAX_ENDPOINTS
    );
    assert_eq!(manifest.settings.as_ref().unwrap().len(), MAX_SETTINGS);
}

#[test]
fn validate_plugin_name_accepts_simple() {
    assert_eq!(validate_plugin_name("my-plugin").unwrap(), "my-plugin");
    assert_eq!(validate_plugin_name("test").unwrap(), "test");
    assert_eq!(validate_plugin_name("a1").unwrap(), "a1");
    assert_eq!(
        validate_plugin_name("my.plugin.name").unwrap(),
        "my.plugin.name"
    );
}

#[test]
fn validate_plugin_name_accepts_scoped() {
    assert_eq!(
        validate_plugin_name("@scope/my-plugin").unwrap(),
        "my-plugin"
    );
}

#[test]
fn validate_plugin_name_rejects_path_traversal() {
    assert!(validate_plugin_name("../escape").is_err());
    assert!(validate_plugin_name("..").is_err());
    assert!(validate_plugin_name("foo\\bar").is_err());
}

#[test]
fn validate_plugin_name_rejects_empty() {
    assert!(validate_plugin_name("").is_err());
}

#[test]
fn scan_plugins_dir_loads_valid_manifest() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plugin_dir = tmp.path().join("my-plugin");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(plugin_dir.join("plugin.json"), minimal_manifest_json()).unwrap();

    let validator = make_validator();
    let result = scan_plugins_dir(tmp.path(), &validator);
    assert_eq!(result.loaded.len(), 1);
    assert_eq!(result.loaded[0].manifest.name, "test");
    assert_eq!(result.loaded[0].dir_name, "my-plugin");
    assert!(result.skipped.is_empty());
}

#[test]
fn scan_plugins_dir_skips_invalid_manifest() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plugin_dir = tmp.path().join("bad-plugin");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(plugin_dir.join("plugin.json"), "not json").unwrap();

    let validator = make_validator();
    let result = scan_plugins_dir(tmp.path(), &validator);
    assert!(result.loaded.is_empty());
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].0, "bad-plugin");
}

#[test]
fn scan_plugins_dir_skips_dir_without_manifest() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plugin_dir = tmp.path().join("no-manifest");
    std::fs::create_dir_all(&plugin_dir).unwrap();

    let validator = make_validator();
    let result = scan_plugins_dir(tmp.path(), &validator);
    assert!(result.loaded.is_empty());
    assert_eq!(result.skipped.len(), 1);
}

#[test]
fn scan_plugins_dir_handles_nonexistent_dir() {
    let validator = make_validator();
    let result = scan_plugins_dir(std::path::Path::new("/nonexistent/path"), &validator);
    assert!(result.loaded.is_empty());
    assert!(result.skipped.is_empty());
}

fn project_root() -> std::path::PathBuf {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .ancestors()
        .find(|path| path.join("plugins/tamux-plugin-gmail-calendar").is_dir())
        .expect("could not locate project root with plugins/ directory")
        .to_path_buf()
}

#[test]
fn gmail_manifest_validates_through_plugin_loader() {
    let root = project_root();
    let path = root.join("plugins/tamux-plugin-gmail-calendar/gmail/plugin.json");
    let raw = std::fs::read(&path).expect("gmail plugin.json should exist");
    let validator = make_validator();

    let (manifest, _) =
        validate_manifest(&raw, &validator).expect("gmail manifest should pass validation");

    assert_eq!(manifest.name, "gmail");
    assert_eq!(manifest.version, "1.1.0");
    assert_eq!(manifest.schema_version, 1);

    let auth = manifest
        .auth
        .as_ref()
        .expect("gmail should have auth section");
    assert_eq!(auth.auth_type, "oauth2");
    assert!(auth.pkce);
    assert!(auth.authorization_url.is_some());
    assert!(auth.token_url.is_some());
    assert_eq!(auth.scopes.as_ref().unwrap().len(), 2);

    let api = manifest
        .api
        .as_ref()
        .expect("gmail should have api section");
    assert!(
        api.endpoints.len() >= 3,
        "gmail should have at least 3 endpoints, got: {:?}",
        api.endpoints.keys().collect::<Vec<_>>()
    );
    assert!(api.endpoints.contains_key("list_inbox"));
    assert!(api.endpoints.contains_key("get_message"));
    assert!(api.endpoints.contains_key("search_messages"));

    let commands = manifest
        .commands
        .as_ref()
        .expect("gmail should have commands");
    assert!(
        commands.len() >= 2,
        "gmail should have at least 2 commands, got: {:?}",
        commands.keys().collect::<Vec<_>>()
    );
    assert!(commands.contains_key("inbox"));
    assert!(commands.contains_key("search"));

    let skills = manifest.skills.as_ref().expect("gmail should have skills");
    assert_eq!(skills.len(), 1);
}

#[test]
fn calendar_manifest_validates_through_plugin_loader() {
    let root = project_root();
    let path = root.join("plugins/tamux-plugin-gmail-calendar/calendar/plugin.json");
    let raw = std::fs::read(&path).expect("calendar plugin.json should exist");
    let validator = make_validator();

    let (manifest, _) =
        validate_manifest(&raw, &validator).expect("calendar manifest should pass validation");

    assert_eq!(manifest.name, "calendar");
    assert_eq!(manifest.version, "1.1.0");
    assert_eq!(manifest.schema_version, 1);

    let auth = manifest
        .auth
        .as_ref()
        .expect("calendar should have auth section");
    assert_eq!(auth.auth_type, "oauth2");
    assert!(auth.pkce);
    assert!(auth.authorization_url.is_some());
    assert!(auth.token_url.is_some());
    assert_eq!(auth.scopes.as_ref().unwrap().len(), 1);

    let api = manifest
        .api
        .as_ref()
        .expect("calendar should have api section");
    assert!(
        !api.endpoints.is_empty(),
        "calendar should have at least 1 endpoint, got: {:?}",
        api.endpoints.keys().collect::<Vec<_>>()
    );
    assert!(api.endpoints.contains_key("list_events"));

    let commands = manifest
        .commands
        .as_ref()
        .expect("calendar should have commands");
    assert!(
        !commands.is_empty(),
        "calendar should have at least 1 command, got: {:?}",
        commands.keys().collect::<Vec<_>>()
    );
    assert!(commands.contains_key("today"));

    let skills = manifest
        .skills
        .as_ref()
        .expect("calendar should have skills");
    assert_eq!(skills.len(), 1);
}
