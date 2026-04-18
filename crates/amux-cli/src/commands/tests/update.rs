use crate::cli::{Commands, GoalAction, ThreadAction};
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
    assert!(should_check_for_updates(&Commands::Operation {
        id: "op-1".to_string(),
        json: false,
    }));
    assert!(should_check_for_updates(&Commands::Setup));
    assert!(should_check_for_updates(&Commands::List));
    assert!(should_check_for_updates(&Commands::Thread {
        action: ThreadAction::List {
            page: 1,
            limit: 20,
            json: false,
        },
    }));
    assert!(should_check_for_updates(&Commands::Goal {
        action: GoalAction::List {
            page: 1,
            limit: 20,
            json: false,
        },
    }));
}

#[test]
fn skips_update_checks_for_internal_and_upgrade_commands() {
    assert!(!should_check_for_updates(&Commands::Upgrade));
    assert!(!should_check_for_updates(&Commands::Stop));
    assert!(!should_check_for_updates(&Commands::Restart));
    assert!(!should_check_for_updates(&Commands::AgentBridge));
    assert!(!should_check_for_updates(&Commands::DbBridge));
}
