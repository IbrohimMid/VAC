//! Adapter Module
//!
//! This module provides adapters between Stakpak TUI types and VAC engine.
//! The TUI shell is from Stakpak, but the engine/runtime is VAC-native.

pub mod types;
pub mod engine;

pub use types::*;
pub use engine::VacEngineAdapter;