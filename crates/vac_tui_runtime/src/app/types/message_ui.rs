use uuid::Uuid;

use super::{MessageLinesCache, PerMessageCache};

/// State for message list rendering (caches, geometry, revert anchor).
#[derive(Debug, Clone, Default)]
pub struct MessageUiState {
    pub per_message_cache: PerMessageCache,
    pub assembled_lines_cache: Option<MessageLinesCache>,
    pub collapsed_message_lines_cache: Option<MessageLinesCache>,
    pub message_area_y: u16,
    pub message_area_height: u16,
    pub line_to_message_map: Vec<Uuid>,
    pub pending_revert_index: Option<usize>,
}
