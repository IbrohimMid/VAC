//! Theme system — named colour palettes for the TUI.
//!
//! Runtime TOML loading and hot-reload are handled by `services::theme_loader`.
//! All view code should call `Theme::style(StyleKey)` rather than constructing
//! raw `Style::new().fg(Color::…)` inline.

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
    // Text highlight (selection / search match)
    HighlightFg,
    HighlightBg,
    Text,
    SearchMatch,
    // Category / badge in shortcuts popup
    CategoryHeader,
    KeybindBadge,
    // Markdown rendering (PR-W25-1)
    MarkdownH1,
    MarkdownH2,
    MarkdownH3,
    MarkdownH4,
    MarkdownH5,
    MarkdownH6,
    MarkdownBold,
    MarkdownItalic,
    MarkdownStrikethrough,
    MarkdownCodeInline,
    MarkdownCodeInlineBg,
    MarkdownCodeBlock,
    MarkdownCodeBlockBg,
    MarkdownLink,
    MarkdownQuote,
    MarkdownListBullet,
    MarkdownTaskOpen,
    MarkdownTaskDone,
    MarkdownImportant,
    MarkdownNote,
    MarkdownTip,
    MarkdownCaution,
    MarkdownSeparator,
    MarkdownTableHeader,
    MarkdownTableCell,
    // Diagnostics (theme-aware fallback)
    DiagError,
    DiagWarning,
    DiagInfo,
    // Focus border (ui/style.rs)
    FocusBorder,
    UnfocusBorder,
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
        // Highlight / selection
        StyleKey::HighlightFg => Style::default().fg(Color::Black),
        StyleKey::HighlightBg => Style::default().bg(Color::White),
        StyleKey::Text => Style::default().fg(Color::White),
        StyleKey::SearchMatch => Style::default().fg(Color::Black).bg(Color::Yellow),
        // Shortcuts popup
        StyleKey::CategoryHeader => Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        StyleKey::KeybindBadge => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        // Markdown
        StyleKey::MarkdownH1 => Style::default().fg(Color::Rgb(100, 150, 255)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH2 => Style::default().fg(Color::Rgb(100, 255, 255)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH3 => Style::default().fg(Color::Rgb(100, 255, 100)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH4 => Style::default().fg(Color::Rgb(255, 100, 255)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH5 => Style::default().fg(Color::Indexed(136)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH6 => Style::default().fg(Color::Rgb(255, 100, 100)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownBold => Style::default().add_modifier(Modifier::BOLD),
        StyleKey::MarkdownItalic => Style::default().add_modifier(Modifier::ITALIC),
        StyleKey::MarkdownStrikethrough => Style::default().add_modifier(Modifier::CROSSED_OUT),
        StyleKey::MarkdownCodeInline => Style::default().fg(Color::Rgb(255, 150, 100)),
        StyleKey::MarkdownCodeInlineBg => Style::default().bg(Color::Rgb(40, 40, 40)),
        StyleKey::MarkdownCodeBlock => Style::default().fg(Color::Rgb(150, 220, 150)),
        StyleKey::MarkdownCodeBlockBg => Style::default().bg(Color::Rgb(30, 30, 30)),
        StyleKey::MarkdownLink => Style::default().fg(Color::Rgb(100, 150, 255)).add_modifier(Modifier::UNDERLINED),
        StyleKey::MarkdownQuote => Style::default().fg(Color::DarkGray),
        StyleKey::MarkdownListBullet => Style::default().fg(Color::DarkGray),
        StyleKey::MarkdownTaskOpen => Style::default().fg(Color::Rgb(255, 200, 50)),
        StyleKey::MarkdownTaskDone => Style::default().fg(Color::Rgb(100, 255, 100)),
        StyleKey::MarkdownImportant => Style::default().fg(Color::Rgb(255, 100, 100)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownNote => Style::default().fg(Color::Rgb(100, 150, 255)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownTip => Style::default().fg(Color::Rgb(100, 255, 100)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownCaution => Style::default().fg(Color::Rgb(255, 100, 100)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownSeparator => Style::default().fg(Color::DarkGray),
        StyleKey::MarkdownTableHeader => Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownTableCell => Style::default().fg(Color::White),
        // Diagnostics
        StyleKey::DiagError => Style::default().fg(Color::Red).add_modifier(Modifier::UNDERLINED),
        StyleKey::DiagWarning => Style::default().fg(Color::Yellow).add_modifier(Modifier::UNDERLINED),
        StyleKey::DiagInfo => Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED),
        // Focus border
        StyleKey::FocusBorder => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        StyleKey::UnfocusBorder => Style::default().fg(Color::DarkGray),
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
        // Highlight / selection
        StyleKey::HighlightFg => Style::default().fg(Color::White),
        StyleKey::HighlightBg => Style::default().bg(Color::Black),
        StyleKey::Text => Style::default().fg(Color::Black),
        StyleKey::SearchMatch => Style::default().fg(Color::White).bg(Color::Blue),
        // Shortcuts popup
        StyleKey::CategoryHeader => Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
        StyleKey::KeybindBadge => Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        // Markdown (light theme — dark colors on light bg)
        StyleKey::MarkdownH1 => Style::default().fg(Color::Indexed(25)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH2 => Style::default().fg(Color::Indexed(30)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH3 => Style::default().fg(Color::Indexed(28)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH4 => Style::default().fg(Color::Indexed(127)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH5 => Style::default().fg(Color::Indexed(130)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH6 => Style::default().fg(Color::Indexed(124)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownBold => Style::default().fg(Color::Indexed(232)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownItalic => Style::default().fg(Color::Black).add_modifier(Modifier::ITALIC),
        StyleKey::MarkdownStrikethrough => Style::default().fg(Color::Gray).add_modifier(Modifier::CROSSED_OUT),
        StyleKey::MarkdownCodeInline => Style::default().fg(Color::Indexed(124)),
        StyleKey::MarkdownCodeInlineBg => Style::default().bg(Color::Indexed(254)),
        StyleKey::MarkdownCodeBlock => Style::default().fg(Color::Indexed(235)),
        StyleKey::MarkdownCodeBlockBg => Style::default().bg(Color::Indexed(254)),
        StyleKey::MarkdownLink => Style::default().fg(Color::Indexed(25)).add_modifier(Modifier::UNDERLINED),
        StyleKey::MarkdownQuote => Style::default().fg(Color::Indexed(241)),
        StyleKey::MarkdownListBullet => Style::default().fg(Color::Indexed(240)),
        StyleKey::MarkdownTaskOpen => Style::default().fg(Color::Indexed(130)),
        StyleKey::MarkdownTaskDone => Style::default().fg(Color::Indexed(28)),
        StyleKey::MarkdownImportant => Style::default().fg(Color::Indexed(160)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownNote => Style::default().fg(Color::Indexed(25)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownTip => Style::default().fg(Color::Indexed(28)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownCaution => Style::default().fg(Color::Indexed(160)).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownSeparator => Style::default().fg(Color::Gray),
        StyleKey::MarkdownTableHeader => Style::default().fg(Color::Black).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownTableCell => Style::default().fg(Color::Black),
        // Diagnostics
        StyleKey::DiagError => Style::default().fg(Color::Red).add_modifier(Modifier::UNDERLINED),
        StyleKey::DiagWarning => Style::default().fg(Color::Yellow).add_modifier(Modifier::UNDERLINED),
        StyleKey::DiagInfo => Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED),
        // Focus border
        StyleKey::FocusBorder => Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
        StyleKey::UnfocusBorder => Style::default().fg(Color::Gray),
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
        // Highlight / selection
        StyleKey::HighlightFg => Style::default().fg(Color::Black),
        StyleKey::HighlightBg => Style::default().bg(Color::White),
        StyleKey::Text => Style::default().fg(Color::White),
        StyleKey::SearchMatch => Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD),
        // Shortcuts popup
        StyleKey::CategoryHeader => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        StyleKey::KeybindBadge => Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        // Markdown (high-contrast — basic named colors)
        StyleKey::MarkdownH1 => Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH2 => Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH3 => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH4 => Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH5 => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH6 => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownBold => Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownItalic => Style::default().add_modifier(Modifier::ITALIC),
        StyleKey::MarkdownStrikethrough => Style::default().add_modifier(Modifier::CROSSED_OUT),
        StyleKey::MarkdownCodeInline => Style::default().fg(Color::Red),
        StyleKey::MarkdownCodeInlineBg => Style::default(),
        StyleKey::MarkdownCodeBlock => Style::default().fg(Color::Cyan),
        StyleKey::MarkdownCodeBlockBg => Style::default(),
        StyleKey::MarkdownLink => Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED),
        StyleKey::MarkdownQuote => Style::default().fg(Color::DarkGray),
        StyleKey::MarkdownListBullet => Style::default().fg(Color::Reset),
        StyleKey::MarkdownTaskOpen => Style::default().fg(Color::Yellow),
        StyleKey::MarkdownTaskDone => Style::default().fg(Color::Green),
        StyleKey::MarkdownImportant => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownNote => Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownTip => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownCaution => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownSeparator => Style::default().fg(Color::DarkGray),
        StyleKey::MarkdownTableHeader => Style::default().fg(Color::Reset).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownTableCell => Style::default().fg(Color::Reset),
        // Diagnostics
        StyleKey::DiagError => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        StyleKey::DiagWarning => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        StyleKey::DiagInfo => Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        // Focus border
        StyleKey::FocusBorder => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        StyleKey::UnfocusBorder => Style::default().fg(Color::White),
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
            StyleKey::HighlightFg,
            StyleKey::HighlightBg,
            StyleKey::Text,
            StyleKey::SearchMatch,
            StyleKey::CategoryHeader,
            StyleKey::KeybindBadge,
            StyleKey::MarkdownH1,
            StyleKey::MarkdownH2,
            StyleKey::MarkdownH3,
            StyleKey::MarkdownH4,
            StyleKey::MarkdownH5,
            StyleKey::MarkdownH6,
            StyleKey::MarkdownBold,
            StyleKey::MarkdownItalic,
            StyleKey::MarkdownStrikethrough,
            StyleKey::MarkdownCodeInline,
            StyleKey::MarkdownCodeInlineBg,
            StyleKey::MarkdownCodeBlock,
            StyleKey::MarkdownCodeBlockBg,
            StyleKey::MarkdownLink,
            StyleKey::MarkdownQuote,
            StyleKey::MarkdownListBullet,
            StyleKey::MarkdownTaskOpen,
            StyleKey::MarkdownTaskDone,
            StyleKey::MarkdownImportant,
            StyleKey::MarkdownNote,
            StyleKey::MarkdownTip,
            StyleKey::MarkdownCaution,
            StyleKey::MarkdownSeparator,
            StyleKey::MarkdownTableHeader,
            StyleKey::MarkdownTableCell,
            StyleKey::DiagError,
            StyleKey::DiagWarning,
            StyleKey::DiagInfo,
            StyleKey::FocusBorder,
            StyleKey::UnfocusBorder,
        ];
        for preset in ThemePreset::ALL {
            let theme = Theme::new(*preset);
            for key in &keys {
                let _ = theme.style(*key);
            }
        }
    }
}
