use ratatui::layout::Rect;

use crate::services::diagnostics_overlay::{DiagnosticsOverlayCache, HoverDetail};

/// Grouped state for LSP diagnostics surface + validation + hover.
#[derive(Debug, Clone, Default)]
pub struct LspUiState {
    pub validation_score: Option<f64>,
    pub validation_issues: Vec<String>,
    pub lsp_available: bool,
    pub lsp_diagnostics: Option<vac_core::lsp::types::LspWorkspaceSnapshot>,
    pub diagnostics_overlay_cache: DiagnosticsOverlayCache,
    pub active_hover: Option<HoverDetail>,
    pub hover_popup_region: Option<Rect>,
    pub active_hover_row_idx: Option<usize>,
}
