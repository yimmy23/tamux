use std::path::{Path, PathBuf};
use std::process::Command;

use zorai_protocol::{AGENT_ID_RAROG, AGENT_ID_SWAROG};
use anyhow::Result;

use crate::setup_wizard::PostSetupAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LaunchTarget {
    Tui,
    Gui,
}

/// Find a binary by checking the same directory as the current executable first,
/// then falling back to the bare name (which uses PATH lookup).
pub(crate) fn resolve_sibling_binary(current_exe: Option<&Path>, name: &str) -> PathBuf {
    let binary_name = platform_binary_name(name);

    if let Some(current) = current_exe {
        if let Some(dir) = current.parent() {
            let sibling = dir.join(&binary_name);
            if sibling.exists() {
                return sibling;
            }
        }
    }

    PathBuf::from(binary_name)
}

pub(crate) fn find_sibling_binary(name: &str) -> PathBuf {
    let current_exe = std::env::current_exe().ok();
    resolve_sibling_binary(current_exe.as_deref(), name)
}

fn platform_binary_name(name: &str) -> String {
    #[cfg(windows)]
    {
        if Path::new(name).extension().is_some() {
            name.to_string()
        } else {
            format!("{name}.exe")
        }
    }

    #[cfg(not(windows))]
    {
        name.to_string()
    }
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
    let binary = find_sibling_binary("zorai-tui");
    match Command::new(&binary).status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("Error: could not launch zorai-tui: {e}");
            eprintln!("Install it or ensure it is on your PATH.");
            std::process::exit(1);
        }
    }
}

pub(crate) fn launch_gui() -> Result<()> {
    let gui_binary = std::env::var("ZORAI_GUI_PATH")
        .map(PathBuf::from)
        .ok()
        .filter(|path| path.exists())
        .unwrap_or_else(|| find_sibling_binary("zorai-desktop"));

    match Command::new(&gui_binary).spawn() {
        Ok(_) => {
            println!("Launched zorai desktop GUI.");
            Ok(())
        }
        Err(_) => {
            #[cfg(target_os = "macos")]
            {
                let status = Command::new("open")
                    .arg("-a")
                    .arg("zorai")
                    .status()
                    .map_err(|e| {
                        eprintln!("Error: could not launch zorai GUI: {e}");
                        eprintln!("Install the desktop app or set ZORAI_GUI_PATH.");
                        std::process::exit(1);
                    });
                if let Ok(status) = status {
                    if !status.success() {
                        eprintln!("Error: could not launch zorai GUI.");
                        eprintln!("Install the desktop app or set ZORAI_GUI_PATH.");
                        std::process::exit(1);
                    }
                }
                println!("Launched zorai desktop GUI.");
                Ok(())
            }
            #[cfg(not(target_os = "macos"))]
            {
                eprintln!("Error: could not launch zorai desktop GUI.");
                eprintln!("Install the desktop app or set ZORAI_GUI_PATH.");
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
