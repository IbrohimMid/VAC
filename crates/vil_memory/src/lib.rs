//! VIL Memory — Three-tier persistent memory system.
//!
//! **Status (2026-04-23):** the redb-backed `MemoryStore` below is
//! the legacy store still consumed by `vac_core::engine::VacEngine`.
//! New consumers should bind [`adapter::VacMemoryBridge`] which
//! delegates to the filesystem-backed `vac_memory` crate
//! (memdir + tf-idf retrieval). Full retirement of the redb path
//! will flip the consumers in `vac_core::engine` behind a single
//! commit once the bridge has proven itself in `vil_swarm`.

pub mod adapter;
pub mod error;
pub mod store;

pub use adapter::VacMemoryBridge;
pub use error::MemoryError;
pub use store::{MemoryConfig, MemoryEntry, MemoryStore, MemoryType};
