use super::*;
use rusqlite::types::{Value, ValueRef};
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

struct TableSchema {
    name: String,
    table_type: String,
    editable: bool,
    columns: Vec<DatabaseColumnInfo>,
}

impl HistoryStore {
    pub async fn list_database_tables(&self) -> Result<Vec<DatabaseTableSummary>> {
        self.read_conn
            .call(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT name, type FROM sqlite_master \
                     WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%' \
                     ORDER BY type ASC, name ASC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
                let mut tables = Vec::new();
                for row in rows {
                    let (name, table_type) = row?;
                    let quoted = quote_identifier(&name);
                    let row_count = conn
                        .query_row(&format!("SELECT COUNT(*) FROM {quoted}"), [], |row| {
                            row.get::<_, i64>(0)
                        })
                        .ok()
                        .and_then(|count| u64::try_from(count).ok());
                    tables.push(DatabaseTableSummary {
                        editable: table_type == "table" && table_has_rowid(conn, &name),
                        name,
                        table_type,
                        row_count,
                    });
                }
                Ok(tables)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn query_database_table_rows(
        &self,
        table_name: &str,
        offset: usize,
        limit: usize,
        sort_column: Option<&str>,
        sort_direction: Option<&str>,
    ) -> Result<DatabaseTablePage> {
        let table_name = table_name.to_string();
        let sort_column = sort_column.map(str::to_string);
        let sort_direction = sort_direction.map(str::to_string);
        let limit = limit.clamp(1, 500);
        self.read_conn
            .call(move |conn: &mut Connection| {
                let schema = load_table_schema(conn, &table_name)?;
                let quoted_table = quote_identifier(&schema.name);
                let total_rows = conn
                    .query_row(&format!("SELECT COUNT(*) FROM {quoted_table}"), [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .ok()
                    .and_then(|count| u64::try_from(count).ok())
                    .unwrap_or(0);
                let column_list = schema
                    .columns
                    .iter()
                    .map(|column| quote_identifier(&column.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                let order_by = build_order_clause(&schema, sort_column.as_deref(), sort_direction.as_deref())?;
                let select_sql = if schema.editable {
                    format!(
                        "SELECT rowid AS __zorai_rowid, {column_list} FROM {quoted_table}{order_by} LIMIT ?1 OFFSET ?2"
                    )
                } else {
                    format!("SELECT {column_list} FROM {quoted_table}{order_by} LIMIT ?1 OFFSET ?2")
                };
                let mut stmt = conn.prepare(&select_sql)?;
                let mut result_rows = stmt.query(rusqlite::params![limit as i64, offset as i64])?;
                let mut rows = Vec::new();
                while let Some(row) = result_rows.next()? {
                    let rowid = if schema.editable {
                        Some(row.get::<_, i64>(0)?)
                    } else {
                        None
                    };
                    let value_offset = usize::from(schema.editable);
                    let mut values = BTreeMap::new();
                    for (index, column) in schema.columns.iter().enumerate() {
                        let value = sqlite_value_to_json(row.get_ref(index + value_offset)?);
                        values.insert(column.name.clone(), value);
                    }
                    rows.push(DatabaseRow { rowid, values });
                }
                Ok::<DatabaseTablePage, tokio_rusqlite::Error>(DatabaseTablePage {
                    table_name: schema.name,
                    columns: schema.columns,
                    rows,
                    total_rows,
                    offset,
                    limit,
                    editable: schema.editable,
                })
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn update_database_table_rows(
        &self,
        table_name: &str,
        updates: Vec<DatabaseRowUpdate>,
    ) -> Result<usize> {
        let table_name = table_name.to_string();
        self.conn
            .call(move |conn: &mut Connection| {
                let schema = load_table_schema(conn, &table_name)?;
                if !schema.editable {
                    return Err(tokio_rusqlite::Error::Rusqlite(
                        rusqlite::Error::InvalidParameterName(format!(
                            "table is not editable: {}",
                            schema.name
                        )),
                    ));
                }
                let known_columns = schema
                    .columns
                    .iter()
                    .map(|column| column.name.as_str())
                    .collect::<BTreeSet<_>>();
                let quoted_table = quote_identifier(&schema.name);
                let transaction = conn.unchecked_transaction()?;
                let mut changed_rows = 0;
                for update in updates {
                    if update.values.is_empty() {
                        continue;
                    }
                    for column in update.values.keys() {
                        if !known_columns.contains(column.as_str()) {
                            return Err(tokio_rusqlite::Error::Rusqlite(
                                rusqlite::Error::InvalidParameterName(format!(
                                    "unknown column: {column}"
                                )),
                            ));
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
                        .map(json_to_sql_value)
                        .collect::<Vec<_>>();
                    bind_values.push(Value::Integer(update.rowid));
                    changed_rows += transaction
                        .execute(&sql, rusqlite::params_from_iter(bind_values.iter()))?;
                }
                transaction.commit()?;
                Ok::<usize, tokio_rusqlite::Error>(changed_rows)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

fn load_table_schema(conn: &Connection, table_name: &str) -> rusqlite::Result<TableSchema> {
    let table_type = conn.query_row(
        "SELECT type FROM sqlite_master WHERE type IN ('table', 'view') AND name = ?1 AND name NOT LIKE 'sqlite_%'",
        [table_name],
        |row| row.get::<_, String>(0),
    )?;
    let editable = table_type == "table" && table_has_rowid(conn, table_name);
    let quoted_table = quote_identifier(table_name);
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({quoted_table})"))?;
    let rows = stmt.query_map([], |row| {
        let name = row.get::<_, String>(1)?;
        let not_null = row.get::<_, i64>(3)?;
        let primary_key = row.get::<_, i64>(5)? > 0;
        Ok(DatabaseColumnInfo {
            name,
            declared_type: row.get::<_, String>(2)?,
            nullable: not_null == 0,
            primary_key,
            editable,
        })
    })?;
    let columns = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    if columns.is_empty() {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "table has no visible columns: {table_name}"
        )));
    }
    Ok(TableSchema {
        name: table_name.to_string(),
        table_type,
        editable,
        columns,
    })
}

fn table_has_rowid(conn: &Connection, table_name: &str) -> bool {
    conn.prepare(&format!(
        "SELECT rowid FROM {} LIMIT 0",
        quote_identifier(table_name)
    ))
    .is_ok()
}

fn build_order_clause(
    schema: &TableSchema,
    sort_column: Option<&str>,
    sort_direction: Option<&str>,
) -> std::result::Result<String, tokio_rusqlite::Error> {
    let Some(sort_column) = sort_column.filter(|column| !column.trim().is_empty()) else {
        return Ok(String::new());
    };
    if !schema
        .columns
        .iter()
        .any(|column| column.name == sort_column)
    {
        return Err(tokio_rusqlite::Error::Rusqlite(
            rusqlite::Error::InvalidParameterName(format!("unknown sort column: {sort_column}")),
        ));
    }
    let direction = match sort_direction
        .unwrap_or("desc")
        .to_ascii_lowercase()
        .as_str()
    {
        "asc" => "ASC",
        "desc" => "DESC",
        other => {
            return Err(tokio_rusqlite::Error::Rusqlite(
                rusqlite::Error::InvalidParameterName(format!("unknown sort direction: {other}")),
            ));
        }
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

fn sqlite_value_to_json(value: ValueRef<'_>) -> serde_json::Value {
    match value {
        ValueRef::Null => serde_json::Value::Null,
        ValueRef::Integer(value) => serde_json::json!(value),
        ValueRef::Real(value) => serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| serde_json::json!(value.to_string())),
        ValueRef::Text(value) => serde_json::json!(String::from_utf8_lossy(value).to_string()),
        ValueRef::Blob(value) => serde_json::json!({
            "type": "blob",
            "bytes": value.len(),
            "preview": value.iter().take(16).map(|byte| format!("{byte:02x}")).collect::<String>(),
        }),
    }
}

fn json_to_sql_value(value: &serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(value) => Value::Integer(i64::from(*value)),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Value::Integer(value)
            } else if let Some(value) = value.as_u64().and_then(|value| i64::try_from(value).ok()) {
                Value::Integer(value)
            } else if let Some(value) = value.as_f64() {
                Value::Real(value)
            } else {
                Value::Text(value.to_string())
            }
        }
        serde_json::Value::String(value) => Value::Text(value.clone()),
        other => Value::Text(other.to_string()),
    }
}
