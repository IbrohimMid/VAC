//! Command registry — single source of truth for slash commands,
//! palette entries, and shortcuts. Replaces the dual `ACTION_SPECS` /
//! `vac_commands()` system in VAC and bypasses the donor's
//! `get_all_commands()` + `commands_to_helper_commands()` legacy layer.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShellCommandKind {
    /// In-process action handled by VAC (e.g. `/model`, `/runtime`).
    BuiltInAction,
    /// Templated prompt that gets fed to the agent verbatim.
    PromptTemplate,
    /// Routes the operator into an overlay/picker.
    OverlayRoute,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellCommandSpec {
    pub id: String,
    pub slash: String,
    pub title: String,
    pub description: String,
    pub kind: ShellCommandKind,
    /// Whether to show in the palette / completion dropdown.
    pub palette_visible: bool,
    /// Optional keybinding string (e.g. `Ctrl+P`) for help renderers.
    pub shortcut: Option<String>,
}

pub trait VacCommandRegistry: Send + Sync {
    /// All commands, including built-ins, predefined, and user-custom.
    /// Donor never merges multiple sources itself.
    fn all(&self) -> Vec<ShellCommandSpec>;

    /// Resolve by exact slash string (e.g. `/model`).
    fn by_slash(&self, slash: &str) -> Option<ShellCommandSpec>;
}
