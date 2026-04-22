//! Shared style helpers used across view and workbench modules.

use ratatui::style::Style;

use crate::services::theme::{StyleKey, Theme};

/// Returns the standard focused/unfocused style for panel borders and titles,
/// resolved through the active [`Theme`].
pub fn focus_style(focused: bool, theme: &Theme) -> Style {
    if focused {
        theme.style(StyleKey::FocusBorder)
    } else {
        theme.style(StyleKey::UnfocusBorder)
    }
}
