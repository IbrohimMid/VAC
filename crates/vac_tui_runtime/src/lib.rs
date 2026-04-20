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
pub mod handlers;
pub mod runner;
pub mod terminal;
pub mod types;

pub mod event_loop;
pub mod services;
pub mod update;
pub mod view;

// Types
pub use types::{ContentPart, FunctionCall, Model, ToolCall, ToolCallResult, ToolCallResultStatus};

pub use adapter::VacEngineAdapter;
pub use terminal::TerminalGuard;

// App module
pub mod action_registry;
pub mod app;
pub mod controller;
mod event;
pub mod overlay;
pub mod ui;
pub mod workbench;

#[cfg(test)]
mod contracts_test;

pub use app::{InputEvent, LoadingOperation, OutputEvent};
pub use event::map_crossterm_event_to_input_event;
pub use event_loop::{RulebookConfig, run_tui};
pub use runner::run_vac_tui;
