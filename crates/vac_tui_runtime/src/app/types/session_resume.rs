use super::SessionResumeEntry;

/// State for the session-resume overlay (Ctrl+R).
#[derive(Debug, Clone, Default)]
pub struct SessionResumeState {
    pub query: String,
    pub selected: usize,
    pub list: Vec<SessionResumeEntry>,
    pub filtered_indices: Vec<usize>,
    pub date_filter_days: Option<u32>,
}
