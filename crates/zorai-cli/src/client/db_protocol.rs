use serde::Deserialize;

/// Commands for the database bridge (JSON over stdin).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub(super) enum DbBridgeCommand {
    AppendCommandLog {
        entry_json: String,
    },
    CompleteCommandLog {
        id: String,
        exit_code: Option<i32>,
        duration_ms: Option<i64>,
    },
    QueryCommandLog {
        workspace_id: Option<String>,
        pane_id: Option<String>,
        limit: Option<usize>,
    },
    ClearCommandLog,
    CreateAgentThread {
        thread_json: String,
    },
    DeleteAgentThread {
        thread_id: String,
    },
    ListAgentThreads,
    GetAgentThread {
        thread_id: String,
        #[serde(default, alias = "trashed")]
        include_deleted: bool,
    },
    AddAgentMessage {
        message_json: String,
    },
    DeleteAgentMessages {
        thread_id: String,
        message_ids: Vec<String>,
    },
    RestoreAgentMessages {
        thread_id: String,
        message_ids: Vec<String>,
    },
    ListAgentMessages {
        thread_id: String,
        limit: Option<usize>,
        #[serde(default, alias = "trashed")]
        include_deleted: bool,
    },
    UpsertTranscriptIndex {
        entry_json: String,
    },
    ListTranscriptIndex {
        workspace_id: Option<String>,
    },
    UpsertSnapshotIndex {
        entry_json: String,
    },
    ListSnapshotIndex {
        workspace_id: Option<String>,
    },
    UpsertAgentEvent {
        event_json: String,
    },
    ListAgentEvents {
        category: Option<String>,
        pane_id: Option<String>,
        limit: Option<usize>,
    },
    ListDatabaseTables,
    QueryDatabaseRows {
        table_name: String,
        offset: usize,
        limit: usize,
        #[serde(default)]
        sort_column: Option<String>,
        #[serde(default)]
        sort_direction: Option<String>,
    },
    UpdateDatabaseRows {
        table_name: String,
        updates_json: String,
    },
    ExecuteDatabaseSql {
        sql: String,
    },
    QueueSemanticBackfill {
        limit: Option<usize>,
    },
    GetSemanticIndexStatus {
        embedding_model: String,
        dimensions: u32,
    },
    Shutdown,
}

#[cfg(test)]
mod tests {
    use super::DbBridgeCommand;

    #[test]
    fn parses_database_row_sort_options_from_electron_payload() {
        let command = serde_json::from_str::<DbBridgeCommand>(
            r#"{"type":"query-database-rows","table_name":"agent_messages","offset":0,"limit":100,"sort_column":"created_at","sort_direction":"desc"}"#,
        )
        .expect("database row query should parse");

        match command {
            DbBridgeCommand::QueryDatabaseRows {
                table_name,
                offset,
                limit,
                sort_column,
                sort_direction,
            } => {
                assert_eq!(table_name, "agent_messages");
                assert_eq!(offset, 0);
                assert_eq!(limit, 100);
                assert_eq!(sort_column.as_deref(), Some("created_at"));
                assert_eq!(sort_direction.as_deref(), Some("desc"));
            }
            other => panic!("expected query database rows command, got {other:?}"),
        }
    }

    #[test]
    fn parses_semantic_index_bridge_commands() {
        let backfill = serde_json::from_str::<DbBridgeCommand>(
            r#"{"type":"queue-semantic-backfill","limit":250}"#,
        )
        .expect("semantic backfill command should parse");
        assert!(matches!(
            backfill,
            DbBridgeCommand::QueueSemanticBackfill { limit: Some(250) }
        ));

        let status = serde_json::from_str::<DbBridgeCommand>(
            r#"{"type":"get-semantic-index-status","embedding_model":"text-embedding-3-small","dimensions":1536}"#,
        )
        .expect("semantic status command should parse");
        match status {
            DbBridgeCommand::GetSemanticIndexStatus {
                embedding_model,
                dimensions,
            } => {
                assert_eq!(embedding_model, "text-embedding-3-small");
                assert_eq!(dimensions, 1536);
            }
            other => panic!("expected semantic status command, got {other:?}"),
        }
    }

    #[test]
    fn parses_execute_database_sql_command() {
        let command = serde_json::from_str::<DbBridgeCommand>(
            r#"{"type":"execute-database-sql","sql":"SELECT 1"}"#,
        )
        .expect("execute database sql command should parse");

        match command {
            DbBridgeCommand::ExecuteDatabaseSql { sql } => {
                assert_eq!(sql, "SELECT 1");
            }
            other => panic!("expected execute database sql command, got {other:?}"),
        }
    }
}
