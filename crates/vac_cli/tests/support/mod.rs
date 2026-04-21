//! Shared helpers for vac_cli integration tests.
//!
//! Kept deliberately small: only primitives reused across multiple
//! integration-test binaries (e.g. `integration_events.rs` + potentially
//! future gate files). Flow-heavy helpers stay inside `tui_flows.rs`.

#![allow(dead_code)]

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::layout::Rect;
use tokio::sync::mpsc;

use vac_tui_runtime::action_ids::ActionId;
use vac_tui_runtime::app::{AppState, OutputEvent};

/// Fresh `AppState` plus a bounded `OutputEvent` channel. The returned
/// receiver is held so callers may assert emitted events (or their
/// absence) after driving a handler.
pub fn fresh_state() -> (
    AppState,
    mpsc::Sender<OutputEvent>,
    mpsc::Receiver<OutputEvent>,
) {
    let state = AppState::default();
    let (tx, rx) = mpsc::channel::<OutputEvent>(64);
    (state, tx, rx)
}

/// Build a `KeyEvent` suitable for feeding into `ChordKeymap::lookup`
/// or `key_event_to_chord` — always a `Press`, no repeat state.
pub fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    }
}

/// Convenience: build an `effective` map as produced by
/// `keybindings::resolve_effective` so we can exercise `ChordKeymap`
/// without touching the disk.
pub fn effective<I>(entries: I) -> HashMap<ActionId, Vec<String>>
where
    I: IntoIterator<Item = (ActionId, Vec<&'static str>)>,
{
    entries
        .into_iter()
        .map(|(id, chords)| (id, chords.into_iter().map(|s| s.to_string()).collect()))
        .collect()
}

/// Drain any currently-buffered output events synchronously. Returns the
/// collected vector so callers can assert on count/content.
pub fn drain_output(rx: &mut mpsc::Receiver<OutputEvent>) -> Vec<OutputEvent> {
    let mut out = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        out.push(ev);
    }
    out
}

/// Shorthand for building a `Rect` in row-major coordinates.
pub fn rect(x: u16, y: u16, w: u16, h: u16) -> Rect {
    Rect::new(x, y, w, h)
}
