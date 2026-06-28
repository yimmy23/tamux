use super::*;
use std::fs;
use zorai_protocol::CommandLogEntry;

fn entry(id: &str, command: &str) -> CommandLogEntry {
    CommandLogEntry {
        id: id.to_string(),
        command: command.to_string(),
        timestamp: 100,
        path: Some("/repo".to_string()),
        cwd: Some("/repo/sub".to_string()),
        workspace_id: Some("ws-1".to_string()),
        surface_id: Some("surface-1".to_string()),
        pane_id: Some("pane-1".to_string()),
        exit_code: None,
        duration_ms: None,
    }
}

// Exercises the command_log path after its migration onto the db facade:
// append (INSERT OR REPLACE), filtered query, completion UPDATE, and the
// soft-delete clear. Verifies round-trip fidelity, the workspace/pane filter
// behavior callers rely on, and that NULL exit_code/duration round-trip as
// `None`.
#[tokio::test]
async fn command_log_append_query_complete_clear_round_trip() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store.append_command_log(&entry("cmd-1", "ls -la")).await?;
    store
        .append_command_log(&entry("cmd-2", "git status"))
        .await?;

    let by_workspace = store.query_command_log(Some("ws-1"), None, None).await?;
    assert_eq!(by_workspace.len(), 2);
    // Pending commands carry NULL exit_code/duration -> None.
    assert!(by_workspace.iter().all(|e| e.exit_code.is_none()));

    // Filter must exclude non-matching workspaces.
    let other_ws = store
        .query_command_log(Some("ws-other"), None, None)
        .await?;
    assert!(other_ws.is_empty());

    store
        .complete_command_log("cmd-1", Some(0), Some(42))
        .await?;
    let completed = store
        .query_command_log(Some("ws-1"), Some("pane-1"), None)
        .await?;
    let cmd1 = completed
        .iter()
        .find(|e| e.id == "cmd-1")
        .expect("cmd-1 present");
    assert_eq!(cmd1.exit_code, Some(0));
    assert_eq!(cmd1.duration_ms, Some(42));
    assert_eq!(cmd1.command, "ls -la");

    store.clear_command_log().await?;
    let after_clear = store.query_command_log(None, None, None).await?;
    assert!(after_clear.is_empty());

    fs::remove_dir_all(root)?;
    Ok(())
}
