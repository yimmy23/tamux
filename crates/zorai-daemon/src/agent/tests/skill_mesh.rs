use anyhow::Result;
use arrow_array::{Array, ListArray, RecordBatch, StringArray};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use tokio::time::{timeout, Duration};

use super::skill_mesh::index::{
    fail_replace_document_once_for_tests, pause_after_document_replace_once_for_tests,
};

use super::skill_mesh::{
    InMemorySkillMeshIndex, LanceDbSkillMeshIndex, SkillMeshDocument, SkillMeshDocumentKey,
    SkillMeshEmbeddingRecord, SkillMeshFeedbackState, SkillMeshIndex, SkillMeshIntent,
    SkillMeshOutcome, SkillMeshResult,
};

fn sample_skill_mesh_document() -> SkillMeshDocument {
    SkillMeshDocument {
        skill_id: "debug-rust-build".to_string(),
        variant_id: Some("debug-rust-build@v1".to_string()),
        skill_name: "Debug Rust Build".to_string(),
        variant_name: Some("default".to_string()),
        source_path: "skills/debug-rust-build/SKILL.md".to_string(),
        source_kind: "builtin".to_string(),
        content_hash: "hash-123".to_string(),
        compile_version: 1,
        summary: Some("Debug Rust build and cargo test failures".to_string()),
        capability_path: vec!["development".to_string(), "rust".to_string(), "debug".to_string()],
        synthetic_queries: vec![
            "debug a failing cargo build".to_string(),
            "fix rust test failure".to_string(),
        ],
        explicit_trigger_phrases: vec!["cargo test".to_string()],
        workspace_affinities: vec!["rust".to_string()],
        required_tools: vec!["read_file".to_string(), "cargo test".to_string()],
        required_platforms: vec!["linux".to_string()],
        required_env_hints: vec!["cargo".to_string()],
        security_risk_level: "low".to_string(),
        trust_tier: "trusted".to_string(),
        provenance: "builtin".to_string(),
        use_count: 0,
        success_count: 0,
        failure_count: 0,
        dismiss_count: 0,
        negative_feedback_weight: 0.0,
        embedding_records: vec![sample_embedding_record()],
    }
}

fn sample_embedding_record() -> SkillMeshEmbeddingRecord {
    SkillMeshEmbeddingRecord {
        embedding_id: "embedding-1".to_string(),
        skill_id: "debug-rust-build".to_string(),
        variant_id: Some("debug-rust-build@v1".to_string()),
        embedding_kind: "synthetic_query".to_string(),
        text: "debug a failing cargo build".to_string(),
        vector: vec![0.1, 0.2, 0.3],
        capability_path: vec!["development".to_string(), "rust".to_string(), "debug".to_string()],
        trust_tier: "trusted".to_string(),
        risk_level: "low".to_string(),
        source_kind: "builtin".to_string(),
        active: true,
    }
}

fn sample_feedback_state() -> SkillMeshFeedbackState {
    SkillMeshFeedbackState {
        use_count: 5,
        success_count: 3,
        failure_count: 1,
        dismiss_count: 1,
        negative_feedback_weight: 0.4,
        requires_recompile: false,
    }
}

fn sample_skill_mesh_variant_document(variant_id: &str, variant_name: &str) -> SkillMeshDocument {
    let mut document = sample_skill_mesh_document();
    document.variant_id = Some(variant_id.to_string());
    document.variant_name = Some(variant_name.to_string());
    document.content_hash = format!("hash-{variant_id}");
    document.summary = Some(format!("{variant_name} variant"));
    document.synthetic_queries = vec![format!("query for {variant_name}")];
    document.embedding_records = vec![SkillMeshEmbeddingRecord {
        embedding_id: format!("embedding-{variant_id}"),
        skill_id: document.skill_id.clone(),
        variant_id: document.variant_id.clone(),
        embedding_kind: "synthetic_query".to_string(),
        text: format!("query for {variant_name}"),
        vector: vec![0.3, 0.2, 0.1],
        capability_path: document.capability_path.clone(),
        trust_tier: document.trust_tier.clone(),
        risk_level: document.security_risk_level.clone(),
        source_kind: document.source_kind.clone(),
        active: true,
    }];
    document
}

async fn lancedb_table_row_count(
    db_path: &std::path::Path,
    table_name: &str,
    filter: Option<String>,
) -> Result<usize> {
    let connection = lancedb::connect(&db_path.to_string_lossy().to_string())
        .execute()
        .await?;
    let table = connection.open_table(table_name).execute().await?;
    let mut row_count = 0usize;
    if let Some(filter) = filter {
        let mut stream = table.query().only_if(filter).execute().await?;
        while let Some(batch) = stream.try_next().await? {
            row_count += batch.num_rows();
        }
    } else {
        let mut stream = table.query().execute().await?;
        while let Some(batch) = stream.try_next().await? {
            row_count += batch.num_rows();
        }
    }
    Ok(row_count)
}

fn document_with_feedback_state(state: SkillMeshFeedbackState) -> SkillMeshDocument {
    let mut document = sample_skill_mesh_document();
    document.use_count = state.use_count;
    document.success_count = state.success_count;
    document.failure_count = state.failure_count;
    document.dismiss_count = state.dismiss_count;
    document.negative_feedback_weight = state.negative_feedback_weight;
    document
}

fn sample_skill_mesh_intent() -> SkillMeshIntent {
    SkillMeshIntent {
        original_query: "debug a failing cargo build".to_string(),
        normalized_query: "debug failing cargo build".to_string(),
        workspace_hints: vec!["rust".to_string()],
        capability_hints: vec!["development/rust/debug".to_string()],
        risk_hints: vec!["low".to_string()],
    }
}

async fn lancedb_embedding_capability_paths(db_path: &std::path::Path) -> Result<Vec<Vec<String>>> {
    let connection = lancedb::connect(&db_path.to_string_lossy().to_string())
        .execute()
        .await?;
    let table = connection.open_table("skill_mesh_embeddings").execute().await?;
    let batches = table.query().execute().await?.try_collect::<Vec<_>>().await?;
    let mut capability_paths = Vec::new();

    for batch in batches {
        let paths = batch
            .column_by_name("capability_path")
            .expect("embedding rows should expose capability_path")
            .as_any()
            .downcast_ref::<ListArray>()
            .expect("capability_path column should be a string list");

        for row_index in 0..batch.num_rows() {
            let path_values = paths.value(row_index);
            let path_segments = path_values
                .as_any()
                .downcast_ref::<StringArray>()
                .expect("capability_path values should be Utf8 strings");
            capability_paths.push(
                (0..path_segments.len())
                    .map(|segment_index| path_segments.value(segment_index).to_string())
                    .collect(),
            );
        }
    }

    Ok(capability_paths)
}

async fn lancedb_document_payloads(db_path: &std::path::Path) -> Result<Vec<(String, String)>> {
    let connection = lancedb::connect(&db_path.to_string_lossy().to_string())
        .execute()
        .await?;
    let table = connection.open_table("skill_mesh_documents").execute().await?;
    let batches = table.query().execute().await?.try_collect::<Vec<_>>().await?;
    let mut payloads = Vec::new();

    for batch in batches {
        let skill_ids = batch
            .column_by_name("skill_id")
            .expect("document rows should expose skill_id")
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("skill_id column should be Utf8");
        let document_json = batch
            .column_by_name("document_json")
            .expect("document rows should expose document_json")
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("document_json column should be Utf8");

        for row_index in 0..batch.num_rows() {
            payloads.push((
                skill_ids.value(row_index).to_string(),
                document_json.value(row_index).to_string(),
            ));
        }
    }

    Ok(payloads)
}

async fn lancedb_embedding_identity_rows(
    db_path: &std::path::Path,
    key: &SkillMeshDocumentKey,
) -> Result<Vec<(String, Option<String>, String, String)>> {
    let connection = lancedb::connect(&db_path.to_string_lossy().to_string())
        .execute()
        .await?;
    let table = connection.open_table("skill_mesh_embeddings").execute().await?;
    let filter = match key.variant_id.as_deref() {
        Some(variant_id) => format!(
            "skill_id = '{}' AND variant_id = '{}'",
            key.skill_id.replace('\'', "''"),
            variant_id.replace('\'', "''")
        ),
        None => format!(
            "skill_id = '{}' AND variant_id IS NULL",
            key.skill_id.replace('\'', "''")
        ),
    };
    let batches = table
        .query()
        .only_if(filter)
        .execute()
        .await?
        .try_collect::<Vec<_>>()
        .await?;
    let mut rows = Vec::new();

    for batch in batches {
        let embedding_ids = batch
            .column_by_name("embedding_id")
            .expect("embedding rows should expose embedding_id")
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("embedding_id column should be Utf8");
        let variant_ids = batch
            .column_by_name("variant_id")
            .expect("embedding rows should expose variant_id")
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("variant_id column should be Utf8");
        let texts = batch
            .column_by_name("text")
            .expect("embedding rows should expose text")
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("text column should be Utf8");
        let source_kinds = batch
            .column_by_name("source_kind")
            .expect("embedding rows should expose source_kind")
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("source_kind column should be Utf8");

        for row_index in 0..batch.num_rows() {
            rows.push((
                embedding_ids.value(row_index).to_string(),
                (!variant_ids.is_null(row_index))
                    .then(|| variant_ids.value(row_index).to_string()),
                texts.value(row_index).to_string(),
                source_kinds.value(row_index).to_string(),
            ));
        }
    }

    Ok(rows)
}

async fn overwrite_lancedb_document_row(
    db_path: &std::path::Path,
    storage_key: &str,
    document_json: &str,
) -> Result<()> {
    let connection = lancedb::connect(&db_path.to_string_lossy().to_string())
        .execute()
        .await?;
    let table = connection.open_table("skill_mesh_documents").execute().await?;
    table
        .delete(&format!("skill_id = '{}'", storage_key.replace('\'', "''")))
        .await?;
    table
        .add(RecordBatch::try_new(
            std::sync::Arc::new(arrow_schema::Schema::new(vec![
                arrow_schema::Field::new("skill_id", arrow_schema::DataType::Utf8, false),
                arrow_schema::Field::new("document_json", arrow_schema::DataType::Utf8, false),
            ])),
            vec![
                std::sync::Arc::new(StringArray::from(vec![storage_key]))
                    as std::sync::Arc<dyn arrow_array::Array>,
                std::sync::Arc::new(StringArray::from(vec![document_json]))
                    as std::sync::Arc<dyn arrow_array::Array>,
            ],
        )?)
        .execute()
        .await?;
    Ok(())
}

#[test]
fn skill_mesh_result_preserves_chosen_variant_identity() -> Result<()> {
    let chosen_key = SkillMeshDocumentKey {
        skill_id: "debug-rust-build".to_string(),
        variant_id: Some("debug-rust-build@v2".to_string()),
    };
    let result = SkillMeshResult {
        intent: sample_skill_mesh_intent(),
        candidates: Vec::new(),
        chosen_document_key: Some(chosen_key.clone()),
        next_action: Some("read_skill".to_string()),
        rationale: vec!["variant matched the operator query".to_string()],
    };

    let serialized = serde_json::to_value(&result)?;
    let round_trip: SkillMeshResult = serde_json::from_value(serialized.clone())?;

    assert_eq!(serialized["chosen_document_key"]["skill_id"], chosen_key.skill_id);
    assert_eq!(
        serialized["chosen_document_key"]["variant_id"],
        chosen_key.variant_id.clone().unwrap()
    );
    assert_eq!(round_trip.chosen_document_key, Some(chosen_key));

    Ok(())
}

#[tokio::test]
async fn skill_mesh_in_memory_index_round_trips_compiled_records() -> Result<()> {
    let index = InMemorySkillMeshIndex::default();
    let document = sample_skill_mesh_document();
    let key = document.document_key();

    index.upsert_document(document.clone()).await?;

    let loaded = index
        .get_document(&key)
        .await?
        .expect("compiled skill mesh document should round-trip");

    assert_eq!(loaded.skill_name, document.skill_name);
    assert_eq!(loaded.embedding_records, document.embedding_records);

    Ok(())
}

#[tokio::test]
async fn skill_mesh_in_memory_index_rejects_mismatched_embedding_identity() -> Result<()> {
    let index = InMemorySkillMeshIndex::default();
    let mut malformed = sample_skill_mesh_document();
    let key = malformed.document_key();
    malformed.embedding_records[0].variant_id = Some("debug-rust-build@wrong".to_string());

    let err = index
        .upsert_document(malformed)
        .await
        .expect_err("in-memory index should reject embedding rows whose identity diverges from the document key");

    assert!(
        err.to_string().contains("embedding") && err.to_string().contains("identity"),
        "mismatched embedding identity should surface the same boundary error in memory: {err:#}"
    );
    assert!(
        index.get_document(&key).await?.is_none(),
        "rejected in-memory writes must not persist malformed documents"
    );

    Ok(())
}

#[tokio::test]
async fn skill_mesh_index_round_trips_compiled_records() -> Result<()> {
    let root = tempfile::tempdir()?;
    let db_path = root.path().join("skill-mesh-index");
    let document = sample_skill_mesh_document();
    let key = document.document_key();

    let index = LanceDbSkillMeshIndex::open(&db_path).await?;
    index.upsert_document(document.clone()).await?;
    drop(index);

    let reopened = LanceDbSkillMeshIndex::open(&db_path).await?;
    let loaded = reopened
        .get_document(&key)
        .await?
        .expect("compiled skill mesh document should round-trip through LanceDB");

    assert_eq!(loaded, document);

    Ok(())
}

#[tokio::test]
async fn skill_mesh_index_embedding_rows_preserve_capability_path() -> Result<()> {
    let root = tempfile::tempdir()?;
    let db_path = root.path().join("skill-mesh-embedding-capability-path");
    let document = sample_skill_mesh_document();

    let index = LanceDbSkillMeshIndex::open(&db_path).await?;
    index.upsert_document(document.clone()).await?;

    let capability_paths = lancedb_embedding_capability_paths(&db_path).await?;

    assert_eq!(
        capability_paths,
        vec![document.embedding_records[0].capability_path.clone()],
        "embedding rows should persist canonical capability_path data"
    );

    Ok(())
}

#[tokio::test]
async fn skill_mesh_feedback_updates_do_not_require_recompile() -> Result<()> {
    let root = tempfile::tempdir()?;
    let db_path = root.path().join("skill-mesh-feedback");
    let state = sample_feedback_state();
    let document = document_with_feedback_state(state.clone());
    let key = document.document_key();

    let index = LanceDbSkillMeshIndex::open(&db_path).await?;
    index.upsert_document(document).await?;

    let updated = index
        .apply_feedback(&key, SkillMeshOutcome::Dismissed)
        .await?
        .expect("feedback should update existing skill mesh document");

    drop(index);

    let reopened = LanceDbSkillMeshIndex::open(&db_path).await?;
    let loaded = reopened
        .get_document(&key)
        .await?
        .expect("updated skill mesh document should reload from LanceDB");

    assert!(!updated.requires_recompile);
    assert_eq!(updated.dismiss_count, state.dismiss_count + 1);
    assert_eq!(updated.use_count, state.use_count + 1);
    assert_eq!(loaded.feedback_state(), updated);

    Ok(())
}

#[tokio::test]
async fn skill_mesh_index_serializes_multi_step_boundary_writes() -> Result<()> {
    let root = tempfile::tempdir()?;
    let db_path = root.path().join("skill-mesh-serialized-boundary-writes");
    let seed = sample_skill_mesh_document();
    let key = seed.document_key();

    let index = std::sync::Arc::new(LanceDbSkillMeshIndex::open(&db_path).await?);
    index.upsert_document(seed).await?;

    let mut first = sample_skill_mesh_document();
    first.content_hash = "hash-first-boundary-write".to_string();
    first.summary = Some("first boundary write".to_string());
    first.embedding_records[0].embedding_id = "embedding-first-boundary-write".to_string();
    first.embedding_records[0].text = "first boundary write".to_string();
    first.embedding_records[0].vector = vec![0.7, 0.2, 0.1];

    let mut second = sample_skill_mesh_document();
    second.content_hash = "hash-second-boundary-write".to_string();
    second.summary = Some("second boundary write".to_string());
    second.embedding_records[0].embedding_id = "embedding-second-boundary-write".to_string();
    second.embedding_records[0].text = "second boundary write".to_string();
    second.embedding_records[0].vector = vec![0.1, 0.8, 0.6];
    let expected = second.clone();

    let (_pause_guard, pause_controller) =
        pause_after_document_replace_once_for_tests(&db_path, &key);

    let first_index = std::sync::Arc::clone(&index);
    let first_task = tokio::spawn(async move { first_index.upsert_document(first).await });
    pause_controller.wait_until_hit().await;

    let second_index = std::sync::Arc::clone(&index);
    let mut second_task = tokio::spawn(async move {
        second_index.upsert_document(second).await
    });

    assert!(
        timeout(Duration::from_millis(250), &mut second_task).await.is_err(),
        "second mutation should remain blocked until the first multi-step write fully completes"
    );

    pause_controller.release();
    first_task.await??;
    second_task.await??;

    let loaded = index
        .get_document(&key)
        .await?
        .expect("serialized writes should leave a readable canonical document payload");
    let embedding_rows = lancedb_embedding_identity_rows(&db_path, &key).await?;

    assert_eq!(loaded, expected);
    assert_eq!(
        embedding_rows,
        vec![(
            expected.embedding_records[0].embedding_id.clone(),
            expected.embedding_records[0].variant_id.clone(),
            expected.embedding_records[0].text.clone(),
            expected.embedding_records[0].source_kind.clone(),
        )],
        "derived embedding rows should stay aligned with the canonical document_json payload after serialized writes"
    );

    Ok(())
}

#[tokio::test]
async fn skill_mesh_index_keeps_distinct_variants_for_the_same_skill() -> Result<()> {
    let root = tempfile::tempdir()?;
    let db_path = root.path().join("skill-mesh-variants");
    let base_document = sample_skill_mesh_document();
    let base_key = base_document.document_key();
    let alternate_document =
        sample_skill_mesh_variant_document("debug-rust-build@v2", "alternate");
    let alternate_key = alternate_document.document_key();

    let index = LanceDbSkillMeshIndex::open(&db_path).await?;
    index.upsert_document(base_document).await?;
    index.upsert_document(alternate_document).await?;

    let base_loaded = index
        .get_document(&base_key)
        .await?
        .expect("base variant should be retrievable by composite key");
    let alternate_loaded = index
        .get_document(&alternate_key)
        .await?
        .expect("alternate variant should be retrievable by composite key");

    let document_rows = lancedb_table_row_count(&db_path, "skill_mesh_documents", None).await?;
    let embedding_rows = lancedb_table_row_count(
        &db_path,
        "skill_mesh_embeddings",
        Some("skill_id = 'debug-rust-build'".to_string()),
    )
    .await?;

    assert_eq!(base_loaded.variant_id, base_key.variant_id);
    assert_eq!(alternate_loaded.variant_id, alternate_key.variant_id);
    assert_eq!(document_rows, 2, "both variants should retain separate document rows");
    assert_eq!(embedding_rows, 2, "both variants should retain separate embedding rows");

    Ok(())
}

#[tokio::test]
async fn skill_mesh_index_replacement_with_no_embeddings_clears_stale_embedding_rows() -> Result<()> {
    let root = tempfile::tempdir()?;
    let db_path = root.path().join("skill-mesh-empty-embeddings");
    let document = sample_skill_mesh_document();
    let document_key = document.document_key();
    let alternate_document =
        sample_skill_mesh_variant_document("debug-rust-build@v2", "alternate");
    let alternate_key = alternate_document.document_key();

    let index = LanceDbSkillMeshIndex::open(&db_path).await?;
    index.upsert_document(document.clone()).await?;
    index.upsert_document(alternate_document.clone()).await?;

    let initial_embedding_rows =
        lancedb_table_row_count(&db_path, "skill_mesh_embeddings", None).await?;
    assert_eq!(
        initial_embedding_rows, 2,
        "fixture should create one embedding row per variant before replacement"
    );

    let mut replacement = document;
    replacement.content_hash = "hash-no-embeddings".to_string();
    replacement.embedding_records.clear();
    index.upsert_document(replacement).await?;

    let embedding_rows = lancedb_table_row_count(&db_path, "skill_mesh_embeddings", None).await?;
    let cleared_variant = index
        .get_document(&document_key)
        .await?
        .expect("updated variant should still exist after clearing embeddings");
    let untouched_variant = index
        .get_document(&alternate_key)
        .await?
        .expect("other variant should remain present");

    assert!(
        cleared_variant.embedding_records.is_empty(),
        "updated variant should reload with no embeddings"
    );
    assert_eq!(
        untouched_variant.embedding_records,
        alternate_document.embedding_records,
        "other variant embeddings should remain intact"
    );
    assert_eq!(
        embedding_rows, 1,
        "replacement should remove only the target variant's stale embedding rows"
    );

    Ok(())
}

#[tokio::test]
async fn skill_mesh_index_reopened_width_mismatch_keeps_document_and_embeddings_consistent(
) -> Result<()> {
    let root = tempfile::tempdir()?;
    let db_path = root.path().join("skill-mesh-width-mismatch");
    let original = sample_skill_mesh_document();
    let original_key = original.document_key();

    let index = LanceDbSkillMeshIndex::open(&db_path).await?;
    index.upsert_document(original.clone()).await?;
    drop(index);

    let mut mismatched = sample_skill_mesh_variant_document("debug-rust-build@v2", "mismatch");
    let mismatched_key = mismatched.document_key();
    mismatched.embedding_records[0].vector = vec![0.9, 0.8];

    let reopened = LanceDbSkillMeshIndex::open(&db_path).await?;
    let err = reopened
        .upsert_document(mismatched)
        .await
        .expect_err("reopened embedding table should reject a different vector width");

    drop(reopened);

    let verified = LanceDbSkillMeshIndex::open(&db_path).await?;
    let loaded = verified
        .get_document(&original_key)
        .await?
        .expect("original document should remain readable after a failed insert");
    let missing = verified.get_document(&mismatched_key).await?;
    let document_rows = lancedb_table_row_count(&db_path, "skill_mesh_documents", None).await?;
    let embedding_rows = lancedb_table_row_count(&db_path, "skill_mesh_embeddings", None).await?;
    let stored_payloads = lancedb_document_payloads(&db_path).await?;

    assert!(
        err.to_string().contains("vector width mismatch"),
        "width mismatch should fail from the reopened-table width validator: {err:#}"
    );
    assert_eq!(loaded, original);
    assert!(
        missing.is_none(),
        "failed insert must not leave a document row behind without embeddings"
    );
    assert_eq!(document_rows, 1, "failed insert must not persist a new document row");
    assert_eq!(embedding_rows, 1, "failed insert must not disturb the original embedding rows");
    assert_eq!(
        stored_payloads,
        vec![(original_key.storage_key(), original.to_storage_json()?)],
        "failed insert must preserve the original persisted payload set"
    );

    Ok(())
}

#[tokio::test]
async fn skill_mesh_index_load_rejects_document_json_identity_mismatch() -> Result<()> {
    let root = tempfile::tempdir()?;
    let db_path = root.path().join("skill-mesh-corrupt-identity");
    let document = sample_skill_mesh_document();
    let key = document.document_key();

    let index = LanceDbSkillMeshIndex::open(&db_path).await?;
    index.upsert_document(document.clone()).await?;
    drop(index);

    let mut corrupted = document.clone();
    corrupted.variant_id = Some("debug-rust-build@corrupt".to_string());
    overwrite_lancedb_document_row(&db_path, &key.storage_key(), &corrupted.to_storage_json()?)
        .await?;

    let reopened = LanceDbSkillMeshIndex::open(&db_path).await?;
    let err = reopened
        .get_document(&key)
        .await
        .expect_err("load should reject payloads whose identity diverges from the stored key");

    assert!(
        err.to_string().contains("identity") || err.to_string().contains("storage key"),
        "corrupt identity should surface a storage identity error: {err:#}"
    );

    Ok(())
}

#[tokio::test]
async fn skill_mesh_index_rejects_mismatched_embedding_identity_without_partial_persist(
) -> Result<()> {
    let root = tempfile::tempdir()?;
    let db_path = root.path().join("skill-mesh-embedding-identity-mismatch");
    let original = sample_skill_mesh_document();
    let key = original.document_key();

    let index = LanceDbSkillMeshIndex::open(&db_path).await?;
    index.upsert_document(original.clone()).await?;

    let mut malformed = original.clone();
    malformed.content_hash = "hash-malformed-embedding-identity".to_string();
    malformed.embedding_records[0].variant_id = Some("debug-rust-build@wrong".to_string());

    let err = index
        .upsert_document(malformed)
        .await
        .expect_err("upsert should reject embedding rows whose identity diverges from the document key");

    drop(index);

    let reopened = LanceDbSkillMeshIndex::open(&db_path).await?;
    let loaded = reopened
        .get_document(&key)
        .await?
        .expect("original document should remain readable after malformed input is rejected");
    let document_rows = lancedb_table_row_count(&db_path, "skill_mesh_documents", None).await?;
    let embedding_rows = lancedb_table_row_count(&db_path, "skill_mesh_embeddings", None).await?;
    let stored_payloads = lancedb_document_payloads(&db_path).await?;

    assert!(
        err.to_string().contains("embedding") && err.to_string().contains("identity"),
        "mismatched embedding identity should surface a boundary error: {err:#}"
    );
    assert_eq!(loaded, original);
    assert_eq!(document_rows, 1, "rejected input must not replace the persisted document row");
    assert_eq!(embedding_rows, 1, "rejected input must not replace the persisted embedding rows");
    assert_eq!(
        stored_payloads,
        vec![(key.storage_key(), original.to_storage_json()?)],
        "rejected input must preserve the prior persisted payload"
    );

    Ok(())
}

#[tokio::test]
async fn skill_mesh_feedback_persistence_preserves_prior_document_on_replace_failure(
) -> Result<()> {
    let root = tempfile::tempdir()?;
    let db_path = root.path().join("skill-mesh-feedback-rollback");
    let original = document_with_feedback_state(sample_feedback_state());
    let key = original.document_key();

    let index = LanceDbSkillMeshIndex::open(&db_path).await?;
    index.upsert_document(original.clone()).await?;

    let _failpoint = fail_replace_document_once_for_tests(&db_path, &key);
    let err = index
        .apply_feedback(&key, SkillMeshOutcome::Failure)
        .await
        .expect_err("apply_feedback should roll back when document replacement fails");

    drop(index);

    let reopened = LanceDbSkillMeshIndex::open(&db_path).await?;
    let loaded = reopened
        .get_document(&key)
        .await?
        .expect("failed feedback persistence should not drop the existing document row");
    let document_rows = lancedb_table_row_count(&db_path, "skill_mesh_documents", None).await?;
    let embedding_rows = lancedb_table_row_count(&db_path, "skill_mesh_embeddings", None).await?;
    let stored_payloads = lancedb_document_payloads(&db_path).await?;

    assert!(
        err.to_string().contains("test failpoint"),
        "failure path should come from the replace-document failpoint: {err:#}"
    );
    assert_eq!(loaded, original);
    assert_eq!(document_rows, 1, "failed feedback persistence must keep the document row");
    assert_eq!(embedding_rows, 1, "failed feedback persistence must keep the embedding row");
    assert_eq!(
        stored_payloads,
        vec![(key.storage_key(), original.to_storage_json()?)],
        "failed feedback persistence must restore the prior payload"
    );

    Ok(())
}