use super::*;

const CLAIM_STALE_AFTER_SECS: i64 = 300;

#[derive(Debug, Clone)]
pub(crate) struct EmbeddingJobInput {
    pub source_kind: String,
    pub source_id: String,
    pub chunk_id: String,
    pub title: String,
    pub body: String,
    pub workspace_id: Option<String>,
    pub thread_id: Option<String>,
    pub agent_id: Option<String>,
    pub source_timestamp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EmbeddingJob {
    pub source_kind: String,
    pub source_id: String,
    pub chunk_id: String,
    pub content_hash: String,
    pub title: String,
    pub body: String,
    pub workspace_id: Option<String>,
    pub thread_id: Option<String>,
    pub agent_id: Option<String>,
    pub source_timestamp: i64,
    pub attempts: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EmbeddingDeletion {
    pub source_kind: String,
    pub source_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticBackfillResult {
    pub messages_queued: u64,
    pub tasks_queued: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticIndexStatus {
    pub queued_jobs: u64,
    pub pending_for_model: u64,
    pub completed_for_model: u64,
    pub queued_deletions: u64,
    pub failed_jobs: u64,
    pub failed_deletions: u64,
}

fn content_hash(body: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn enqueue_embedding_job_on_connection(
    connection: &Connection,
    job: &EmbeddingJobInput,
    now: i64,
) -> rusqlite::Result<()> {
    let body = job.body.trim();
    if body.is_empty() {
        return Ok(());
    }
    let title = job.title.trim();
    let content_hash = content_hash(body);
    connection.execute(
        "INSERT INTO embedding_jobs (
            source_kind,
            source_id,
            chunk_id,
            content_hash,
            title,
            body,
            workspace_id,
            thread_id,
            agent_id,
            source_timestamp,
            queued_at,
            updated_at,
            claimed_at,
            attempts,
            last_error
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, NULL, 0, NULL)
        ON CONFLICT(source_kind, source_id, chunk_id) DO UPDATE SET
            content_hash = excluded.content_hash,
            title = excluded.title,
            body = excluded.body,
            workspace_id = excluded.workspace_id,
            thread_id = excluded.thread_id,
            agent_id = excluded.agent_id,
            source_timestamp = excluded.source_timestamp,
            updated_at = excluded.updated_at,
            claimed_at = CASE
                WHEN embedding_jobs.content_hash = excluded.content_hash THEN embedding_jobs.claimed_at
                ELSE NULL
            END,
            attempts = CASE
                WHEN embedding_jobs.content_hash = excluded.content_hash THEN embedding_jobs.attempts
                ELSE 0
            END,
            last_error = CASE
                WHEN embedding_jobs.content_hash = excluded.content_hash THEN embedding_jobs.last_error
                ELSE NULL
            END",
        params![
            job.source_kind,
            job.source_id,
            job.chunk_id,
            content_hash,
            title,
            body,
            job.workspace_id,
            job.thread_id,
            job.agent_id,
            job.source_timestamp,
            now,
        ],
    )?;
    Ok(())
}

pub(super) fn enqueue_message_embedding_job(
    connection: &Connection,
    message: &AgentDbMessage,
    workspace_id: Option<&str>,
    now: i64,
) -> rusqlite::Result<()> {
    if message.content.trim().is_empty() {
        return queue_embedding_deletion_on_connection(
            connection,
            "agent_message",
            &message.id,
            now,
        );
    }
    enqueue_embedding_job_on_connection(
        connection,
        &EmbeddingJobInput {
            source_kind: "agent_message".to_string(),
            source_id: message.id.clone(),
            chunk_id: "0".to_string(),
            title: message.role.clone(),
            body: message.content.clone(),
            workspace_id: workspace_id.map(str::to_string),
            thread_id: Some(message.thread_id.clone()),
            agent_id: None,
            source_timestamp: message.created_at,
        },
        now,
    )
}

pub(super) fn enqueue_task_embedding_job(
    connection: &Connection,
    task: &AgentTask,
    now: i64,
) -> rusqlite::Result<()> {
    let body = match (task.title.trim(), task.description.trim()) {
        ("", "") => String::new(),
        (title, "") => title.to_string(),
        ("", description) => description.to_string(),
        (title, description) => format!("{title}\n\n{description}"),
    };
    if body.trim().is_empty() {
        return queue_embedding_deletion_on_connection(connection, "agent_task", &task.id, now);
    }
    enqueue_embedding_job_on_connection(
        connection,
        &EmbeddingJobInput {
            source_kind: "agent_task".to_string(),
            source_id: task.id.clone(),
            chunk_id: "0".to_string(),
            title: task.title.clone(),
            body,
            workspace_id: None,
            thread_id: task.thread_id.clone(),
            agent_id: None,
            source_timestamp: task.created_at as i64,
        },
        now,
    )
}

pub(super) fn queue_embedding_deletion_on_connection(
    connection: &Connection,
    source_kind: &str,
    source_id: &str,
    now: i64,
) -> rusqlite::Result<()> {
    connection.execute(
        "DELETE FROM embedding_jobs WHERE source_kind = ?1 AND source_id = ?2",
        params![source_kind, source_id],
    )?;
    connection.execute(
        "DELETE FROM embedding_job_completions WHERE source_kind = ?1 AND source_id = ?2",
        params![source_kind, source_id],
    )?;
    connection.execute(
        "INSERT INTO embedding_deletions (
            source_kind,
            source_id,
            queued_at,
            claimed_at,
            attempts,
            last_error
        ) VALUES (?1, ?2, ?3, NULL, 0, NULL)
        ON CONFLICT(source_kind, source_id) DO UPDATE SET
            queued_at = excluded.queued_at,
            claimed_at = NULL,
            last_error = NULL",
        params![source_kind, source_id, now],
    )?;
    Ok(())
}

impl HistoryStore {
    pub(crate) async fn enqueue_embedding_job(&self, job: EmbeddingJobInput) -> Result<()> {
        self.conn
            .call(move |conn| {
                enqueue_embedding_job_on_connection(conn, &job, now_ts() as i64)?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn claim_embedding_jobs(
        &self,
        embedding_model: &str,
        dimensions: u32,
        limit: usize,
    ) -> Result<Vec<EmbeddingJob>> {
        let embedding_model = embedding_model.trim().to_string();
        if embedding_model.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let limit = limit.min(512) as i64;
        let dimensions = dimensions as i64;
        self.conn
            .call(move |conn| {
                let transaction = conn.transaction()?;
                let now = now_ts() as i64;
                let stale_before = now.saturating_sub(CLAIM_STALE_AFTER_SECS);
                let jobs = {
                    let mut stmt = transaction.prepare(
                        "SELECT
                            source_kind,
                            source_id,
                            chunk_id,
                            content_hash,
                            title,
                            body,
                            workspace_id,
                            thread_id,
                            agent_id,
                            source_timestamp,
                            attempts
                        FROM embedding_jobs job
                        WHERE (job.claimed_at IS NULL OR job.claimed_at <= ?3)
                          AND NOT EXISTS (
                            SELECT 1
                            FROM embedding_job_completions completion
                            WHERE completion.source_kind = job.source_kind
                              AND completion.source_id = job.source_id
                              AND completion.chunk_id = job.chunk_id
                              AND completion.content_hash = job.content_hash
                              AND completion.embedding_model = ?1
                              AND completion.dimensions = ?2
                          )
                        ORDER BY job.updated_at ASC
                        LIMIT ?4",
                    )?;
                    let rows = stmt.query_map(
                        params![embedding_model, dimensions, stale_before, limit],
                        |row| {
                            Ok(EmbeddingJob {
                                source_kind: row.get(0)?,
                                source_id: row.get(1)?,
                                chunk_id: row.get(2)?,
                                content_hash: row.get(3)?,
                                title: row.get(4)?,
                                body: row.get(5)?,
                                workspace_id: row.get(6)?,
                                thread_id: row.get(7)?,
                                agent_id: row.get(8)?,
                                source_timestamp: row.get(9)?,
                                attempts: row.get(10)?,
                            })
                        },
                    )?;
                    rows.collect::<std::result::Result<Vec<_>, _>>()?
                };

                for job in &jobs {
                    transaction.execute(
                        "UPDATE embedding_jobs
                         SET claimed_at = ?4, attempts = attempts + 1
                         WHERE source_kind = ?1 AND source_id = ?2 AND chunk_id = ?3",
                        params![job.source_kind, job.source_id, job.chunk_id, now],
                    )?;
                }
                transaction.commit()?;
                Ok(jobs)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn complete_embedding_job(
        &self,
        job: &EmbeddingJob,
        embedding_model: &str,
        dimensions: u32,
    ) -> Result<()> {
        let job = job.clone();
        let embedding_model = embedding_model.trim().to_string();
        let dimensions = dimensions as i64;
        self.conn
            .call(move |conn| {
                let transaction = conn.transaction()?;
                transaction.execute(
                    "INSERT OR REPLACE INTO embedding_job_completions (
                        source_kind,
                        source_id,
                        chunk_id,
                        content_hash,
                        embedding_model,
                        dimensions,
                        completed_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        job.source_kind,
                        job.source_id,
                        job.chunk_id,
                        job.content_hash,
                        embedding_model,
                        dimensions,
                        now_ts() as i64,
                    ],
                )?;
                transaction.execute(
                    "UPDATE embedding_jobs
                     SET claimed_at = NULL, last_error = NULL
                     WHERE source_kind = ?1 AND source_id = ?2 AND chunk_id = ?3",
                    params![job.source_kind, job.source_id, job.chunk_id],
                )?;
                transaction.commit()?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn fail_embedding_job(&self, job: &EmbeddingJob, error: &str) -> Result<()> {
        let job = job.clone();
        let error = error.chars().take(2000).collect::<String>();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE embedding_jobs
                     SET claimed_at = ?4, last_error = ?5
                     WHERE source_kind = ?1 AND source_id = ?2 AND chunk_id = ?3",
                    params![
                        job.source_kind,
                        job.source_id,
                        job.chunk_id,
                        now_ts() as i64,
                        error
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn claim_embedding_deletions(
        &self,
        limit: usize,
    ) -> Result<Vec<EmbeddingDeletion>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let limit = limit.min(512) as i64;
        self.conn
            .call(move |conn| {
                let transaction = conn.transaction()?;
                let now = now_ts() as i64;
                let stale_before = now.saturating_sub(CLAIM_STALE_AFTER_SECS);
                let deletions = {
                    let mut stmt = transaction.prepare(
                        "SELECT source_kind, source_id
                         FROM embedding_deletions
                         WHERE claimed_at IS NULL OR claimed_at <= ?1
                         ORDER BY queued_at ASC
                         LIMIT ?2",
                    )?;
                    let rows = stmt.query_map(params![stale_before, limit], |row| {
                        Ok(EmbeddingDeletion {
                            source_kind: row.get(0)?,
                            source_id: row.get(1)?,
                        })
                    })?;
                    rows.collect::<std::result::Result<Vec<_>, _>>()?
                };
                for deletion in &deletions {
                    transaction.execute(
                        "UPDATE embedding_deletions
                         SET claimed_at = ?3, attempts = attempts + 1
                         WHERE source_kind = ?1 AND source_id = ?2",
                        params![deletion.source_kind, deletion.source_id, now],
                    )?;
                }
                transaction.commit()?;
                Ok(deletions)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn complete_embedding_deletion(
        &self,
        deletion: &EmbeddingDeletion,
    ) -> Result<()> {
        let deletion = deletion.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM embedding_deletions WHERE source_kind = ?1 AND source_id = ?2",
                    params![deletion.source_kind, deletion.source_id],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn fail_embedding_deletion(
        &self,
        deletion: &EmbeddingDeletion,
        error: &str,
    ) -> Result<()> {
        let deletion = deletion.clone();
        let error = error.chars().take(2000).collect::<String>();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE embedding_deletions
                     SET claimed_at = ?3, last_error = ?4
                     WHERE source_kind = ?1 AND source_id = ?2",
                    params![
                        deletion.source_kind,
                        deletion.source_id,
                        now_ts() as i64,
                        error
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn queue_semantic_backfill(
        &self,
        limit: Option<usize>,
    ) -> Result<SemanticBackfillResult> {
        let limit = limit.unwrap_or(usize::MAX).max(1);
        self.conn
            .call(move |conn| {
                let transaction = conn.transaction()?;
                let now = now_ts() as i64;
                let mut remaining = limit;
                let messages = {
                    let sql = if remaining == usize::MAX {
                        "SELECT message.id, message.thread_id, message.created_at, message.role, message.content, message.provider, message.model, message.input_tokens, message.output_tokens, message.total_tokens, message.cost_usd, message.reasoning, message.tool_calls_json, message.metadata_json, thread.workspace_id
                         FROM agent_messages message
                         LEFT JOIN agent_threads thread ON thread.id = message.thread_id
                         WHERE message.deleted_at IS NULL AND trim(message.content) <> ''
                         ORDER BY message.created_at ASC"
                            .to_string()
                    } else {
                        "SELECT message.id, message.thread_id, message.created_at, message.role, message.content, message.provider, message.model, message.input_tokens, message.output_tokens, message.total_tokens, message.cost_usd, message.reasoning, message.tool_calls_json, message.metadata_json, thread.workspace_id
                         FROM agent_messages message
                         LEFT JOIN agent_threads thread ON thread.id = message.thread_id
                         WHERE message.deleted_at IS NULL AND trim(message.content) <> ''
                         ORDER BY message.created_at ASC
                         LIMIT ?1"
                            .to_string()
                    };
                    let mut stmt = transaction.prepare(&sql)?;
                    let rows = if remaining == usize::MAX {
                        stmt.query_map([], |row| {
                            Ok((map_agent_message(row)?, row.get::<_, Option<String>>(14)?))
                        })?
                        .collect::<std::result::Result<Vec<_>, _>>()?
                    } else {
                        stmt.query_map(params![remaining as i64], |row| {
                            Ok((map_agent_message(row)?, row.get::<_, Option<String>>(14)?))
                        })?
                        .collect::<std::result::Result<Vec<_>, _>>()?
                    };
                    rows
                };
                let messages_queued = messages.len() as u64;
                remaining = remaining.saturating_sub(messages.len());
                for (message, workspace_id) in &messages {
                    enqueue_message_embedding_job(
                        &transaction,
                        message,
                        workspace_id.as_deref(),
                        now,
                    )?;
                }

                let tasks = if remaining == 0 {
                    Vec::new()
                } else {
                    let sql = if remaining == usize::MAX {
                        "SELECT id, title, description, created_at, thread_id
                         FROM agent_tasks
                         WHERE deleted_at IS NULL AND trim(title || char(10) || description) <> ''
                         ORDER BY created_at ASC"
                            .to_string()
                    } else {
                        "SELECT id, title, description, created_at, thread_id
                         FROM agent_tasks
                         WHERE deleted_at IS NULL AND trim(title || char(10) || description) <> ''
                         ORDER BY created_at ASC
                         LIMIT ?1"
                            .to_string()
                    };
                    let mut stmt = transaction.prepare(&sql)?;
                    if remaining == usize::MAX {
                        stmt.query_map([], |row| {
                            Ok(EmbeddingJobInput {
                                source_kind: "agent_task".to_string(),
                                source_id: row.get(0)?,
                                chunk_id: "0".to_string(),
                                title: row.get(1)?,
                                body: format!(
                                    "{}\n\n{}",
                                    row.get::<_, String>(1)?,
                                    row.get::<_, String>(2)?
                                ),
                                workspace_id: None,
                                thread_id: row.get(4)?,
                                agent_id: None,
                                source_timestamp: row.get(3)?,
                            })
                        })?
                        .collect::<std::result::Result<Vec<_>, _>>()?
                    } else {
                        stmt.query_map(params![remaining as i64], |row| {
                            Ok(EmbeddingJobInput {
                                source_kind: "agent_task".to_string(),
                                source_id: row.get(0)?,
                                chunk_id: "0".to_string(),
                                title: row.get(1)?,
                                body: format!(
                                    "{}\n\n{}",
                                    row.get::<_, String>(1)?,
                                    row.get::<_, String>(2)?
                                ),
                                workspace_id: None,
                                thread_id: row.get(4)?,
                                agent_id: None,
                                source_timestamp: row.get(3)?,
                            })
                        })?
                        .collect::<std::result::Result<Vec<_>, _>>()?
                    }
                };
                let tasks_queued = tasks.len() as u64;
                for task in &tasks {
                    enqueue_embedding_job_on_connection(&transaction, task, now)?;
                }

                transaction.commit()?;
                Ok(SemanticBackfillResult {
                    messages_queued,
                    tasks_queued,
                })
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn semantic_index_status(
        &self,
        embedding_model: &str,
        dimensions: u32,
    ) -> Result<SemanticIndexStatus> {
        let embedding_model = embedding_model.trim().to_string();
        let dimensions = dimensions as i64;
        self.read_conn
            .call(move |conn| {
                let queued_jobs = count_i64(conn, "SELECT COUNT(*) FROM embedding_jobs", [])?;
                let queued_deletions =
                    count_i64(conn, "SELECT COUNT(*) FROM embedding_deletions", [])?;
                let failed_jobs = count_i64(
                    conn,
                    "SELECT COUNT(*) FROM embedding_jobs WHERE last_error IS NOT NULL",
                    [],
                )?;
                let failed_deletions = count_i64(
                    conn,
                    "SELECT COUNT(*) FROM embedding_deletions WHERE last_error IS NOT NULL",
                    [],
                )?;
                let completed_for_model = if embedding_model.is_empty() {
                    0
                } else {
                    count_i64(
                        conn,
                        "SELECT COUNT(*) FROM embedding_job_completions
                         WHERE embedding_model = ?1 AND dimensions = ?2",
                        params![embedding_model, dimensions],
                    )?
                };
                let pending_for_model = if embedding_model.is_empty() {
                    0
                } else {
                    count_i64(
                        conn,
                        "SELECT COUNT(*)
                         FROM embedding_jobs job
                         WHERE NOT EXISTS (
                            SELECT 1
                            FROM embedding_job_completions completion
                            WHERE completion.source_kind = job.source_kind
                              AND completion.source_id = job.source_id
                              AND completion.chunk_id = job.chunk_id
                              AND completion.content_hash = job.content_hash
                              AND completion.embedding_model = ?1
                              AND completion.dimensions = ?2
                         )",
                        params![embedding_model, dimensions],
                    )?
                };
                Ok(SemanticIndexStatus {
                    queued_jobs,
                    pending_for_model,
                    completed_for_model,
                    queued_deletions,
                    failed_jobs,
                    failed_deletions,
                })
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

fn count_i64<P>(conn: &Connection, sql: &str, params: P) -> rusqlite::Result<u64>
where
    P: rusqlite::Params,
{
    let count = conn.query_row(sql, params, |row| row.get::<_, i64>(0))?;
    Ok(count.max(0) as u64)
}
