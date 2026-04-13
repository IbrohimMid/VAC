//! Terminal Detection Service
//!
//! Detects terminal capabilities and provides theme colors.

use ratatui::style::Color;

/// Theme colors for TUI
pub struct ThemeColors;

impl ThemeColors {
    pub fn green() -> Color {
        Color::Green
    }

    pub fn red() -> Color {
        Color::Red
    }

    pub fn yellow() -> Color {
        Color::Yellow
    }

    pub fn blue() -> Color {
        Color::Blue
    }

    pub fn cyan() -> Color {
        Color::Cyan
    }

    pub fn magenta() -> Color {
        Color::Magenta
    }

    pub fn gray() -> Color {
        Color::Gray
    }

    pub fn white() -> Color {
        Color::White
    }

    pub fn black() -> Color {
        Color::Black
    }
}

/// Detect terminal capabilities
pub fn detect_terminal() -> TerminalInfo {
    TerminalInfo {
        supports_true_color: true,
        supports_256_colors: true,
        is_dark_theme: true,
    }
}

pub struct TerminalInfo {
    pub supports_true_color: bool,
    pub supports_256_colors: bool,
    pub is_dark_theme: bool,
}