/// Pinned content surfaced in the side panel (files, diffs, diagnostics,
/// runtime items, plan items).
#[derive(Debug, Clone, Default)]
pub struct PinsState {
    pub files: Vec<String>,
    pub diffs: Vec<String>,
    pub diagnostics: Vec<String>,
    pub runtime_items: Vec<String>,
    pub plan_items: Vec<String>,
}
