use std::time::Instant;

use uuid::Uuid;

/// LLM streaming state.
#[derive(Debug, Clone, Default)]
pub struct StreamingState {
    pub is_streaming: bool,
    pub message_id: Option<Uuid>,
    pub start: Option<Instant>,
    pub tokens: u64,
}
