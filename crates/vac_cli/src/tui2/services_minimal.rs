//! Minimal Services Module
//!
//! Provides essential UI services for TUI operation.

pub mod message;
pub mod detect_term;
pub mod textarea;
pub mod helper_block;

// Re-export commonly used types
pub use detect_term::ThemeColors;
pub use textarea::{TextArea, TextAreaState};
pub use helper_block::welcome_messages;