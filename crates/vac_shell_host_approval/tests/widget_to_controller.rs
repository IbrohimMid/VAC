//! Slice 4 acceptance proof — full chain widget → controller →
//! VAC-owned queue mutation.
//!
//! The widget never holds queue state; the host crate does. This
//! test wires the two together as a real shell would: build a
//! queue, project to a view, drive the view via key events, apply
//! emitted intents through the `ApprovalController` trait, and
//! assert the queue mutated as expected.

use vac_shell_approval_bar::{
    ApprovalBarEvent, ApprovalBarKey, ApprovalBarViewState, ApprovalStatus, on_key,
};
use vac_shell_bridge::ApprovalController;
use vac_shell_host_approval::{ApprovalQueue, ApprovalQueueController, ApprovalRequest};

fn build() -> (ApprovalQueue, ApprovalQueueController) {
    let q = ApprovalQueue::new();
    q.enqueue(ApprovalRequest::new("a", "run_command"));
    q.enqueue(ApprovalRequest::new("b", "create"));
    q.enqueue(ApprovalRequest::new("c", "str_replace"));
    let c = ApprovalQueueController::new(q.clone());
    (q, c)
}

fn apply(controller: &ApprovalQueueController, event: ApprovalBarEvent) {
    match event {
        ApprovalBarEvent::Toggle(id) => controller.toggle(&id).unwrap(),
        ApprovalBarEvent::SubmitAll => controller.submit_all().unwrap(),
        ApprovalBarEvent::RejectAll => controller.reject_all().unwrap(),
        ApprovalBarEvent::EscPrimed
        | ApprovalBarEvent::Consumed
        | ApprovalBarEvent::Ignored => {}
    }
}

#[test]
fn space_then_enter_submits_with_one_rejection() {
    let (queue, controller) = build();
    let mut view = queue.to_view();

    // Right twice → row "c", Space toggles, Enter submits.
    apply(&controller, on_key(&mut view, ApprovalBarKey::Right));
    apply(&controller, on_key(&mut view, ApprovalBarKey::Right));
    let toggle = on_key(&mut view, ApprovalBarKey::Space);
    assert_eq!(toggle, ApprovalBarEvent::Toggle("c".into()));
    apply(&controller, toggle);

    // Re-project so the view sees the new status before the
    // submission. Same flow a real shell would run on every tick.
    view = queue.to_view();
    let after_toggle = view.actions.iter().find(|a| a.id == "c").unwrap().status;
    assert_eq!(after_toggle, ApprovalStatus::Rejected);

    apply(&controller, on_key(&mut view, ApprovalBarKey::Enter));

    let outcome = queue.last_outcome().expect("outcome must record");
    assert_eq!(outcome.approved, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(outcome.rejected, vec!["c".to_string()]);
    assert!(queue.snapshot().is_empty(), "queue drained on submit");
}

#[test]
fn double_escape_rejects_all_via_widget() {
    let (queue, controller) = build();
    let mut view = queue.to_view();

    let first = on_key(&mut view, ApprovalBarKey::Escape);
    assert_eq!(first, ApprovalBarEvent::EscPrimed);
    apply(&controller, first);

    let second = on_key(&mut view, ApprovalBarKey::Escape);
    assert_eq!(second, ApprovalBarEvent::RejectAll);
    apply(&controller, second);

    for r in queue.snapshot() {
        assert_eq!(r.status, ApprovalStatus::Rejected);
    }
}

#[test]
fn invisible_view_emits_ignored_and_does_not_mutate() {
    let queue = ApprovalQueue::new();
    let controller = ApprovalQueueController::new(queue.clone());
    let mut view = ApprovalBarViewState::default(); // visible = false

    let event = on_key(&mut view, ApprovalBarKey::Enter);
    assert_eq!(event, ApprovalBarEvent::Ignored);
    apply(&controller, event);

    assert!(queue.snapshot().is_empty());
    assert!(queue.last_outcome().is_none());
}
