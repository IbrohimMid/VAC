//! VIL Memory — Three-tier persistent memory system.

pub mod episodic;
pub mod error;
pub mod semantic;
pub mod store;

pub use error::MemoryError;
pub use store::{MemoryConfig, MemoryStore};
