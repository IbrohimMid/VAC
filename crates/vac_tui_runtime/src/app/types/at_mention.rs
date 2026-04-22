/// @-mention picker state (file/path completion triggered in the input bar).
#[derive(Debug, Clone, Default)]
pub struct AtMentionState {
    pub trigger_active: bool,
    pub query: String,
    pub results: Vec<String>,
    pub selected_idx: usize,
}
