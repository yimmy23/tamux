use super::super::DatabaseRowUpdate;
use super::make_test_store;
use std::fs;

#[tokio::test]
async fn database_viewer_lists_queries_and_updates_table_rows() {
    let (store, root) = make_test_store().await.expect("create history store");

    store
        .conn
        .call(|conn| {
            conn.execute(
                "CREATE TABLE database_viewer_items (id INTEGER PRIMARY KEY, name TEXT NOT NULL, priority INTEGER)",
                [],
            )?;
            conn.execute(
                "INSERT INTO database_viewer_items (name, priority) VALUES ('alpha', 1), ('beta', 2)",
                [],
            )?;
            Ok(())
        })
        .await
        .expect("seed viewer table");

    let tables = store
        .list_database_tables()
        .await
        .expect("list database tables");
    let table = tables
        .iter()
        .find(|table| table.name == "database_viewer_items")
        .expect("viewer table should be listed");
    assert_eq!(table.row_count, Some(2));
    assert!(table.editable);

    let page = store
        .query_database_table_rows("database_viewer_items", 0, 100, None, None)
        .await
        .expect("query viewer table");
    assert_eq!(page.total_rows, 2);
    assert_eq!(
        page.columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        ["id", "name", "priority"]
    );
    assert_eq!(page.rows[0].values["name"], serde_json::json!("alpha"));

    store
        .update_database_table_rows(
            "database_viewer_items",
            vec![DatabaseRowUpdate {
                rowid: page.rows[0].rowid.expect("rowid"),
                values: [("priority".to_string(), serde_json::json!(7))]
                    .into_iter()
                    .collect(),
            }],
        )
        .await
        .expect("update changed column");

    let page = store
        .query_database_table_rows("database_viewer_items", 0, 100, None, None)
        .await
        .expect("query updated viewer table");
    assert_eq!(page.rows[0].values["name"], serde_json::json!("alpha"));
    assert_eq!(page.rows[0].values["priority"], serde_json::json!(7));

    fs::remove_dir_all(root).expect("cleanup history root");
}

#[tokio::test]
async fn database_viewer_sorts_rows_by_selected_column() {
    let (store, root) = make_test_store().await.expect("create history store");

    store
        .conn
        .call(|conn| {
            conn.execute(
                "CREATE TABLE database_viewer_sort_items (id INTEGER PRIMARY KEY, name TEXT NOT NULL, priority INTEGER)",
                [],
            )?;
            conn.execute(
                "INSERT INTO database_viewer_sort_items (name, priority) VALUES ('alpha', 2), ('charlie', 3), ('bravo', 1)",
                [],
            )?;
            Ok(())
        })
        .await
        .expect("seed sorted table");

    let desc = store
        .query_database_table_rows(
            "database_viewer_sort_items",
            0,
            100,
            Some("priority"),
            Some("desc"),
        )
        .await
        .expect("query descending rows");
    assert_eq!(
        desc.rows
            .iter()
            .map(|row| row.values["name"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        ["charlie", "alpha", "bravo"],
    );

    let asc = store
        .query_database_table_rows(
            "database_viewer_sort_items",
            0,
            100,
            Some("name"),
            Some("asc"),
        )
        .await
        .expect("query ascending rows");
    assert_eq!(
        asc.rows
            .iter()
            .map(|row| row.values["name"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        ["alpha", "bravo", "charlie"],
    );

    fs::remove_dir_all(root).expect("cleanup history root");
}

#[tokio::test]
async fn database_viewer_includes_soft_deleted_rows_in_raw_table_dump() {
    let (store, root) = make_test_store().await.expect("create history store");

    store
        .conn
        .call(|conn| {
            conn.execute(
                "CREATE TABLE database_viewer_deleted_items (id INTEGER PRIMARY KEY, name TEXT NOT NULL, deleted_at INTEGER)",
                [],
            )?;
            conn.execute(
                "INSERT INTO database_viewer_deleted_items (name, deleted_at) VALUES ('visible', NULL), ('trashed', 12345)",
                [],
            )?;
            Ok(())
        })
        .await
        .expect("seed soft-deleted table rows");

    let tables = store
        .list_database_tables()
        .await
        .expect("list database tables");
    let table = tables
        .iter()
        .find(|table| table.name == "database_viewer_deleted_items")
        .expect("viewer table should be listed");
    assert_eq!(table.row_count, Some(2));

    let page = store
        .query_database_table_rows("database_viewer_deleted_items", 0, 100, None, None)
        .await
        .expect("query viewer table");
    assert_eq!(page.total_rows, 2);
    assert_eq!(
        page.rows
            .iter()
            .map(|row| row.values["name"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        ["visible", "trashed"],
    );
    assert_eq!(page.rows[1].values["deleted_at"], serde_json::json!(12345));

    fs::remove_dir_all(root).expect("cleanup history root");
}

#[tokio::test]
async fn database_viewer_rejects_unknown_update_columns() {
    let (store, root) = make_test_store().await.expect("create history store");

    store
        .conn
        .call(|conn| {
            conn.execute(
                "CREATE TABLE database_viewer_guarded (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
                [],
            )?;
            conn.execute(
                "INSERT INTO database_viewer_guarded (name) VALUES ('alpha')",
                [],
            )?;
            Ok(())
        })
        .await
        .expect("seed guarded table");

    let error = store
        .update_database_table_rows(
            "database_viewer_guarded",
            vec![DatabaseRowUpdate {
                rowid: 1,
                values: [("name = 'hacked' --".to_string(), serde_json::json!("bad"))]
                    .into_iter()
                    .collect(),
            }],
        )
        .await
        .expect_err("unknown columns must be rejected");

    assert!(error.to_string().contains("unknown column"));
    fs::remove_dir_all(root).expect("cleanup history root");
}
