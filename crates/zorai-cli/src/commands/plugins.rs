use anyhow::{bail, Result};

use crate::cli::PluginAction;
use crate::output::truncate_for_display;
use crate::{client, plugins};

pub(crate) async fn run(action: PluginAction) -> Result<()> {
    match action {
        PluginAction::Add { source } => {
            println!("Installing plugin from '{}'...", source);
            let results = plugins::install_plugin_v2(&source)?;

            if results.is_empty() {
                bail!("No plugins found in '{}'", source);
            }

            for (dir_name, _) in &results {
                println!("Files installed to ~/.zorai/plugins/{}/", dir_name);
            }

            let mut registered = Vec::new();
            let mut failed = false;

            for (dir_name, source_label) in &results {
                match client::send_plugin_install(dir_name, source_label).await {
                    Ok((true, message)) => {
                        println!("{}", message);
                        registered.push(dir_name.clone());
                    }
                    Ok((false, message)) => {
                        eprintln!("Registration failed for '{}': {}", dir_name, message);
                        failed = true;
                        break;
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Could not register '{}' with daemon ({}). Start the daemon to activate.",
                            dir_name, e
                        );
                        registered.push(dir_name.clone());
                    }
                }
            }

            if failed {
                for (dir_name, _) in &results {
                    let _ = plugins::remove_plugin_files(dir_name);
                }
                for name in &registered {
                    let _ = client::send_plugin_uninstall(name).await;
                }
                eprintln!("Cleaned up all plugin files from this package.");
                std::process::exit(1);
            }
        }
        PluginAction::Remove { name } => {
            match client::send_plugin_uninstall(&name).await {
                Ok((true, message)) => {
                    println!("{}", message);
                }
                Ok((false, message)) => {
                    eprintln!("Warning: {}", message);
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Could not reach daemon ({}). Removing files only.",
                        e
                    );
                }
            }

            plugins::remove_plugin_files(&name)?;
            println!("Plugin '{}' removed.", name);
        }
        PluginAction::Ls => match client::send_plugin_list().await {
            Ok(plugins) => {
                if plugins.is_empty() {
                    println!("No plugins installed.");
                } else {
                    println!(
                        "{:<24} {:>8} {:>8} {:>6} {}",
                        "NAME", "VERSION", "ENABLED", "AUTH", "SOURCE"
                    );
                    for plugin in &plugins {
                        let auth_status = if plugin.has_auth { "yes" } else { "-" };
                        let enabled = if plugin.enabled { "yes" } else { "no" };
                        println!(
                            "{:<24} {:>8} {:>8} {:>6} {}",
                            truncate_for_display(&plugin.name, 24),
                            truncate_for_display(&plugin.version, 8),
                            enabled,
                            auth_status,
                            truncate_for_display(&plugin.install_source, 40),
                        );
                    }
                    println!("\n{} plugin(s) installed.", plugins.len());
                }
            }
            Err(e) => {
                eprintln!("Could not reach daemon: {}", e);
                std::process::exit(1);
            }
        },
        PluginAction::Enable { name } => {
            let (success, message) = client::send_plugin_enable(&name).await?;
            if success {
                println!("{}", message);
            } else {
                eprintln!("{}", message);
                std::process::exit(1);
            }
        }
        PluginAction::Disable { name } => {
            let (success, message) = client::send_plugin_disable(&name).await?;
            if success {
                println!("{}", message);
            } else {
                eprintln!("{}", message);
                std::process::exit(1);
            }
        }
        PluginAction::Commands => match client::send_plugin_list_commands().await {
            Ok(commands) => plugins::plugin_commands(&commands),
            Err(e) => {
                eprintln!("Could not reach daemon: {}", e);
                std::process::exit(1);
            }
        },
    }

    Ok(())
}
