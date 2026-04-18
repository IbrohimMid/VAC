//! Adapter Module
//!
//! This module provides adapters between Stakpak TUI types and VAC engine.
//! The TUI shell is from Stakpak, but the engine/runtime is VAC-native.

pub mod engine;
pub mod types;

pub use engine::VacEngineAdapter;
pub use types::*;
