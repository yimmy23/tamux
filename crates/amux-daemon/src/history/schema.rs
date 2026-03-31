use super::*;

use super::schema_migrations::{apply_schema_migrations, ensure_context_archive_fts};
use super::schema_sql::base_schema_sql;
use super::schema_sql_extra::extended_schema_sql;

impl HistoryStore {
    pub(super) async fn init_schema(&self) -> Result<()> {
        self.conn
            .call(|connection| {
                let schema_sql = format!("{}{}", base_schema_sql(), extended_schema_sql());
                connection.execute_batch(&schema_sql)?;
                ensure_context_archive_fts(connection);
                apply_schema_migrations(connection)?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
