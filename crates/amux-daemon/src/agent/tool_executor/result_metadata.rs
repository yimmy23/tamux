async fn execute_onecontext_search_with_runner<F, Fut>(
    args: &serde_json::Value,
    aline_available: bool,
    runner: F,
) -> Result<String>
where
    F: FnOnce(OnecontextSearchRequest) -> Fut,
    Fut: Future<Output = Result<std::process::Output>>,
{
    let request = onecontext_search_request(args)?;

    if !aline_available {
        return Ok("OneContext search unavailable: `aline` CLI not found on PATH.".into());
    }

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(request.timeout_seconds),
        runner(request.clone()),
    )
    .await
    .map_err(|_| anyhow::anyhow!("onecontext search timed out"))??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            return Err(anyhow::anyhow!(
                "onecontext search failed in {} scope with exit status {}",
                request.scope,
                output.status
            ));
        }
        return Err(anyhow::anyhow!("onecontext search failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(format!(
            "No OneContext matches for \"{}\" in {} scope.",
            request.bounded_query, request.scope
        ));
    }

    let trimmed_chars = trimmed.chars().count();
    let output_text = if trimmed_chars > ONECONTEXT_TOOL_OUTPUT_MAX_CHARS {
        let shortened = trimmed
            .chars()
            .take(ONECONTEXT_TOOL_OUTPUT_MAX_CHARS)
            .collect::<String>();
        format!(
            "{}\n\n(truncated, {} chars total)",
            shortened, trimmed_chars
        )
    } else {
        trimmed.to_string()
    };

    Ok(format!(
        "OneContext results for \"{}\" ({} scope):\n\n{output_text}",
        request.bounded_query, request.scope
    ))
}

// ---------------------------------------------------------------------------
// Source authority classification for web search results (UNCR-03)
// ---------------------------------------------------------------------------

/// Classify URL source authority for web search/read results (UNCR-03).
/// Uses URL domain pattern matching -- deterministic, zero-latency.
fn classify_source_authority(url: &str) -> &'static str {
    let lower = url.to_lowercase();
    if lower.contains("docs.")
        || lower.contains("/docs/")
        || lower.contains("developer.")
        || lower.contains(".readthedocs.")
        || lower.contains("man7.org")
        || lower.contains("cppreference.com")
        || lower.contains(".github.io/")
        || lower.contains("spec.")
        || lower.contains("rfc-editor.org")
        || lower.contains("w3.org")
    {
        "official"
    } else if lower.contains("stackoverflow.com")
        || lower.contains("reddit.com")
        || lower.contains("medium.com")
        || lower.contains("dev.to")
        || lower.contains("blog.")
        || lower.contains("forum.")
        || lower.contains("discuss.")
        || lower.contains("news.ycombinator.com")
    {
        "community"
    } else {
        "unknown"
    }
}

/// Format a single search result line with source authority label prepended.
fn format_result_with_authority(title: &str, url: &str, snippet: &str) -> String {
    format_result_with_metadata(title, url, snippet, None)
}

fn classify_freshness(published_at: Option<&str>) -> &'static str {
    let Some(value) = published_at
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return "unknown";
    };
    let Some(date_prefix) = value.get(..10) else {
        return "unknown";
    };
    let Ok(date) = chrono::NaiveDate::parse_from_str(date_prefix, "%Y-%m-%d") else {
        return "unknown";
    };
    let now = chrono::Utc::now().date_naive();
    let age_days = now.signed_duration_since(date).num_days();
    if age_days <= 30 {
        "recent"
    } else if age_days <= 365 {
        "stale"
    } else {
        "old"
    }
}

fn format_result_with_metadata(
    title: &str,
    url: &str,
    snippet: &str,
    published_at: Option<&str>,
) -> String {
    format!(
        "- [{}] [freshness: {}] **{}**\n  {}\n  {}",
        classify_source_authority(url),
        classify_freshness(published_at),
        title,
        url,
        snippet
    )
}

// ---------------------------------------------------------------------------
// Tool reordering by learned heuristic effectiveness (D-08)
