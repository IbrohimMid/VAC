//! Slice 11 — `OverlayStack`.
//!
//! Single host-side controller for popup focus. Existing widget
//! crates remain unchanged; the TUI event loop calls `apply_intent`
//! to push/pop overlays and renders by checking `top()`.
//!
//! Behaviour:
//!
//! * `Open(Same)` is a no-op (already visible).
//! * `Open(Other)` pushes the new overlay above whatever's already
//!   active, so two overlays can coexist (e.g. ModelSwitcher above
//!   Palette).
//! * `Toggle(Same)` closes; `Toggle(Other)` opens.
//! * `CloseTop` pops one entry; `CloseAll` clears.
//! * `top()` returns the highest active overlay, or
//!   `ShellOverlay::None` when the stack is empty.

use std::sync::{Arc, Mutex};

use vac_shell_contracts::{OverlayIntent, ShellOverlay};

#[derive(Debug, Clone, Default)]
pub struct OverlayStack {
    inner: Arc<Mutex<Vec<ShellOverlay>>>,
}

impl OverlayStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn top(&self) -> ShellOverlay {
        self.inner
            .lock()
            .expect("overlay stack lock poisoned")
            .last()
            .copied()
            .unwrap_or(ShellOverlay::None)
    }

    pub fn is_empty(&self) -> bool {
        self.inner
            .lock()
            .expect("overlay stack lock poisoned")
            .is_empty()
    }

    pub fn snapshot(&self) -> Vec<ShellOverlay> {
        self.inner
            .lock()
            .expect("overlay stack lock poisoned")
            .clone()
    }

    pub fn apply_intent(&self, intent: OverlayIntent) {
        let mut guard = self.inner.lock().expect("overlay stack lock poisoned");
        match intent {
            OverlayIntent::Open(o) => {
                if matches!(o, ShellOverlay::None) {
                    return;
                }
                if guard.last() != Some(&o) {
                    guard.push(o);
                }
            }
            OverlayIntent::CloseTop => {
                guard.pop();
            }
            OverlayIntent::CloseAll => {
                guard.clear();
            }
            OverlayIntent::Toggle(o) => {
                if matches!(o, ShellOverlay::None) {
                    return;
                }
                if guard.last() == Some(&o) {
                    guard.pop();
                } else {
                    guard.push(o);
                }
            }
        }
    }
}
