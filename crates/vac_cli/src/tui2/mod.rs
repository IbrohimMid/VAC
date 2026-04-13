//! TUI2 - VAC Terminal User Interface
//!
//! This module contains the TUI implementation for VAC (Vastar Agentic CLI).
//!
//! ## Architecture
//!
//! - `app/` - Application state and event definitions
//! - `services_minimal/` - Essential UI services
//! - `adapter/` - Bridge between TUI types and VAC engine
//! - `event_loop_minimal.rs` - Main event loop
//! - `view_minimal.rs` - Rendering logic
//!
//! ## License Attribution
//!
//! Portions of this TUI are derived from Stakpak (https://github.com/stakpak/agent)
//! licensed under Apache 2.0. See LICENSE_ATTRIBUTION.md for details.

// TUI crate performs heavy string slicing for text rendering, markdown parsing,
// cursor positioning, and layout. All indices come from find()/rfind()/char_indices()
// on the same strings. Allowing at crate level to avoid 120+ individual annotations.
#![allow(clippy::string_slice)]

pub mod adapter;
pub mod constants;
pub mod stub_types;
pub mod terminal;

// Minimal implementations
pub mod services_minimal;
pub mod view_minimal;
pub mod event_loop_minimal;

// Stub types that replace Stakpak dependencies
pub use stub_types::{
    ContentPart, FunctionCall, Model, ToolCall, ToolCallResult, ToolCallResultStatus,
};

pub use adapter::VacEngineAdapter;
pub use terminal::TerminalGuard;

// App module
mod app;
mod event;

pub use app::{InputEvent, LoadingOperation, OutputEvent};
pub use event::map_crossterm_event_to_input_event;
pub use event_loop_minimal::{run_tui, RulebookConfig};