//! Event Loop Module

pub use crate::handlers::input::handle_input_event;
pub use crate::update::{
    classify_critical_banner, handle_backend_event, open_ask_user_popup,
};

// Rulebook configuration was originally here but now it's in controller.
// Wait, is RulebookConfig used elsewhere? Let's check.
