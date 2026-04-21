//! VAC-native bash block detection and rendering.
//!
//! Detects fenced bash/shell code blocks in text and renders them
//! with appropriate styling for TUI display.

use crate::services::theme::{StyleKey, Theme};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// A detected bash block extracted from text.
#[derive(Debug, Clone)]
pub struct BashBlock {
    pub language: String,
    pub content: String,
}

impl BashBlock {
    pub fn render(&self, theme: &Theme) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        for line in self.content.lines() {
            lines.push(Line::from(vec![Span::styled(
                line.to_string(),
                theme.style(StyleKey::Normal),
            )]));
        }
        lines
    }
}
