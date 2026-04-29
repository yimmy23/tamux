#[cfg(feature = "lancedb-vector")]
pub mod compiler;
#[cfg(feature = "lancedb-vector")]
pub mod index;
#[cfg(feature = "lancedb-vector")]
pub mod retrieval;
#[cfg(feature = "lancedb-vector")]
pub mod store;
pub mod types;
#[cfg(feature = "lancedb-vector")]
pub mod watcher;

#[cfg(all(test, feature = "lancedb-vector"))]
pub use index::{InMemorySkillMeshIndex, LanceDbSkillMeshIndex, SkillMeshIndex};
#[cfg(all(test, feature = "lancedb-vector"))]
pub use types::{
    SkillMeshDocument, SkillMeshDocumentKey, SkillMeshEmbeddingRecord, SkillMeshFeedbackState,
    SkillMeshIntent, SkillMeshOutcome, SkillMeshResult,
};
