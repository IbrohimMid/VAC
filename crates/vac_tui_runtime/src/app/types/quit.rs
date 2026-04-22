use std::time::Instant;

/// Quit-safety + cancel state. Prevents accidental session loss.
#[derive(Debug, Clone, Default)]
pub struct QuitState {
    pub press_count: u8,
    pub first_press: Option<Instant>,
    pub cancel_requested: bool,
}
