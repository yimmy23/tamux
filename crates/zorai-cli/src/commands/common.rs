use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use zorai_protocol::{AGENT_ID_RAROG, AGENT_ID_SWAROG};

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

pub(crate) fn resolve_gui_binary(
    explicit_gui_path: Option<&str>,
    current_exe: Option<&Path>,
    cwd: Option<&Path>,
) -> PathBuf {
    if let Some(path) = explicit_gui_path
        .map(PathBuf::from)
        .filter(|path| path.exists())
    {
        return path;
    }

    if let Some(path) = resolve_existing_sibling_binary(current_exe, "zorai-desktop") {
        return path;
    }

    if let Some(path) = cwd.and_then(resolve_development_gui_binary) {
        return path;
    }

    resolve_sibling_binary(current_exe, "zorai-desktop")
}

fn resolve_existing_sibling_binary(current_exe: Option<&Path>, name: &str) -> Option<PathBuf> {
    let binary_name = platform_binary_name(name);
    let current = current_exe?;
    let dir = current.parent()?;
    let sibling = dir.join(binary_name);
    sibling.exists().then_some(sibling)
}

fn resolve_development_gui_binary(cwd: &Path) -> Option<PathBuf> {
    for root in cwd.ancestors() {
        for release_root in [root.join("frontend").join("release"), root.join("release")] {
            if let Some(path) = find_development_gui_in_release_root(&release_root) {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn find_development_gui_in_release_root(release_root: &Path) -> Option<PathBuf> {
    let unpacked = release_root.join("linux-unpacked").join("zorai");
    if unpacked.exists() {
        return Some(unpacked);
    }

    find_linux_appimage(release_root)
}

#[cfg(target_os = "linux")]
fn find_linux_appimage(release_root: &Path) -> Option<PathBuf> {
    let mut entries = std::fs::read_dir(release_root)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    entries.sort();

    entries.into_iter().find(|path| {
        path.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("zorai"))
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("AppImage"))
    })
}

#[cfg(target_os = "macos")]
fn find_development_gui_in_release_root(release_root: &Path) -> Option<PathBuf> {
    let app_binary = release_root
        .join("mac")
        .join("zorai.app")
        .join("Contents")
        .join("MacOS")
        .join("zorai");
    app_binary.exists().then_some(app_binary)
}

#[cfg(windows)]
fn find_development_gui_in_release_root(release_root: &Path) -> Option<PathBuf> {
    let unpacked = release_root.join("win-unpacked").join("zorai.exe");
    unpacked.exists().then_some(unpacked)
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
    let explicit_gui_path = std::env::var("ZORAI_GUI_PATH").ok();
    let current_exe = std::env::current_exe().ok();
    let cwd = std::env::current_dir().ok();
    let gui_binary = resolve_gui_binary(
        explicit_gui_path.as_deref(),
        current_exe.as_deref(),
        cwd.as_deref(),
    );

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
