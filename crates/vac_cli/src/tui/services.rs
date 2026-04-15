//! Minimal Services Module
//!
//! Provides essential UI services for TUI operation.

pub mod bash_block;
pub mod approval_bar;
pub mod changeset;
pub mod detect_term;
pub mod file_diff;
pub mod file_search;
pub mod helper_block;
pub mod markdown_renderer;
pub mod message;
pub mod review;
pub mod syntax_highlighter;
pub mod textarea;
pub mod toast;

// Re-export commonly used types
pub use detect_term::ThemeColors;
pub use approval_bar::approval_preview;
pub use changeset::{ChangesetEntry, build_changeset};
pub use file_diff::{preview_file_diff, render_diff};
pub use file_search::{build_file_index, fuzzy_search_files};
pub use helper_block::welcome_messages;
pub use markdown_renderer::{
    MarkdownComponent, MarkdownStyle, render_markdown_to_lines, render_markdown_to_lines_safe,
};
pub use textarea::{TextArea, TextAreaState};
pub use toast::{Toast, ToastStyle};
