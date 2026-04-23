//! Minimal Services Module
//!
//! Provides essential UI services for TUI operation.

pub mod approval_bar;
pub mod ask_user;
pub mod banner;
pub mod bash_block;
pub mod clipboard_paste;
pub mod detect_term;
pub mod diagnostics_overlay;
pub mod file_changes_popup;
pub mod file_diff;
pub mod file_search;
pub mod helper_block;
pub mod helper_dropdown;
pub mod image_preview_cache;
pub mod isolation_switcher;
pub mod keybindings_loader;
pub mod keybindings_runtime;
pub mod keybindings_watcher;
pub mod kitty_image;
pub mod hunk_fold;
pub mod markdown_renderer;
pub mod memory_banner;
pub mod memory_consolidator;
pub mod message;
pub mod message_action_popup;
pub mod notifier;
pub mod plan;
pub mod plan_review;
pub mod prevent_sleep;
pub mod profile_switcher;
pub mod prompt_suggest;
pub mod rate_limit;
pub mod recent_commands;
pub mod recorder;
pub mod replay_bridge;
pub mod review;
pub mod review_preview;
pub mod rewind;
pub mod rulebook_switcher;
pub mod shortcuts_popup;
pub mod side_panel;
pub mod sparkline;
pub mod statusline;
pub mod syntax_highlighter;
pub mod text_selection;
pub mod token_estimation;
pub mod textarea;
pub mod theme;
pub mod theme_loader;
pub mod toast;
pub mod todo_extractor;
pub mod vil_dev_runner;
pub mod vil_expr_lint;
pub mod vil_workbench;
pub mod vwfd_diff_render;
pub mod vwfd_inspector;

// Re-export commonly used types
pub use approval_bar::approval_preview;
pub use clipboard_paste::{
    copy_to_clipboard, extract_file_paths_from_text, paste_image_to_temp_png,
};
pub use file_diff::{preview_file_diff, render_diff};
pub use file_search::{build_file_index, fuzzy_search_files, ranked_search_files};
pub use helper_block::welcome_messages;
pub use markdown_renderer::{
    MarkdownComponent, MarkdownStyle, render_markdown_to_lines, render_markdown_to_lines_safe,
};
pub use shortcuts_popup::{Shortcut, get_all_shortcuts, render_shortcuts_popup};
pub use text_selection::SelectionState;
pub use textarea::{TextArea, TextAreaState};
pub use toast::{Toast, ToastStyle};
pub mod commands;
