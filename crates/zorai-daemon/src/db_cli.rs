//! `zorai-daemon db <status|sync|pull|push>` — inspect and manage the libSQL
//! database backend (sync an embedded replica, or seed a local database up to a
//! remote libSQL/Turso server).

use anyhow::{anyhow, Context, Result};

use crate::history::db::{self, libsql::LibsqlConn, sqlite::SqliteWriteConn, DbConn};

pub async fn run_db_subcommand() -> Result<()> {
    match std::env::args().nth(2).as_deref() {
        Some("status") => db_status().await,
        Some("sync") | Some("pull") => db_sync().await,
        Some("push") => db_push().await,
        Some(other) => Err(anyhow!(
            "unknown `db` subcommand {other:?}; use: status | sync | pull | push"
        )),
        None => {
            println!("usage: zorai-daemon db <status|sync|pull|push>");
            Ok(())
        }
    }
}

fn local_db_path() -> Result<std::path::PathBuf> {
    Ok(zorai_protocol::ensure_zorai_data_dir()?
        .join("history")
        .join("command-history.db"))
}

fn remote_config() -> Result<(String, String)> {
    let url = zorai_protocol::ZoraiConfig::load()
        .db_sync_url
        .filter(|u| !u.trim().is_empty())
        .ok_or_else(|| {
            anyhow!("no sync URL configured; set db_sync_url in config or ZORAI_DB_SYNC_URL")
        })?;
    let token = zorai_protocol::ZoraiConfig::db_auth_token().unwrap_or_default();
    Ok((url, token))
}

async fn db_status() -> Result<()> {
    let cfg = zorai_protocol::ZoraiConfig::load();
    println!(
        "db_backend:  {}",
        cfg.db_backend.as_deref().unwrap_or("local (sqlite)")
    );
    println!(
        "db_sync_url: {}",
        cfg.db_sync_url.as_deref().unwrap_or("(unset)")
    );
    println!(
        "auth token:  {}",
        if zorai_protocol::ZoraiConfig::db_auth_token().is_some() {
            "set"
        } else {
            "unset"
        }
    );
    println!("local db:    {}", local_db_path()?.display());
    Ok(())
}

async fn db_sync() -> Result<()> {
    let (url, token) = remote_config()?;
    let conn = LibsqlConn::open_remote_replica(&local_db_path()?, url, token)
        .await
        .context("open replica")?;
    conn.sync().await.context("sync replica")?;
    println!("database replica synced.");
    Ok(())
}

async fn db_push() -> Result<()> {
    let (url, token) = remote_config()?;
    let db_path = local_db_path()?;
    if !db_path.exists() {
        return Err(anyhow!("local database not found at {}", db_path.display()));
    }
    let offloaded_dir = zorai_protocol::ensure_zorai_data_dir()?.join("offloaded-payloads");

    let local = SqliteWriteConn::new(
        tokio_rusqlite::Connection::open(&db_path)
            .await
            .context("open local database")?,
        db_path.clone(),
    );
    let remote = LibsqlConn::open_remote(url, token)
        .await
        .context("connect to remote server")?;

    println!("initializing schema on remote...");
    crate::history::init_schema_on_connection(&mut db::ConnExecutor(&remote), &offloaded_dir)
        .await
        .context("initialize schema on remote")?;

    let tables: Vec<String> = local
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '%fts%' ORDER BY name",
            db::Params::None,
        )
        .await?
        .iter()
        .map(|row| row.get::<String>(0))
        .collect::<Result<_>>()?;

    let mut total = 0u64;
    for table in &tables {
        let (cols, rows) = local
            .query_with_columns(&format!("SELECT * FROM \"{table}\""), db::Params::None)
            .await
            .with_context(|| format!("read {table}"))?;
        if rows.is_empty() {
            continue;
        }
        let col_list = cols
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let placeholders = (1..=cols.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql =
            format!("INSERT OR REPLACE INTO \"{table}\" ({col_list}) VALUES ({placeholders})");
        for row in &rows {
            remote
                .execute(&sql, db::Params::Positional(row.values().to_vec()))
                .await
                .with_context(|| format!("seed row into {table}"))?;
            total += 1;
        }
        println!("  {table}: {} rows", rows.len());
    }

    println!("rebuilding external-content FTS indexes on remote...");
    for fts in ["episodes_fts", "context_archive_fts"] {
        if let Err(e) = remote
            .execute(
                &format!("INSERT INTO {fts}({fts}) VALUES('rebuild')"),
                db::Params::None,
            )
            .await
        {
            eprintln!("  warning: FTS rebuild for {fts} failed: {e}");
        }
    }

    let now = crate::history::now_ts() as i64;
    let _ = remote
        .execute(
            "INSERT OR REPLACE INTO agent_config_items (key_path, value_json, updated_at, deleted_at) VALUES ('db_seeded_remote_at', ?1, ?2, NULL)",
            db::db_params![now.to_string(), now],
        )
        .await;

    println!(
        "seeded {total} rows across {} tables to remote.",
        tables.len()
    );
    eprintln!(
        "NOTE: `history_fts` (history search index) is not seeded by this path, and this seed is \
         not yet end-to-end tested against a live server — verify before relying on it."
    );
    Ok(())
}
