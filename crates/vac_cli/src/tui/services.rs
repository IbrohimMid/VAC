//! Minimal Services Module
//!
//! Provides essential UI services for TUI operation.

pub mod approval_bar;
pub mod bash_block;
pub mod changeset;
pub mod clipboard_paste;
pub mod detect_term;
pub mod file_diff;
pub mod file_search;
pub mod helper_block;
pub mod helper_dropdown;
pub mod markdown_renderer;
pub mod message;
pub mod message_action_popup;
pub mod review;
pub mod shell_mode;
pub mod shortcuts_popup;
pub mod side_panel;
pub mod profile_switcher;
pub mod isolation_switcher;
pub mod rulebook_switcher;
pub mod syntax_highlighter;
pub mod text_selection;
pub mod textarea;
pub mod toast;

// Re-export commonly used types
pub use approval_bar::approval_preview;
pub use changeset::{ChangesetEntry, ChangesetStore, FileState, build_changeset};
pub use clipboard_paste::{extract_file_paths_from_text, paste_image_to_temp_png, copy_to_clipboard};
pub use detect_term::ThemeColors;
pub use file_diff::{preview_file_diff, render_diff};
pub use file_search::{build_file_index, fuzzy_search_files};
pub use helper_block::welcome_messages;
pub use markdown_renderer::{
    MarkdownComponent, MarkdownStyle, render_markdown_to_lines, render_markdown_to_lines_safe,
};
pub use shell_mode::{ShellCommand, ShellEvent, run_pty_command};
pub use shortcuts_popup::{Shortcut, get_all_shortcuts, render_shortcuts_popup};
pub use textarea::{TextArea, TextAreaState};
pub use text_selection::SelectionState;
pub use toast::{Toast, ToastStyle};
