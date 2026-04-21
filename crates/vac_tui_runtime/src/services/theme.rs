//! Theme system — named colour palettes for the TUI.
//!
//! Themes are plain Rust structs (no TOML at runtime for now; TOML loading can
//! be layered on later).  All view code should call `Theme::style(StyleKey)`
//! rather than constructing raw `Style::new().fg(Color::…)` inline.

use ratatui::style::{Color, Modifier, Style};

// ── StyleKey ──────────────────────────────────────────────────────────────────

/// Semantic token for a UI element's visual style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StyleKey {
    // Text roles
    Normal,
    Muted,
    Accent,
    Warning,
    Error,
    Success,
    // Message roles
    UserMessage,
    AssistantMessage,
    SystemMessage,
    // UI chrome
    BorderNormal,
    BorderFocused,
    BorderActive,
    StatusBarBg,
    StatusBarFg,
    InputBg,
    InputFg,
    InputCursor,
    // Streaming / loading
    Streaming,
    Spinner,
    // Overlay
    OverlayBg,
    OverlayBorder,
    OverlaySelected,
    // Code blocks
    CodeFg,
    CodeBg,
    // Task tray
    TaskRunning,
    TaskQueued,
    TaskCompleted,
    TaskFailed,
    // Toast (bg+fg combos)
    ToastSuccess,
    ToastError,
    ToastWarning,
    ToastInfo,
    // List row selection (Yellow+BOLD in dark)
    ListSelected,
    // App title brand colour (Magenta in dark)
    AppTitle,
    // Diff
    DiffAdded,
    DiffRemoved,
    // VIL issue kinds
    VilKindSemantic,
    VilKindZeroCopy,
    VilKindPlumbing,
    VilKindIrDrift,
    VilKindCanonical,
    VilKindOther,
    // MCP trust classes
    McpTrusted,
    McpVerified,
    McpUntrusted,
    // Score/badge
    ScoreGood,
    ScoreOk,
    ScoreBad,
    // vil-expr live linter (PR-T12)
    ValidationError,
    ValidationWarning,
    ValidationOk,
}

// ── ThemePreset ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemePreset {
    Dark,
    Light,
    HighContrast,
}

impl ThemePreset {
    pub const ALL: &'static [ThemePreset] = &[
        ThemePreset::Dark,
        ThemePreset::Light,
        ThemePreset::HighContrast,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemePreset::Dark => "Dark",
            ThemePreset::Light => "Light",
            ThemePreset::HighContrast => "High Contrast",
        }
    }
}

// ── Theme ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Theme {
    pub preset: ThemePreset,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            preset: ThemePreset::Dark,
        }
    }
}

impl Theme {
    pub fn new(preset: ThemePreset) -> Self {
        Self { preset }
    }

    /// Resolve a semantic style key to a concrete Ratatui `Style`.
    pub fn style(&self, key: StyleKey) -> Style {
        match self.preset {
            ThemePreset::Dark => dark(key),
            ThemePreset::Light => light(key),
            ThemePreset::HighContrast => high_contrast(key),
        }
    }
}

// ── Dark palette ─────────────────────────────────────────────────────────────

fn dark(key: StyleKey) -> Style {
    match key {
        StyleKey::Normal => Style::default().fg(Color::White),
        StyleKey::Muted => Style::default().fg(Color::DarkGray),
        StyleKey::Accent => Style::default().fg(Color::Cyan),
        StyleKey::Warning => Style::default().fg(Color::Yellow),
        StyleKey::Error => Style::default().fg(Color::Red),
        StyleKey::Success => Style::default().fg(Color::Green),
        StyleKey::UserMessage => Style::default().fg(Color::Cyan),
        StyleKey::AssistantMessage => Style::default().fg(Color::White),
        StyleKey::SystemMessage => Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
        StyleKey::BorderNormal => Style::default().fg(Color::DarkGray),
        StyleKey::BorderFocused => Style::default().fg(Color::Cyan),
        StyleKey::BorderActive => Style::default().fg(Color::Yellow),
        StyleKey::StatusBarBg => Style::default().bg(Color::DarkGray),
        StyleKey::StatusBarFg => Style::default().fg(Color::White),
        StyleKey::InputBg => Style::default(),
        StyleKey::InputFg => Style::default().fg(Color::White),
        StyleKey::InputCursor => Style::default().bg(Color::White).fg(Color::Black),
        StyleKey::Streaming => Style::default().fg(Color::Magenta),
        StyleKey::Spinner => Style::default().fg(Color::Magenta),
        StyleKey::OverlayBg => Style::default().bg(Color::DarkGray),
        StyleKey::OverlayBorder => Style::default().fg(Color::Cyan),
        StyleKey::OverlaySelected => Style::default().bg(Color::Blue).fg(Color::White),
        StyleKey::CodeFg => Style::default().fg(Color::Green),
        StyleKey::CodeBg => Style::default().bg(Color::Black),
        StyleKey::TaskRunning => Style::default().fg(Color::Cyan),
        StyleKey::TaskQueued => Style::default().fg(Color::Yellow),
        StyleKey::TaskCompleted => Style::default().fg(Color::Green),
        StyleKey::TaskFailed => Style::default().fg(Color::Red),
        StyleKey::ToastSuccess => Style::default()
            .bg(Color::Green)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
        StyleKey::ToastError => Style::default()
            .bg(Color::Red)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
        StyleKey::ToastWarning => Style::default()
            .bg(Color::Yellow)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
        StyleKey::ToastInfo => Style::default()
            .bg(Color::Blue)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
        StyleKey::ListSelected => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        StyleKey::AppTitle => Style::default().fg(Color::Magenta),
        StyleKey::DiffAdded => Style::default().fg(Color::Green),
        StyleKey::DiffRemoved => Style::default().fg(Color::Red),
        StyleKey::VilKindSemantic => Style::default().fg(Color::Magenta),
        StyleKey::VilKindZeroCopy => Style::default().fg(Color::Yellow),
        StyleKey::VilKindPlumbing => Style::default().fg(Color::Cyan),
        StyleKey::VilKindIrDrift => Style::default().fg(Color::LightRed),
        StyleKey::VilKindCanonical => Style::default().fg(Color::Green),
        StyleKey::VilKindOther => Style::default().fg(Color::DarkGray),
        StyleKey::McpTrusted => Style::default().fg(Color::Green),
        StyleKey::McpVerified => Style::default().fg(Color::Yellow),
        StyleKey::McpUntrusted => Style::default().fg(Color::LightRed),
        StyleKey::ScoreGood => Style::default().fg(Color::Green),
        StyleKey::ScoreOk => Style::default().fg(Color::Yellow),
        StyleKey::ScoreBad => Style::default().fg(Color::Red),
        StyleKey::ValidationError => Style::default().fg(Color::Red).add_modifier(Modifier::UNDERLINED),
        StyleKey::ValidationWarning => Style::default().fg(Color::Yellow),
        StyleKey::ValidationOk => Style::default().fg(Color::Green),
    }
}

// ── Light palette ─────────────────────────────────────────────────────────────

fn light(key: StyleKey) -> Style {
    match key {
        StyleKey::Normal => Style::default().fg(Color::Black),
        StyleKey::Muted => Style::default().fg(Color::Gray),
        StyleKey::Accent => Style::default().fg(Color::Blue),
        StyleKey::Warning => Style::default().fg(Color::Yellow),
        StyleKey::Error => Style::default().fg(Color::Red),
        StyleKey::Success => Style::default().fg(Color::Green),
        StyleKey::UserMessage => Style::default().fg(Color::Blue),
        StyleKey::AssistantMessage => Style::default().fg(Color::Black),
        StyleKey::SystemMessage => Style::default()
            .fg(Color::Gray)
            .add_modifier(Modifier::ITALIC),
        StyleKey::BorderNormal => Style::default().fg(Color::Gray),
        StyleKey::BorderFocused => Style::default().fg(Color::Blue),
        StyleKey::BorderActive => Style::default().fg(Color::Magenta),
        StyleKey::StatusBarBg => Style::default().bg(Color::Gray),
        StyleKey::StatusBarFg => Style::default().fg(Color::Black),
        StyleKey::InputBg => Style::default().bg(Color::White),
        StyleKey::InputFg => Style::default().fg(Color::Black),
        StyleKey::InputCursor => Style::default().bg(Color::Black).fg(Color::White),
        StyleKey::Streaming => Style::default().fg(Color::Magenta),
        StyleKey::Spinner => Style::default().fg(Color::Magenta),
        StyleKey::OverlayBg => Style::default().bg(Color::White),
        StyleKey::OverlayBorder => Style::default().fg(Color::Blue),
        StyleKey::OverlaySelected => Style::default().bg(Color::Blue).fg(Color::White),
        StyleKey::CodeFg => Style::default().fg(Color::DarkGray),
        StyleKey::CodeBg => Style::default().bg(Color::White),
        StyleKey::TaskRunning => Style::default().fg(Color::Blue),
        StyleKey::TaskQueued => Style::default().fg(Color::Yellow),
        StyleKey::TaskCompleted => Style::default().fg(Color::Green),
        StyleKey::TaskFailed => Style::default().fg(Color::Red),
        StyleKey::ToastSuccess => Style::default()
            .bg(Color::Green)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
        StyleKey::ToastError => Style::default()
            .bg(Color::Red)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
        StyleKey::ToastWarning => Style::default()
            .bg(Color::Yellow)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
        StyleKey::ToastInfo => Style::default()
            .bg(Color::Blue)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
        StyleKey::ListSelected => Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::BOLD),
        StyleKey::AppTitle => Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::BOLD),
        StyleKey::DiffAdded => Style::default().fg(Color::Green),
        StyleKey::DiffRemoved => Style::default().fg(Color::Red),
        StyleKey::VilKindSemantic => Style::default().fg(Color::Magenta),
        StyleKey::VilKindZeroCopy => Style::default().fg(Color::Yellow),
        StyleKey::VilKindPlumbing => Style::default().fg(Color::Cyan),
        StyleKey::VilKindIrDrift => Style::default().fg(Color::Red),
        StyleKey::VilKindCanonical => Style::default().fg(Color::Green),
        StyleKey::VilKindOther => Style::default().fg(Color::Gray),
        StyleKey::McpTrusted => Style::default().fg(Color::Green),
        StyleKey::McpVerified => Style::default().fg(Color::Yellow),
        StyleKey::McpUntrusted => Style::default().fg(Color::Red),
        StyleKey::ScoreGood => Style::default().fg(Color::Green),
        StyleKey::ScoreOk => Style::default().fg(Color::Yellow),
        StyleKey::ScoreBad => Style::default().fg(Color::Red),
        StyleKey::ValidationError => Style::default().fg(Color::Red).add_modifier(Modifier::UNDERLINED),
        StyleKey::ValidationWarning => Style::default().fg(Color::Yellow),
        StyleKey::ValidationOk => Style::default().fg(Color::Green),
    }
}

// ── High-contrast palette ─────────────────────────────────────────────────────

fn high_contrast(key: StyleKey) -> Style {
    match key {
        StyleKey::Normal => Style::default().fg(Color::White),
        StyleKey::Muted => Style::default().fg(Color::Gray),
        StyleKey::Accent => Style::default().fg(Color::Yellow),
        StyleKey::Warning => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        StyleKey::Error => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        StyleKey::Success => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        StyleKey::UserMessage => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        StyleKey::AssistantMessage => Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
        StyleKey::SystemMessage => Style::default().fg(Color::Gray),
        StyleKey::BorderNormal => Style::default().fg(Color::White),
        StyleKey::BorderFocused => Style::default().fg(Color::Yellow),
        StyleKey::BorderActive => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        StyleKey::StatusBarBg => Style::default().bg(Color::Black),
        StyleKey::StatusBarFg => Style::default().fg(Color::White),
        StyleKey::InputBg => Style::default().bg(Color::Black),
        StyleKey::InputFg => Style::default().fg(Color::White),
        StyleKey::InputCursor => Style::default().bg(Color::Yellow).fg(Color::Black),
        StyleKey::Streaming => Style::default().fg(Color::Yellow),
        StyleKey::Spinner => Style::default().fg(Color::Yellow),
        StyleKey::OverlayBg => Style::default().bg(Color::Black),
        StyleKey::OverlayBorder => Style::default().fg(Color::Yellow),
        StyleKey::OverlaySelected => Style::default().bg(Color::Yellow).fg(Color::Black),
        StyleKey::CodeFg => Style::default().fg(Color::White),
        StyleKey::CodeBg => Style::default().bg(Color::Black),
        StyleKey::TaskRunning => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        StyleKey::TaskQueued => Style::default().fg(Color::White),
        StyleKey::TaskCompleted => Style::default().fg(Color::Green),
        StyleKey::TaskFailed => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        StyleKey::ToastSuccess => Style::default()
            .bg(Color::Green)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
        StyleKey::ToastError => Style::default()
            .bg(Color::Red)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
        StyleKey::ToastWarning => Style::default()
            .bg(Color::Yellow)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
        StyleKey::ToastInfo => Style::default()
            .bg(Color::White)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
        StyleKey::ListSelected => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        StyleKey::AppTitle => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        StyleKey::DiffAdded => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        StyleKey::DiffRemoved => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        StyleKey::VilKindSemantic => Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        StyleKey::VilKindZeroCopy => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        StyleKey::VilKindPlumbing => Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        StyleKey::VilKindIrDrift => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        StyleKey::VilKindCanonical => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        StyleKey::VilKindOther => Style::default().fg(Color::Gray),
        StyleKey::McpTrusted => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        StyleKey::McpVerified => Style::default().fg(Color::Yellow),
        StyleKey::McpUntrusted => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        StyleKey::ScoreGood => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        StyleKey::ScoreOk => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        StyleKey::ScoreBad => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        StyleKey::ValidationError => Style::default()
            .fg(Color::Red)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        StyleKey::ValidationWarning => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        StyleKey::ValidationOk => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_presets_cover_all_keys() {
        let keys = [
            StyleKey::Normal,
            StyleKey::Muted,
            StyleKey::Accent,
            StyleKey::Warning,
            StyleKey::Error,
            StyleKey::Success,
            StyleKey::UserMessage,
            StyleKey::AssistantMessage,
            StyleKey::SystemMessage,
            StyleKey::BorderNormal,
            StyleKey::BorderFocused,
            StyleKey::BorderActive,
            StyleKey::StatusBarBg,
            StyleKey::StatusBarFg,
            StyleKey::InputBg,
            StyleKey::InputFg,
            StyleKey::InputCursor,
            StyleKey::Streaming,
            StyleKey::Spinner,
            StyleKey::OverlayBg,
            StyleKey::OverlayBorder,
            StyleKey::OverlaySelected,
            StyleKey::CodeFg,
            StyleKey::CodeBg,
            StyleKey::TaskRunning,
            StyleKey::TaskQueued,
            StyleKey::TaskCompleted,
            StyleKey::TaskFailed,
            StyleKey::ToastSuccess,
            StyleKey::ToastError,
            StyleKey::ToastWarning,
            StyleKey::ToastInfo,
            StyleKey::ListSelected,
            StyleKey::AppTitle,
            StyleKey::DiffAdded,
            StyleKey::DiffRemoved,
            StyleKey::VilKindSemantic,
            StyleKey::VilKindZeroCopy,
            StyleKey::VilKindPlumbing,
            StyleKey::VilKindIrDrift,
            StyleKey::VilKindCanonical,
            StyleKey::VilKindOther,
            StyleKey::McpTrusted,
            StyleKey::McpVerified,
            StyleKey::McpUntrusted,
            StyleKey::ScoreGood,
            StyleKey::ScoreOk,
            StyleKey::ScoreBad,
            StyleKey::ValidationError,
            StyleKey::ValidationWarning,
            StyleKey::ValidationOk,
        ];
        for preset in ThemePreset::ALL {
            let theme = Theme::new(*preset);
            for key in &keys {
                let _ = theme.style(*key);
            }
        }
    }
}
