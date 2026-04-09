pub(crate) mod common;
mod core;
mod plugins;
mod skills;
mod tools;

use anyhow::Result;

use crate::cli::{Commands, PluginAction, SkillAction, ToolAction};
use crate::update;

pub(crate) async fn run_default() -> Result<()> {
    core::run_default().await
}

pub(crate) async fn run(command: Commands) -> Result<()> {
    if matches!(
        &command,
        Commands::Skill { .. } | Commands::Plugin { .. } | Commands::Tool { .. }
    ) {
        update::print_upgrade_notice_if_available(env!("CARGO_PKG_VERSION")).await;
    }

    match command {
        Commands::Skill { action } => run_skill(action).await,
        Commands::Plugin { action } => run_plugin(action).await,
        Commands::Tool { action } => run_tool(action).await,
        other => core::run(other).await,
    }
}

async fn run_skill(action: SkillAction) -> Result<()> {
    skills::run(action).await
}

async fn run_plugin(action: PluginAction) -> Result<()> {
    plugins::run(action).await
}

async fn run_tool(action: ToolAction) -> Result<()> {
    tools::run(action).await
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
