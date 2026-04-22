use super::workbench::ReviewDiffState;

/// Grouped state for the changeset list + diff viewer.
#[derive(Debug, Clone, Default)]
pub struct ChangesetUiState {
    pub selected_idx: usize,
    pub diff_scroll: usize,
    pub selected_path: Option<String>,
    pub diff: Option<ReviewDiffState>,
}
