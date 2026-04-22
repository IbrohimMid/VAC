use super::StyleKey;
use ratatui::style::{Color, Modifier, Style};

// ── High-contrast palette ─────────────────────────────────────────────────────

pub(super) fn high_contrast(key: StyleKey) -> Style {
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
        StyleKey::DiffAdded => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        StyleKey::DiffRemoved => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        StyleKey::VilKindSemantic => Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
        StyleKey::VilKindZeroCopy => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        StyleKey::VilKindPlumbing => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        StyleKey::VilKindIrDrift => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        StyleKey::VilKindCanonical => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        StyleKey::VilKindOther => Style::default().fg(Color::Gray),
        StyleKey::McpTrusted => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        StyleKey::McpVerified => Style::default().fg(Color::Yellow),
        StyleKey::McpUntrusted => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        StyleKey::ScoreGood => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        StyleKey::ScoreOk => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
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
        StyleKey::SearchMatch => Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        // Shortcuts popup
        StyleKey::CategoryHeader => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        StyleKey::KeybindBadge => Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
        // Markdown (high-contrast — basic named colors)
        StyleKey::MarkdownH1 => Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH2 => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH3 => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH4 => Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH5 => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        StyleKey::MarkdownH6 => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownBold => Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
        StyleKey::MarkdownItalic => Style::default().add_modifier(Modifier::ITALIC),
        StyleKey::MarkdownStrikethrough => Style::default().add_modifier(Modifier::CROSSED_OUT),
        StyleKey::MarkdownCodeInline => Style::default().fg(Color::Red),
        StyleKey::MarkdownCodeInlineBg => Style::default(),
        StyleKey::MarkdownCodeBlock => Style::default().fg(Color::Cyan),
        StyleKey::MarkdownCodeBlockBg => Style::default(),
        StyleKey::MarkdownLink => Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::UNDERLINED),
        StyleKey::MarkdownQuote => Style::default().fg(Color::DarkGray),
        StyleKey::MarkdownListBullet => Style::default().fg(Color::Reset),
        StyleKey::MarkdownTaskOpen => Style::default().fg(Color::Yellow),
        StyleKey::MarkdownTaskDone => Style::default().fg(Color::Green),
        StyleKey::MarkdownImportant => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownNote => Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::BOLD),
        StyleKey::MarkdownTip => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        StyleKey::MarkdownCaution => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        StyleKey::MarkdownSeparator => Style::default().fg(Color::DarkGray),
        StyleKey::MarkdownTableHeader => Style::default()
            .fg(Color::Reset)
            .add_modifier(Modifier::BOLD),
        StyleKey::MarkdownTableCell => Style::default().fg(Color::Reset),
        // Diagnostics
        StyleKey::DiagError => Style::default()
            .fg(Color::Red)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        StyleKey::DiagWarning => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        StyleKey::DiagInfo => Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        // Focus border
        StyleKey::FocusBorder => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        StyleKey::UnfocusBorder => Style::default().fg(Color::White),
    }
}
