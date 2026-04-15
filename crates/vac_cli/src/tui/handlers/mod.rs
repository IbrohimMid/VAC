//! Popup and feature handlers for TUI.
//!
//! This module provides dedicated controllers for each popup/feature,
//! implementing a reducer-style API to keep event_loop clean.

pub mod approval;
pub mod changeset;
pub mod file_search;
pub mod model_switcher;
pub mod review;

use crate::tui::app::{AppState, OutputEvent};
use tokio::sync::mpsc::Sender;

/// Result type for handler operations.
pub type HandlerResult = Result<(), String>;

/// Common context passed to all handlers.
pub struct HandlerContext<'a> {
    pub state: &'a mut AppState,
    pub output_tx: &'a Sender<OutputEvent>,
}

impl<'a> HandlerContext<'a> {
    pub fn new(state: &'a mut AppState, output_tx: &'a Sender<OutputEvent>) -> Self {
        Self { state, output_tx }
    }
}
