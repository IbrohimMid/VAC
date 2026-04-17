//! Popup and feature handlers for TUI.
//!
//! This module provides dedicated controllers for each popup/feature,
//! implementing a reducer-style API to keep event_loop clean.
//!
//! ## Architecture
//!
//! Each handler module provides functions that operate on `HandlerContext`,
//! which encapsulates mutable state access and output event emission.
//!
//! ## Handler API Pattern
//!
//! Handlers implement a consistent reducer-style API:
//! - `open(ctx)` - Open popup/feature
//! - `close(ctx)` - Close popup/feature
//! - `select_next(ctx)` - Navigate forward
//! - `select_prev(ctx)` - Navigate backward
//! - `submit(ctx)` / `insert(ctx)` / `revert(ctx)` - Confirm action
//!
//! ## Example
//!
//! ```ignore
//! use crate::tui::handlers::{HandlerContext, file_search};
//!
//! let mut ctx = HandlerContext::new(&mut state, &output_tx);
//! file_search::open(&mut ctx);
//! file_search::update_query(&mut ctx, "main.rs".to_string());
//! file_search::insert_selected(&mut ctx);
//! ```

pub mod approval;
pub mod changeset;
pub mod file_search;
pub mod isolation_switcher;
pub mod message_action;
pub mod model_switcher;
pub mod profile_switcher;
pub mod review;
pub mod rulebook_switcher;
pub mod vil_workbench;

use crate::tui::app::{AppState, OutputEvent};
use tokio::sync::mpsc::Sender;

/// Result type for handler operations.
///
/// Currently all handlers are infallible and return `()`.
/// Using Result for future extensibility (e.g., validation errors).
pub type HandlerResult = Result<(), String>;

/// Common context passed to all handlers.
///
/// Provides unified access to application state and output event channel.
pub struct HandlerContext<'a> {
    /// Mutable reference to application state
    pub state: &'a mut AppState,
    /// Channel for emitting output events to runner/engine
    pub output_tx: &'a Sender<OutputEvent>,
}

impl<'a> HandlerContext<'a> {
    /// Create a new handler context.
    pub fn new(state: &'a mut AppState, output_tx: &'a Sender<OutputEvent>) -> Self {
        Self { state, output_tx }
    }
}
