import re

with open("crates/vac_cli/src/tui/services/shortcuts_popup.rs", "r") as f:
    content = f.read()

# We want to:
# 1. Remove `Command` and `CommandAction` definitions.
# 2. Add `use crate::tui::services::commands::{Command, CommandAction};`
# 3. Replace the hardcoded `Commands` category in `get_all_shortcuts()` with dynamic ones from `vac_commands()`.

content = re.sub(
    r"pub struct Command \{.*?\n\}",
    "",
    content,
    flags=re.DOTALL
)

content = re.sub(
    r"pub enum CommandAction \{.*?\n\}",
    "",
    content,
    flags=re.DOTALL
)

content = content.replace(
    "use crate::tui::constants::SCROLL_BUFFER_LINES;",
    "use crate::tui::constants::SCROLL_BUFFER_LINES;\nuse crate::tui::services::commands::{Command, CommandAction};"
)

# For get_all_shortcuts(), we want to dynamically generate the Commands category.
# Actually, the task says: "Reklasifikasi command lama dan hilangkan phantom commands".
# So removing the phantom ones from the static list is also fine, or dynamically generating them.
# Let's dynamically generate them.
old_shortcuts = """        // Commands
        Shortcut::new("/help", "Show help information", "Commands"),
        Shortcut::new("/clear", "Clear screen", "Commands"),
        Shortcut::new("/status", "Show account status", "Commands"),
        Shortcut::new("/sessions", "List available sessions", "Commands"),
        Shortcut::new("/resume", "Resume last session", "Commands"),
        Shortcut::new("/export", "Export bundle JSON (redacted)", "Commands"),
        Shortcut::new("/import", "Import bundle JSON", "Commands"),
        Shortcut::new("/memorize", "Memorize conversation", "Commands"),
        Shortcut::new("/model", "Switch model", "Commands"),
        Shortcut::new(
            "/summarize",
            "Summarize session into summary.md",
            "Commands",
        ),
        Shortcut::new("/usage", "Show token usage for this session", "Commands"),
        Shortcut::new(
            "/list_approved_tools",
            "List auto-approved tools",
            "Commands",
        ),
        Shortcut::new(
            "/toggle_auto_approve",
            "Toggle auto-approve for tool",
            "Commands",
        ),
        Shortcut::new("/mouse_capture", "Toggle mouse capture", "Commands"),
        Shortcut::new("/profiles", "Switch profile", "Commands"),
        Shortcut::new("/quit", "Quit application", "Commands"),"""

new_shortcuts = """        // Commands are injected dynamically"""
content = content.replace(old_shortcuts, new_shortcuts)

# We need to modify `get_all_shortcuts()` to add commands from `vac_commands()`
replacement = """pub fn get_all_shortcuts() -> Vec<Shortcut> {
    let mut shortcuts = vec![
        // Navigation
        Shortcut::new("↑/↓", "Navigate messages", "Navigation"),
        Shortcut::new("Page Up/Down", "Page through messages", "Navigation"),
        Shortcut::new("Ctrl+↑/↓", "Navigate dropdown/dialog", "Navigation"),
        Shortcut::new("Tab", "Complete command or select file", "Navigation"),
        Shortcut::new("Esc", "Close dialogs/popups", "Navigation"),
        // Text Input
        Shortcut::new("Ctrl+A", "Move cursor to start of line", "Text Input"),
        Shortcut::new("Ctrl+E", "Move cursor to end of line", "Text Input"),
        Shortcut::new("Ctrl+F", "Move cursor right", "Text Input"),
        Shortcut::new("Ctrl+B", "Move cursor left", "Text Input"),
        Shortcut::new("Alt+F", "Move cursor to next word", "Text Input"),
        Shortcut::new("Alt+B", "Move cursor to previous word", "Text Input"),
        Shortcut::new("Ctrl+U", "Delete to start of line", "Text Input"),
        Shortcut::new("Ctrl+W", "Delete previous word", "Text Input"),
        Shortcut::new("Ctrl+H", "Delete previous character", "Text Input"),
        Shortcut::new("Ctrl+J", "Insert newline", "Text Input"),
        Shortcut::new("Enter", "Submit input", "Text Input"),
        Shortcut::new("Backspace", "Delete previous character", "Text Input"),
        // Tool Management
        Shortcut::new("Ctrl+O", "Toggle auto-approve mode", "Tool Management"),
        Shortcut::new("Ctrl+Y", "Toggle side panel", "Tool Management"),
        Shortcut::new("Ctrl+R", "Retry last tool call", "Tool Management"),
        // UI Controls
        Shortcut::new("Ctrl+C", "Quit (double press)", "UI Controls"),
        Shortcut::new("Ctrl+T", "Toggle collapsed messages", "UI Controls"),
        Shortcut::new("Ctrl+L", "Toggle mouse capture", "UI Controls"),
        Shortcut::new("Ctrl+F", "Show profile switcher", "UI Controls"),
        Shortcut::new("Ctrl+P", "Show command palette", "UI Controls"),
        Shortcut::new("Ctrl+S", "Show shortcuts (this popup)", "UI Controls"),
        Shortcut::new("Ctrl+G", "Show file changes", "UI Controls"),
        Shortcut::new("Ctrl+X", "Copy session ID", "UI Controls"),
        // File Search
        Shortcut::new("@", "Trigger file search", "File Search"),
        Shortcut::new("$", "Enter interactive shell mode", "File Search"),
        Shortcut::new("Tab", "Select file from search", "File Search"),
        // Mouse
        Shortcut::new("Scroll Up/Down", "Scroll messages", "Mouse"),
        Shortcut::new("Click", "Interact with UI elements", "Mouse"),
    ];

    for spec in crate::tui::services::helper_block::vac_commands() {
        shortcuts.push(Shortcut::new(&spec.command, &spec.description, "Commands"));
    }

    shortcuts
}"""

content = re.sub(
    r"pub fn get_all_shortcuts\(\) -> Vec<Shortcut> \{.*?\n\}",
    replacement,
    content,
    flags=re.DOTALL
)

with open("crates/vac_cli/src/tui/services/shortcuts_popup.rs", "w") as f:
    f.write(content)

