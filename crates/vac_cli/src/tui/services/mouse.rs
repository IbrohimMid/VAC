//! Mouse utilities — wheel scroll routing, click-to-focus.
//! Pattern adapted from stakpak/tui (Apache-2.0).

use crossterm::event::{MouseEvent, MouseEventKind};

use super::scroll::{ScrollManager, ScrollablePane};
use super::transcript::count_transcript_lines;
use crate::tui::app::{FocusPane, TuiApp};

/// Handle mouse event, returns true if consumed.
/// Takes app mutably and accesses scroll through app.
pub fn handle_mouse(event: MouseEvent, app: &mut TuiApp) -> bool {
    match event.kind {
        MouseEventKind::ScrollUp { .. } => {
            if let Some(pane) = app.scroll.pane_at(event.column, event.row) {
                match pane {
                    ScrollablePane::Transcript => { app.scroll.transcript.scroll_up(1); }
                    ScrollablePane::History => { app.scroll.history.scroll_up(1); }
                    ScrollablePane::Detail => { app.scroll.detail.scroll_up(1); }
                }
            }
            true
        }
        MouseEventKind::ScrollDown { .. } => {
            if let Some(pane) = app.scroll.pane_at(event.column, event.row) {
                match pane {
                    ScrollablePane::Transcript => {
                        let max_w = app.scroll.transcript.area.width.saturating_sub(4) as usize;
                        let total = count_transcript_lines(&app.session().transcript, max_w.max(20));
                        let height = app.scroll.transcript.area.height.saturating_sub(2) as usize;
                        app.scroll.transcript.scroll_down(1, total, height);
                    }
                    ScrollablePane::History => {
                        let total = app.session().history.len() * 2;
                        let height = app.scroll.history.area.height.saturating_sub(2) as usize;
                        app.scroll.history.scroll_down(1, total, height);
                    }
                    ScrollablePane::Detail => {
                        let height = app.scroll.detail.area.height.saturating_sub(2) as usize;
                        app.scroll.detail.scroll_down(1, 50, height);
                    }
                }
            }
            true
        }
        MouseEventKind::Down(_) => {
            if let Some(pane) = app.scroll.pane_at(event.column, event.row) {
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
