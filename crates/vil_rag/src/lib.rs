//! VIL RAG — Semantic indexing and retrieval for codebase search.

pub mod embedding;
pub mod error;
pub mod index;

pub use error::RagError;
pub use index::RagIndex;
