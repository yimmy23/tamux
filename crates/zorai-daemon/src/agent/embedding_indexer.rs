use super::*;
use zorai_protocol::HistorySearchHit;

const EMBEDDING_IDLE_SLEEP_SECS: u64 = 30;
const EMBEDDING_DISABLED_SLEEP_SECS: u64 = 60;
const EMBEDDING_ACTIVE_SLEEP_MILLIS: u64 = 500;
#[cfg(feature = "lancedb-vector")]
const EMBEDDING_REQUEST_TIMEOUT_SECS: u64 = 90;
const MAX_EMBEDDING_BATCH_SIZE: usize = 256;

#[cfg(feature = "lancedb-vector")]
#[derive(Debug, serde::Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingDatum>,
}

#[cfg(feature = "lancedb-vector")]
#[derive(Debug, serde::Deserialize)]
struct EmbeddingDatum {
    #[serde(default)]
    index: Option<usize>,
    embedding: Vec<f32>,
}

fn openai_like_endpoint(base_url: &str, endpoint: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let endpoint = endpoint.trim_start_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/{endpoint}")
    } else {
        format!("{base}/v1/{endpoint}")
    }
}

fn embedding_batch_size(configured: u32) -> usize {
    (configured as usize).clamp(1, MAX_EMBEDDING_BATCH_SIZE)
}

#[cfg(feature = "lancedb-vector")]
fn semantic_score_from_distance(distance: f64) -> f64 {
    if distance.is_finite() && distance >= 0.0 {
        1.0 / (1.0 + distance)
    } else {
        0.0
    }
}

#[cfg(feature = "lancedb-vector")]
fn vector_hits_to_history_hits(
    hits: Vec<crate::history::vector_index::VectorSearchHit>,
) -> Vec<HistorySearchHit> {
    hits.into_iter()
        .map(|hit| HistorySearchHit {
            id: hit.source_id,
            kind: hit.source_kind.as_str().to_string(),
            title: hit.title,
            excerpt: hit.snippet.unwrap_or_default(),
            path: None,
            timestamp: hit.timestamp.unwrap_or_default().max(0) as u64,
            score: semantic_score_from_distance(hit.score),
        })
        .collect()
}

#[cfg(feature = "lancedb-vector")]
fn resolved_embedding_model(config: &AgentConfig) -> String {
    config.semantic.embedding.model.trim().to_string()
}

#[cfg(feature = "lancedb-vector")]
fn resolve_embedding_provider(
    config: &AgentConfig,
) -> Result<(String, ProviderConfig, AuthMethod)> {
    let provider_id = config.semantic.embedding.provider.trim();
    if provider_id.is_empty() {
        anyhow::bail!("semantic embedding provider is empty");
    }
    let model = resolved_embedding_model(config);
    if model.is_empty() {
        anyhow::bail!("semantic embedding model is empty");
    }
    let mut provider_config = resolve_provider_config_for(config, provider_id, Some(&model))?;
    provider_config.model = model;
    let auth_method = get_provider_definition(provider_id)
        .map(|definition| definition.auth_method)
        .unwrap_or(AuthMethod::Bearer);
    Ok((provider_id.to_string(), provider_config, auth_method))
}

#[cfg(feature = "lancedb-vector")]
async fn request_embeddings(
    http_client: &reqwest::Client,
    config: &AgentConfig,
    inputs: &[String],
) -> Result<Vec<Vec<f32>>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    let (provider_id, provider_config, auth_method) = resolve_embedding_provider(config)?;
    let api_key = provider_config.api_key.trim();
    if api_key.is_empty() {
        anyhow::bail!(
            "No API key configured for embedding provider '{}'. Log in or configure provider credentials.",
            provider_id
        );
    }
    let endpoint = openai_like_endpoint(&provider_config.base_url, "embeddings");
    let mut body = serde_json::json!({
        "model": provider_config.model,
        "input": inputs,
    });
    if config.semantic.embedding.dimensions > 0 {
        body["dimensions"] = serde_json::json!(config.semantic.embedding.dimensions);
    }

    let response = auth_method
        .apply(http_client.post(endpoint), api_key)
        .timeout(std::time::Duration::from_secs(
            EMBEDDING_REQUEST_TIMEOUT_SECS,
        ))
        .json(&body)
        .send()
        .await
        .context("embedding provider request failed")?;
    let status = response.status();
    let response_text = response
        .text()
        .await
        .context("failed to read embedding provider response")?;
    if !status.is_success() {
        let preview = response_text.chars().take(1000).collect::<String>();
        anyhow::bail!("embedding provider returned HTTP {status}: {preview}");
    }

    let mut parsed: EmbeddingResponse =
        serde_json::from_str(&response_text).context("invalid embedding provider response")?;
    if parsed.data.len() != inputs.len() {
        anyhow::bail!(
            "embedding provider returned {} embeddings for {} inputs",
            parsed.data.len(),
            inputs.len()
        );
    }
    parsed
        .data
        .sort_by_key(|datum| datum.index.unwrap_or(usize::MAX));
    let embeddings = parsed
        .data
        .into_iter()
        .map(|datum| datum.embedding)
        .collect::<Vec<_>>();
    let expected_dimensions = config.semantic.embedding.dimensions as usize;
    if expected_dimensions > 0 {
        for embedding in &embeddings {
            if embedding.len() != expected_dimensions {
                anyhow::bail!(
                    "embedding provider returned {} dimensions, expected {}",
                    embedding.len(),
                    expected_dimensions
                );
            }
        }
    }
    Ok(embeddings)
}

impl AgentEngine {
    pub(crate) async fn run_embedding_index_loop(
        self: Arc<Self>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        loop {
            let enabled = self.config.read().await.semantic.embedding.enabled;
            let processed = match self.process_embedding_queue_once().await {
                Ok(processed) => processed,
                Err(error) => {
                    tracing::warn!(error = %error, "semantic embedding queue processing failed");
                    0
                }
            };

            let sleep_duration = if !enabled {
                std::time::Duration::from_secs(EMBEDDING_DISABLED_SLEEP_SECS)
            } else if processed > 0 {
                std::time::Duration::from_millis(EMBEDDING_ACTIVE_SLEEP_MILLIS)
            } else {
                std::time::Duration::from_secs(EMBEDDING_IDLE_SLEEP_SECS)
            };

            tokio::select! {
                _ = tokio::time::sleep(sleep_duration) => {}
                _ = self.config_notify.notified() => {}
                _ = shutdown.changed() => break,
            }
        }
    }

    pub(crate) async fn process_embedding_queue_once(&self) -> Result<usize> {
        let deleted = self.process_embedding_deletions_once().await?;
        let config = self.get_config().await;
        if !config.semantic.embedding.enabled {
            return Ok(deleted);
        }
        Ok(deleted + self.process_embedding_queue_once_inner(config).await?)
    }

    pub(crate) async fn search_history_semantic_first(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<(String, Vec<HistorySearchHit>)> {
        let limit = limit.max(1);
        let config = self.get_config().await;
        if !config.semantic.embedding.enabled {
            return self.history.search(query, limit).await;
        }
        match self
            .semantic_vector_history_search(query, limit, &config)
            .await
        {
            Ok((summary, hits)) if !hits.is_empty() => Ok((summary, hits)),
            Ok(_) => self.history.search(query, limit).await,
            Err(error) => {
                tracing::warn!(error = %error, "semantic history search failed; falling back to SQLite FTS");
                self.history.search(query, limit).await
            }
        }
    }

    #[cfg(not(feature = "lancedb-vector"))]
    async fn semantic_vector_history_search(
        &self,
        _query: &str,
        _limit: usize,
        _config: &AgentConfig,
    ) -> Result<(String, Vec<HistorySearchHit>)> {
        Ok((
            "Semantic vector search is not compiled in.".to_string(),
            Vec::new(),
        ))
    }

    #[cfg(feature = "lancedb-vector")]
    async fn semantic_vector_history_search(
        &self,
        query: &str,
        limit: usize,
        config: &AgentConfig,
    ) -> Result<(String, Vec<HistorySearchHit>)> {
        let embedding_model = resolved_embedding_model(config);
        if embedding_model.is_empty() {
            return Ok((
                "No semantic embedding model configured.".to_string(),
                Vec::new(),
            ));
        }
        let query_embeddings =
            request_embeddings(&self.http_client, config, &[query.trim().to_string()]).await?;
        let Some(query_embedding) = query_embeddings.into_iter().next() else {
            return Ok(("No query embedding returned.".to_string(), Vec::new()));
        };
        let index = crate::history::vector_index::VectorIndex::open(self.history.data_root());
        let vector_hits = index
            .search(crate::history::vector_index::VectorSearchRequest {
                embedding: query_embedding,
                embedding_model,
                limit,
                source_kinds: vec![
                    crate::history::vector_index::VectorSourceKind::AgentMessage,
                    crate::history::vector_index::VectorSourceKind::AgentTask,
                ],
                workspace_id: None,
                thread_id: None,
                agent_id: None,
            })
            .await?;
        let hits = vector_hits_to_history_hits(vector_hits);
        Ok((
            format!(
                "Found {} semantic history matches for '{}'.",
                hits.len(),
                query
            ),
            hits,
        ))
    }

    #[cfg(not(feature = "lancedb-vector"))]
    async fn process_embedding_deletions_once(&self) -> Result<usize> {
        Ok(0)
    }

    #[cfg(feature = "lancedb-vector")]
    async fn process_embedding_deletions_once(&self) -> Result<usize> {
        let deletions = self.history.claim_embedding_deletions(256).await?;
        if deletions.is_empty() {
            return Ok(0);
        }
        let index = crate::history::vector_index::VectorIndex::open(self.history.data_root());
        let mut completed = 0usize;
        for deletion in deletions {
            let Some(source_kind) =
                crate::history::vector_index::VectorSourceKind::from_embedding_source_kind(
                    &deletion.source_kind,
                )
            else {
                self.history.complete_embedding_deletion(&deletion).await?;
                continue;
            };
            match index.delete_source(source_kind, &deletion.source_id).await {
                Ok(()) => {
                    self.history.complete_embedding_deletion(&deletion).await?;
                    completed += 1;
                }
                Err(error) => {
                    self.history
                        .fail_embedding_deletion(&deletion, &error.to_string())
                        .await?;
                }
            }
        }
        Ok(completed)
    }

    #[cfg(not(feature = "lancedb-vector"))]
    async fn process_embedding_queue_once_inner(&self, _config: AgentConfig) -> Result<usize> {
        Ok(0)
    }

    #[cfg(feature = "lancedb-vector")]
    async fn process_embedding_queue_once_inner(&self, config: AgentConfig) -> Result<usize> {
        let embedding_model = resolved_embedding_model(&config);
        if embedding_model.is_empty() {
            return Ok(0);
        }
        let dimensions = config.semantic.embedding.dimensions;
        let batch_size = embedding_batch_size(config.semantic.embedding.batch_size);
        let jobs = self
            .history
            .claim_embedding_jobs(&embedding_model, dimensions, batch_size)
            .await?;
        if jobs.is_empty() {
            return Ok(0);
        }

        let inputs = jobs.iter().map(|job| job.body.clone()).collect::<Vec<_>>();
        let embeddings = match request_embeddings(&self.http_client, &config, &inputs).await {
            Ok(embeddings) => embeddings,
            Err(error) => {
                let message = error.to_string();
                for job in &jobs {
                    let _ = self.history.fail_embedding_job(job, &message).await;
                }
                return Err(error);
            }
        };

        let index = crate::history::vector_index::VectorIndex::open(self.history.data_root());
        let mut completed = 0usize;
        for (job, embedding) in jobs.into_iter().zip(embeddings) {
            let Some(source_kind) =
                crate::history::vector_index::VectorSourceKind::from_embedding_source_kind(
                    &job.source_kind,
                )
            else {
                self.history
                    .fail_embedding_job(&job, "unsupported embedding source kind")
                    .await?;
                continue;
            };
            let upsert_result = index
                .upsert(crate::history::vector_index::VectorDocument {
                    source_kind,
                    source_id: job.source_id.clone(),
                    chunk_id: job.chunk_id.clone(),
                    title: job.title.clone(),
                    body: job.body.clone(),
                    workspace_id: job.workspace_id.clone(),
                    thread_id: job.thread_id.clone(),
                    agent_id: job.agent_id.clone(),
                    timestamp: job.source_timestamp,
                    embedding_model: embedding_model.clone(),
                    embedding,
                    metadata_json: Some(
                        serde_json::json!({ "content_hash": job.content_hash }).to_string(),
                    ),
                })
                .await;
            match upsert_result {
                Ok(()) => {
                    self.history
                        .complete_embedding_job(&job, &embedding_model, dimensions)
                        .await?;
                    completed += 1;
                }
                Err(error) => {
                    self.history
                        .fail_embedding_job(&job, &error.to_string())
                        .await?;
                }
            }
        }
        Ok(completed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_like_endpoint_appends_v1_once() {
        assert_eq!(
            openai_like_endpoint("https://api.openai.com/v1", "embeddings"),
            "https://api.openai.com/v1/embeddings"
        );
        assert_eq!(
            openai_like_endpoint("http://localhost:11434", "/embeddings"),
            "http://localhost:11434/v1/embeddings"
        );
    }

    #[test]
    fn embedding_batch_size_is_bounded() {
        assert_eq!(embedding_batch_size(0), 1);
        assert_eq!(embedding_batch_size(64), 64);
        assert_eq!(embedding_batch_size(10_000), MAX_EMBEDDING_BATCH_SIZE);
    }

    #[cfg(feature = "lancedb-vector")]
    #[test]
    fn vector_history_hits_are_rendered_as_protocol_hits() {
        let hits =
            vector_hits_to_history_hits(vec![crate::history::vector_index::VectorSearchHit {
                source_kind: crate::history::vector_index::VectorSourceKind::AgentMessage,
                source_id: "msg-1".to_string(),
                chunk_id: "0".to_string(),
                title: "user".to_string(),
                snippet: Some("body".to_string()),
                timestamp: Some(42),
                score: 0.25,
                metadata_json: None,
            }]);

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "msg-1");
        assert_eq!(hits[0].kind, "agent_message");
        assert_eq!(hits[0].excerpt, "body");
        assert!((hits[0].score - 0.8).abs() < f64::EPSILON);
    }
}
