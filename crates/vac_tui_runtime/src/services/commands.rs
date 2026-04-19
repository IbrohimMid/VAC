pub type CommandId<'a> = &'a str;

/// Describes where/how a command is visible in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandSurface {
    /// Visible as an operator-grade action (command palette, shortcuts popup, slash input).
    OperatorAction,
    /// Sent verbatim to the agent as a prompt directive.
    AgentPrompt,
    /// A prompt template with user-provided arguments.
    Template,
    /// Internal-only — not shown in any operator surface.
    Hidden,
}

#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,
    pub description: String,
    pub shortcut: String,
    pub action: CommandAction,
}

#[derive(Debug, Clone)]
pub enum CommandAction {
    OpenProfileSwitcher,
    OpenRulebookSwitcher,
    OpenSessions,
    OpenShortcuts,
    ClearScreen,
    ToggleAutoApprove,
    Quit,
    InsertSlashCommand(String),
}
