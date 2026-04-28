//! Search and filtering logic for commands and shortcuts
//!
//! This module provides filtering and sorting operations for:
//! - Command palette (fuzzy search by name/description, sorted by frequency)
//! - Shortcuts filtering (substring match across key, description, category)

use crate::services::commands::{Command, CommandAction};

/// Static UI-action entries that have no slash-command equivalent.
fn ui_action_commands() -> Vec<Command> {
    vec![
        Command {
            name: "Switch Profile".into(),
            description: "Change the active user profile".into(),
            shortcut: "Ctrl+F".into(),
            action: CommandAction::OpenProfileSwitcher,
        },
        Command {
            name: "Switch Rulebook".into(),
            description: "Change the active VIL rulebook".into(),
            shortcut: "".into(),
            action: CommandAction::OpenRulebookSwitcher,
        },
        Command {
            name: "Resume Session".into(),
            description: "View and resume previous sessions".into(),
            shortcut: "".into(),
            action: CommandAction::OpenSessions,
        },
        Command {
            name: "Keyboard Shortcuts".into(),
            description: "View all keyboard shortcuts".into(),
            shortcut: "Ctrl+S".into(),
            action: CommandAction::OpenShortcuts,
        },
        Command {
            name: "Clear Screen".into(),
            description: "Clear the terminal output".into(),
            shortcut: "".into(),
            action: CommandAction::ClearScreen,
        },
        Command {
            name: "Auto-Approve".into(),
            description: "Toggle tool auto-approval".into(),
            shortcut: "Ctrl+O".into(),
            action: CommandAction::ToggleAutoApprove,
        },
        Command {
            name: "Quit".into(),
            description: "Exit the application".into(),
            shortcut: "Ctrl+C".into(),
            action: CommandAction::Quit,
        },
    ]
}

/// Build the full command list from UI-action commands plus slash commands
/// derived from the canonical `vac_commands()` registry.
fn get_all_commands() -> Vec<Command> {
    let mut cmds = ui_action_commands();
    // Derive InsertSlashCommand entries from the single canonical registry so
    // the palette never lists phantom commands or drifts from the dispatcher.
    for spec in crate::services::helper_block::vac_commands() {
        // Skip commands marked as Hidden — they should not appear in operator surfaces.
        if spec.surface == crate::services::commands::CommandSurface::Hidden {
            continue;
        }
        let display_name = spec
            .command
            .strip_prefix('/')
            .map(|s| {
                let mut c = s.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => s.to_string(),
                }
            })
            .unwrap_or_else(|| spec.command.clone());
        let shortcut = spec.shortcut.clone().unwrap_or_default();
        cmds.push(Command {
            name: display_name,
            description: spec.description.clone(),
            shortcut,
            action: CommandAction::InsertSlashCommand(spec.command.clone()),
        });
    }
    cmds
}

pub fn filter_commands(query: &str, state: &crate::app::AppState) -> Vec<Command> {
    let mut cmds = if query.is_empty() {
        get_all_commands()
    } else {
        let query_lower = query.to_lowercase();
        get_all_commands()
            .into_iter()
            .filter(|cmd| {
                cmd.name.to_lowercase().contains(&query_lower)
                    || cmd.description.to_lowercase().contains(&query_lower)
            })
            .collect()
    };

    cmds.sort_by_key(|cmd| {
        let cmd_id = match &cmd.action {
            CommandAction::InsertSlashCommand(s) => s.clone(),
            _ => cmd.name.clone(),
        };
        let freq = state
            .layout
            .command_palette
            .recent_commands
            .frequencies
            .get(&cmd_id)
            .copied()
            .unwrap_or(0);
        let recent_idx = state
            .layout
            .command_palette
            .recent_commands
            .history
            .iter()
            .position(|h| h == &cmd_id)
            .unwrap_or(usize::MAX);
        (std::cmp::Reverse(freq), recent_idx)
    });

    cmds
}
