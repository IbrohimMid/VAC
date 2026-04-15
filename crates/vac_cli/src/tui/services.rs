//! Minimal Services Module
//!
//! Provides essential UI services for TUI operation.

pub mod message;
pub mod detect_term;
pub mod textarea;
pub mod helper_block;
pub mod syntax_highlighter;
pub mod markdown_renderer;
pub mod file_diff;
pub mod bash_block;
pub mod review;

// Re-export commonly used types
pub use detect_term::ThemeColors;
pub use textarea::{TextArea, TextAreaState};
pub use helper_block::welcome_messages;
pub use markdown_renderer::{MarkdownComponent, MarkdownStyle, render_markdown_to_lines, render_markdown_to_lines_safe};
pub use file_diff::{render_diff, preview_file_diff};
