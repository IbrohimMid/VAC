//! Mouse utilities — wheel scroll routing, click-to-focus.
//! Pattern adapted from stakpak/tui (Apache-2.0).

use crossterm::event::{MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::scroll::{ScrollManager, ScrollablePane};
use crate::tui::app::{FocusPane, TuiApp};

/// Handle mouse event, returns true if consumed.
pub fn handle_mouse(event: MouseEvent, app: &mut TuiApp, scroll: &mut ScrollManager) -> bool {
    match event.kind {
        MouseEventKind::ScrollUp { .. } => {
            // Route to hovered pane
            if let Some(pane) = scroll.pane_at(event.column, event.row) {
                match pane {
                    ScrollablePane::Transcript => {
                        scroll.transcript.scroll_up(1);
                    }
                    ScrollablePane::History => {
                        scroll.history.scroll_up(1);
                    }
                    ScrollablePane::Detail => {
                        scroll.detail.scroll_up(1);
                    }
                }
            }
            true
        }
        MouseEventKind::ScrollDown { .. } => {
            // Route to hovered pane
            if let Some(pane) = scroll.pane_at(event.column, event.row) {
                match pane {
                    ScrollablePane::Transcript => {
                        let total = app.session().transcript.len();
                        scroll.transcript.scroll_down(1, total, scroll.transcript.area.height as usize);
                    }
                    ScrollablePane::History => {
                        let total = app.session().history.len();
                        scroll.history.scroll_down(1, total, scroll.history.area.height as usize);
                    }
                    ScrollablePane::Detail => {
                        scroll.detail.scroll_down(1, 20, scroll.detail.area.height as usize);
                    }
                }
            }
            true
        }
        MouseEventKind::Down(_) => {
            // Click to focus
            if let Some(pane) = scroll.pane_at(event.column, event.row) {
                app.focus = match pane {
                    ScrollablePane::Transcript => FocusPane::Transcript,
                    ScrollablePane::History => FocusPane::History,
                    ScrollablePane::Detail => FocusPane::Detail,
                };
            }
            true
        }
        _ => false,
    }
}

/// Resolve which pane is under the cursor.
pub fn hovered_pane(scroll: &ScrollManager, x: u16, y: u16) -> Option<ScrollablePane> {
    scroll.pane_at(x, y)
}
