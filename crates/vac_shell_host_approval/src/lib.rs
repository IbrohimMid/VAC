//! Step 2 slice 4 — VAC-owned approval queue + decision state.
//!
//! Mirrors the `vac_shell_host_surface` pattern. Owns the actual
//! mutable queue and the per-action decisions; implements
//! `vac_shell_bridge::ApprovalController` so the bridge can route
//! widget events into state mutations without touching this crate's
//! types.
//!
//! Auto-approve, hook-gate, and rulebook-policy logic are
//! deliberately absent — those decisions are owned by VAC core and
//! land in a later slice. For now the operator's per-row toggle is
//! the only decision recorded.

use std::sync::{Arc, Mutex};

use vac_shell_approval_bar::{ApprovalActionView, ApprovalBarViewState};
use vac_shell_bridge::{ApprovalController, DispatchError};

// Re-export so downstream callers (e.g. `vac_shell_composition`)
// can assert on row status without taking a direct dep on
// `vac_shell_approval_bar`.
pub use vac_shell_approval_bar::ApprovalStatus;

/// One pending action in the queue. The widget never sees this type.
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub id: String,
    pub tool_name: String,
    pub label: String,
    pub status: ApprovalStatus,
}

impl ApprovalRequest {
    pub fn new(id: impl Into<String>, tool_name: impl Into<String>) -> Self {
        let tool_name = tool_name.into();
        let label = vac_shell_approval_bar::format_tool_label(&tool_name);
        Self {
            id: id.into(),
            tool_name,
            label,
            status: ApprovalStatus::Approved,
        }
    }
}

/// Outcome of `submit_all` — the host drains the queue into one of
/// these so the live VAC engine can act on the decisions. Submission
/// itself does not run tool dispatch; it only collects which ids
/// were approved vs rejected, leaving the actual execution to the
/// caller.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApprovalOutcome {
    pub approved: Vec<String>,
    pub rejected: Vec<String>,
}

/// Shared, observable approval queue.
#[derive(Debug, Clone, Default)]
pub struct ApprovalQueue {
    inner: Arc<Mutex<QueueInner>>,
}

#[derive(Debug, Default)]
struct QueueInner {
    requests: Vec<ApprovalRequest>,
    last_outcome: Option<ApprovalOutcome>,
}

impl ApprovalQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue(&self, request: ApprovalRequest) {
        self.inner
            .lock()
            .expect("ApprovalQueue lock poisoned")
            .requests
            .push(request);
    }

    pub fn snapshot(&self) -> Vec<ApprovalRequest> {
        self.inner
            .lock()
            .expect("ApprovalQueue lock poisoned")
            .requests
            .clone()
    }

    pub fn last_outcome(&self) -> Option<ApprovalOutcome> {
        self.inner
            .lock()
            .expect("ApprovalQueue lock poisoned")
            .last_outcome
            .clone()
    }

    /// Build a fresh `ApprovalBarViewState` from the queue snapshot.
    /// `selected_index` defaults to `0` and `visible` is `true` when
    /// the queue is non-empty; callers that maintain selection
    /// across renders should overwrite those after calling.
    pub fn to_view(&self) -> ApprovalBarViewState {
        let snap = self.snapshot();
        let visible = !snap.is_empty();
        ApprovalBarViewState {
            actions: snap
                .iter()
                .map(|r| ApprovalActionView {
                    id: r.id.clone(),
                    label: r.label.clone(),
                    status: r.status,
                })
                .collect(),
            selected_index: 0,
            visible,
            esc_pending: false,
        }
    }
}

/// Concrete `ApprovalController` over an `ApprovalQueue`. Cheap
/// to clone — only the queue's `Arc` is shared.
#[derive(Debug, Clone)]
pub struct ApprovalQueueController {
    queue: ApprovalQueue,
}

impl ApprovalQueueController {
    pub fn new(queue: ApprovalQueue) -> Self {
        Self { queue }
    }

    pub fn queue(&self) -> ApprovalQueue {
        self.queue.clone()
    }
}

impl ApprovalController for ApprovalQueueController {
    fn toggle(&self, id: &str) -> Result<(), DispatchError> {
        let mut inner = self.queue.inner.lock().expect("queue lock poisoned");
        let req = inner
            .requests
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or_else(|| DispatchError::Host(format!("approval id not found: {id}")))?;
        req.status = match req.status {
            ApprovalStatus::Approved => ApprovalStatus::Rejected,
            ApprovalStatus::Rejected => ApprovalStatus::Approved,
        };
        Ok(())
    }

    fn reject_all(&self) -> Result<(), DispatchError> {
        let mut inner = self.queue.inner.lock().expect("queue lock poisoned");
        for r in inner.requests.iter_mut() {
            r.status = ApprovalStatus::Rejected;
        }
        Ok(())
    }

    fn submit_all(&self) -> Result<(), DispatchError> {
        let mut inner = self.queue.inner.lock().expect("queue lock poisoned");
        let mut outcome = ApprovalOutcome::default();
        for r in inner.requests.drain(..) {
            match r.status {
                ApprovalStatus::Approved => outcome.approved.push(r.id),
                ApprovalStatus::Rejected => outcome.rejected.push(r.id),
            }
        }
        inner.last_outcome = Some(outcome);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed() -> (ApprovalQueue, ApprovalQueueController) {
        let q = ApprovalQueue::new();
        q.enqueue(ApprovalRequest::new("a", "run_command"));
        q.enqueue(ApprovalRequest::new("b", "create"));
        let c = ApprovalQueueController::new(q.clone());
        (q, c)
    }

    #[test]
    fn toggle_flips_status_for_id() {
        let (q, c) = seed();
        c.toggle("a").unwrap();
        let snap = q.snapshot();
        assert_eq!(snap[0].status, ApprovalStatus::Rejected);
        assert_eq!(snap[1].status, ApprovalStatus::Approved);
    }

    #[test]
    fn toggle_unknown_id_returns_host_error() {
        let (_, c) = seed();
        let err = c.toggle("missing").unwrap_err();
        assert!(matches!(err, DispatchError::Host(msg) if msg.contains("missing")));
    }

    #[test]
    fn reject_all_marks_every_row_rejected() {
        let (q, c) = seed();
        c.reject_all().unwrap();
        for r in q.snapshot() {
            assert_eq!(r.status, ApprovalStatus::Rejected);
        }
    }

    #[test]
    fn submit_all_drains_queue_and_records_outcome() {
        let (q, c) = seed();
        c.toggle("b").unwrap();
        c.submit_all().unwrap();
        assert!(q.snapshot().is_empty(), "queue must drain");
        let outcome = q.last_outcome().expect("outcome must be recorded");
        assert_eq!(outcome.approved, vec!["a".to_string()]);
        assert_eq!(outcome.rejected, vec!["b".to_string()]);
    }

    #[test]
    fn empty_queue_yields_invisible_view() {
        let q = ApprovalQueue::new();
        assert!(!q.to_view().is_visible());
    }
}
