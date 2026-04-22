use super::{HelperCommand, ShortcutsPopupMode};
use crate::services::recent_commands::RecentCommands;

/// Command-palette + helper-browser + shortcuts popup state.
#[derive(Debug, Clone)]
pub struct CommandPaletteState {
    pub input: String,
    pub selected: usize,
    pub scroll: usize,
    pub helper_scroll: usize,
    pub helper_selected: usize,
    pub filtered_helpers: Vec<HelperCommand>,
    pub recent_commands: RecentCommands,
    pub shortcuts_mode: ShortcutsPopupMode,
    pub shortcuts_scroll: usize,
}

impl Default for CommandPaletteState {
    fn default() -> Self {
        Self {
            input: String::new(),
            selected: 0,
            scroll: 0,
            helper_scroll: 0,
            helper_selected: 0,
            filtered_helpers: Vec::new(),
            recent_commands: RecentCommands::load(),
            shortcuts_mode: ShortcutsPopupMode::default(),
            shortcuts_scroll: 0,
        }
    }
}
