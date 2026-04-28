use zorai_protocol::{AGENT_ID_RAROG, AGENT_ID_SWAROG};

use crate::commands::common::{
    handle_post_setup_action, resolve_dm_target, resolve_gui_binary, resolve_sibling_binary,
    LaunchTarget,
};
use crate::setup_wizard::PostSetupAction;

#[test]
fn resolve_dm_target_prefers_rarog_routes() {
    assert_eq!(resolve_dm_target(false, true, false, false), AGENT_ID_RAROG);
    assert_eq!(resolve_dm_target(false, false, false, true), AGENT_ID_RAROG);
}

#[test]
fn resolve_dm_target_defaults_to_swarog_routes() {
    assert_eq!(
        resolve_dm_target(true, false, false, false),
        AGENT_ID_SWAROG
    );
    assert_eq!(
        resolve_dm_target(false, false, true, false),
        AGENT_ID_SWAROG
    );
    assert_eq!(
        resolve_dm_target(false, false, false, false),
        AGENT_ID_SWAROG
    );
}

#[test]
fn handle_post_setup_action_maps_launch_targets() {
    assert_eq!(
        handle_post_setup_action(PostSetupAction::LaunchTui),
        Some(LaunchTarget::Tui)
    );
    assert_eq!(
        handle_post_setup_action(PostSetupAction::LaunchElectron),
        Some(LaunchTarget::Gui)
    );
    assert_eq!(handle_post_setup_action(PostSetupAction::NotNow), None);
}

#[test]
fn resolve_sibling_binary_prefers_current_exe_directory() {
    let temp_dir = std::env::temp_dir().join(format!(
        "zorai-cli-common-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");

    let current_exe = temp_dir.join(if cfg!(windows) { "zorai.exe" } else { "zorai" });
    let daemon = temp_dir.join(if cfg!(windows) {
        "zorai-daemon.exe"
    } else {
        "zorai-daemon"
    });

    std::fs::write(&current_exe, []).expect("write current exe");
    std::fs::write(&daemon, []).expect("write daemon binary");

    let resolved = resolve_sibling_binary(Some(current_exe.as_path()), "zorai-daemon");
    assert_eq!(resolved, daemon);

    std::fs::remove_dir_all(temp_dir).expect("remove temp dir");
}

#[cfg(target_os = "linux")]
#[test]
fn resolve_gui_binary_finds_development_linux_unpacked_app_from_repo_root() {
    let temp_dir = std::env::temp_dir().join(format!(
        "zorai-cli-gui-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let bin_dir = temp_dir.join("bin");
    let gui_dir = temp_dir
        .join("frontend")
        .join("release")
        .join("linux-unpacked");
    std::fs::create_dir_all(&bin_dir).expect("create bin dir");
    std::fs::create_dir_all(&gui_dir).expect("create gui dir");

    let current_exe = bin_dir.join("zorai");
    let gui_binary = gui_dir.join("zorai");
    std::fs::write(&current_exe, []).expect("write current exe");
    std::fs::write(&gui_binary, []).expect("write gui binary");

    let resolved = resolve_gui_binary(None, Some(current_exe.as_path()), Some(temp_dir.as_path()));
    assert_eq!(resolved, gui_binary);

    std::fs::remove_dir_all(temp_dir).expect("remove temp dir");
}
