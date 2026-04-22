//! Unified Commands, Shortcuts & Sessions Popup
//!
//! This module provides a unified popup with:
//! - Commands section: Searchable and triggerable command palette items
//! - Shortcuts section: Read-only keyboard shortcuts grouped by category
//! - Sessions section: List of previous sessions to resume

pub mod catalog;
pub mod render;
pub mod search;

pub use catalog::{Shortcut, build_shortcuts_content, get_all_shortcuts, get_shortcuts_count};
pub use render::render_shortcuts_popup;
pub use search::filter_commands;
