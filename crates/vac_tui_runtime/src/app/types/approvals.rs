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
    /// F6.2 — checkbox selection overlay: indices of
    /// `pending_approvals` the operator has ticked for bulk
    /// approve/reject. Empty when no bulk selection is active.
    pub bulk_selected: HashSet<usize>,
}

impl ApprovalsState {
    /// F6.2 — Mark `indices` as bulk-selected. Invalid indices (out
    /// of range vs current `pending_approvals.len()`) are filtered.
    pub fn bulk_select(&mut self, indices: impl IntoIterator<Item = usize>) {
        let limit = self.pending_approvals.len();
        self.bulk_selected
            .extend(indices.into_iter().filter(|i| *i < limit));
    }

    pub fn bulk_clear(&mut self) {
        self.bulk_selected.clear();
    }

    pub fn bulk_toggle(&mut self, idx: usize) {
        if idx >= self.pending_approvals.len() {
            return;
        }
        if !self.bulk_selected.insert(idx) {
            self.bulk_selected.remove(&idx);
        }
    }

    /// Drain the tool-calls corresponding to the current bulk
    /// selection and return them in ascending-index order. Clears the
    /// selection afterwards. Driver then feeds these into the
    /// existing per-call approve/reject pipeline.
    pub fn drain_bulk_selection(&mut self) -> Vec<ToolCall> {
        if self.bulk_selected.is_empty() {
            return Vec::new();
        }
        let mut idxs: Vec<_> = self.bulk_selected.iter().copied().collect();
        idxs.sort_unstable();
        // Walk descending so removing by index doesn't shift later hits.
        let mut out = Vec::with_capacity(idxs.len());
        for i in idxs.iter().rev() {
            if *i < self.pending_approvals.len() {
                out.push(self.pending_approvals.remove(*i));
            }
        }
        // Flip back to original (ascending-index) order for the caller.
        out.reverse();
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
        assert!(!s.bulk_selected.contains(&99));
    }

    #[test]
    fn bulk_toggle_is_symmetric() {
        let mut s = ApprovalsState::default();
        s.pending_approvals = vec![tc("a")];
        s.bulk_toggle(0);
        assert!(s.bulk_selected.contains(&0));
        s.bulk_toggle(0);
        assert!(!s.bulk_selected.contains(&0));
    }

    #[test]
    fn drain_bulk_selection_returns_ascending_and_clears() {
        let mut s = ApprovalsState::default();
        s.pending_approvals = vec![tc("a"), tc("b"), tc("c"), tc("d")];
        s.bulk_select([0, 2]);
        let drained = s.drain_bulk_selection();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].id, "a");
        assert_eq!(drained[1].id, "c");
        assert_eq!(s.pending_approvals.len(), 2);
        assert_eq!(s.pending_approvals[0].id, "b");
        assert_eq!(s.pending_approvals[1].id, "d");
        assert!(s.bulk_selected.is_empty());
    }

    #[test]
    fn drain_empty_selection_is_noop() {
        let mut s = ApprovalsState::default();
        s.pending_approvals = vec![tc("a")];
        assert!(s.drain_bulk_selection().is_empty());
        assert_eq!(s.pending_approvals.len(), 1);
    }
}
