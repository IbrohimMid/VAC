//! TUI - VAC Terminal User Interface
//!
//! This module contains the TUI implementation for VAC (Vastar Agentic CLI).
//!
//! ## Architecture
//!
//! - `app/` - Application state and event definitions
//! - `services/` - UI services
//! - `adapter/` - Bridge between TUI types and VAC engine
//! - `event_loop.rs` - Main event loop
//! - `view.rs` - Rendering logic
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
pub mod types;
pub mod terminal;
pub mod runner;

pub mod services;
pub mod view;
pub mod event_loop;

// Types
pub use types::{
    ContentPart, FunctionCall, Model, ToolCall, ToolCallResult, ToolCallResultStatus,
};

pub use adapter::VacEngineAdapter;
pub use terminal::TerminalGuard;

// App module
pub mod app;
mod event;

pub use app::{InputEvent, LoadingOperation, OutputEvent};
pub use event::map_crossterm_event_to_input_event;
pub use event_loop::{run_tui, RulebookConfig};
pub use runner::run_vac_tui;