use super::*;

pub(super) fn is_local_provider(id: &str) -> bool {
    matches!(id, "ollama" | "lmstudio")
}

pub(super) fn default_security_index(tier: &str) -> usize {
    match tier {
        "newcomer" => 0,
        "familiar" => 1,
        "power_user" | "expert" => 2,
        _ => 1,
    }
}

pub(super) fn tier_shows_step(tier: &str, step: &str) -> bool {
    match step {
        "model" | "data_dir" | "advanced_agents" => {
            matches!(tier, "familiar" | "power_user" | "expert")
        }
        _ => false,
    }
}

pub(super) fn security_level_from_index(index: usize) -> (&'static str, &'static str) {
    match index {
        0 => ("highest", "Approve risky actions"),
        1 => ("moderate", "Approve risky actions"),
        2 => ("lowest", "Approve destructive only"),
        3 => ("yolo", "Minimize interruptions"),
        _ => ("moderate", "Approve risky actions"),
    }
}

pub(super) fn post_setup_choices() -> [(&'static str, &'static str); 3] {
    [
        ("TUI", "Terminal interface"),
        ("Electron", "Desktop app"),
        ("Not now", "Finish setup without launching"),
    ]
}

pub(super) fn post_setup_action_from_index(index: usize) -> PostSetupAction {
    match index {
        0 => PostSetupAction::LaunchTui,
        1 => PostSetupAction::LaunchElectron,
        2 => PostSetupAction::NotNow,
        _ => PostSetupAction::NotNow,
    }
}
