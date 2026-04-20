//! Shared style helpers used across view and workbench modules.

use ratatui::style::{Color, Modifier, Style};

/// Returns the standard focused/unfocused style for panel borders and titles.
///
/// - Focused: Yellow + Bold
/// - Unfocused: DarkGray (visually receded, not invisible)
pub fn focus_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}
