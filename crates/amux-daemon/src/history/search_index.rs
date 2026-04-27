use anyhow::{anyhow, Context, Result};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak},
    time::{Duration, Instant},
};
use tantivy::collector::TopDocs;
use tantivy::directory::error::LockError;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, TantivyDocument, Value, FAST, STORED, STRING, TEXT};
use tantivy::snippet::SnippetGenerator;
use tantivy::{Index, IndexWriter, TantivyError, Term};

const INDEX_WRITER_HEAP_BYTES: usize = 50_000_000;
const INDEX_WRITER_LOCK_WAIT: Duration = Duration::from_millis(200);
const INDEX_WRITER_LOCK_RETRY_DELAY: Duration = Duration::from_millis(10);
const SOURCE_KEY_SEPARATOR: &str = ":";
static TANTIVY_WRITER_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SearchSourceKind {
    HistoryEntry,
    Guideline,
    AgentMessage,
    AgentEvent,
    CausalTrace,
    Counterfactual,
    MetaCognition,
    ActionAudit,
}

impl SearchSourceKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::HistoryEntry => "history_entry",
            Self::Guideline => "guideline",
            Self::AgentMessage => "agent_message",
            Self::AgentEvent => "agent_event",
            Self::CausalTrace => "causal_trace",
            Self::Counterfactual => "counterfactual",
            Self::MetaCognition => "meta_cognition",
            Self::ActionAudit => "action_audit",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "history_entry" => Some(Self::HistoryEntry),
            "guideline" => Some(Self::Guideline),
            "agent_message" => Some(Self::AgentMessage),
            "agent_event" => Some(Self::AgentEvent),
            "causal_trace" => Some(Self::CausalTrace),
            "counterfactual" => Some(Self::Counterfactual),
            "meta_cognition" => Some(Self::MetaCognition),
            "action_audit" => Some(Self::ActionAudit),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SearchDocument {
    pub source_kind: SearchSourceKind,
    pub source_id: String,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub workspace_id: Option<String>,
    pub thread_id: Option<String>,
    pub agent_id: Option<String>,
    pub timestamp: i64,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct SearchRequest {
    pub query: String,
    pub limit: usize,
    pub source_kinds: Vec<SearchSourceKind>,
    pub workspace_id: Option<String>,
    pub thread_id: Option<String>,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct SearchHitRef {
    pub source_kind: SearchSourceKind,
    pub source_id: String,
    pub score: f32,
    pub title: String,
    pub snippet: Option<String>,
    pub timestamp: Option<i64>,
}

#[derive(Clone)]
pub(crate) struct SearchIndex {
    index: Index,
    fields: SearchFields,
    writer_lock: Arc<Mutex<()>>,
}

#[derive(Clone)]
struct SearchFields {
    source_key: Field,
    source_kind: Field,
    source_id: Field,
    workspace_id: Field,
    thread_id: Field,
    agent_id: Field,
    title: Field,
    body: Field,
    tags: Field,
    timestamp: Field,
    metadata_json: Field,
}

impl SearchIndex {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        std::fs::create_dir_all(path)
            .with_context(|| format!("create search index directory {}", path.display()))?;
        let schema = build_schema();
        let index = match Index::open_in_dir(path) {
            Ok(index) => index,
            Err(_) => Index::create_in_dir(path, schema.clone())
                .with_context(|| format!("create tantivy index {}", path.display()))?,
        };
        let fields = SearchFields::from_schema(index.schema())?;
        let writer_lock = writer_lock_for_path(path)?;
        Ok(Self {
            index,
            fields,
            writer_lock,
        })
    }

    pub(crate) fn upsert(&self, document: SearchDocument) -> Result<()> {
        self.upsert_many(vec![document])
    }

    pub(crate) fn upsert_many(&self, documents: Vec<SearchDocument>) -> Result<()> {
        if documents.is_empty() {
            return Ok(());
        }

        let deadline = Instant::now() + INDEX_WRITER_LOCK_WAIT;
        let _writer_guard = self
            .writer_lock
            .lock()
            .map_err(|_| anyhow!("tantivy index writer lock poisoned"))?;
        let mut writer = self.open_writer_with_retry(deadline)?;
        for document in documents {
            writer.delete_term(Term::from_field_text(
                self.fields.source_key,
                &source_key(document.source_kind, &document.source_id),
            ));
            writer.add_document(self.to_tantivy_document(document))?;
        }
        writer.commit().context("commit tantivy document upserts")?;
        Ok(())
    }

    fn open_writer_with_retry(&self, deadline: Instant) -> Result<IndexWriter<TantivyDocument>> {
        loop {
            match self.index.writer(INDEX_WRITER_HEAP_BYTES) {
                Ok(writer) => return Ok(writer),
                Err(error) if is_writer_lock_busy(&error) && !writer_deadline_elapsed(deadline) => {
                    std::thread::sleep(INDEX_WRITER_LOCK_RETRY_DELAY);
                }
                Err(error) => return Err(error).context("open tantivy index writer"),
            }
        }
    }

    pub(crate) fn search(&self, request: SearchRequest) -> Result<Vec<SearchHitRef>> {
        if request.limit == 0 || request.query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let reader = self.index.reader().context("open tantivy index reader")?;
        let searcher = reader.searcher();
        let query_parser = QueryParser::for_index(
            &self.index,
            vec![self.fields.title, self.fields.body, self.fields.tags],
        );
        let query = query_parser
            .parse_query(request.query.trim())
            .or_else(|_| query_parser.parse_query(&escape_query_terms(&request.query)))
            .context("parse tantivy search query")?;

        let top_docs = searcher
            .search(
                &query,
                &TopDocs::with_limit(request.limit.saturating_mul(8).max(16)),
            )
            .context("run tantivy search")?;
        let mut snippet_generator =
            SnippetGenerator::create(&searcher, query.as_ref(), self.fields.body).ok();

        let mut hits = Vec::new();
        for (score, address) in top_docs {
            let doc: TantivyDocument = searcher.doc(address)?;
            let Some(source_kind) =
                get_text(&doc, self.fields.source_kind).and_then(SearchSourceKind::from_str)
            else {
                continue;
            };
            if !request.source_kinds.is_empty() && !request.source_kinds.contains(&source_kind) {
                continue;
            }
            if !matches_optional_filter(
                &doc,
                self.fields.workspace_id,
                request.workspace_id.as_deref(),
            ) || !matches_optional_filter(
                &doc,
                self.fields.thread_id,
                request.thread_id.as_deref(),
            ) || !matches_optional_filter(
                &doc,
                self.fields.agent_id,
                request.agent_id.as_deref(),
            ) {
                continue;
            }

            let snippet = snippet_generator
                .as_mut()
                .map(|generator| generator.snippet_from_doc(&doc).to_html());
            hits.push(SearchHitRef {
                source_kind,
                source_id: get_text(&doc, self.fields.source_id)
                    .unwrap_or_default()
                    .to_string(),
                score,
                title: get_text(&doc, self.fields.title)
                    .unwrap_or_default()
                    .to_string(),
                snippet,
                timestamp: doc
                    .get_first(self.fields.timestamp)
                    .and_then(|value| value.as_i64()),
            });
            if hits.len() >= request.limit {
                break;
            }
        }

        Ok(hits)
    }

    fn to_tantivy_document(&self, document: SearchDocument) -> TantivyDocument {
        let mut tantivy_doc = TantivyDocument::default();
        tantivy_doc.add_text(
            self.fields.source_key,
            source_key(document.source_kind, &document.source_id),
        );
        tantivy_doc.add_text(self.fields.source_kind, document.source_kind.as_str());
        tantivy_doc.add_text(self.fields.source_id, document.source_id);
        if let Some(workspace_id) = document.workspace_id {
            tantivy_doc.add_text(self.fields.workspace_id, workspace_id);
        }
        if let Some(thread_id) = document.thread_id {
            tantivy_doc.add_text(self.fields.thread_id, thread_id);
        }
        if let Some(agent_id) = document.agent_id {
            tantivy_doc.add_text(self.fields.agent_id, agent_id);
        }
        tantivy_doc.add_text(self.fields.title, document.title);
        tantivy_doc.add_text(self.fields.body, document.body);
        for tag in document.tags {
            tantivy_doc.add_text(self.fields.tags, tag);
        }
        tantivy_doc.add_i64(self.fields.timestamp, document.timestamp);
        if let Some(metadata_json) = document.metadata_json {
            tantivy_doc.add_text(self.fields.metadata_json, metadata_json);
        }
        tantivy_doc
    }
}

impl super::HistoryStore {
    pub(crate) fn upsert_search_document(&self, document: SearchDocument) {
        let Some(index) = &self.search_index else {
            return;
        };
        let source_kind = document.source_kind.as_str();
        let source_id = document.source_id.clone();
        if let Err(error) = index.upsert(document) {
            if error_contains_writer_lock_busy(&error) {
                tracing::debug!(
                    error = %error,
                    source_kind,
                    source_id,
                    "tantivy writer busy; skipping search index upsert"
                );
            } else {
                tracing::warn!(
                    error = %error,
                    source_kind,
                    source_id,
                    "failed to upsert document into tantivy search index"
                );
            }
        }
    }

    pub(crate) fn upsert_search_documents_background(&self, documents: Vec<SearchDocument>) {
        if documents.is_empty() {
            return;
        }
        let Some(index) = &self.search_index else {
            return;
        };
        let index = index.clone();
        let document_count = documents.len();
        let spawn_result = std::thread::Builder::new()
            .name("tamux-search-index".to_string())
            .spawn(move || {
                if let Err(error) = index.upsert_many(documents) {
                    if error_contains_writer_lock_busy(&error) {
                        tracing::debug!(
                            error = %error,
                            document_count,
                            "tantivy writer busy; skipping search index batch upsert"
                        );
                    } else {
                        tracing::warn!(
                            error = %error,
                            document_count,
                            "failed to upsert document batch into tantivy search index"
                        );
                    }
                }
            });
        if let Err(error) = spawn_result {
            tracing::warn!(
                error = %error,
                document_count,
                "failed to spawn tantivy search index batch worker"
            );
        }
    }
}

fn build_schema() -> Schema {
    let mut builder = Schema::builder();
    builder.add_text_field("source_key", STRING | STORED);
    builder.add_text_field("source_kind", STRING | STORED);
    builder.add_text_field("source_id", STRING | STORED);
    builder.add_text_field("workspace_id", STRING | STORED);
    builder.add_text_field("thread_id", STRING | STORED);
    builder.add_text_field("agent_id", STRING | STORED);
    builder.add_text_field("title", TEXT | STORED);
    builder.add_text_field("body", TEXT | STORED);
    builder.add_text_field("tags", TEXT | STORED);
    builder.add_i64_field("timestamp", FAST | STORED);
    builder.add_text_field("metadata_json", STORED);
    builder.build()
}

impl SearchFields {
    fn from_schema(schema: Schema) -> Result<Self> {
        Ok(Self {
            source_key: schema.get_field("source_key")?,
            source_kind: schema.get_field("source_kind")?,
            source_id: schema.get_field("source_id")?,
            workspace_id: schema.get_field("workspace_id")?,
            thread_id: schema.get_field("thread_id")?,
            agent_id: schema.get_field("agent_id")?,
            title: schema.get_field("title")?,
            body: schema.get_field("body")?,
            tags: schema.get_field("tags")?,
            timestamp: schema.get_field("timestamp")?,
            metadata_json: schema.get_field("metadata_json")?,
        })
    }
}

fn source_key(source_kind: SearchSourceKind, source_id: &str) -> String {
    format!(
        "{}{}{}",
        source_kind.as_str(),
        SOURCE_KEY_SEPARATOR,
        source_id
    )
}

fn get_text(document: &TantivyDocument, field: Field) -> Option<&str> {
    document.get_first(field).and_then(|value| value.as_str())
}

fn matches_optional_filter(
    document: &TantivyDocument,
    field: Field,
    expected: Option<&str>,
) -> bool {
    match expected {
        Some(expected) => get_text(document, field) == Some(expected),
        None => true,
    }
}

fn is_writer_lock_busy(error: &TantivyError) -> bool {
    matches!(error, TantivyError::LockFailure(LockError::LockBusy, _))
}

fn writer_lock_for_path(path: &Path) -> Result<Arc<Mutex<()>>> {
    let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let registry = TANTIVY_WRITER_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut registry = registry
        .lock()
        .map_err(|_| anyhow!("tantivy index writer lock registry poisoned"))?;
    if let Some(existing) = registry.get(&key).and_then(Weak::upgrade) {
        return Ok(existing);
    }

    let lock = Arc::new(Mutex::new(()));
    registry.insert(key, Arc::downgrade(&lock));
    Ok(lock)
}

fn writer_deadline_elapsed(deadline: Instant) -> bool {
    Instant::now() >= deadline
}

fn error_contains_writer_lock_busy(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<TantivyError>()
            .is_some_and(is_writer_lock_busy)
    })
}

fn escape_query_terms(query: &str) -> String {
    query
        .split_whitespace()
        .map(|term| format!("\"{}\"", term.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" ")
}
