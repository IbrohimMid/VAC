//! VIL Context — SHM-backed context engine.

pub mod attention;
pub mod chunking;
pub mod engine;
pub mod error;
pub mod shm;

pub use engine::{ContextConfig, ContextEngine};
pub use error::ContextError;
