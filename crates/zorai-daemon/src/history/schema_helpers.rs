use super::*;

pub(super) async fn ensure_column<E: db::DbExecutor + ?Sized>(
    exec: &mut E,
    table: &str,
    column: &str,
    definition: &str,
) -> anyhow::Result<()> {
    if table_has_column(&mut *exec, table, column).await? {
        return Ok(());
    }

    exec.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        db::Params::None,
    )
    .await?;
    Ok(())
}

pub(super) async fn table_has_column<E: db::DbExecutor + ?Sized>(
    exec: &mut E,
    table: &str,
    column: &str,
) -> anyhow::Result<bool> {
    let rows = exec
        .query(&format!("PRAGMA table_xinfo({table})"), db::Params::None)
        .await?;
    for row in &rows {
        if row.get::<String>(1)? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
pub(crate) fn table_has_column_sync(
    connection: &Connection,
    table: &str,
    column: &str,
) -> std::result::Result<bool, rusqlite::Error> {
    let mut stmt = connection.prepare(&format!("PRAGMA table_xinfo({table})"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == column {
            return Ok(true);
        }
    }
    Ok(false)
}
