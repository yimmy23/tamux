use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DatabaseTableSummary {
    pub name: String,
    pub table_type: String,
    pub row_count: Option<u64>,
    pub editable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DatabaseColumnInfo {
    pub name: String,
    pub declared_type: String,
    pub nullable: bool,
    pub primary_key: bool,
    pub editable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DatabaseRow {
    pub rowid: Option<i64>,
    pub values: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DatabaseTablePage {
    pub table_name: String,
    pub columns: Vec<DatabaseColumnInfo>,
    pub rows: Vec<DatabaseRow>,
    pub total_rows: u64,
    pub offset: usize,
    pub limit: usize,
    pub editable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DatabaseRowUpdate {
    pub rowid: i64,
    pub values: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DatabaseSqlResult {
    pub columns: Vec<String>,
    pub rows: Vec<BTreeMap<String, serde_json::Value>>,
    pub rows_affected: usize,
    pub statement_count: usize,
    pub message: String,
}

struct TableSchema {
    name: String,
    table_type: String,
    editable: bool,
    columns: Vec<DatabaseColumnInfo>,
}

impl HistoryStore {
    pub async fn list_database_tables(&self) -> Result<Vec<DatabaseTableSummary>> {
        let table_rows = self
            .read_db
            .query(
                "SELECT name, type FROM sqlite_master \
                 WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%' \
                 ORDER BY type ASC, name ASC",
                db::Params::None,
            )
            .await?;
        let names = table_rows
            .iter()
            .map(|row| Ok((row.get::<String>(0)?, row.get::<String>(1)?)))
            .collect::<Result<Vec<(String, String)>>>()?;
        let mut tables = Vec::with_capacity(names.len());
        for (name, table_type) in names {
            let quoted = quote_identifier(&name);
            let row_count = self
                .read_db
                .query_opt(&format!("SELECT COUNT(*) FROM {quoted}"), db::Params::None)
                .await
                .ok()
                .flatten()
                .and_then(|row| row.get::<i64>(0).ok())
                .and_then(|count| u64::try_from(count).ok());
            let editable = table_type == "table" && table_has_rowid(&*self.read_db, &name).await;
            tables.push(DatabaseTableSummary {
                editable,
                name,
                table_type,
                row_count,
            });
        }
        Ok(tables)
    }

    pub async fn query_database_table_rows(
        &self,
        table_name: &str,
        offset: usize,
        limit: usize,
        sort_column: Option<&str>,
        sort_direction: Option<&str>,
    ) -> Result<DatabaseTablePage> {
        let limit = limit.clamp(1, 500);
        let schema = load_table_schema(&*self.read_db, table_name).await?;
        let quoted_table = quote_identifier(&schema.name);
        let total_rows = self
            .read_db
            .query_opt(
                &format!("SELECT COUNT(*) FROM {quoted_table}"),
                db::Params::None,
            )
            .await
            .ok()
            .flatten()
            .and_then(|row| row.get::<i64>(0).ok())
            .and_then(|count| u64::try_from(count).ok())
            .unwrap_or(0);
        let column_list = schema
            .columns
            .iter()
            .map(|column| quote_identifier(&column.name))
            .collect::<Vec<_>>()
            .join(", ");
        let order_by = build_order_clause(&schema, sort_column, sort_direction)?;
        let select_sql = if schema.editable {
            format!(
                "SELECT rowid AS __zorai_rowid, {column_list} FROM {quoted_table}{order_by} LIMIT ?1 OFFSET ?2"
            )
        } else {
            format!("SELECT {column_list} FROM {quoted_table}{order_by} LIMIT ?1 OFFSET ?2")
        };
        let result_rows = self
            .read_db
            .query(&select_sql, db::db_params![limit as i64, offset as i64])
            .await?;
        let value_offset = usize::from(schema.editable);
        let mut rows = Vec::with_capacity(result_rows.len());
        for row in result_rows.iter() {
            let rowid = if schema.editable {
                Some(row.get::<i64>(0)?)
            } else {
                None
            };
            let cells = row.values();
            let mut values = BTreeMap::new();
            for (index, column) in schema.columns.iter().enumerate() {
                let value = cells
                    .get(index + value_offset)
                    .map(db_value_to_json)
                    .unwrap_or(serde_json::Value::Null);
                values.insert(column.name.clone(), value);
            }
            rows.push(DatabaseRow { rowid, values });
        }
        Ok(DatabaseTablePage {
            table_name: schema.name,
            columns: schema.columns,
            rows,
            total_rows,
            offset,
            limit,
            editable: schema.editable,
        })
    }

    pub async fn update_database_table_rows(
        &self,
        table_name: &str,
        updates: Vec<DatabaseRowUpdate>,
    ) -> Result<usize> {
        let schema = load_table_schema(&*self.conn_db, table_name).await?;
        if !schema.editable {
            anyhow::bail!("table is not editable: {}", schema.name);
        }
        let known_columns = schema
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<BTreeSet<_>>();
        let quoted_table = quote_identifier(&schema.name);
        let mut txn = self.conn_db.transaction().await?;
        let mut changed_rows = 0u64;
        for update in updates {
            if update.values.is_empty() {
                continue;
            }
            for column in update.values.keys() {
                if !known_columns.contains(column.as_str()) {
                    anyhow::bail!("unknown column: {column}");
                }
            }
            let assignments = update
                .values
                .keys()
                .map(|column| format!("{} = ?", quote_identifier(column)))
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!("UPDATE {quoted_table} SET {assignments} WHERE rowid = ?");
            let mut bind_values = update
                .values
                .values()
                .map(json_to_db_value)
                .collect::<Vec<_>>();
            bind_values.push(db::Value::Integer(update.rowid));
            changed_rows += txn
                .execute(&sql, db::Params::Positional(bind_values))
                .await?;
        }
        txn.commit().await?;
        Ok(changed_rows as usize)
    }

    pub async fn execute_database_sql(&self, sql: &str) -> Result<DatabaseSqlResult> {
        let sql = sql.trim().to_string();
        if sql.is_empty() {
            anyhow::bail!("SQL query is empty");
        }
        let statement_count = count_sql_statements(&sql);
        if statement_count == 1 {
            // A single statement is prepared+stepped via `query_with_columns`,
            // which executes it exactly once regardless of whether it returns
            // rows. SELECT-like statements report named columns; mutating
            // statements report empty columns and we surface the change count.
            let before_changes = self.conn_db.total_changes().await?;
            let (columns, result_rows) = self
                .conn_db
                .query_with_columns(&sql, db::Params::None)
                .await?;
            if !columns.is_empty() {
                let rows = result_rows
                    .iter()
                    .map(|row| {
                        let cells = row.values();
                        columns
                            .iter()
                            .enumerate()
                            .map(|(index, column)| {
                                let value = cells
                                    .get(index)
                                    .map(db_value_to_json)
                                    .unwrap_or(serde_json::Value::Null);
                                (column.clone(), value)
                            })
                            .collect::<BTreeMap<String, serde_json::Value>>()
                    })
                    .collect::<Vec<_>>();
                let message = format!(
                    "Returned {} row{}.",
                    rows.len(),
                    if rows.len() == 1 { "" } else { "s" },
                );
                return Ok(DatabaseSqlResult {
                    columns,
                    rows,
                    rows_affected: 0,
                    statement_count,
                    message,
                });
            }
            let rows_affected = self
                .conn_db
                .total_changes()
                .await?
                .saturating_sub(before_changes) as usize;
            return Ok(DatabaseSqlResult {
                columns: Vec::new(),
                rows: Vec::new(),
                rows_affected,
                statement_count,
                message: format!(
                    "Executed 1 statement; {} row{} affected.",
                    rows_affected,
                    if rows_affected == 1 { "" } else { "s" },
                ),
            });
        }

        let before_changes = self.conn_db.total_changes().await?;
        self.conn_db.execute_batch(&sql).await?;
        let rows_affected = self
            .conn_db
            .total_changes()
            .await?
            .saturating_sub(before_changes) as usize;
        Ok(DatabaseSqlResult {
            columns: Vec::new(),
            rows: Vec::new(),
            rows_affected,
            statement_count,
            message: format!(
                "Executed {statement_count} statements; {} row{} affected.",
                rows_affected,
                if rows_affected == 1 { "" } else { "s" },
            ),
        })
    }
}

fn count_sql_statements(sql: &str) -> usize {
    sql.split(';')
        .filter(|part| !part.trim().is_empty())
        .count()
        .max(1)
}

async fn load_table_schema(conn: &dyn db::DbConn, table_name: &str) -> Result<TableSchema> {
    let table_type: String = conn
        .query_opt(
            "SELECT type FROM sqlite_master WHERE type IN ('table', 'view') AND name = ?1 AND name NOT LIKE 'sqlite_%'",
            db::db_params![table_name],
        )
        .await?
        .map(|row| row.get::<String>(0))
        .transpose()?
        .ok_or_else(|| anyhow::anyhow!("unknown table: {table_name}"))?;
    let editable = table_type == "table" && table_has_rowid(conn, table_name).await;
    let quoted_table = quote_identifier(table_name);
    let rows = conn
        .query(
            &format!("PRAGMA table_info({quoted_table})"),
            db::Params::None,
        )
        .await?;
    let columns = rows
        .iter()
        .map(|row| {
            let not_null = row.get::<i64>(3)?;
            Ok(DatabaseColumnInfo {
                name: row.get(1)?,
                declared_type: row.get(2)?,
                nullable: not_null == 0,
                primary_key: row.get::<i64>(5)? > 0,
                editable,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    if columns.is_empty() {
        anyhow::bail!("table has no visible columns: {table_name}");
    }
    Ok(TableSchema {
        name: table_name.to_string(),
        table_type,
        editable,
        columns,
    })
}

async fn table_has_rowid(conn: &dyn db::DbConn, table_name: &str) -> bool {
    conn.query(
        &format!("SELECT rowid FROM {} LIMIT 0", quote_identifier(table_name)),
        db::Params::None,
    )
    .await
    .is_ok()
}

fn build_order_clause(
    schema: &TableSchema,
    sort_column: Option<&str>,
    sort_direction: Option<&str>,
) -> Result<String> {
    let Some(sort_column) = sort_column.filter(|column| !column.trim().is_empty()) else {
        return Ok(String::new());
    };
    if !schema
        .columns
        .iter()
        .any(|column| column.name == sort_column)
    {
        anyhow::bail!("unknown sort column: {sort_column}");
    }
    let direction = match sort_direction
        .unwrap_or("desc")
        .to_ascii_lowercase()
        .as_str()
    {
        "asc" => "ASC",
        "desc" => "DESC",
        other => anyhow::bail!("unknown sort direction: {other}"),
    };
    let tie_breaker = if schema.editable { ", rowid ASC" } else { "" };
    Ok(format!(
        " ORDER BY {} {direction}{tie_breaker}",
        quote_identifier(sort_column)
    ))
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn db_value_to_json(value: &db::Value) -> serde_json::Value {
    match value {
        db::Value::Null => serde_json::Value::Null,
        db::Value::Integer(value) => serde_json::json!(value),
        db::Value::Real(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| serde_json::json!(value.to_string())),
        db::Value::Text(value) => serde_json::json!(value),
        db::Value::Blob(value) => serde_json::json!({
            "type": "blob",
            "bytes": value.len(),
            "preview": value.iter().take(16).map(|byte| format!("{byte:02x}")).collect::<String>(),
        }),
    }
}

fn json_to_db_value(value: &serde_json::Value) -> db::Value {
    match value {
        serde_json::Value::Null => db::Value::Null,
        serde_json::Value::Bool(value) => db::Value::Integer(i64::from(*value)),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                db::Value::Integer(value)
            } else if let Some(value) = value.as_u64().and_then(|value| i64::try_from(value).ok()) {
                db::Value::Integer(value)
            } else if let Some(value) = value.as_f64() {
                db::Value::Real(value)
            } else {
                db::Value::Text(value.to_string())
            }
        }
        serde_json::Value::String(value) => db::Value::Text(value.clone()),
        other => db::Value::Text(other.to_string()),
    }
}
