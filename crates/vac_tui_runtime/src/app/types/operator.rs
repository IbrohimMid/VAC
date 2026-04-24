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
    /// D.4 — operator-customised statusline template. When
    /// `Some`, the statusline renderer swaps in this template
    /// (loaded from `.vac/statusline.tmpl`) instead of the
    /// built-in layout. Placeholders: `{model}`, `{tokens}`,
    /// `{mode}`, `{mcp}`, `{pulse}`.
    pub statusline_template: Option<String>,
    /// D.4 — output style preset. Controls conversation-lane
    /// rendering verbosity.
    pub output_style: OutputStyle,
}

/// D.4 — `/output-style <normal|quiet|json>` toggles.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputStyle {
    #[default]
    Normal,
    /// Suppresses activity-timer + non-essential decoration.
    Quiet,
    /// Each assistant / tool event emits one NDJSON line (for
    /// scripting + downstream pipes).
    Json,
}

impl OutputStyle {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Quiet => "quiet",
            Self::Json => "json",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "normal" | "default" => Some(Self::Normal),
            "quiet" | "silent" => Some(Self::Quiet),
            "json" | "ndjson" => Some(Self::Json),
            _ => None,
        }
    }
}
