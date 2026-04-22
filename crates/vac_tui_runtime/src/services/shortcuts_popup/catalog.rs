//! Shortcut catalog and static entries
//!
//! This module provides the shortcut data structure, the catalog of all
//! keyboard shortcuts, and rendering helpers for cached shortcut content.

use crate::services::theme::{StyleKey, Theme};
use ratatui::{
    style::Modifier,
    text::{Line, Span},
};

#[derive(Debug, Clone)]
pub struct Shortcut {
    pub key: String,
    pub description: String,
    pub category: String,
}

impl Shortcut {
    pub fn new(key: &str, description: &str, category: &str) -> Self {
        Self {
            key: key.to_string(),
            description: description.to_string(),
            category: category.to_string(),
        }
    }
}

pub fn get_all_shortcuts() -> Vec<Shortcut> {
    let mut shortcuts = vec![
        // Navigation
        Shortcut::new("↑/↓", "Navigate messages", "Navigation"),
        Shortcut::new("Page Up/Down", "Page through messages", "Navigation"),
        Shortcut::new("Ctrl+↑/↓", "Navigate dropdown/dialog", "Navigation"),
        Shortcut::new("Tab", "Complete command or select file", "Navigation"),
        Shortcut::new("Esc", "Close dialogs/popups", "Navigation"),
        // Text Input
        Shortcut::new("Ctrl+A", "Move cursor to start of line", "Text Input"),
        Shortcut::new("Ctrl+E", "Move cursor to end of line", "Text Input"),
        Shortcut::new("Ctrl+F", "Move cursor right", "Text Input"),
        Shortcut::new("Ctrl+B", "Move cursor left", "Text Input"),
        Shortcut::new("Alt+F", "Move cursor to next word", "Text Input"),
        Shortcut::new("Alt+B", "Move cursor to previous word", "Text Input"),
        Shortcut::new("Ctrl+U", "Delete to start of line", "Text Input"),
        Shortcut::new("Ctrl+W", "Delete previous word", "Text Input"),
        Shortcut::new("Ctrl+H", "Delete previous character", "Text Input"),
        Shortcut::new("Ctrl+J", "Insert newline", "Text Input"),
        Shortcut::new("Enter", "Submit input", "Text Input"),
        Shortcut::new("Backspace", "Delete previous character", "Text Input"),
        // Tool Management
        Shortcut::new("Ctrl+O", "Toggle auto-approve mode", "Tool Management"),
        Shortcut::new("Ctrl+Y", "Toggle side panel", "Tool Management"),
        Shortcut::new("Ctrl+R", "Retry last tool call", "Tool Management"),
        // UI Controls
        Shortcut::new("Ctrl+C", "Quit (double press)", "UI Controls"),
        Shortcut::new("Ctrl+L", "Toggle mouse capture", "UI Controls"),
        Shortcut::new("Ctrl+F", "Show profile switcher", "UI Controls"),
        Shortcut::new("Ctrl+P", "Show command palette", "UI Controls"),
        Shortcut::new("Ctrl+S", "Show shortcuts (this popup)", "UI Controls"),
        Shortcut::new("Ctrl+G", "Show file changes", "UI Controls"),
        Shortcut::new("Ctrl+X", "Copy session ID", "UI Controls"),
        Shortcut::new("Alt+H", "Show vil-expr type help", "UI Controls"),
        // File Search
        Shortcut::new("@", "Trigger file search", "File Search"),
        Shortcut::new("$", "Enter interactive shell mode", "File Search"),
        Shortcut::new("Tab", "Select file from search", "File Search"),
        // Mouse
        Shortcut::new("Scroll Up/Down", "Scroll messages", "Mouse"),
        Shortcut::new("Click", "Interact with UI elements", "Mouse"),
    ];

    for spec in crate::services::helper_block::vac_commands() {
        if spec.surface == crate::services::commands::CommandSurface::Hidden {
            continue;
        }
        shortcuts.push(Shortcut::new(&spec.command, &spec.description, "Commands"));
    }

    shortcuts
}

pub fn build_shortcuts_content(theme: &Theme, width: Option<usize>) -> Vec<Line<'static>> {
    let shortcuts = get_all_shortcuts();

    // Group shortcuts by category
    let mut categories: std::collections::HashMap<&str, Vec<&Shortcut>> =
        std::collections::HashMap::new();
    for shortcut in &shortcuts {
        categories
            .entry(&shortcut.category)
            .or_default()
            .push(shortcut);
    }

    // Define the EXACT order we want categories to appear
    let category_order = vec![
        "Navigation",
        "Text Input",
        "Tool Management",
        "UI Controls",
        "Commands",
        "File Search",
        "Mouse",
    ];

    // Create all lines for the popup
    let mut all_lines = Vec::new();
    // push empty line
    all_lines.push(Line::from(""));

    // Process categories in the EXACT order defined above
    for category_name in &category_order {
        if let Some(category_shortcuts) = categories.get(category_name) {
            // Add category header
            let category_style = theme
                .style(StyleKey::CategoryHeader)
                .add_modifier(Modifier::BOLD);
            let category_width = width.unwrap_or(40).saturating_sub(category_name.len() + 5);
            all_lines.push(Line::from(vec![
                Span::styled(format!(" {} ", category_name), category_style),
                Span::styled(
                    "─".repeat(category_width).to_string(),
                    theme.style(StyleKey::Muted),
                ), // Fixed width to avoid recalculation
            ]));

            // Add shortcuts for this category - FIXED ALIGNMENT
            for shortcut in category_shortcuts {
                // Use fixed-width formatting for perfect alignment
                let key_formatted = format!(" {:<25}", shortcut.key); // Left-align in 25 chars
                let description_formatted = format!("{:<40} ", shortcut.description); // Left-align in 40 chars

                let spans = vec![
                    Span::styled(
                        key_formatted,
                        theme
                            .style(StyleKey::KeybindBadge)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(description_formatted, theme.style(StyleKey::Text)),
                ];

                all_lines.push(Line::from(spans));
            }

            // Add empty line between categories
            all_lines.push(Line::from(""));
        }
    }

    all_lines
}

/// Get the total count of actual shortcuts (green items only)
pub fn get_shortcuts_count() -> usize {
    get_all_shortcuts().len()
}
