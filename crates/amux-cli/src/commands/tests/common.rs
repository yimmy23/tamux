use amux_protocol::{AGENT_ID_RAROG, AGENT_ID_SWAROG};

use crate::commands::common::{handle_post_setup_action, resolve_dm_target, LaunchTarget};
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
