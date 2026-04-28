//! Slice 5 — controller-shape normalization proof.
//!
//! Drives every variant of `ShellAction` through a `CompositeShellHost`
//! that wraps the existing per-domain controllers, asserting:
//!
//! 1. The bundled enum routes correctly to the right sub-controller.
//! 2. Missing sub-controllers surface a `Host` error, not a panic.
//! 3. The slash-command handler built from the unified seam
//!    (`host_dispatcher`) lights up `/runtime` and `/chat` against
//!    real surface state.
//!
//! Approval state lives in `vac_shell_host_approval`, surface state
//! in `vac_shell_host_surface`. Both are wired in here as dev-deps so
//! the test exercises the same composition the product code will.

use std::sync::{Arc, RwLock};

use vac_shell_approval_bar::ApprovalStatus;
use vac_shell_bridge::{
    ApprovalController, CommandDispatcher, CompositeShellHost, DispatchError,
    InMemoryCommandRegistry, ModelController, ProviderId, ShellAction, ShellHost,
    SurfaceController, SurfaceTarget, host_dispatcher,
};
use vac_shell_contracts::{ShellCommandKind, ShellCommandSpec, VacCommandRegistry};
use vac_shell_host_approval::{ApprovalQueue, ApprovalQueueController, ApprovalRequest};
use vac_shell_host_surface::{Surface as VacSurface, SurfaceState, SurfaceStateController};

fn build_full_host() -> (Arc<dyn ShellHost>, SurfaceState, ApprovalQueue) {
    let surface_state = SurfaceState::new(VacSurface::Chat);
    let surface_ctrl: Arc<dyn SurfaceController> =
        Arc::new(SurfaceStateController::new(surface_state.clone()));

    let approval_queue = ApprovalQueue::new();
    approval_queue.enqueue(ApprovalRequest::new("a", "run_command"));
    approval_queue.enqueue(ApprovalRequest::new("b", "create"));
    let approval_ctrl: Arc<dyn ApprovalController> =
        Arc::new(ApprovalQueueController::new(approval_queue.clone()));

    let host: Arc<dyn ShellHost> = Arc::new(
        CompositeShellHost::new()
            .with_surface(surface_ctrl)
            .with_approval(approval_ctrl),
    );
    (host, surface_state, approval_queue)
}

#[test]
fn enter_surface_runtime_flips_state_through_unified_seam() {
    let (host, state, _) = build_full_host();
    assert_eq!(state.current(), VacSurface::Chat);
    host.handle(ShellAction::EnterSurface(SurfaceTarget::Runtime))
        .unwrap();
    assert_eq!(state.current(), VacSurface::Runtime);
    host.handle(ShellAction::EnterSurface(SurfaceTarget::Chat))
        .unwrap();
    assert_eq!(state.current(), VacSurface::Chat);
}

#[test]
fn toggle_then_submit_drives_approval_outcome_via_unified_seam() {
    let (host, _, queue) = build_full_host();
    host.handle(ShellAction::ToggleApproval { id: "a".into() })
        .unwrap();
    let snap = queue.snapshot();
    assert_eq!(snap[0].status, ApprovalStatus::Rejected);
    assert_eq!(snap[1].status, ApprovalStatus::Approved);

    host.handle(ShellAction::SubmitApprovals).unwrap();
    let outcome = queue.last_outcome().expect("submit must record outcome");
    assert_eq!(outcome.approved, vec!["b".to_string()]);
    assert_eq!(outcome.rejected, vec!["a".to_string()]);
}

#[test]
fn reject_all_marks_every_request_rejected_via_unified_seam() {
    let (host, _, queue) = build_full_host();
    host.handle(ShellAction::RejectAllApprovals).unwrap();
    for r in queue.snapshot() {
        assert_eq!(r.status, ApprovalStatus::Rejected);
    }
}

#[derive(Default)]
struct RecordingModelController {
    selected: RwLock<Option<(ProviderId, String)>>,
}

impl ModelController for RecordingModelController {
    fn select_model(&self, provider: &ProviderId, id: &str) -> Result<(), DispatchError> {
        *self.selected.write().unwrap() = Some((provider.clone(), id.to_string()));
        Ok(())
    }
}

#[test]
fn select_model_routes_through_unified_seam() {
    let ctrl = Arc::new(RecordingModelController::default());
    let host: Arc<dyn ShellHost> =
        Arc::new(CompositeShellHost::new().with_model(ctrl.clone() as Arc<dyn ModelController>));
    host.handle(ShellAction::SelectModel {
        provider: ProviderId("anthropic".into()),
        id: "claude-sonnet-4.5".into(),
    })
    .unwrap();
    let observed = ctrl.selected.read().unwrap().clone();
    assert_eq!(
        observed,
        Some((ProviderId("anthropic".into()), "claude-sonnet-4.5".into())),
    );
}

#[test]
fn unbound_model_controller_returns_host_error_not_panic() {
    let host: Arc<dyn ShellHost> = Arc::new(CompositeShellHost::new());
    let err = host
        .handle(ShellAction::SelectModel {
            provider: ProviderId("anthropic".into()),
            id: "claude".into(),
        })
        .unwrap_err();
    match err {
        DispatchError::Host(msg) => assert!(msg.contains("model"), "msg: {msg}"),
        other => panic!("expected Host(...), got {other:?}"),
    }
}

#[test]
fn unbound_subcontroller_returns_host_error_not_panic() {
    // Surface only — approval missing.
    let surface_state = SurfaceState::new(VacSurface::Chat);
    let surface_ctrl: Arc<dyn SurfaceController> =
        Arc::new(SurfaceStateController::new(surface_state.clone()));
    let host: Arc<dyn ShellHost> = Arc::new(CompositeShellHost::new().with_surface(surface_ctrl));

    let err = host
        .handle(ShellAction::ToggleApproval { id: "a".into() })
        .unwrap_err();
    match err {
        DispatchError::Host(msg) => assert!(msg.contains("approval"), "msg: {msg}"),
        other => panic!("expected Host(...), got {other:?}"),
    }
}

#[test]
fn host_dispatcher_routes_runtime_and_chat_through_unified_seam() {
    let (host, state, _) = build_full_host();
    let registry: Arc<dyn VacCommandRegistry> = Arc::new(InMemoryCommandRegistry::new(vec![
        ShellCommandSpec {
            id: "runtime".into(),
            slash: "/runtime".into(),
            title: "Runtime".into(),
            description: String::new(),
            kind: ShellCommandKind::BuiltInAction,
            palette_visible: true,
            shortcut: None,
            ..Default::default()
        },
        ShellCommandSpec {
            id: "chat".into(),
            slash: "/chat".into(),
            title: "Chat".into(),
            description: String::new(),
            kind: ShellCommandKind::BuiltInAction,
            palette_visible: true,
            shortcut: None,
            ..Default::default()
        },
    ]));
    let dispatcher = CommandDispatcher::new(registry, host_dispatcher(host));

    dispatcher.dispatch("/runtime").unwrap();
    assert_eq!(state.current(), VacSurface::Runtime);
    dispatcher.dispatch("/chat").unwrap();
    assert_eq!(state.current(), VacSurface::Chat);
    let unknown = dispatcher.dispatch("/nope").unwrap_err();
    assert_eq!(unknown, DispatchError::UnknownSlash("/nope".into()));
}
