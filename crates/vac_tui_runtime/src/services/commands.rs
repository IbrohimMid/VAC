use crate::app::AppState;
use crate::app::OutputEvent;
use tokio::sync::mpsc::Sender;

pub type CommandId<'a> = &'a str;

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
    ToggleCollapsedMessages,
    ClearScreen,
    ToggleAutoApprove,
    Quit,
    InsertSlashCommand(String),
}

pub struct CommandContext<'a> {
    pub state: &'a mut AppState,
    pub output_tx: &'a Sender<OutputEvent>,
}
