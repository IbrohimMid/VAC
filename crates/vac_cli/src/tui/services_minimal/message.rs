//! Message Service
//!
//! Handles message rendering and caching.

use crate::tui::app::Message;
use ratatui::text::Line;

/// Invalidate the message lines cache
pub fn invalidate_message_lines_cache(_state: &mut crate::tui::app::AppState) {
    // Stub - will be implemented when full services are activated
}

/// Render messages to lines
pub fn render_messages_to_lines(
    messages: &[Message],
    _width: usize,
) -> Vec<Line<'static>> {
    messages
        .iter()
        .flat_map(|msg| {
            let prefix = match msg.role.as_str() {
                "user" => "You: ",
                "assistant" => "VAC: ",
                _ => "",
            };
            vec![
                Line::raw(format!("{}{}", prefix, msg.content)),
                Line::raw(""),
            ]
        })
        .collect()
}