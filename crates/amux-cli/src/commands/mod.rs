pub(crate) mod common;
mod core;
mod plugins;
mod skills;

use anyhow::Result;

use crate::cli::{Commands, PluginAction, SkillAction};

pub(crate) async fn run_default() -> Result<()> {
    core::run_default().await
}

pub(crate) async fn run(command: Commands) -> Result<()> {
    match command {
        Commands::Skill { action } => run_skill(action).await,
        Commands::Plugin { action } => run_plugin(action).await,
        other => core::run(other).await,
    }
}

async fn run_skill(action: SkillAction) -> Result<()> {
    skills::run(action).await
}

async fn run_plugin(action: PluginAction) -> Result<()> {
    plugins::run(action).await
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
