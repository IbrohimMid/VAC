//! `OperatorState` — groups operator-facing cursor/selection state
//! that used to live as a flat scatter on `AppState`. Keeping these
//! together clarifies that they all track "where the human's attention
//! is", distinct from session metadata or runtime state.

use crate::types::Model;
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct OperatorState {
    /// Currently active model (if the operator picked one).
    pub current_model: Option<Model>,
    /// Highlighted row in the sessions list overlay.
    pub sessions_selected_idx: usize,
    /// Highlighted row in the theme picker overlay.
    pub theme_picker_selected: usize,
    /// Highlighted row in the per-message action popup.
    pub message_action_popup_selected: usize,
    /// Message whose action popup is currently open, if any.
    pub message_action_target_id: Option<Uuid>,
}
