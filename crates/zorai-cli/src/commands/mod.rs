pub(crate) mod common;
mod core;
mod guideline_sync;
mod guidelines;
mod plugins;
mod skill_sync;
mod skills;
mod tools;
mod workspace;
mod workspace_filters;

use anyhow::Result;

use crate::cli::{
    Commands, GuidelineAction, MigrateAction, PluginAction, SkillAction, ToolAction,
    WorkspaceAction,
};
use crate::update;

pub(crate) async fn run_default() -> Result<()> {
    core::run_default().await
}

pub(crate) async fn run(command: Commands) -> Result<()> {
    core::run_startup_preflight(&command).await?;

    if matches!(
        &command,
        Commands::Guideline { .. }
            | Commands::Skill { .. }
            | Commands::Plugin { .. }
            | Commands::Tool { .. }
            | Commands::Migrate { .. }
            | Commands::Workspace { .. }
    ) {
        update::print_upgrade_notice_if_available(env!("CARGO_PKG_VERSION")).await;
    }

    match command {
        Commands::Guideline { action } => run_guideline(action).await,
        Commands::Skill { action } => run_skill(action).await,
        Commands::Plugin { action } => run_plugin(action).await,
        Commands::Tool { action } => run_tool(action).await,
        Commands::Migrate { action } => run_migrate(action).await,
        Commands::Workspace { action } => run_workspace(action).await,
        other => core::run(other).await,
    }
}

async fn run_skill(action: SkillAction) -> Result<()> {
    skills::run(action).await
}

async fn run_guideline(action: GuidelineAction) -> Result<()> {
    guidelines::run(action).await
}

async fn run_plugin(action: PluginAction) -> Result<()> {
    plugins::run(action).await
}

async fn run_tool(action: ToolAction) -> Result<()> {
    tools::run(action).await
}

async fn run_migrate(action: MigrateAction) -> Result<()> {
    core::run(Commands::Migrate { action }).await
}

async fn run_workspace(action: WorkspaceAction) -> Result<()> {
    workspace::run(action).await
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
