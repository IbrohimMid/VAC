//! Minimal Services Module
//!
//! Provides essential UI services for TUI operation.

pub mod bash_block;
pub mod detect_term;
pub mod file_diff;
pub mod helper_block;
pub mod markdown_renderer;
pub mod message;
pub mod review;
pub mod syntax_highlighter;
pub mod textarea;

// Re-export commonly used types
pub use detect_term::ThemeColors;
pub use file_diff::{preview_file_diff, render_diff};
pub use helper_block::welcome_messages;
pub use markdown_renderer::{
    MarkdownComponent, MarkdownStyle, render_markdown_to_lines, render_markdown_to_lines_safe,
};
pub use textarea::{TextArea, TextAreaState};
