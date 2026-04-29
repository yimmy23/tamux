#![allow(dead_code)]

use super::*;

async fn apply_sqlite_connection_pragmas(
    conn: &tokio_rusqlite::Connection,
    query_only: bool,
) -> Result<()> {
    conn.call(move |conn| {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "wal_autocheckpoint", "1000")?;
        conn.pragma_update(None, "busy_timeout", "5000")?;
        conn.pragma_update(None, "query_only", if query_only { "ON" } else { "OFF" })?;
        Ok(())
    })
    .await
    .context("failed to apply SQLite connection pragmas")?;
    Ok(())
}

impl HistoryStore {
    async fn open_for_root(root: &Path) -> Result<Self> {
        let history_dir = root.join("history");
        let skill_dir = root.join("skills").join("generated");
        let telemetry_dir = root.join("semantic-logs");
        let worm_dir = telemetry_dir.join("worm");

        std::fs::create_dir_all(&history_dir)?;
        std::fs::create_dir_all(&skill_dir)?;
        std::fs::create_dir_all(&telemetry_dir)?;
        std::fs::create_dir_all(&worm_dir)?;

        let db_path = history_dir.join("command-history.db");
        let conn = tokio_rusqlite::Connection::open(&db_path)
            .await
            .context("failed to open SQLite connection via tokio-rusqlite")?;
        apply_sqlite_connection_pragmas(&conn, false).await?;

        let offloaded_payloads_dir = root.join("offloaded-payloads");
        conn.call(move |connection| {
            Ok(super::schema::init_schema_on_connection(
                connection,
                &offloaded_payloads_dir,
            )?)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

        let read_conn = tokio_rusqlite::Connection::open(&db_path)
            .await
            .context("failed to open read SQLite connection via tokio-rusqlite")?;
        apply_sqlite_connection_pragmas(&read_conn, true).await?;

        let store = Self {
            conn,
            read_conn,
            skill_dir,
            telemetry_dir,
            worm_dir,
        };
        Ok(store)
    }

    pub fn data_dir(&self) -> &Path {
        self.skill_dir.parent().unwrap_or(self.skill_dir.as_path())
    }

    pub(crate) fn data_root(&self) -> &Path {
        self.skill_dir
            .parent()
            .and_then(Path::parent)
            .unwrap_or(self.skill_dir.as_path())
    }

    pub(crate) fn offloaded_payloads_dir(&self) -> PathBuf {
        self.data_root().join("offloaded-payloads")
    }

    pub(crate) fn offloaded_payload_path(&self, thread_id: &str, payload_id: &str) -> PathBuf {
        self.offloaded_payloads_dir()
            .join(thread_id)
            .join(format!("{payload_id}.txt"))
    }

    pub(crate) fn tool_output_preview_path(
        &self,
        thread_id: &str,
        goal_run_id: Option<&str>,
        tool_name: &str,
        timestamp: u64,
    ) -> PathBuf {
        let safe_tool_name = sanitize_tool_output_segment(tool_name);
        let preview_dir = zorai_protocol::thread_previews_dir(self.data_root(), thread_id);
        match goal_run_id {
            Some(goal_run_id) => preview_dir.join(format!(
                "{}-{}-{}.txt",
                safe_tool_name,
                sanitize_tool_output_segment(goal_run_id),
                timestamp
            )),
            None => preview_dir.join(format!("{safe_tool_name}-{timestamp}.txt")),
        }
    }

    pub async fn new() -> Result<Self> {
        let base = zorai_protocol::ensure_zorai_data_dir()?;
        Self::open_for_root(&base).await
    }

    pub async fn list_agent_config_items(&self) -> Result<Vec<(String, serde_json::Value)>> {
        self.read_conn.call(|conn| {
            let mut stmt = conn.prepare(
                "SELECT key_path, value_json FROM agent_config_items WHERE deleted_at IS NULL ORDER BY length(key_path) ASC, key_path ASC",
            )?;
            let rows = stmt.query_map([], |row| {
                let key_path = row.get::<_, String>(0)?;
                let value_json = row.get::<_, String>(1)?;
                Ok((key_path, value_json))
            })?;
            let mut items = Vec::new();
            for row in rows {
                let (key_path, value_json) = row?;
                let value = serde_json::from_str::<serde_json::Value>(&value_json)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                items.push((key_path, value));
            }
            Ok(items)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn replace_agent_config_items(
        &self,
        items: &[(String, serde_json::Value)],
    ) -> Result<()> {
        let items: Vec<(String, String)> = items
            .iter()
            .map(|(k, v)| Ok((k.clone(), serde_json::to_string(v)?)))
            .collect::<Result<Vec<_>>>()?;
        let items = items.clone();
        self.conn.call(move |conn| {
            let transaction = conn.unchecked_transaction()?;
            let now = now_ts() as i64;
            transaction.execute(
                "UPDATE agent_config_items SET deleted_at = ?1 WHERE deleted_at IS NULL",
                params![now],
            )?;
            for (key_path, value_json) in &items {
                transaction.execute(
                    "INSERT OR REPLACE INTO agent_config_items (key_path, value_json, updated_at, deleted_at) VALUES (?1, ?2, ?3, NULL)",
                    params![key_path, value_json, now],
                )?;
            }
            transaction.commit()?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_agent_config_item(
        &self,
        key_path: &str,
        value: &serde_json::Value,
    ) -> Result<()> {
        let key_path = key_path.to_string();
        let value_json = serde_json::to_string(value)?;
        let _value = value.clone();
        self.conn.call(move |conn| {
            let transaction = conn.unchecked_transaction()?;
            let prefix = format!("{key_path}/%");
            let now = now_ts() as i64;
            transaction.execute(
                "UPDATE agent_config_items SET deleted_at = ?3 \
                 WHERE deleted_at IS NULL AND (key_path = ?1 OR key_path LIKE ?2 OR ?1 LIKE key_path || '/%')",
                params![key_path, prefix, now],
            )?;
            transaction.execute(
                "INSERT OR REPLACE INTO agent_config_items (key_path, value_json, updated_at, deleted_at) VALUES (?1, ?2, ?3, NULL)",
                params![key_path, value_json.clone(), now],
            )?;
            transaction.execute(
                "INSERT INTO agent_config_updates (id, key_path, value_json, updated_at) VALUES (?1, ?2, ?3, ?4)",
                params![uuid::Uuid::new_v4().to_string(), key_path, value_json, now],
            )?;
            transaction.commit()?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn load_provider_auth_state(
        &self,
        provider_id: &str,
        auth_mode: &str,
    ) -> Result<Option<ProviderAuthStateRow>> {
        let provider_id = provider_id.to_string();
        let auth_mode = auth_mode.to_string();
        self.read_conn
            .call(move |conn| {
                let row = conn
                    .query_row(
                        "SELECT provider_id, auth_mode, state_json, updated_at
                         FROM provider_auth_state
                         WHERE provider_id = ?1 AND auth_mode = ?2 AND deleted_at IS NULL",
                        params![provider_id, auth_mode],
                        |row| {
                            let state_json = row.get::<_, String>(2)?;
                            let parsed = serde_json::from_str::<serde_json::Value>(&state_json)
                                .map_err(|e| {
                                    rusqlite::Error::ToSqlConversionFailure(Box::new(e))
                                })?;
                            Ok(ProviderAuthStateRow {
                                provider_id: row.get(0)?,
                                auth_mode: row.get(1)?,
                                state_json: parsed,
                                updated_at: row.get(3)?,
                            })
                        },
                    )
                    .optional()?;
                Ok(row)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn save_provider_auth_state(
        &self,
        provider_id: &str,
        auth_mode: &str,
        state: &serde_json::Value,
    ) -> Result<()> {
        let provider_id = provider_id.to_string();
        let auth_mode = auth_mode.to_string();
        let state_json = serde_json::to_string(state)?;
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO provider_auth_state
                     (provider_id, auth_mode, state_json, updated_at, deleted_at)
                     VALUES (?1, ?2, ?3, ?4, NULL)",
                    params![provider_id, auth_mode, state_json, now_ts() as i64],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_provider_auth_state(
        &self,
        provider_id: &str,
        auth_mode: &str,
    ) -> Result<()> {
        let provider_id = provider_id.to_string();
        let auth_mode = auth_mode.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE provider_auth_state SET deleted_at = ?3 WHERE provider_id = ?1 AND auth_mode = ?2 AND deleted_at IS NULL",
                    params![provider_id, auth_mode, now_ts() as i64],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn new_test_store(root: &Path) -> Result<Self> {
        Self::open_for_root(root).await
    }

    pub(crate) async fn open_for_data_root(root: &Path) -> Result<Self> {
        Self::open_for_root(root).await
    }

    pub async fn record_managed_finish(&self, record: &ManagedHistoryRecord) -> Result<()> {
        let timestamp = now_ts() as i64;
        let excerpt = format!(
            "exit={:?} duration_ms={:?} snapshot={} rationale={}",
            record.exit_code,
            record.duration_ms,
            record.snapshot_path.as_deref().unwrap_or("none"),
            record.rationale
        );

        let execution_id = record.execution_id.clone();
        let command = record.command.clone();
        let rationale = record.rationale.clone();
        let snapshot_path = record.snapshot_path.clone();
        let excerpt_clone = excerpt.clone();
        let record = record;
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO history_entries (id, kind, title, excerpt, content, path, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    execution_id,
                    "managed-command",
                    command,
                    excerpt_clone,
                    format!("{}\n{}", command, rationale),
                    snapshot_path,
                    timestamp,
                ],
            )?;
            conn.execute(
                "INSERT OR REPLACE INTO history_fts (id, title, excerpt, content) VALUES (?1, ?2, ?3, ?4)",
                params![execution_id, command, excerpt_clone, rationale],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

        self.append_telemetry(
            "operational",
            json!({
                "timestamp": timestamp,
                "execution_id": record.execution_id,
                "session_id": record.session_id,
                "workspace_id": record.workspace_id,
                "command": record.command,
                "exit_code": record.exit_code,
                "duration_ms": record.duration_ms,
                "snapshot": record.snapshot_path,
            }),
        )
        .await?;
        self.append_telemetry(
            "cognitive",
            json!({
                "timestamp": timestamp,
                "execution_id": record.execution_id,
                "source": record.source,
                "rationale": record.rationale,
            }),
        )
        .await?;

        let mut system = System::new_all();
        system.refresh_memory();
        system.refresh_cpu();
        self.append_telemetry(
            "contextual",
            json!({
                "timestamp": timestamp,
                "execution_id": record.execution_id,
                "total_memory": system.total_memory(),
                "used_memory": system.used_memory(),
                "cpu_usage": system.global_cpu_info().cpu_usage(),
            }),
        )
        .await?;

        Ok(())
    }

    pub async fn get_managed_finish(
        &self,
        execution_id: &str,
    ) -> Result<Option<ManagedCommandFinishedRecord>> {
        let execution_id = execution_id.to_string();
        self.read_conn
            .call(move |conn| {
                Ok(conn
                    .query_row(
                    "SELECT title, excerpt, path FROM history_entries WHERE id = ?1 AND kind = 'managed-command'",
                    params![execution_id],
                    |row| {
                        let command: String = row.get(0)?;
                        let excerpt: String = row.get(1)?;
                        let snapshot_path: Option<String> = row.get(2)?;
                        let (exit_code, duration_ms) = parse_managed_history_excerpt(&excerpt);
                        Ok(ManagedCommandFinishedRecord {
                            command,
                            exit_code,
                            duration_ms,
                            snapshot_path,
                        })
                    },
                )
                .optional()?)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<(String, Vec<HistorySearchHit>)> {
        self.search_with_sqlite_fts(query, limit).await
    }

    async fn search_with_sqlite_fts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<(String, Vec<HistorySearchHit>)> {
        let query = query.to_string();
        self.read_conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT history_entries.id, history_entries.kind, history_entries.title, history_entries.excerpt, history_entries.path, history_entries.timestamp, bm25(history_fts) \
                 FROM history_fts JOIN history_entries ON history_entries.id = history_fts.id \
                 WHERE history_fts MATCH ?1 ORDER BY bm25(history_fts) LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![query, limit as i64], |row| {
                Ok(HistorySearchHit {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    title: row.get(2)?,
                    excerpt: row.get(3)?,
                    path: row.get(4)?,
                    timestamp: row.get::<_, i64>(5)? as u64,
                    score: row.get(6)?,
                })
            })?;

            let hits = rows.filter_map(|row| row.ok()).collect::<Vec<_>>();
            let summary = if hits.is_empty() {
                format!("No prior runs matched '{query}'.")
            } else {
                format!("Found {} historical matches for '{query}'.", hits.len())
            };
            Ok((summary, hits))
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn generate_skill(
        &self,
        query: Option<&str>,
        title: Option<&str>,
    ) -> Result<(String, String)> {
        let title = title.unwrap_or("Recovered Workflow").trim();
        let (summary, hits) = self
            .search(query.unwrap_or("*"), 8)
            .await
            .unwrap_or_else(|_| ("No history available.".to_string(), Vec::new()));
        let safe_name = title
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_ascii_lowercase();
        let path = self.skill_dir.join(format!(
            "{}.md",
            if safe_name.is_empty() {
                "recovered-workflow"
            } else {
                &safe_name
            }
        ));
        let mut body = format!(
            "# {}\n\n## Summary\n{}\n\n## Retrieved Steps\n",
            title, summary
        );
        for hit in &hits {
            body.push_str(&format!("- {}\n", hit.title));
            body.push_str(&format!("  {}\n", hit.excerpt));
        }
        if hits.is_empty() {
            body.push_str("- No matching executions were available.\n");
        }
        std::fs::write(&path, body)
            .with_context(|| format!("failed to write {}", path.display()))?;
        self.register_skill_document(&path).await?;
        Ok((title.to_string(), path.to_string_lossy().into_owned()))
    }
}

fn sanitize_tool_output_segment(raw: &str) -> String {
    raw.chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => ch,
            _ => '_',
        })
        .collect()
}

fn parse_managed_history_excerpt(excerpt: &str) -> (Option<i32>, Option<u64>) {
    let Some(rest) = excerpt.strip_prefix("exit=") else {
        return (None, None);
    };
    let Some((exit_raw, remaining)) = rest.split_once(" duration_ms=") else {
        return (None, None);
    };
    let duration_raw = remaining
        .split_once(" snapshot=")
        .map(|(value, _)| value)
        .unwrap_or(remaining);

    (
        parse_debug_option_i32(exit_raw),
        parse_debug_option_u64(duration_raw),
    )
}

fn parse_debug_option_i32(raw: &str) -> Option<i32> {
    raw.strip_prefix("Some(")
        .and_then(|value| value.strip_suffix(')'))
        .and_then(|value| value.parse::<i32>().ok())
}

fn parse_debug_option_u64(raw: &str) -> Option<u64> {
    raw.strip_prefix("Some(")
        .and_then(|value| value.strip_suffix(')'))
        .and_then(|value| value.parse::<u64>().ok())
}
