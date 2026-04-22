//! Persistence layer for the RAG index (Paket C scaffold for B2).
//!
//! **Scaffolding only.** The runtime still keeps everything in memory via
//! [`crate::index::RagIndex`]; this module exists so the next session can
//! land the redb-backed store without another round of module layout churn.
//!
//! Planned on-disk layout (rooted at `.vac/rag/`):
//!
//! ```text
//! .vac/rag/
//!   ├── store.redb          # id -> IndexedDocument (embedded + metadata)
//!   ├── graph.hnsw          # instant_distance::Hnsw serialised via bincode
//!   └── meta.json           # schema version, embedding model id, dim, ...
//! ```
//!
//! The API surface here is intentionally opinionated: one struct
//! ([`RagStore`]) that owns the redb `Database` and exposes
//! open/insert/scan/close. Pulling the serde-derived types out of
//! [`crate::index`] into this module in a later PR will keep
//! `index.rs` as a thin facade over `store` + `ann`.

use crate::error::{RagError, RagResult};
use std::path::{Path, PathBuf};

/// Schema marker persisted in `meta.json`. Bump when the on-disk layout
/// changes in a backwards-incompatible way.
pub const STORE_SCHEMA_VERSION: u32 = 1;

/// Paths derived from a `.vac/rag/` root. Exposed so callers don't have to
/// duplicate the layout convention when wiring up Paket C's persistence.
#[derive(Debug, Clone)]
pub struct StoreLayout {
    pub root: PathBuf,
    pub database: PathBuf,
    pub graph: PathBuf,
    pub meta: PathBuf,
}

impl StoreLayout {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        Self {
            database: root.join("store.redb"),
            graph: root.join("graph.hnsw"),
            meta: root.join("meta.json"),
            root,
        }
    }

    /// Ensure the root directory exists. We do this synchronously because
    /// store initialisation is an infrequent, startup-only event.
    pub fn ensure_dirs(&self) -> RagResult<()> {
        let root = &self.root;
        let res = std::fs::create_dir_all(root); // allow_sync_io: one-time init mkdir, not hot-path
        res.map_err(|e| RagError::Database(format!("create {}: {e}", root.display())))?;
        Ok(())
    }
}

/// Placeholder store handle. Today it only records the layout; Paket C's
/// next session will promote this to own a `redb::Database` and expose
/// `insert` / `get` / `scan` methods that the `RagIndex` can delegate to.
#[derive(Debug)]
pub struct RagStore {
    layout: StoreLayout,
}

impl RagStore {
    /// Open (or prepare to open) a store at the given root. Today this is a
    /// no-op beyond layout resolution and directory creation; it returns a
    /// handle so downstream code can already pass `&RagStore` around and
    /// compile against the final shape.
    pub fn open(root: impl AsRef<Path>) -> RagResult<Self> {
        let layout = StoreLayout::new(root);
        layout.ensure_dirs()?;
        Ok(Self { layout })
    }

    pub fn layout(&self) -> &StoreLayout {
        &self.layout
    }

    /// Placeholder — future persistence entrypoint.
    ///
    /// Returns `Err(Indexing("not implemented"))` today. When Paket C lands
    /// the redb wiring, this becomes the one-stop `insert` used by both the
    /// HashMap migration path and the HNSW rebuild path.
    pub fn persist_placeholder(&self) -> RagResult<()> {
        Err(RagError::Indexing(
            "vil_rag::store: persistence not implemented (Paket C follow-up)".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_derives_expected_paths() {
        let l = StoreLayout::new("/tmp/vac/rag");
        assert_eq!(l.database, std::path::Path::new("/tmp/vac/rag/store.redb"));
        assert_eq!(l.graph, std::path::Path::new("/tmp/vac/rag/graph.hnsw"));
        assert_eq!(l.meta, std::path::Path::new("/tmp/vac/rag/meta.json"));
    }

    #[test]
    fn open_creates_root_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = RagStore::open(tmp.path()).expect("open");
        assert!(store.layout().root.exists());
    }

    #[test]
    fn persist_placeholder_is_not_implemented_yet() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = RagStore::open(tmp.path()).expect("open");
        assert!(store.persist_placeholder().is_err());
    }
}
