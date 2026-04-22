//! VIL RAG — Semantic indexing and retrieval for codebase search.
//!
//! Layout (as of Paket C / B2 scaffold):
//!
//! * [`embedding`] — fastembed-backed text → vector.
//! * [`index`]     — current HashMap-backed in-memory index (still the
//!                    public entrypoint; unchanged during scaffolding).
//! * [`ann`]       — HNSW wrapper (opt-in via the `hnsw` feature).
//! * [`store`]     — persistence scaffold for the future redb store.
//! * [`error`]     — shared error + result types.

pub mod ann;
pub mod embedding;
pub mod error;
pub mod index;
pub mod store;

pub use error::RagError;
pub use index::RagIndex;
