use crate::services::clipboard_paste::PastedItem;

/// Paste ledger + attachment tray state.
#[derive(Debug, Clone, Default)]
pub struct PasteState {
    pub pending_pastes: Vec<PastedItem>,
    pub is_pasting: bool,
    pub paste_counter: usize,
    pub pending_paste_selected: usize,
    pub pending_paste_reorder_mode: bool,
}
