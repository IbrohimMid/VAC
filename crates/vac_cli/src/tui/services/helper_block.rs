//! Helper Block Service
//!
//! Provides welcome messages and helper UI elements.

use crate::tui::app::{CommandSource, HelperCommand, Message};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Maximum description length when derived from the first line of content.
const MAX_DESCRIPTION_LEN: usize = 60;

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
pub fn welcome_messages(version: Option<&str>, state: &crate::tui::app::AppState) -> Vec<Message> {
    let version_str = version.unwrap_or("unknown");
    let permission_mode = if state.auto_approve {
        "AUTO-APPROVE (Tools will run without confirmation)"
    } else {
        "PROMPT (You will be prompted for tool execution)"
    };

    vec![Message::assistant(format!(
        "{}\n\
            ═══════════════════════════════════════\n\
            Vastar Agentic CLI v{}\n\
            Powered by VIL Engine\n\
            Permission Mode: {}\n\
            ═══════════════════════════════════════\n\n\
            Shortcuts:\n\
            • Ctrl+P - Command palette\n\
            • Ctrl+C - Quit\n\
            • Esc    - Cancel/Close\n\
            • Up/Down - Scroll\n\n\
            Type your message and press Enter to start.",
        VAC_LOGO, version_str, permission_mode
    ))]
}

/// Default VAC commands
pub fn vac_commands() -> Vec<HelperCommand> {
    let mut commands = vec![
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
        // Project/Task Commands
        HelperCommand {
            command: "/review".to_string(),
            description: "Review current changes".to_string(),
            source: CommandSource::BuiltIn,
        },
        HelperCommand {
            command: "/fix".to_string(),
            description: "Fix linter/build errors".to_string(),
            source: CommandSource::BuiltInWithPrompt {
                prompt_content: "Fix linter/build errors in this repo. If needed, run the appropriate checks and apply minimal safe changes.".to_string(),
            },
        },
        HelperCommand {
            command: "/explain".to_string(),
            description: "Explain code or concepts".to_string(),
            source: CommandSource::BuiltInWithPrompt {
                prompt_content: "Explain the relevant code or concept. Ask for the specific file/path and what to explain if unclear.".to_string(),
            },
        },
    ];

    // Load custom commands from .vac/commands/
    commands.extend(load_custom_commands());

    commands
}

/// Load custom commands from `.vac/commands/`
pub fn load_custom_commands() -> Vec<HelperCommand> {
    let mut commands = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();

    let local_dir = PathBuf::from(".vac/commands");
    load_from_directory(&local_dir, &mut commands, &mut seen_names);

    // Global commands
    if let Some(home) = home_dir() {
        let global_dir = home.join(".vac/commands");
        load_from_directory(&global_dir, &mut commands, &mut seen_names);
    }

    commands
}

fn home_dir() -> Option<PathBuf> {
    #[allow(deprecated)]
    std::env::home_dir()
}

fn load_from_directory(
    dir: &Path,
    commands: &mut Vec<HelperCommand>,
    seen_names: &mut HashSet<String>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        if stem.is_empty()
            || !stem
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            continue;
        }

        let command_name = format!("/{stem}");
        if seen_names.contains(&command_name) {
            continue;
        }

        if let Some(helper) = parse_command_file(&path, &command_name) {
            seen_names.insert(command_name);
            commands.push(helper);
        }
    }
}

fn parse_command_file(path: &Path, command_name: &str) -> Option<HelperCommand> {
    let content = std::fs::read_to_string(path).ok()?;
    let content = content.trim();

    if content.is_empty() {
        return None;
    }

    let (description, prompt_content) = extract_front_matter(content);

    let description = description.unwrap_or_else(|| {
        let first_line = prompt_content
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("Custom command");
        let first_line = first_line.trim();
        if first_line.len() > MAX_DESCRIPTION_LEN {
            let truncated: String = first_line.chars().take(MAX_DESCRIPTION_LEN).collect();
            format!("{truncated}...")
        } else {
            first_line.to_string()
        }
    });

    Some(HelperCommand {
        command: command_name.to_string(),
        description,
        source: CommandSource::Custom {
            prompt_content: prompt_content.to_string(),
        },
    })
}

fn extract_front_matter(content: &str) -> (Option<String>, &str) {
    if !content.starts_with("---") {
        return (None, content);
    }

    let after_first = &content[3..];
    let closing = after_first
        .match_indices("---")
        .find(|(pos, _)| {
            *pos == 0 || after_first.as_bytes().get(pos.wrapping_sub(1)) == Some(&b'\n')
        })
        .map(|(pos, _)| pos);

    match closing {
        Some(pos) => {
            let front_matter = after_first[..pos].trim();
            let body = after_first[pos + 3..].trim();

            let description = front_matter.lines().find_map(|line| {
                let line = line.trim();
                if let Some(value) = line.strip_prefix("description:") {
                    let value = value.trim();
                    let value = value
                        .strip_prefix('"')
                        .and_then(|v| v.strip_suffix('"'))
                        .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                        .unwrap_or(value);
                    if !value.is_empty() {
                        Some(value.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

            if body.is_empty() {
                (description, content)
            } else {
                (description, body)
            }
        }
        None => (None, content),
    }
}
