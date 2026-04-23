//! VIL Memory — Three-tier persistent memory system.

pub mod error;
pub mod store;

pub use error::MemoryError;
pub use store::{MemoryConfig, MemoryStore, MemoryEntry, MemoryType};
