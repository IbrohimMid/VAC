//! Step 2 slice 2 — VAC-owned surface state.
//!
//! The new shell's `Surface` enum and live state live here, owned by
//! VAC, mutated only through the [`SurfaceController`] trait that
//! `vac_shell_bridge` calls into. The legacy `vac_tui_runtime`
//! shell's own `Surface` enum (in `app/types/workbench.rs`) is
//! independent and remains untouched while the legacy TUI is in
//! parallel use; once the new shell ships, the legacy enum retires.
//!
//! No `vac_core` / `vac_session_engine` dependency yet. The "VAC
//! side effect" this slice exercises is a state mutation on the
//! `SurfaceState` Arc — observable, testable, and the seam through
//! which a richer effect (broadcast, trace, etc.) can be added later
//! without the bridge knowing.

use std::sync::{Arc, Mutex};

use vac_shell_bridge::{DispatchError, SurfaceController};

/// Top-level operator surface. The new shell only ships two
/// destinations today; the legacy TUI surfaces (review/workbench/mcp)
/// will be reintroduced as their own slices land.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Surface {
    #[default]
    Chat,
    Runtime,
}

impl Surface {
    pub fn label(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Runtime => "runtime",
        }
    }
}

/// Shared, observable surface state. Cheap to clone (`Arc` inside),
/// safe to read from any thread.
#[derive(Debug, Clone, Default)]
pub struct SurfaceState {
    inner: Arc<Mutex<Surface>>,
}

impl SurfaceState {
    pub fn new(initial: Surface) -> Self {
        Self {
            inner: Arc::new(Mutex::new(initial)),
        }
    }

    pub fn current(&self) -> Surface {
        *self.inner.lock().expect("SurfaceState lock poisoned")
    }

    fn set(&self, next: Surface) {
        *self.inner.lock().expect("SurfaceState lock poisoned") = next;
    }
}

/// Concrete `SurfaceController` impl bound to a `SurfaceState`.
/// Cheap to clone — the underlying `SurfaceState` is the only piece
/// of shared mutable state.
#[derive(Debug, Clone)]
pub struct SurfaceStateController {
    state: SurfaceState,
}

impl SurfaceStateController {
    pub fn new(state: SurfaceState) -> Self {
        Self { state }
    }

    pub fn state(&self) -> SurfaceState {
        self.state.clone()
    }
}

impl SurfaceController for SurfaceStateController {
    fn enter_chat(&self) -> Result<(), DispatchError> {
        self.state.set(Surface::Chat);
        Ok(())
    }

    fn enter_runtime(&self) -> Result<(), DispatchError> {
        self.state.set(Surface::Runtime);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_flips_state_through_trait() {
        let state = SurfaceState::new(Surface::Chat);
        let ctrl = SurfaceStateController::new(state.clone());
        assert_eq!(state.current(), Surface::Chat);
        ctrl.enter_runtime().unwrap();
        assert_eq!(state.current(), Surface::Runtime);
        ctrl.enter_chat().unwrap();
        assert_eq!(state.current(), Surface::Chat);
    }
}
