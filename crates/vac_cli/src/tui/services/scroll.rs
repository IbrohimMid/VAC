//! Scroll state utilities — clamp, writeback, per-pane tracking.
//! Pattern adapted from stakpak/tui/src/view.rs (Apache-2.0).

use ratatui::layout::Rect;

/// Per-pane scroll state with geometry tracking.
#[derive(Default, Clone)]
pub struct PaneScroll {
    /// Current scroll offset (0 = top).
    /// usize::MAX means "pinned to bottom" (auto-scroll).
    pub offset: usize,
    /// Geometry of this pane (set after render for click detection).
    pub area: Rect,
}

impl PaneScroll {
    /// Create new scroll state pinned to bottom.
    pub fn pinned() -> Self {
        Self { offset: usize::MAX, area: Rect::default() }
    }

    /// Clamp scroll to valid range, returns clamped value.
    /// If offset is usize::MAX, returns max_scroll (pinned to bottom).
    pub fn clamp(&mut self, total_lines: usize, visible_height: usize) -> usize {
        if total_lines <= visible_height {
            self.offset = 0;
            return 0;
        }
        let max_scroll = total_lines.saturating_sub(visible_height);
        let clamped = if self.offset == usize::MAX || self.offset > max_scroll {
            max_scroll
        } else {
            self.offset
        };
        self.offset = clamped;
        clamped
    }

    /// Scroll up by n lines (toward top).
    pub fn scroll_up(&mut self, n: usize) {
        if self.offset != usize::MAX {
            self.offset = self.offset.saturating_sub(n);
        } else {
            // Was pinned, unpin and stay at top
            self.offset = 0;
        }
    }

    /// Scroll down by n lines (toward bottom).
    /// If pinned (usize::MAX), stays pinned.
    pub fn scroll_down(&mut self, n: usize, total_lines: usize, visible_height: usize) {
        if self.offset == usize::MAX {
            return; // Stay pinned
        }
        let max_scroll = total_lines.saturating_sub(visible_height);
        self.offset = (self.offset + n).min(max_scroll);
    }

    /// Pin to bottom (auto-follow new content).
    pub fn pin_to_bottom(&mut self) {
        self.offset = usize::MAX;
    }

    /// Check if currently pinned to bottom.
    pub fn is_pinned(&self) -> bool {
        self.offset == usize::MAX
    }
}

/// Global scroll state for all panes.
#[derive(Clone)]
pub struct ScrollManager {
    pub transcript: PaneScroll,
    pub history: PaneScroll,
    pub detail: PaneScroll,
}

impl Default for ScrollManager {
    fn default() -> Self {
        Self {
            transcript: PaneScroll::pinned(),
            history: PaneScroll::default(),
            detail: PaneScroll::default(),
        }
    }
}

impl ScrollManager {
    /// Check if a point (x, y) is within a pane's area.
    pub fn pane_at(&self, x: u16, y: u16) -> Option<ScrollablePane> {
        // Check in z-order: detail on top, then history, then transcript
        if self.detail.area.contains(ratatui::layout::Position { x, y }) {
            return Some(ScrollablePane::Detail);
        }
        if self.history.area.contains(ratatui::layout::Position { x, y }) {
            return Some(ScrollablePane::History);
        }
        if self.transcript.area.contains(ratatui::layout::Position { x, y }) {
            return Some(ScrollablePane::Transcript);
        }
        None
    }
}

/// Identifies which pane is scrollable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScrollablePane {
    Transcript,
    History,
    Detail,
}
