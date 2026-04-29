use anyhow::Result;
use zorai_protocol::{ClientMessage, DaemonMessage, ToolListResultPublic, ToolSearchResultPublic};

use super::connection::roundtrip;

pub async fn send_tool_list(
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<ToolListResultPublic> {
    match roundtrip(ClientMessage::AgentListTools { limit, offset }).await? {
        DaemonMessage::AgentToolList { result } => Ok(result),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_tool_search(
    query: &str,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<ToolSearchResultPublic> {
    match roundtrip(ClientMessage::AgentSearchTools {
        query: query.to_string(),
        limit,
        offset,
    })
    .await?
    {
        DaemonMessage::AgentToolSearchResult { result } => Ok(result),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}
