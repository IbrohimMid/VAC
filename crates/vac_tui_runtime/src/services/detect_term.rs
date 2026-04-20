//! Terminal Detection Service
//!
//! Thin compatibility shim delegating to `crate::capabilities`.
//! New code should use `crate::capabilities::TerminalCapabilities::detect()` directly.

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

    pub fn dark_gray() -> Color {
        Color::DarkGray
    }

    pub fn highlight_fg() -> Color {
        if is_light_mode() {
            Color::White
        } else {
            Color::Black
        }
    }

    pub fn highlight_bg() -> Color {
        if is_light_mode() {
            Color::Black
        } else {
            Color::White
        }
    }

    pub fn accent() -> Color {
        Color::Cyan
    }

    pub fn title() -> Color {
        Color::Yellow
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

    pub fn text() -> Color {
        if is_light_mode() {
            Color::Black
        } else {
            Color::White
        }
    }

    pub fn muted() -> Color {
        if is_light_mode() {
            Color::DarkGray
        } else {
            Color::Gray
        }
    }

    pub fn warning() -> Color {
        Color::Yellow
    }

    pub fn danger() -> Color {
        Color::Red
    }

    pub fn success() -> Color {
        Color::Green
    }
}

/// Detect terminal capabilities — delegates to the capabilities registry.
pub fn detect_terminal() -> TerminalInfo {
    let caps = crate::capabilities::TerminalCapabilities::detect();
    TerminalInfo {
        supports_true_color: caps.truecolor,
        supports_256_colors: caps.color_256,
        is_dark_theme: caps.is_dark_theme,
    }
}

pub struct TerminalInfo {
    pub supports_true_color: bool,
    pub supports_256_colors: bool,
    pub is_dark_theme: bool,
}

/// Check if terminal is in light mode
pub fn is_light_mode() -> bool {
    crate::capabilities::is_light_mode()
}

/// Check if terminal should use RGB colors
pub fn should_use_rgb_colors() -> bool {
    crate::capabilities::should_use_rgb_colors()
}

/// Adaptive colors that work on both light and dark backgrounds
pub struct AdaptiveColors;

impl AdaptiveColors {
    pub fn text() -> Color {
        if is_light_mode() {
            Color::Black
        } else {
            Color::White
        }
    }

    pub fn heading() -> Color {
        if is_light_mode() {
            Color::Blue
        } else {
            Color::Cyan
        }
    }

    pub fn code() -> Color {
        if is_light_mode() {
            Color::Rgb(200, 100, 100)
        } else {
            Color::Rgb(255, 150, 150)
        }
    }

    pub fn code_bg() -> Color {
        if is_light_mode() {
            Color::Rgb(240, 240, 240)
        } else {
            Color::Rgb(40, 40, 40)
        }
    }

    pub fn code_block_bg() -> Color {
        if is_light_mode() {
            Color::Rgb(245, 245, 245)
        } else {
            Color::Rgb(30, 30, 30)
        }
    }

    pub fn link() -> Color {
        if is_light_mode() {
            Color::Blue
        } else {
            Color::Cyan
        }
    }

    pub fn quote() -> Color {
        if is_light_mode() {
            Color::DarkGray
        } else {
            Color::Gray
        }
    }
}
