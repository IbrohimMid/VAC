use std::collections::{HashMap, HashSet};

use crate::types::ToolCall;

/// Approval domain state grouped out of `AppState`.
#[derive(Debug, Clone, Default)]
pub struct ApprovalsState {
    pub pending_approvals: Vec<ToolCall>,
    pub pending_tool_calls: Vec<ToolCall>,
    pub approved_tools: Vec<ToolCall>,
    pub rejected_tools: Vec<ToolCall>,
    pub approval_selected_idx: usize,
    pub approval_detail_scroll: usize,
    pub approval_explanations: HashMap<String, Option<String>>,
    pub reject_reason_input: Option<String>,
    /// F6.2 — checkbox selection overlay: `ToolCall.id`s the
    /// operator has ticked for bulk approve/reject. Keyed by id
    /// (not index) so arrivals / single-approves cannot silently
    /// shift the selection onto the wrong call. Empty when no bulk
    /// selection is active.
    pub bulk_selected: HashSet<String>,
}

impl ApprovalsState {
    /// F6.2 — Mark the approvals at `indices` as bulk-selected by
    /// resolving each index to its `ToolCall.id`. Invalid indices
    /// (out of range vs current `pending_approvals.len()`) are
    /// filtered.
    pub fn bulk_select(&mut self, indices: impl IntoIterator<Item = usize>) {
        for idx in indices {
            if let Some(tc) = self.pending_approvals.get(idx) {
                self.bulk_selected.insert(tc.id.clone());
            }
        }
    }

    pub fn bulk_clear(&mut self) {
        self.bulk_selected.clear();
    }

    /// Toggle bulk membership for the approval at `idx`. Resolves
    /// through `ToolCall.id` so subsequent mutations to
    /// `pending_approvals` don't invalidate the selection.
    pub fn bulk_toggle(&mut self, idx: usize) {
        let Some(id) = self.pending_approvals.get(idx).map(|tc| tc.id.clone()) else {
            return;
        };
        if !self.bulk_selected.insert(id.clone()) {
            self.bulk_selected.remove(&id);
        }
    }

    /// Return clones of the `ToolCall`s whose ids are in the current
    /// bulk selection, preserving the order they appear in
    /// `pending_approvals`. Does NOT mutate `pending_approvals` —
    /// the driver's existing per-call approve/reject pipeline is the
    /// single place that removes entries, so bulk and single paths
    /// cannot diverge. Clears the selection set afterwards.
    pub fn drain_bulk_selection(&mut self) -> Vec<ToolCall> {
        if self.bulk_selected.is_empty() {
            return Vec::new();
        }
        let out: Vec<ToolCall> = self
            .pending_approvals
            .iter()
            .filter(|tc| self.bulk_selected.contains(&tc.id))
            .cloned()
            .collect();
        self.bulk_selected.clear();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolCall;

    fn tc(id: &str) -> ToolCall {
        ToolCall {
            id: id.into(),
            r#type: "function".into(),
            function: crate::types::FunctionCall {
                name: "t".into(),
                arguments: "{}".into(),
            },
            metadata: None,
        }
    }

    #[test]
    fn bulk_select_filters_out_of_range() {
        let mut s = ApprovalsState::default();
        s.pending_approvals = vec![tc("a"), tc("b")];
        s.bulk_select([0, 1, 99]);
        assert_eq!(s.bulk_selected.len(), 2);
        assert!(s.bulk_selected.contains("a"));
        assert!(s.bulk_selected.contains("b"));
    }

    #[test]
    fn bulk_toggle_is_symmetric() {
        let mut s = ApprovalsState::default();
        s.pending_approvals = vec![tc("a")];
        s.bulk_toggle(0);
        assert!(s.bulk_selected.contains("a"));
        s.bulk_toggle(0);
        assert!(!s.bulk_selected.contains("a"));
    }

    #[test]
    fn drain_bulk_selection_returns_ids_in_pending_order_without_removing() {
        let mut s = ApprovalsState::default();
        s.pending_approvals = vec![tc("a"), tc("b"), tc("c"), tc("d")];
        s.bulk_select([2, 0]); // deliberately out of order
        let drained = s.drain_bulk_selection();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].id, "a");
        assert_eq!(drained[1].id, "c");
        // Critical: pending_approvals UNCHANGED — driver's single-call
        // pipeline is the only place that removes.
        assert_eq!(
            s.pending_approvals
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c", "d"],
        );
        assert!(s.bulk_selected.is_empty());
    }

    #[test]
    fn bulk_selection_survives_reordering_of_pending_approvals() {
        let mut s = ApprovalsState::default();
        s.pending_approvals = vec![tc("a"), tc("b"), tc("c")];
        s.bulk_select([1]); // select "b" at index 1
        // Driver swaps index 0 and 1 (e.g. a new high-priority call
        // arrived and displaced "a").
        s.pending_approvals.swap(0, 1);
        // Selection still means "b" — not whatever is at index 1 now.
        let drained = s.drain_bulk_selection();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].id, "b");
    }

    #[test]
    fn drain_empty_selection_is_noop() {
        let mut s = ApprovalsState::default();
        s.pending_approvals = vec![tc("a")];
        assert!(s.drain_bulk_selection().is_empty());
        assert_eq!(s.pending_approvals.len(), 1);
    }
}
