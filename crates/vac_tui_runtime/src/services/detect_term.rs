//! Terminal Detection Service
//!
//! Thin compatibility shim delegating to `crate::capabilities`.
//! New code should use `crate::capabilities::TerminalCapabilities::detect()` directly.
//! Theme colours are resolved via `crate::services::theme::Theme::style(StyleKey)`.

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
