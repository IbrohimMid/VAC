use std::sync::Arc;

/// File-index, file-search, and file-changes browser state.
#[derive(Debug, Clone, Default)]
pub struct FileIndexState {
    /// Complete project file list (populated by ingest).
    pub all_files: Vec<String>,
    /// BM25 index loaded from `.vac/bm25.index`
    pub bm25_index: Option<Arc<vac_ingest::Bm25Index>>,
    /// Current @-style search query (file_search popup).
    pub search_query: String,
    pub search_selected_idx: usize,
    pub search_results: Vec<String>,
    // File-changes browser (modified files panel).
    pub changes_selected: usize,
    pub changes_search: String,
    pub changes_scroll: usize,
}
