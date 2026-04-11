use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use std::sync::{Mutex, OnceLock};

use anyhow::{bail, Context, Result};
use arrow_array::builder::{ListBuilder, StringBuilder};
use arrow_array::types::Float32Type;
use arrow_array::{ArrayRef, BooleanArray, FixedSizeListArray, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use async_trait::async_trait;
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, Table};
#[cfg(test)]
use tokio::sync::Notify;
use tokio::sync::{Mutex as AsyncMutex, RwLock};

use super::types::{
    SkillMeshDocument, SkillMeshDocumentKey, SkillMeshEmbeddingRecord, SkillMeshFeedbackState,
    SkillMeshOutcome, SkillMeshStoredDocument,
};

const DOCUMENTS_TABLE: &str = "skill_mesh_documents";
const EMBEDDINGS_TABLE: &str = "skill_mesh_embeddings";

#[cfg(test)]
static REPLACE_DOCUMENT_FAILPOINT: OnceLock<Mutex<Option<(PathBuf, String)>>> = OnceLock::new();
#[cfg(test)]
static PAUSE_AFTER_DOCUMENT_REPLACE: OnceLock<Mutex<Option<DocumentReplacePausepoint>>> =
    OnceLock::new();

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct ReplaceDocumentFailpointGuard(Option<(PathBuf, String)>);

#[cfg(test)]
#[derive(Debug)]
struct DocumentReplacePausepoint {
    db_path: PathBuf,
    storage_key: String,
    hit: Arc<Notify>,
    resume: Arc<Notify>,
    hit_flag: Arc<AtomicBool>,
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct DocumentReplacePauseGuard(Option<DocumentReplacePausepoint>);

#[cfg(test)]
#[derive(Debug, Clone)]
pub(crate) struct DocumentReplacePauseController {
    hit: Arc<Notify>,
    resume: Arc<Notify>,
    hit_flag: Arc<AtomicBool>,
}

#[cfg(test)]
pub(crate) fn fail_replace_document_once_for_tests(
    db_path: impl AsRef<Path>,
    key: &SkillMeshDocumentKey,
) -> ReplaceDocumentFailpointGuard {
    let mut slot = replace_document_failpoint_slot()
        .lock()
        .expect("replace-document failpoint mutex poisoned");
    ReplaceDocumentFailpointGuard(slot.replace((db_path.as_ref().to_path_buf(), key.storage_key())))
}

#[cfg(test)]
pub(crate) fn pause_after_document_replace_once_for_tests(
    db_path: impl AsRef<Path>,
    key: &SkillMeshDocumentKey,
) -> (DocumentReplacePauseGuard, DocumentReplacePauseController) {
    let hit = Arc::new(Notify::new());
    let resume = Arc::new(Notify::new());
    let hit_flag = Arc::new(AtomicBool::new(false));
    let pausepoint = DocumentReplacePausepoint {
        db_path: db_path.as_ref().to_path_buf(),
        storage_key: key.storage_key(),
        hit: Arc::clone(&hit),
        resume: Arc::clone(&resume),
        hit_flag: Arc::clone(&hit_flag),
    };
    let mut slot = pause_after_document_replace_slot()
        .lock()
        .expect("document-replace pausepoint mutex poisoned");
    (
        DocumentReplacePauseGuard(slot.replace(pausepoint)),
        DocumentReplacePauseController {
            hit,
            resume,
            hit_flag,
        },
    )
}

#[cfg(test)]
impl Drop for ReplaceDocumentFailpointGuard {
    fn drop(&mut self) {
        let mut slot = replace_document_failpoint_slot()
            .lock()
            .expect("replace-document failpoint mutex poisoned");
        *slot = self.0.take();
    }
}

#[cfg(test)]
impl Drop for DocumentReplacePauseGuard {
    fn drop(&mut self) {
        let mut slot = pause_after_document_replace_slot()
            .lock()
            .expect("document-replace pausepoint mutex poisoned");
        *slot = self.0.take();
    }
}

#[cfg(test)]
impl DocumentReplacePauseController {
    pub(crate) async fn wait_until_hit(&self) {
        if self.hit_flag.load(Ordering::SeqCst) {
            return;
        }
        self.hit.notified().await;
    }

    pub(crate) fn release(&self) {
        self.resume.notify_waiters();
    }
}

#[async_trait]
pub trait SkillMeshIndex: Send + Sync {
    async fn upsert_document(&self, document: SkillMeshDocument) -> Result<()>;

    async fn get_document(&self, key: &SkillMeshDocumentKey) -> Result<Option<SkillMeshDocument>>;

    async fn apply_feedback(
        &self,
        key: &SkillMeshDocumentKey,
        outcome: SkillMeshOutcome,
    ) -> Result<Option<SkillMeshFeedbackState>>;
}

#[derive(Debug, Default)]
pub struct InMemorySkillMeshIndex {
    documents: RwLock<HashMap<SkillMeshDocumentKey, SkillMeshDocument>>,
}

#[async_trait]
impl SkillMeshIndex for InMemorySkillMeshIndex {
    async fn upsert_document(&self, document: SkillMeshDocument) -> Result<()> {
        let key = document.document_key();
        validate_embedding_identities(&key, &document.embedding_records)?;
        self.documents.write().await.insert(key, document);
        Ok(())
    }

    async fn get_document(&self, key: &SkillMeshDocumentKey) -> Result<Option<SkillMeshDocument>> {
        Ok(self.documents.read().await.get(key).cloned())
    }

    async fn apply_feedback(
        &self,
        key: &SkillMeshDocumentKey,
        outcome: SkillMeshOutcome,
    ) -> Result<Option<SkillMeshFeedbackState>> {
        let mut documents = self.documents.write().await;
        let Some(document) = documents.get_mut(key) else {
            return Ok(None);
        };
        Ok(Some(document.apply_feedback(outcome)))
    }
}

pub struct LanceDbSkillMeshIndex {
    db_path: PathBuf,
    connection: Connection,
    mutation_lock: AsyncMutex<()>,
}

impl std::fmt::Debug for LanceDbSkillMeshIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanceDbSkillMeshIndex")
            .field("db_path", &self.db_path)
            .finish_non_exhaustive()
    }
}

impl LanceDbSkillMeshIndex {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db_path = path.as_ref().to_path_buf();
        let db_uri = db_path.to_string_lossy().to_string();
        let connection = lancedb::connect(&db_uri)
            .execute()
            .await
            .with_context(|| format!("connect LanceDB at {}", db_path.display()))?;
        let index = Self {
            db_path,
            connection,
            mutation_lock: AsyncMutex::new(()),
        };
        index.ensure_documents_table().await?;
        Ok(index)
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    async fn ensure_documents_table(&self) -> Result<Table> {
        if let Ok(table) = self.connection.open_table(DOCUMENTS_TABLE).execute().await {
            return Ok(table);
        }

        self.connection
            .create_empty_table(DOCUMENTS_TABLE, documents_schema())
            .execute()
            .await
            .with_context(|| format!("create LanceDB table {DOCUMENTS_TABLE}"))?;

        self.connection
            .open_table(DOCUMENTS_TABLE)
            .execute()
            .await
            .with_context(|| format!("open LanceDB table {DOCUMENTS_TABLE}"))
            .map_err(Into::into)
    }

    async fn ensure_embeddings_table(&self, vector_len: i32) -> Result<Table> {
        if let Ok(table) = self.connection.open_table(EMBEDDINGS_TABLE).execute().await {
            self.validate_embeddings_table_vector_len(&table, vector_len)
                .await?;
            return Ok(table);
        }

        self.connection
            .create_empty_table(EMBEDDINGS_TABLE, embeddings_schema(vector_len))
            .execute()
            .await
            .with_context(|| format!("create LanceDB table {EMBEDDINGS_TABLE}"))?;

        self.connection
            .open_table(EMBEDDINGS_TABLE)
            .execute()
            .await
            .with_context(|| format!("open LanceDB table {EMBEDDINGS_TABLE}"))
            .map_err(Into::into)
    }

    async fn validate_embeddings_table_vector_len(
        &self,
        table: &Table,
        expected_vector_len: i32,
    ) -> Result<()> {
        let schema = table
            .schema()
            .await
            .with_context(|| format!("read LanceDB schema for {EMBEDDINGS_TABLE}"))?;
        let vector_field = schema
            .field_with_name("vector")
            .context("skill mesh embeddings table missing vector column")?;
        let actual_vector_len = match vector_field.data_type() {
            DataType::FixedSizeList(_, vector_len) => *vector_len,
            other => {
                bail!("skill mesh embeddings table vector column has unexpected type {other:?}")
            }
        };

        if actual_vector_len != expected_vector_len {
            bail!(
                "skill mesh embeddings table vector width mismatch: expected {expected_vector_len}, found {actual_vector_len}"
            );
        }

        Ok(())
    }

    async fn delete_document(&self, key: &SkillMeshDocumentKey) -> Result<()> {
        let table = self.ensure_documents_table().await?;
        let storage_key = key.storage_key();
        table
            .delete(&format!(
                "skill_id = '{}'",
                escape_sql_literal(&storage_key)
            ))
            .await
            .with_context(|| format!("delete stale LanceDB document for {storage_key}"))?;
        Ok(())
    }

    async fn replace_document(
        &self,
        key: &SkillMeshDocumentKey,
        document: &SkillMeshDocument,
    ) -> Result<()> {
        let storage_key = key.storage_key();
        self.delete_document(key).await?;

        #[cfg(test)]
        maybe_fail_replace_document_for_tests(&self.db_path, &storage_key)?;

        let table = self.ensure_documents_table().await?;
        table
            .add(document_batch(document)?)
            .execute()
            .await
            .with_context(|| format!("insert LanceDB document for {storage_key}"))?;

        Ok(())
    }

    async fn open_embeddings_table_for_replace(
        &self,
        prepared: Option<&PreparedEmbeddingBatch>,
    ) -> Result<Option<Table>> {
        match prepared {
            Some(prepared) => self
                .ensure_embeddings_table(prepared.vector_len)
                .await
                .map(Some),
            None => match self.connection.open_table(EMBEDDINGS_TABLE).execute().await {
                Ok(table) => Ok(Some(table)),
                Err(_) => Ok(None),
            },
        }
    }

    async fn replace_embeddings(
        &self,
        key: &SkillMeshDocumentKey,
        records: &[SkillMeshEmbeddingRecord],
    ) -> Result<()> {
        let prepared = prepare_embedding_batch(records)?;
        let Some(table) = self
            .open_embeddings_table_for_replace(prepared.as_ref())
            .await?
        else {
            return Ok(());
        };
        let filter = embedding_key_filter(key);
        table
            .delete(&filter)
            .await
            .with_context(|| format!("delete stale LanceDB embeddings for {filter}"))?;

        let Some(prepared) = prepared else {
            return Ok(());
        };

        table
            .add(prepared.batch)
            .execute()
            .await
            .with_context(|| format!("insert LanceDB embeddings for {filter}"))?;

        Ok(())
    }

    async fn restore_document_state(
        &self,
        key: &SkillMeshDocumentKey,
        previous: Option<&SkillMeshDocument>,
    ) -> Result<()> {
        match previous {
            Some(previous) => {
                self.replace_document(key, previous).await?;
                self.replace_embeddings(key, &previous.embedding_records)
                    .await?;
            }
            None => {
                self.delete_document(key).await?;
                self.replace_embeddings(key, &[]).await?;
            }
        }

        Ok(())
    }

    async fn load_document(&self, key: &SkillMeshDocumentKey) -> Result<Option<SkillMeshDocument>> {
        let table = self.ensure_documents_table().await?;
        let storage_key = key.storage_key();
        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(format!("skill_id = '{}'", escape_sql_literal(&storage_key)))
            .limit(1)
            .execute()
            .await
            .with_context(|| format!("query LanceDB document for {storage_key}"))?
            .try_collect()
            .await
            .with_context(|| format!("read LanceDB document batch for {storage_key}"))?;

        for batch in batches {
            if batch.num_rows() == 0 {
                continue;
            }

            let skill_ids = batch
                .column_by_name("skill_id")
                .context("skill mesh documents batch missing skill_id column")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("skill mesh documents skill_id column is not Utf8")?;
            let payloads = batch
                .column_by_name("document_json")
                .context("skill mesh documents batch missing document_json column")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("skill mesh documents document_json column is not Utf8")?;

            for row_index in 0..batch.num_rows() {
                if skill_ids.value(row_index) != storage_key {
                    continue;
                }

                let stored = SkillMeshStoredDocument {
                    skill_id: skill_ids.value(row_index).to_string(),
                    document_json: payloads.value(row_index).to_string(),
                };
                let stored_key = SkillMeshDocumentKey::from_storage_key(&stored.skill_id);
                if stored_key != *key {
                    bail!(
                        "skill mesh document storage key mismatch: requested {}, found {}",
                        storage_key,
                        stored.skill_id
                    );
                }

                let document = stored
                    .into_document()
                    .with_context(|| format!("deserialize LanceDB document for {storage_key}"))?;
                if document.document_key() != stored_key {
                    bail!(
                        "skill mesh document identity mismatch for {}: payload key {} does not match stored key {}",
                        storage_key,
                        document.document_key().storage_key(),
                        stored_key.storage_key()
                    );
                }

                return Ok(Some(document));
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl SkillMeshIndex for LanceDbSkillMeshIndex {
    async fn upsert_document(&self, document: SkillMeshDocument) -> Result<()> {
        let key = document.document_key();
        validate_embedding_identities(&key, &document.embedding_records)?;
        let _mutation_guard = self.mutation_lock.lock().await;
        let previous = self.load_document(&key).await?;

        let write_result = async {
            // document_json is the canonical stored payload; embeddings are a derived index.
            self.replace_document(&key, &document).await?;
            #[cfg(test)]
            maybe_pause_after_document_replace_for_tests(&self.db_path, &key.storage_key()).await;
            self.replace_embeddings(&key, &document.embedding_records)
                .await
        }
        .await;

        if let Err(error) = write_result {
            if let Err(rollback_error) = self.restore_document_state(&key, previous.as_ref()).await
            {
                return Err(error.context(format!(
                    "skill mesh LanceDB upsert rollback failed for {}: {rollback_error:#}",
                    key.storage_key()
                )));
            }
            return Err(error);
        }

        Ok(())
    }

    async fn get_document(&self, key: &SkillMeshDocumentKey) -> Result<Option<SkillMeshDocument>> {
        self.load_document(key).await
    }

    async fn apply_feedback(
        &self,
        key: &SkillMeshDocumentKey,
        outcome: SkillMeshOutcome,
    ) -> Result<Option<SkillMeshFeedbackState>> {
        let _mutation_guard = self.mutation_lock.lock().await;
        let Some(mut document) = self.load_document(key).await? else {
            return Ok(None);
        };
        let previous = document.clone();
        let state = document.apply_feedback(outcome);

        if let Err(error) = self.replace_document(key, &document).await {
            if let Err(rollback_error) = self.restore_document_state(key, Some(&previous)).await {
                return Err(error.context(format!(
                    "skill mesh feedback rollback failed for {}: {rollback_error:#}",
                    key.storage_key()
                )));
            }
            return Err(error);
        }

        Ok(Some(state))
    }
}

struct PreparedEmbeddingBatch {
    vector_len: i32,
    batch: RecordBatch,
}

fn prepare_embedding_batch(
    records: &[SkillMeshEmbeddingRecord],
) -> Result<Option<PreparedEmbeddingBatch>> {
    if records.is_empty() {
        return Ok(None);
    }

    let vector_len =
        i32::try_from(records[0].vector.len()).context("skill mesh vector dimension overflow")?;
    let batch = embedding_batch(records)?;
    Ok(Some(PreparedEmbeddingBatch { vector_len, batch }))
}

fn validate_embedding_identities(
    key: &SkillMeshDocumentKey,
    records: &[SkillMeshEmbeddingRecord],
) -> Result<()> {
    for (index, record) in records.iter().enumerate() {
        let record_key = SkillMeshDocumentKey {
            skill_id: record.skill_id.clone(),
            variant_id: record.variant_id.clone(),
        };
        if record_key != *key {
            bail!(
                "skill mesh embedding identity mismatch at record {index}: expected {}, found {}",
                key.storage_key(),
                record_key.storage_key()
            );
        }
    }
    Ok(())
}

fn documents_schema() -> SchemaRef {
    std::sync::Arc::new(Schema::new(vec![
        Field::new("skill_id", DataType::Utf8, false),
        Field::new("document_json", DataType::Utf8, false),
    ]))
}

fn embeddings_schema(vector_len: i32) -> SchemaRef {
    std::sync::Arc::new(Schema::new(vec![
        Field::new("embedding_id", DataType::Utf8, false),
        Field::new("skill_id", DataType::Utf8, false),
        Field::new("variant_id", DataType::Utf8, true),
        Field::new("embedding_kind", DataType::Utf8, false),
        Field::new("text", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                std::sync::Arc::new(Field::new("item", DataType::Float32, true)),
                vector_len,
            ),
            false,
        ),
        Field::new(
            "capability_path",
            DataType::List(std::sync::Arc::new(Field::new(
                "item",
                DataType::Utf8,
                true,
            ))),
            false,
        ),
        Field::new("trust_tier", DataType::Utf8, false),
        Field::new("risk_level", DataType::Utf8, false),
        Field::new("source_kind", DataType::Utf8, false),
        Field::new("active", DataType::Boolean, false),
    ]))
}

fn document_batch(document: &SkillMeshDocument) -> Result<RecordBatch> {
    let stored = SkillMeshStoredDocument::from_document(document)
        .context("serialize skill mesh document for LanceDB storage")?;
    RecordBatch::try_new(
        documents_schema(),
        vec![
            std::sync::Arc::new(StringArray::from(vec![stored.skill_id.as_str()])) as ArrayRef,
            std::sync::Arc::new(StringArray::from(vec![stored.document_json.as_str()])) as ArrayRef,
        ],
    )
    .context("build skill mesh document record batch")
}

fn embedding_batch(records: &[SkillMeshEmbeddingRecord]) -> Result<RecordBatch> {
    if records.is_empty() {
        bail!("skill mesh embedding batch requires at least one record");
    }

    let vector_len = records[0].vector.len();
    if vector_len == 0 {
        bail!("skill mesh embeddings require at least one dimension");
    }
    if records
        .iter()
        .any(|record| record.vector.len() != vector_len)
    {
        bail!("skill mesh embeddings in a batch must share the same dimension");
    }

    let vector_len = i32::try_from(vector_len).context("skill mesh vector dimension overflow")?;
    let schema = embeddings_schema(vector_len);

    let embedding_ids = StringArray::from(
        records
            .iter()
            .map(|record| record.embedding_id.as_str())
            .collect::<Vec<_>>(),
    );
    let skill_ids = StringArray::from(
        records
            .iter()
            .map(|record| record.skill_id.as_str())
            .collect::<Vec<_>>(),
    );
    let variant_ids = StringArray::from(
        records
            .iter()
            .map(|record| record.variant_id.clone())
            .collect::<Vec<_>>(),
    );
    let embedding_kinds = StringArray::from(
        records
            .iter()
            .map(|record| record.embedding_kind.as_str())
            .collect::<Vec<_>>(),
    );
    let texts = StringArray::from(
        records
            .iter()
            .map(|record| record.text.as_str())
            .collect::<Vec<_>>(),
    );
    let vectors = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
        records
            .iter()
            .map(|record| Some(record.vector.iter().copied().map(Some).collect::<Vec<_>>())),
        vector_len,
    );
    let mut capability_paths = ListBuilder::new(StringBuilder::new());
    for record in records {
        for segment in &record.capability_path {
            capability_paths.values().append_value(segment);
        }
        capability_paths.append(true);
    }
    let trust_tiers = StringArray::from(
        records
            .iter()
            .map(|record| record.trust_tier.as_str())
            .collect::<Vec<_>>(),
    );
    let risk_levels = StringArray::from(
        records
            .iter()
            .map(|record| record.risk_level.as_str())
            .collect::<Vec<_>>(),
    );
    let source_kinds = StringArray::from(
        records
            .iter()
            .map(|record| record.source_kind.as_str())
            .collect::<Vec<_>>(),
    );
    let active = BooleanArray::from(
        records
            .iter()
            .map(|record| record.active)
            .collect::<Vec<_>>(),
    );

    RecordBatch::try_new(
        schema,
        vec![
            std::sync::Arc::new(embedding_ids) as ArrayRef,
            std::sync::Arc::new(skill_ids) as ArrayRef,
            std::sync::Arc::new(variant_ids) as ArrayRef,
            std::sync::Arc::new(embedding_kinds) as ArrayRef,
            std::sync::Arc::new(texts) as ArrayRef,
            std::sync::Arc::new(vectors) as ArrayRef,
            std::sync::Arc::new(capability_paths.finish()) as ArrayRef,
            std::sync::Arc::new(trust_tiers) as ArrayRef,
            std::sync::Arc::new(risk_levels) as ArrayRef,
            std::sync::Arc::new(source_kinds) as ArrayRef,
            std::sync::Arc::new(active) as ArrayRef,
        ],
    )
    .context("build skill mesh embedding record batch")
}

fn escape_sql_literal(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
fn replace_document_failpoint_slot() -> &'static Mutex<Option<(PathBuf, String)>> {
    REPLACE_DOCUMENT_FAILPOINT.get_or_init(|| Mutex::new(None))
}

#[cfg(test)]
fn pause_after_document_replace_slot() -> &'static Mutex<Option<DocumentReplacePausepoint>> {
    PAUSE_AFTER_DOCUMENT_REPLACE.get_or_init(|| Mutex::new(None))
}

#[cfg(test)]
fn maybe_fail_replace_document_for_tests(db_path: &Path, storage_key: &str) -> Result<()> {
    let mut slot = replace_document_failpoint_slot()
        .lock()
        .expect("replace-document failpoint mutex poisoned");
    if slot
        .as_ref()
        .is_some_and(|(path, key)| path == db_path && key == storage_key)
    {
        slot.take();
        bail!("test failpoint: replace_document failed for {storage_key}");
    }
    Ok(())
}

#[cfg(test)]
async fn maybe_pause_after_document_replace_for_tests(db_path: &Path, storage_key: &str) {
    let pausepoint = {
        let mut slot = pause_after_document_replace_slot()
            .lock()
            .expect("document-replace pausepoint mutex poisoned");
        if slot.as_ref().is_some_and(|pausepoint| {
            pausepoint.db_path == db_path && pausepoint.storage_key == storage_key
        }) {
            slot.take()
        } else {
            None
        }
    };

    if let Some(pausepoint) = pausepoint {
        pausepoint.hit_flag.store(true, Ordering::SeqCst);
        pausepoint.hit.notify_waiters();
        pausepoint.resume.notified().await;
    }
}

fn embedding_key_filter(key: &SkillMeshDocumentKey) -> String {
    let skill_id = escape_sql_literal(&key.skill_id);
    match key.variant_id.as_deref() {
        Some(variant_id) => format!(
            "skill_id = '{}' AND variant_id = '{}'",
            skill_id,
            escape_sql_literal(variant_id)
        ),
        None => format!("skill_id = '{}' AND variant_id IS NULL", skill_id),
    }
}
