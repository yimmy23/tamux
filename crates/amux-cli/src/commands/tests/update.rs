use crate::cli::{Commands, ThreadAction};
use crate::commands::core::should_check_for_updates;

#[test]
fn checks_updates_for_user_facing_commands() {
    assert!(should_check_for_updates(&Commands::Status));
    assert!(should_check_for_updates(&Commands::Prompt {
        agent: None,
        weles: false,
        concierge: false,
        rarog: false,
        json: false,
    }));
    assert!(should_check_for_updates(&Commands::Setup));
    assert!(should_check_for_updates(&Commands::List));
    assert!(should_check_for_updates(&Commands::Thread {
        action: ThreadAction::List { json: false },
    }));
}

#[test]
fn skips_update_checks_for_internal_and_upgrade_commands() {
    assert!(!should_check_for_updates(&Commands::Upgrade));
    assert!(!should_check_for_updates(&Commands::AgentBridge));
    assert!(!should_check_for_updates(&Commands::DbBridge));
}
