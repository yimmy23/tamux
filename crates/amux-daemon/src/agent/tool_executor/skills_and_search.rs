async fn execute_list_skills(
    args: &serde_json::Value,
    agent_data_dir: &std::path::Path,
    history: &HistoryStore,
) -> Result<String> {
    let skills_root = super::skills_dir(agent_data_dir);
    let query = args
        .get("query")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty());
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .unwrap_or(20)
        .clamp(1, 100) as usize;

    let mut entries = sync_skill_catalog(&skills_root, history).await?;
    if entries.is_empty() {
        return Ok(format!(
            "No local skills found under {}.",
            skills_root.display()
        ));
    }

    entries.retain(|entry| match query.as_ref() {
        Some(needle) => {
            entry.skill_name.to_ascii_lowercase().contains(needle)
                || entry.variant_name.to_ascii_lowercase().contains(needle)
                || entry.relative_path.to_ascii_lowercase().contains(needle)
                || entry
                    .context_tags
                    .iter()
                    .any(|tag| tag.to_ascii_lowercase().contains(needle))
        }
        None => true,
    });
    entries.truncate(limit);

    if entries.is_empty() {
        return Ok(format!(
            "No local skills matched under {}.",
            skills_root.display()
        ));
    }

    let mut body = format!("Local skills under {}:\n", skills_root.display());
    for entry in entries {
        let tags = if entry.context_tags.is_empty() {
            "none".to_string()
        } else {
            entry.context_tags.join(", ")
        };
        body.push_str(&format!(
            "- {} [{} | status={}] ({}) tags={} uses={} success={:.0}%\n",
            entry.skill_name,
            entry.variant_name,
            entry.status,
            entry.relative_path,
            tags,
            entry.use_count,
            entry.success_rate() * 100.0,
        ));
    }
    Ok(body)
}

async fn execute_read_skill(
    args: &serde_json::Value,
    agent: &AgentEngine,
    agent_data_dir: &std::path::Path,
    history: &HistoryStore,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    let skill = args
        .get("skill")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'skill' argument"))?
        .trim();
    if skill.is_empty() {
        return Err(anyhow::anyhow!("'skill' must not be empty"));
    }

    let max_lines = args
        .get("max_lines")
        .and_then(|value| value.as_u64())
        .unwrap_or(200)
        .clamp(20, 1000) as usize;
    let skills_root = super::skills_dir(agent_data_dir);
    sync_skill_catalog(&skills_root, history).await?;
    let context_tags = resolve_skill_context_tags(session_manager, session_id).await;
    let variant = history.resolve_skill_variant(skill, &context_tags).await?;
    let candidate_variants = match variant.as_ref() {
        Some(selected) => history
            .list_skill_variants(Some(&selected.skill_name), 8)
            .await
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let skill_path = resolve_skill_path(&skills_root, skill, variant.as_ref())?;
    let content = tokio::fs::read_to_string(&skill_path).await?;
    if let Some(variant) = variant.as_ref() {
        let (goal_run_id, _, _) = agent.goal_context_for_task(task_id).await;
        agent
            .persist_skill_selection_causal_trace(
                thread_id,
                goal_run_id.as_deref(),
                task_id,
                variant,
                &candidate_variants,
                &context_tags,
            )
            .await;
        agent
            .record_skill_consultation(thread_id, task_id, variant, &context_tags)
            .await;
    }
    let total_lines = content.lines().count();
    let lines = content.lines().take(max_lines).collect::<Vec<_>>();
    let relative = skill_path
        .strip_prefix(&skills_root)
        .unwrap_or(skill_path.as_path())
        .display()
        .to_string();

    let mut body = if let Some(ref variant) = variant {
        let tags = if variant.context_tags.is_empty() {
            "none".to_string()
        } else {
            variant.context_tags.join(", ")
        };
        format!(
            "Skill {} [{} | {} | uses={} | success={:.0}% | tags={}]:\n\n{}",
            relative,
            variant.skill_name,
            variant.variant_name,
            variant.use_count.saturating_add(1),
            variant.success_rate() * 100.0,
            tags,
            lines.join("\n")
        )
    } else {
        format!("Skill {}:\n\n{}", relative, lines.join("\n"))
    };
    if total_lines > max_lines {
        body.push_str(&format!(
            "\n\n... (truncated, showing {max_lines} of {total_lines} lines)"
        ));
    }
    let compliance_identifier = variant
        .as_ref()
        .map(|entry| entry.skill_name.as_str())
        .unwrap_or(skill);
    let _ = agent
        .record_thread_skill_read_compliance(thread_id, compliance_identifier)
        .await;
    Ok(body)
}

async fn execute_justify_skill_skip(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
) -> Result<String> {
    let rationale = args
        .get("rationale")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'rationale' argument"))?;
    let state = agent
        .record_thread_skill_skip_rationale(thread_id, rationale)
        .await?;
    Ok(format!(
        "Recorded skill skip rationale. Confidence={} compliant={} next_action={}.",
        state.confidence_tier, state.compliant, state.recommended_action
    ))
}

async fn execute_update_todo(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    let raw_items = args
        .get("items")
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing 'items' argument"))?;

    let now = super::now_millis();
    let mut items = Vec::new();
    for (index, raw_item) in raw_items.iter().enumerate() {
        let content = raw_item
            .get("content")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("todo item {index} is missing non-empty 'content'"))?;
        let status = match raw_item
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("pending")
        {
            "pending" => TodoStatus::Pending,
            "in_progress" => TodoStatus::InProgress,
            "completed" => TodoStatus::Completed,
            "blocked" => TodoStatus::Blocked,
            other => {
                return Err(anyhow::anyhow!(
                    "todo item {index} has invalid status '{other}'"
                ));
            }
        };

        items.push(TodoItem {
            id: format!("todo_{}", uuid::Uuid::new_v4()),
            content: content.to_string(),
            status,
            position: index,
            step_index: raw_item
                .get("step_index")
                .and_then(|value| value.as_u64())
                .map(|value| value as usize),
            created_at: now,
            updated_at: now,
        });
    }

    agent
        .replace_thread_todos(thread_id, items.clone(), task_id)
        .await;

    Ok(format!("Updated todo list with {} item(s).", items.len()))
}

async fn execute_web_search(
    args: &serde_json::Value,
    http_client: &reqwest::Client,
    search_provider: &str,
    exa_api_key: &str,
    tavily_api_key: &str,
) -> Result<String> {
    execute_web_search_with_runner(
        args,
        search_provider,
        exa_api_key,
        tavily_api_key,
        |request, provider| async move {
            match provider {
                "exa" => {
                    execute_exa_search(
                        http_client,
                        &request.query,
                        request.max_results,
                        exa_api_key,
                    )
                    .await
                }
                "tavily" => {
                    execute_tavily_search(
                        http_client,
                        &request.query,
                        request.max_results,
                        tavily_api_key,
                    )
                    .await
                }
                _ => execute_ddg_search(http_client, &request.query, request.max_results).await,
            }
        },
    )
    .await
}

async fn execute_web_search_with_runner<F, Fut>(
    args: &serde_json::Value,
    search_provider: &str,
    exa_api_key: &str,
    tavily_api_key: &str,
    runner: F,
) -> Result<String>
where
    F: FnOnce(WebSearchRequest, &'static str) -> Fut,
    Fut: Future<Output = Result<String>>,
{
    let request = web_search_request(args)?;
    let timeout_seconds = request.timeout_seconds;
    let provider = match search_provider {
        "exa" if !exa_api_key.is_empty() => "exa",
        "tavily" if !tavily_api_key.is_empty() => "tavily",
        _ => "ddg",
    };

    tokio::time::timeout(
        std::time::Duration::from_secs(timeout_seconds),
        runner(request, provider),
    )
    .await
    .map_err(|_| anyhow::anyhow!("web search timed out after {timeout_seconds} seconds"))?
}

fn safe_snippet_preview(text: &str, max_chars: usize) -> String {
    truncate_on_char_boundary(text, max_chars, "...")
}

fn safe_text_excerpt(text: &str, max_chars: usize) -> String {
    truncate_on_char_boundary(text, max_chars, "")
}

fn truncate_on_char_boundary(text: &str, max_chars: usize, suffix: &str) -> String {
    if let Some((idx, _)) = text.char_indices().nth(max_chars) {
        let mut truncated = text[..idx].to_string();
        truncated.push_str(suffix);
        truncated
    } else {
        text.to_string()
    }
}

async fn execute_exa_search(
    http_client: &reqwest::Client,
    query: &str,
    max_results: u64,
    api_key: &str,
) -> Result<String> {
    let body = serde_json::json!({
        "query": query,
        "numResults": max_results,
        "type": "auto",
        "contents": {
            "text": { "maxCharacters": 1000 },
            "highlights": { "numSentences": 2 },
        },
    });

    let resp = http_client
        .post("https://api.exa.ai/search")
        .header("x-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "Exa API returned {status}: {}",
            safe_text_excerpt(&text, 200)
        );
    }

    let json: serde_json::Value = resp.json().await?;
    let results = json["results"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|r| {
                    let title = r["title"].as_str().unwrap_or("(no title)");
                    let url = r["url"].as_str().unwrap_or("");
                    let text = r["text"].as_str().unwrap_or("");
                    let published_at = r["publishedDate"]
                        .as_str()
                        .or_else(|| r["published_date"].as_str())
                        .or_else(|| r["publishedAt"].as_str());
                    let snippet = safe_snippet_preview(text, 300);
                    format_result_with_metadata(title, url, &snippet, published_at)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if results.is_empty() {
        Ok(format!("No web results found for: {query}"))
    } else {
        Ok(format!(
            "Web results for \"{query}\":\n\n{}",
            results.join("\n\n")
        ))
    }
}

async fn execute_tavily_search(
    http_client: &reqwest::Client,
    query: &str,
    max_results: u64,
    api_key: &str,
) -> Result<String> {
    let body = serde_json::json!({
        "query": query,
        "max_results": max_results,
        "search_depth": "basic",
    });

    let resp = http_client
        .post("https://api.tavily.com/search")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "Tavily API returned {status}: {}",
            safe_text_excerpt(&text, 200)
        );
    }

    let json: serde_json::Value = resp.json().await?;
    let results = json["results"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|r| {
                    let title = r["title"].as_str().unwrap_or("(no title)");
                    let url = r["url"].as_str().unwrap_or("");
                    let content = r["content"].as_str().unwrap_or("");
                    let published_at = r["published_date"]
                        .as_str()
                        .or_else(|| r["publishedDate"].as_str())
                        .or_else(|| r["publishedAt"].as_str());
                    let snippet = safe_snippet_preview(content, 300);
                    format_result_with_metadata(title, url, &snippet, published_at)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if results.is_empty() {
        Ok(format!("No web results found for: {query}"))
    } else {
        Ok(format!(
            "Web results for \"{query}\":\n\n{}",
            results.join("\n\n")
        ))
    }
}

async fn execute_ddg_search(
    http_client: &reqwest::Client,
    query: &str,
    max_results: u64,
) -> Result<String> {
    let url = format!(
        "https://lite.duckduckgo.com/lite/?q={}&kl=us-en",
        urlencoding::encode(query)
    );

    let resp = http_client
        .get(&url)
        .header("User-Agent", "tamux-agent/0.1")
        .send()
        .await?;

    let text = resp.text().await?;

    let mut results = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<a rel=\"nofollow\"") {
            if let (Some(href_start), Some(href_end)) =
                (trimmed.find("href=\""), trimmed.find("\">"))
            {
                let url = &trimmed[href_start + 6..href_end];
                let text_start = href_end + 2;
                if let Some(text_end) = trimmed[text_start..].find("</a>") {
                    let title = &trimmed[text_start..text_start + text_end];
                    results.push(format_result_with_metadata(
                        title,
                        url,
                        "No snippet available.",
                        None,
                    ));
                }
            }
        }
        if results.len() >= max_results as usize {
            break;
        }
    }

    if results.is_empty() {
        Ok(format!("No web results found for: {query}"))
    } else {
        Ok(format!(
            "Web results for \"{query}\":\n\n{}",
            results.join("\n\n")
        ))
    }
}
