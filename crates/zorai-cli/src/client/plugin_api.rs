use anyhow::Result;
use zorai_protocol::{ClientMessage, DaemonMessage};

use super::connection::roundtrip;

/// Send PluginInstall to daemon for registration. Returns (success, message).
pub async fn send_plugin_install(dir_name: &str, install_source: &str) -> Result<(bool, String)> {
    match roundtrip(ClientMessage::PluginInstall {
        dir_name: dir_name.to_string(),
        install_source: install_source.to_string(),
    })
    .await?
    {
        DaemonMessage::PluginActionResult { success, message } => Ok((success, message)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

/// Send PluginUninstall to daemon for deregistration. Returns (success, message).
pub async fn send_plugin_uninstall(name: &str) -> Result<(bool, String)> {
    match roundtrip(ClientMessage::PluginUninstall {
        name: name.to_string(),
    })
    .await?
    {
        DaemonMessage::PluginActionResult { success, message } => Ok((success, message)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

/// Send PluginList to daemon. Returns list of PluginInfo. Per INST-05.
pub async fn send_plugin_list() -> Result<Vec<zorai_protocol::PluginInfo>> {
    match roundtrip(ClientMessage::PluginList {}).await? {
        DaemonMessage::PluginListResult { plugins } => Ok(plugins),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

/// Send PluginEnable to daemon. Returns (success, message). Per INST-06.
pub async fn send_plugin_enable(name: &str) -> Result<(bool, String)> {
    match roundtrip(ClientMessage::PluginEnable {
        name: name.to_string(),
    })
    .await?
    {
        DaemonMessage::PluginActionResult { success, message } => Ok((success, message)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

/// Send PluginDisable to daemon. Returns (success, message). Per INST-06.
pub async fn send_plugin_disable(name: &str) -> Result<(bool, String)> {
    match roundtrip(ClientMessage::PluginDisable {
        name: name.to_string(),
    })
    .await?
    {
        DaemonMessage::PluginActionResult { success, message } => Ok((success, message)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

/// Send PluginListCommands to daemon. Returns list of registered plugin commands. Per PSKL-05.
pub async fn send_plugin_list_commands() -> Result<Vec<zorai_protocol::PluginCommandInfo>> {
    match roundtrip(ClientMessage::PluginListCommands {}).await {
        Ok(DaemonMessage::PluginCommandsResult { commands }) => Ok(commands),
        Ok(DaemonMessage::Error { message }) => {
            tracing::warn!("daemon error listing plugin commands: {message}");
            Ok(vec![])
        }
        Ok(_other) => {
            tracing::warn!("unexpected response for PluginListCommands");
            Ok(vec![])
        }
        Err(e) => {
            tracing::warn!("failed to list plugin commands: {e}");
            Ok(vec![])
        }
    }
}
