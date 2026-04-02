use std::path::PathBuf;
use std::process::Command;

use amux_protocol::{AGENT_ID_RAROG, AGENT_ID_SWAROG};
use anyhow::Result;

use crate::setup_wizard::PostSetupAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LaunchTarget {
    Tui,
    Gui,
}

/// Find a binary by checking the same directory as the current executable first,
/// then falling back to the bare name (which uses PATH lookup).
pub(crate) fn find_sibling_binary(name: &str) -> PathBuf {
    if let Ok(current) = std::env::current_exe() {
        if let Some(dir) = current.parent() {
            let sibling = dir.join(name);
            if sibling.exists() {
                return sibling;
            }
        }
    }
    PathBuf::from(name)
}

pub(crate) fn resolve_dm_target(
    svarog: bool,
    rarog: bool,
    main_target: bool,
    concierge: bool,
) -> &'static str {
    if rarog || concierge {
        AGENT_ID_RAROG
    } else if svarog || main_target {
        AGENT_ID_SWAROG
    } else {
        AGENT_ID_SWAROG
    }
}

pub(crate) fn launch_tui() -> ! {
    let binary = find_sibling_binary("tamux-tui");
    match Command::new(&binary).status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("Error: could not launch tamux-tui: {e}");
            eprintln!("Install it or ensure it is on your PATH.");
            std::process::exit(1);
        }
    }
}

pub(crate) fn launch_gui() -> Result<()> {
    let gui_binary = std::env::var("TAMUX_GUI_PATH")
        .map(PathBuf::from)
        .ok()
        .filter(|path| path.exists())
        .unwrap_or_else(|| find_sibling_binary("tamux-desktop"));

    match Command::new(&gui_binary).spawn() {
        Ok(_) => {
            println!("Launched tamux desktop GUI.");
            Ok(())
        }
        Err(_) => {
            #[cfg(target_os = "macos")]
            {
                let status = Command::new("open")
                    .arg("-a")
                    .arg("tamux")
                    .status()
                    .map_err(|e| {
                        eprintln!("Error: could not launch tamux GUI: {e}");
                        eprintln!("Install the desktop app or set TAMUX_GUI_PATH.");
                        std::process::exit(1);
                    });
                if let Ok(status) = status {
                    if !status.success() {
                        eprintln!("Error: could not launch tamux GUI.");
                        eprintln!("Install the desktop app or set TAMUX_GUI_PATH.");
                        std::process::exit(1);
                    }
                }
                println!("Launched tamux desktop GUI.");
                Ok(())
            }
            #[cfg(not(target_os = "macos"))]
            {
                eprintln!("Error: could not launch tamux desktop GUI.");
                eprintln!("Install the desktop app or set TAMUX_GUI_PATH.");
                std::process::exit(1);
            }
        }
    }
}

pub(crate) fn handle_post_setup_action(action: PostSetupAction) -> Option<LaunchTarget> {
    match action {
        PostSetupAction::LaunchTui => Some(LaunchTarget::Tui),
        PostSetupAction::LaunchElectron => Some(LaunchTarget::Gui),
        PostSetupAction::NotNow => None,
    }
}
