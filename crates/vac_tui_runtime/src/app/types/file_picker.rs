use std::collections::HashSet;
use std::path::PathBuf;

/// Grouped state for the file-picker popup.
#[derive(Debug, Clone)]
pub struct FilePickerState {
    pub query: String,
    pub selected: usize,
    pub results: Vec<PathBuf>,
    pub multi_selected: HashSet<usize>,
    pub cwd: PathBuf,
    pub type_filter: Option<String>,
    pub preview: Option<String>,
}

impl FilePickerState {
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            query: String::new(),
            selected: 0,
            results: Vec::new(),
            multi_selected: HashSet::new(),
            cwd,
            type_filter: None,
            preview: None,
        }
    }
}

impl Default for FilePickerState {
    fn default() -> Self {
        Self::new(std::env::current_dir().unwrap_or_default())
    }
}
