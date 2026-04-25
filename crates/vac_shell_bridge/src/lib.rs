//! Step 2 — command-only bridge slice.
//!
//! This crate is the first concrete implementation of a
//! `vac_shell_contracts` trait. Per reviewer guidance, the slice is
//! intentionally narrow:
//!
//! * registry: `InMemoryCommandRegistry` impl of `VacCommandRegistry`
//! * dispatch: `CommandDispatcher` accepting a slash and routing it
//!   to a host-supplied callback
//!
//! The crate does **not yet** depend on `vac_core`,
//! `vac_session_engine`, or `vac_tools`. The dispatch callback is
//! injected by the host so we can prove the integrated path
//! `palette → registry → bridge → effect` without taking a live
//! dependency on the VAC engine. Wiring those engines in is a later
//! Step 2 follow-up once this slice is reviewed.
//!
//! Approvals, sessions, model switcher, and shell runtime bridges
//! are explicitly out of scope here.

use std::sync::{Arc, RwLock};

pub use vac_shell_contracts::{ShellCommandKind, ShellCommandSpec, VacCommandRegistry};

/// In-memory `VacCommandRegistry` impl — the simplest possible
/// concrete registry. Hosts seed it with the registered specs at
/// boot and (optionally) hand a clone to the palette renderer.
pub struct InMemoryCommandRegistry {
    inner: RwLock<Vec<ShellCommandSpec>>,
}

impl InMemoryCommandRegistry {
    pub fn new(specs: Vec<ShellCommandSpec>) -> Self {
        Self {
            inner: RwLock::new(specs),
        }
    }

    /// Append or replace by `id`. Returns true when an existing entry
    /// was overwritten.
    pub fn upsert(&self, spec: ShellCommandSpec) -> bool {
        let mut guard = self.inner.write().expect("registry lock poisoned");
        if let Some(slot) = guard.iter_mut().find(|s| s.id == spec.id) {
            *slot = spec;
            true
        } else {
            guard.push(spec);
            false
        }
    }
}

impl VacCommandRegistry for InMemoryCommandRegistry {
    fn all(&self) -> Vec<ShellCommandSpec> {
        self.inner.read().expect("registry lock poisoned").clone()
    }

    fn by_slash(&self, slash: &str) -> Option<ShellCommandSpec> {
        self.inner
            .read()
            .expect("registry lock poisoned")
            .iter()
            .find(|s| s.slash == slash)
            .cloned()
    }
}

/// Host callback for command dispatch. Called once per resolved
/// slash; hosts attach VAC-side effects here (open the model picker,
/// switch the runtime surface, send a submit, …).
///
/// `Arc<dyn Fn>` keeps the bridge `Send + Sync` so it can be cloned
/// across the TUI event loop and async tasks once a real VAC engine
/// host wires in.
pub type DispatchHandler = Arc<dyn Fn(&ShellCommandSpec) -> Result<(), DispatchError> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchError {
    /// Slash did not resolve in the registry.
    UnknownSlash(String),
    /// The host callback failed; carries the host-supplied reason.
    Host(String),
}

impl std::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSlash(s) => write!(f, "unknown slash: {s}"),
            Self::Host(s) => write!(f, "host error: {s}"),
        }
    }
}

impl std::error::Error for DispatchError {}

/// The bridge's command dispatch surface. Holds the registry plus a
/// host callback; resolves slashes and routes them.
pub struct CommandDispatcher {
    registry: Arc<dyn VacCommandRegistry>,
    handler: DispatchHandler,
}

impl CommandDispatcher {
    pub fn new(registry: Arc<dyn VacCommandRegistry>, handler: DispatchHandler) -> Self {
        Self { registry, handler }
    }

    /// Look up the slash and invoke the host callback. The whole call
    /// chain `palette select → registry lookup → handler` runs
    /// synchronously today; the type signature does not preclude an
    /// async variant later.
    pub fn dispatch(&self, slash: &str) -> Result<ShellCommandSpec, DispatchError> {
        let spec = self
            .registry
            .by_slash(slash)
            .ok_or_else(|| DispatchError::UnknownSlash(slash.to_string()))?;
        (self.handler)(&spec)?;
        Ok(spec)
    }

    pub fn registry(&self) -> Arc<dyn VacCommandRegistry> {
        Arc::clone(&self.registry)
    }
}

// =====================================================================
// Unified host action seam — slice 5 (controller normalization)
// =====================================================================
//
// The bridge's host-facing surface used to be a growing pile of
// per-domain traits (`SurfaceController`, `ApprovalController`, …).
// Each new donor widget threatened to add another. The unified seam
// is `ShellAction` + `ShellHost`:
//
//   * `ShellAction` is the one closed enum the bridge routes through.
//   * `ShellHost::handle(action)` is the single method hosts impl.
//
// Per-domain traits (`SurfaceController`, `ApprovalController`) stay
// as the underlying primitives that the bundled `CompositeShellHost`
// composes. New code targets `ShellHost`; product hosts that already
// own a `SurfaceState` + `ApprovalQueue` build a composite from them.

/// Closed action enum routed by [`ShellHost`]. Variants are added
/// only when a new product behaviour ships, so the bridge surface
/// grows by enum variant rather than by trait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellAction {
    EnterSurface(SurfaceTarget),
    ToggleApproval { id: String },
    RejectAllApprovals,
    SubmitApprovals,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceTarget {
    Chat,
    Runtime,
}

/// Single host-facing seam. Hosts that compose multiple
/// sub-controllers route by action variant; hosts that only own one
/// domain return `DispatchError::UnknownAction` for the rest.
pub trait ShellHost: Send + Sync {
    fn handle(&self, action: ShellAction) -> Result<(), DispatchError>;
}

/// Concrete `ShellHost` that bundles the existing per-domain
/// controllers. The bundling is pure routing — no state of its own.
pub struct CompositeShellHost {
    surface: Option<Arc<dyn SurfaceController>>,
    approval: Option<Arc<dyn ApprovalController>>,
}

impl CompositeShellHost {
    pub fn new() -> Self {
        Self {
            surface: None,
            approval: None,
        }
    }

    pub fn with_surface(mut self, controller: Arc<dyn SurfaceController>) -> Self {
        self.surface = Some(controller);
        self
    }

    pub fn with_approval(mut self, controller: Arc<dyn ApprovalController>) -> Self {
        self.approval = Some(controller);
        self
    }
}

impl Default for CompositeShellHost {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellHost for CompositeShellHost {
    fn handle(&self, action: ShellAction) -> Result<(), DispatchError> {
        match action {
            ShellAction::EnterSurface(target) => {
                let ctrl = self
                    .surface
                    .as_ref()
                    .ok_or_else(|| DispatchError::Host("surface controller not bound".into()))?;
                match target {
                    SurfaceTarget::Chat => ctrl.enter_chat(),
                    SurfaceTarget::Runtime => ctrl.enter_runtime(),
                }
            }
            ShellAction::ToggleApproval { id } => {
                let ctrl = self
                    .approval
                    .as_ref()
                    .ok_or_else(|| DispatchError::Host("approval controller not bound".into()))?;
                ctrl.toggle(&id)
            }
            ShellAction::RejectAllApprovals => self
                .approval
                .as_ref()
                .ok_or_else(|| DispatchError::Host("approval controller not bound".into()))?
                .reject_all(),
            ShellAction::SubmitApprovals => self
                .approval
                .as_ref()
                .ok_or_else(|| DispatchError::Host("approval controller not bound".into()))?
                .submit_all(),
        }
    }
}

/// Host-action dispatcher for slash commands. Replaces
/// [`surface_dispatcher`] for new wiring; old wiring keeps working.
/// The closure resolves slashes onto `ShellAction` values and routes
/// via the unified host. Other slashes return
/// `DispatchError::UnknownSlash` so further handlers can compose.
pub fn host_dispatcher(host: Arc<dyn ShellHost>) -> DispatchHandler {
    Arc::new(move |spec: &ShellCommandSpec| -> Result<(), DispatchError> {
        let action = match spec.slash.as_str() {
            "/chat" => ShellAction::EnterSurface(SurfaceTarget::Chat),
            "/runtime" => ShellAction::EnterSurface(SurfaceTarget::Runtime),
            other => return Err(DispatchError::UnknownSlash(other.to_string())),
        };
        host.handle(action)
    })
}

// =====================================================================
// Surface controller — first real-effect seam
// =====================================================================
//
// Reviewer guidance for slice 2: the bridge keeps registry lookup +
// dispatch routing; the host owns the VAC-side effect. The trait
// below is the seam. A concrete impl lives outside this crate (see
// `vac_shell_host_surface`) so `vac_shell_bridge` does not take a
// link-time dependency on any VAC engine type.
//
// Stays synchronous on purpose; switching to async only when a real
// effect provably needs it.

/// VAC-side surface controller. The bridge calls into this trait to
/// flip the active operator surface (chat ↔ runtime) without
/// reaching into VAC types directly.
///
/// **Status:** building block for [`CompositeShellHost`]. New
/// integration code should target [`ShellHost`] + [`ShellAction`];
/// this trait is the underlying primitive that the composite
/// dispatches into.
pub trait SurfaceController: Send + Sync {
    fn enter_chat(&self) -> Result<(), DispatchError>;
    fn enter_runtime(&self) -> Result<(), DispatchError>;
}

// =====================================================================
// Approval controller — slice 4 seam
// =====================================================================
//
// Mirrors the SurfaceController pattern exactly: bridge owns the
// trait shape, host crate owns the actual queue + decision logic.
// The UI widget never reaches into either; it operates on a view
// projection and emits events that the host applies through this
// trait.
//
// Sync per the running constraint. Approval decision policy
// (auto-approve, hook gate, rulebook) stays in VAC core; the bridge
// trait surface is intentionally narrow.

/// Per-action decision the operator can hold while reviewing the
/// queue. Mirrors the donor `ApprovalStatus` but is owned here so the
/// UI widget never depends on donor types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    Approve,
    Reject,
}

/// VAC-side approval queue controller. The bridge calls into this
/// trait when the operator interacts with the widget.
///
/// **Status:** building block for [`CompositeShellHost`]. New
/// integration code should target [`ShellHost`] + [`ShellAction`];
/// this trait is the underlying primitive that the composite
/// dispatches into.
pub trait ApprovalController: Send + Sync {
    /// Toggle the decision attached to `id` between Approve / Reject.
    fn toggle(&self, id: &str) -> Result<(), DispatchError>;
    /// Set every pending action to Reject. Used on the second Esc.
    fn reject_all(&self) -> Result<(), DispatchError>;
    /// Commit decisions and clear the queue. The host decides what
    /// "commit" means (dispatch approved tools, append rejections to
    /// the transcript, …); the bridge does not.
    fn submit_all(&self) -> Result<(), DispatchError>;
}

/// Build a `DispatchHandler` that routes `/runtime` and `/chat`
/// through a `SurfaceController`. Other slashes return
/// `DispatchError::UnknownSlash` so the host knows to compose with
/// further handlers later. Composition strategy is deferred — for
/// this slice there is one effect.
pub fn surface_dispatcher(controller: Arc<dyn SurfaceController>) -> DispatchHandler {
    Arc::new(move |spec: &ShellCommandSpec| -> Result<(), DispatchError> {
        match spec.slash.as_str() {
            "/runtime" => controller.enter_runtime(),
            "/chat" => controller.enter_chat(),
            other => Err(DispatchError::UnknownSlash(other.to_string())),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(id: &str, slash: &str) -> ShellCommandSpec {
        ShellCommandSpec {
            id: id.into(),
            slash: slash.into(),
            title: id.into(),
            description: String::new(),
            kind: ShellCommandKind::BuiltInAction,
            palette_visible: true,
            shortcut: None,
        }
    }

    #[test]
    fn registry_upsert_replaces_by_id() {
        let r = InMemoryCommandRegistry::new(vec![spec("model", "/model")]);
        let mut updated = spec("model", "/model");
        updated.title = "renamed".into();
        assert!(r.upsert(updated));
        assert_eq!(r.all().len(), 1);
        assert_eq!(r.all()[0].title, "renamed");
    }

    #[test]
    fn unknown_slash_returns_error() {
        let registry: Arc<dyn VacCommandRegistry> =
            Arc::new(InMemoryCommandRegistry::new(vec![spec("model", "/model")]));
        let handler: DispatchHandler = Arc::new(|_| Ok(()));
        let dispatcher = CommandDispatcher::new(registry, handler);
        let err = dispatcher.dispatch("/missing").unwrap_err();
        assert_eq!(err, DispatchError::UnknownSlash("/missing".into()));
    }
}
