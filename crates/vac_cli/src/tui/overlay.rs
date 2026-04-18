//! Overlay Contract
//!
//! Provides a unified interface for all modal overlays and popups.

use crate::tui::app::{AppState, InputEvent, OutputEvent};
use crate::tui::handlers::HandlerResult;
use ratatui::Frame;
use tokio::sync::mpsc::Sender;

/// Trait for rendering and handling input for UI overlays.
pub trait Overlay {
    /// Render the overlay
    fn render(&self, f: &mut Frame, state: &AppState);

    /// Handle an input event.
    /// Returns `true` if the event was consumed (and thus should not be propagated),
    /// or `false` if it should fall through to the underlying workspace.
    fn handle_event(
        &mut self,
        state: &mut AppState,
        output_tx: &Sender<OutputEvent>,
        event: InputEvent,
    ) -> Result<bool, String>;

    /// Indicates whether this overlay is currently active/visible.
    fn is_active(&self, state: &AppState) -> bool;
}
