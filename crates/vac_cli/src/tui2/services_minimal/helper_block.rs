//! Helper Block Service
//!
//! Provides welcome messages and helper UI elements.

use crate::tui2::app::{HelperCommand, CommandSource, Message};

/// VAC ASCII logo
const VAC_LOGO: &str = r#"
 ██╗   ██╗ █████╗  ██████╗ ██╗   ██╗
 ██║   ██║██╔══██╗██╔════╝ ██║   ██║
 ██║   ██║███████║██║  ███╗███████║
 ╚██╗ ██╔╝██╔══██║██║   ██║██╔══██║
  ╚████╔╝ ██║  ██║╚██████╔╝██║  ██║
   ╚═══╝  ╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝
"#;

/// Generate welcome messages for TUI
pub fn welcome_messages(
    version: Option<&str>,
    _state: &crate::tui2::app::AppState,
) -> Vec<Message> {
    let version_str = version.unwrap_or("unknown");
    vec![
        Message::assistant(format!(
            "{}\n\
            ═══════════════════════════════════════\n\
            Vastar Agentic CLI v{}\n\
            Powered by VIL Engine\n\
            ═══════════════════════════════════════\n\n\
            Shortcuts:\n\
            • Ctrl+P - Command palette\n\
            • Ctrl+C - Quit\n\
            • Esc    - Cancel/Close\n\
            • Up/Down - Scroll\n\n\
            Type your message and press Enter to start.",
            VAC_LOGO, version_str
        )),
    ]
}

/// Default VAC commands
pub fn vac_commands() -> Vec<HelperCommand> {
    vec![
        // VIL-specific commands
        HelperCommand {
            command: "/vil".to_string(),
            description: "Show VIL engine status".to_string(),
            source: CommandSource::BuiltIn,
        },
        HelperCommand {
            command: "/swarm".to_string(),
            description: "Show swarm status".to_string(),
            source: CommandSource::BuiltIn,
        },
        HelperCommand {
            command: "/rulebook".to_string(),
            description: "Manage rulebooks".to_string(),
            source: CommandSource::BuiltIn,
        },
        HelperCommand {
            command: "/context".to_string(),
            description: "Show context budget".to_string(),
            source: CommandSource::BuiltIn,
        },
        HelperCommand {
            command: "/runtime".to_string(),
            description: "Show runtime status".to_string(),
            source: CommandSource::BuiltIn,
        },
        // Session commands
        HelperCommand {
            command: "/clear".to_string(),
            description: "Clear conversation".to_string(),
            source: CommandSource::BuiltIn,
        },
        HelperCommand {
            command: "/new".to_string(),
            description: "Start new session".to_string(),
            source: CommandSource::BuiltIn,
        },
        HelperCommand {
            command: "/sessions".to_string(),
            description: "List sessions".to_string(),
            source: CommandSource::BuiltIn,
        },
        HelperCommand {
            command: "/resume".to_string(),
            description: "Resume from checkpoint".to_string(),
            source: CommandSource::BuiltIn,
        },
        // Help
        HelperCommand {
            command: "/help".to_string(),
            description: "Show available commands".to_string(),
            source: CommandSource::BuiltIn,
        },
    ]
}