//! Step 2 slice 3 — shell popup widget extracted from Stakpak.
//!
//! # Provenance
//!
//! Source: `vendor/stakpak/tui/src/services/shell_popup.rs`
//! at commit `2e75bd56970d114ab41653aed045300b9a3257a7` (Apache 2.0).
//! The render layout (rounded border, status-coloured title,
//! collapsed-with-overflow indicator, scroll-from-bottom math, cursor
//! placement) is preserved verbatim.
//!
//! # What changed
//!
//! Two coupling points were severed:
//!
//! 1. **`AppState` removed.** Donor reads
//!    `state.shell_popup_state.*` and `state.shell_runtime_state.*`
//!    directly. Here, all popup-facing fields live on a self-contained
//!    [`ShellPopupViewState`] that the caller owns.
//!
//! 2. **PTY/vt100 dependency removed.** Donor calls
//!    `capture_styled_screen(&mut screen)` and `trim_shell_lines(...)`
//!    from `handlers::shell` to derive renderable lines from a live
//!    `vt100::Parser`. Per the extraction map, the donor PTY runtime
//!    stays rejected (VAC `vac_shell` owns it). This crate accepts
//!    pre-built `Vec<Line<'static>>` plus a logical cursor position;
//!    transforming a real PTY's screen into those is the host's job.
//!
//! Theme colours are sensible defaults — wiring to the VAC theme
//! service is a Step 4 concern. Cursor blink helpers
//! (`update_cursor_blink`, `reset_cursor_blink`) operate on the
//! popup state; the host calls them from its tick loop.

use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

/// Minimum height when collapsed — 2 lines of content + 2 for borders.
pub const SHELL_POPUP_MIN_HEIGHT: u16 = 4;

/// Maximum height as a fraction of terminal height when expanded.
pub const SHELL_POPUP_MAX_HEIGHT_PERCENT: f32 = 0.6;

/// View state owned entirely by the caller. The donor distributed
/// these fields between `AppState.shell_popup_state` and
/// `AppState.shell_runtime_state`; pulling them into one struct is
/// what lets the widget travel.
#[derive(Debug, Clone, Default)]
pub struct ShellPopupViewState {
    pub visible: bool,
    pub expanded: bool,

    /// `Some(name)` while a command is running; `None` once completed.
    pub active_command: Option<String>,
    /// True after the host has actually started the command — donor
    /// uses this to colour the title yellow during the brief window
    /// before vt100 produces output.
    pub pending_command_executed: bool,
    /// True when the command came from a tool call (vs operator
    /// typed `$`). Drives the "Initializing..." title colour.
    pub is_tool_call_shell_command: bool,
    /// Command string used in the title bar.
    pub pending_command_value: Option<String>,

    /// How many lines from the bottom the operator has scrolled up.
    pub scroll: usize,

    /// Cursor blink toggle, owned by the popup's tick loop.
    pub cursor_visible: bool,
    pub cursor_blink_timer: u64,

    /// Pre-built display lines (host transforms PTY → ratatui Lines).
    pub screen_lines: Vec<Line<'static>>,

    /// Logical cursor position within the popup's content rect, in
    /// (row, col) terms. `None` suppresses cursor placement.
    pub cursor_position: Option<(u16, u16)>,

    /// Number of *non-empty* rows the host found in its PTY screen
    /// (used for collapsed/expanded sizing). Donor computed this by
    /// scanning the vt100 screen; we let the host do that and pass
    /// the number in.
    pub content_rows: u16,
}

/// Compute popup height. Same shape as the donor — collapsed mode
/// snaps to one of two heights; expanded mode clamps a content-driven
/// desired height between [`SHELL_POPUP_MIN_HEIGHT`] and 60% of the
/// terminal.
pub fn calculate_popup_height(view: &ShellPopupViewState, terminal_height: u16) -> u16 {
    if !view.visible {
        return 0;
    }
    let content_lines = view.content_rows;
    if !view.expanded {
        return if content_lines > 2 {
            5
        } else {
            SHELL_POPUP_MIN_HEIGHT
        };
    }
    let content_lines = content_lines.max(2);
    let desired = content_lines.saturating_add(2);
    let max = (terminal_height as f32 * SHELL_POPUP_MAX_HEIGHT_PERCENT) as u16;
    desired.clamp(SHELL_POPUP_MIN_HEIGHT, max.max(SHELL_POPUP_MIN_HEIGHT))
}

/// Render the popup. No-op when invisible or zero-area.
pub fn render_shell_popup(f: &mut Frame, view: &ShellPopupViewState, area: Rect) {
    if !view.visible {
        return;
    }

    // Theme defaults — VAC theme wiring is Step 4.
    let cyan = Color::Cyan;
    let yellow = Color::Yellow;
    let green = Color::Green;
    let dark_gray = Color::DarkGray;

    let (border_color, title_suffix) = if view.expanded {
        if view.active_command.is_some() {
            if !view.pending_command_executed && view.is_tool_call_shell_command {
                (yellow, "[Initializing...]")
            } else {
                (cyan, "[Active] . Option + ↑/↓ to scroll")
            }
        } else {
            (green, "[Completed]")
        }
    } else {
        (dark_gray, "[Background] '$' to expand")
    };

    let command_name = view
        .pending_command_value
        .as_ref()
        .map(|c| {
            if c.chars().count() > 50 {
                let truncated: String = c.chars().take(47).collect();
                format!("{}...", truncated)
            } else {
                c.clone()
            }
        })
        .unwrap_or_else(|| "Shell".to_string());

    let title = format!(" $ {} {} ", command_name, title_suffix);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            title,
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        ));

    let inner_area = block.inner(area);
    f.render_widget(block, area);
    if inner_area.width == 0 || inner_area.height == 0 {
        return;
    }

    let total_lines = view.screen_lines.len();
    let inner_height = inner_area.height as usize;
    let max_scroll = total_lines.saturating_sub(inner_height);
    let scroll_from_bottom = view.scroll.min(max_scroll);
    let skip = max_scroll.saturating_sub(scroll_from_bottom);

    let visible_lines: Vec<Line<'static>> = if !view.expanded && total_lines > 2 {
        let mut lines = Vec::new();
        let hidden = total_lines.saturating_sub(2);
        lines.push(Line::from(Span::styled(
            format!(" + {} hidden lines", hidden),
            Style::default().fg(dark_gray),
        )));
        let start = total_lines.saturating_sub(2);
        for line in view.screen_lines.iter().skip(start) {
            lines.push(line.clone());
        }
        lines
    } else {
        view.screen_lines
            .iter()
            .skip(skip)
            .take(inner_height)
            .cloned()
            .collect()
    };

    let content = Paragraph::new(visible_lines);
    f.render_widget(content, inner_area);

    // Cursor placement — same gating as donor: expanded + active +
    // scrolled-to-bottom + blink-on.
    if view.expanded
        && view.active_command.is_some()
        && view.scroll == 0
        && view.cursor_visible
        && let Some((cursor_row, cursor_col)) = view.cursor_position
    {
        let cursor_line_in_content = cursor_row as usize;
        if cursor_line_in_content >= skip && cursor_line_in_content < skip + inner_height {
            let screen_row = (cursor_line_in_content - skip) as u16;
            let screen_x = inner_area.x + cursor_col;
            let screen_y = inner_area.y + screen_row;
            if screen_x < inner_area.x + inner_area.width
                && screen_y < inner_area.y + inner_area.height
            {
                f.set_cursor_position(Position::new(screen_x, screen_y));
            }
        }
    }
}

/// Cursor blink tick — call once per popup-render cadence (~100 ms in
/// the donor). Toggles `cursor_visible` every 5 ticks (~500 ms).
pub fn update_cursor_blink(view: &mut ShellPopupViewState) {
    view.cursor_blink_timer = view.cursor_blink_timer.wrapping_add(1);
    if view.cursor_blink_timer.is_multiple_of(5) {
        view.cursor_visible = !view.cursor_visible;
    }
}

/// Reset blink so the cursor is visible immediately — call on input.
pub fn reset_cursor_blink(view: &mut ShellPopupViewState) {
    view.cursor_visible = true;
    view.cursor_blink_timer = 0;
}

// =====================================================================
// Slice 17 — shell popup v2 helpers
// =====================================================================

use vac_shell_contracts::{ShellCommandView, ShellStatus};

/// One-line status badge string for a shell command. Pure helper —
/// the renderer can drop this into the title area.
pub fn status_badge(view: &ShellCommandView) -> String {
    let elapsed = view
        .ended_at
        .or(Some(view.started_at))
        .map(|end| end.saturating_sub(view.started_at))
        .unwrap_or(0);
    match view.status {
        ShellStatus::Running => format!("running · {}s", elapsed),
        ShellStatus::Succeeded => format!("ok · {}s", elapsed),
        ShellStatus::Failed => match view.exit_code {
            Some(c) => format!("failed · exit {} · {}s", c, elapsed),
            None => format!("failed · {}s", elapsed),
        },
        ShellStatus::Cancelled => format!("cancelled · {}s", elapsed),
    }
}

/// In-buffer line-text search — case-insensitive substring scan.
/// Returns the indices into `lines` that match `needle`. Empty
/// needle returns an empty result so the caller knows to clear
/// any highlight.
pub fn search_lines(lines: &[String], needle: &str) -> Vec<usize> {
    let n = needle.trim().to_lowercase();
    if n.is_empty() {
        return Vec::new();
    }
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.to_lowercase().contains(&n))
        .map(|(i, _)| i)
        .collect()
}

/// Single-line preview rendering for the collapsed popup overflow:
/// the latest non-empty line, trimmed.
pub fn collapsed_preview_line(lines: &[String]) -> Option<String> {
    lines.iter().rev().find(|l| !l.trim().is_empty()).cloned()
}

#[cfg(test)]
mod v2_tests {
    use super::*;

    #[test]
    fn status_badge_running_then_failed() {
        let mut v = ShellCommandView {
            id: "x".into(),
            command: "ls".into(),
            cwd: "/tmp".into(),
            status: ShellStatus::Running,
            exit_code: None,
            started_at: 100,
            ended_at: None,
        };
        let s = status_badge(&v);
        assert!(s.starts_with("running"));
        v.status = ShellStatus::Failed;
        v.ended_at = Some(105);
        v.exit_code = Some(2);
        let s = status_badge(&v);
        assert!(s.contains("exit 2"));
        assert!(s.contains("5s"));
    }

    #[test]
    fn search_lines_case_insensitive() {
        let lines: Vec<String> = vec![
            "hello".into(),
            "World".into(),
            "WORLD again".into(),
            "foo".into(),
        ];
        let hits = search_lines(&lines, "world");
        assert_eq!(hits, vec![1, 2]);
    }

    #[test]
    fn search_empty_needle_returns_empty() {
        let lines: Vec<String> = vec!["hello".into()];
        assert!(search_lines(&lines, "").is_empty());
        assert!(search_lines(&lines, "   ").is_empty());
    }

    #[test]
    fn collapsed_preview_picks_latest_non_empty() {
        let lines: Vec<String> = vec!["  ".into(), "build ok".into(), "".into(), "   ".into()];
        assert_eq!(collapsed_preview_line(&lines).as_deref(), Some("build ok"));
    }
}
