use crate::agent::thread_crud::ThreadListFilter;

const DEFAULT_GET_THREAD_LIMIT: usize = 5;

fn validate_offloaded_payload_id(payload_id: &str) -> Result<()> {
    use std::ffi::OsStr;
    use std::path::{Component, Path};

    let mut components = Path::new(payload_id).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(component)), None)
            if component == OsStr::new(payload_id) && !payload_id.contains('\\') =>
        {
            Ok(())
        }
        _ => anyhow::bail!("offloaded payload not found"),
    }
}

async fn resolve_canonical_offloaded_payload_read_path(
    agent: &AgentEngine,
    thread_id: &str,
    payload_id: &str,
) -> Result<std::path::PathBuf> {
    let root = agent.history.offloaded_payloads_dir();
    let thread_root = root.join(thread_id);
    let derived_path = agent.history.offloaded_payload_path(thread_id, payload_id);
    let canonical_root = tokio::fs::canonicalize(&root)
        .await
        .with_context(|| format!("failed to resolve offloaded payload root {}", root.display()))?;
    let canonical_thread_root = tokio::fs::canonicalize(&thread_root)
        .await
        .with_context(|| {
            format!(
                "failed to resolve offloaded payload thread root {}",
                thread_root.display()
            )
        })?;
    let canonical_path = tokio::fs::canonicalize(&derived_path)
        .await
        .with_context(|| format!("failed to resolve offloaded payload {}", derived_path.display()))?;

    if !canonical_thread_root.starts_with(&canonical_root) {
        anyhow::bail!(
            "offloaded payload thread root {} resolves outside daemon-owned root {}",
            thread_id,
            canonical_root.display()
        );
    }

    if !canonical_path.starts_with(&canonical_root) {
        anyhow::bail!(
            "offloaded payload {} resolves outside daemon-owned root {}",
            payload_id,
            canonical_root.display()
        );
    }

    if !canonical_path.starts_with(&canonical_thread_root) {
        anyhow::bail!("offloaded payload not found");
    }

    Ok(canonical_path)
}

fn parse_non_negative_u64_arg(args: &serde_json::Value, field: &str) -> Result<Option<u64>> {
    if args.get(field).is_some_and(|value| value.as_u64().is_none()) {
        anyhow::bail!("'{field}' must be a non-negative integer");
    }

    Ok(args.get(field).and_then(|value| value.as_u64()))
}

fn parse_non_negative_usize_arg(args: &serde_json::Value, field: &str) -> Result<Option<usize>> {
    let value = parse_non_negative_u64_arg(args, field)?;
    value
        .map(|value| {
            usize::try_from(value)
                .map_err(|_| anyhow::anyhow!("'{field}' is too large for this platform"))
        })
        .transpose()
}

async fn execute_list_threads(args: &serde_json::Value, agent: &AgentEngine) -> Result<String> {
    let filter = ThreadListFilter {
        created_after: parse_non_negative_u64_arg(args, "created_after")?,
        created_before: parse_non_negative_u64_arg(args, "created_before")?,
        updated_after: parse_non_negative_u64_arg(args, "updated_after")?,
        updated_before: parse_non_negative_u64_arg(args, "updated_before")?,
        agent_name: args
            .get("agent_name")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        title_query: args
            .get("title_query")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        pinned: args.get("pinned").and_then(|value| value.as_bool()),
        include_internal: args
            .get("include_internal")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        limit: parse_non_negative_usize_arg(args, "limit")?,
        offset: parse_non_negative_usize_arg(args, "offset")?.unwrap_or(0),
    };

    let threads = agent.list_threads_filtered(&filter).await;
    Ok(serde_json::to_string_pretty(&threads).unwrap_or_else(|_| "[]".to_string()))
}

async fn execute_get_thread(args: &serde_json::Value, agent: &AgentEngine) -> Result<String> {
    let thread_id = match args.get("thread_id") {
        Some(value) => value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("'thread_id' must be a non-empty string"))?,
        None => anyhow::bail!("missing 'thread_id' argument"),
    };
    let message_limit = parse_non_negative_usize_arg(args, "limit")?
        .or(parse_non_negative_usize_arg(args, "message_limit")?)
        .unwrap_or(DEFAULT_GET_THREAD_LIMIT);
    let message_offset = parse_non_negative_usize_arg(args, "offset")?.unwrap_or(0);
    let include_internal = args
        .get("include_internal")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let detail = agent
        .get_thread_filtered(thread_id, include_internal, Some(message_limit), message_offset)
        .await;

    let Some(detail) = detail else {
        if !include_internal
            && agent
                .get_thread_filtered(thread_id, true, None, 0)
                .await
                .is_some()
        {
            tracing::info!(
                tool = "get_thread",
                thread_id = %thread_id,
                "masked hidden internal thread as not found"
            );
        }
        anyhow::bail!("thread not found");
    };

    Ok(serde_json::to_string_pretty(&detail).unwrap_or_else(|_| "{}".to_string()))
}

async fn execute_read_offloaded_payload(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
) -> Result<String> {
    let payload_id = match args.get("payload_id") {
        Some(value) => value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("'payload_id' must be a non-empty string"))?,
        None => anyhow::bail!("missing 'payload_id' argument"),
    };
    validate_offloaded_payload_id(payload_id)?;

    let metadata = agent
        .history
        .get_offloaded_payload_metadata(payload_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("offloaded payload not found"))?;

    if metadata.thread_id != thread_id {
        anyhow::bail!("offloaded payload not found");
    }

    let derived_path = agent
        .history
        .offloaded_payload_path(&metadata.thread_id, payload_id);
    let derived_storage_path = derived_path.to_string_lossy().into_owned();
    if metadata.storage_path != derived_storage_path {
        tracing::warn!(
            payload_id = %payload_id,
            thread_id = %metadata.thread_id,
            stored_path = %metadata.storage_path,
            canonical_path = %derived_storage_path,
            "offloaded payload metadata path mismatch; reading from canonical daemon path"
        );
    }

    let canonical_path = resolve_canonical_offloaded_payload_read_path(
        agent,
        &metadata.thread_id,
        payload_id,
    )
    .await?;

    tokio::fs::read_to_string(&canonical_path)
        .await
        .with_context(|| {
            format!(
                "failed to read offloaded payload {} from {}",
                payload_id,
                canonical_path.display()
            )
        })
}