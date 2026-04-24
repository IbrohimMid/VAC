//! Helper Block Service
//!
//! Provides welcome messages and helper UI elements.

use crate::app::{CommandSource, HelperCommand, Message};
use crate::services::commands::CommandSurface;
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
pub fn welcome_messages(version: Option<&str>, state: &crate::app::AppState) -> Vec<Message> {
    let version_str = version.unwrap_or(&state.core.startup.version);
    let permission_mode = if state.core.view_flags.auto_approve {
        "AUTO-APPROVE (tool requests run without confirmation)"
    } else {
        "PROMPT (tool requests require confirmation)"
    };

    // Phase 3: Show model recovery hint if no active model
    let model_hint = if state.core.startup.active_model.is_none() && state.operator_config.operator.current_model.is_none() {
        "\n⚠ No active model configured. Use /model to select one."
    } else {
        ""
    };

    vec![Message::assistant(format!(
        "{}\n\
            ═══════════════════════════════════════\n\
            Vastar Agentic CLI v{}\n\
            Powered by VIL Engine\n\
            Permission Mode: {}\n\
            ═══════════════════════════════════════\n\n\
            Shortcuts:\n\
            • Ctrl+P - Command palette (all /commands, including hook/cron/subagent/fetch/monitor/signal)\n\
            • Ctrl+S - Shortcuts popup (context-aware keybindings)\n\
            • Ctrl+C - Quit\n\
            • Esc    - Cancel/Close\n\
            • /      - Open palette with slash filter\n\n\
            Type your message and press Enter to start.{}",
        VAC_LOGO, version_str, permission_mode, model_hint
    ))]
}

/// Default VAC commands.
///
/// This is the single source of truth consumed by the command palette,
/// shortcuts popup, footer help, and slash-command dispatcher.
pub fn vac_commands() -> Vec<HelperCommand> {
    let mut commands = vec![
        // --- Passthrough (no TUI handler; forwarded to agent as-is) ---
        HelperCommand {
            command: "/vil".to_string(),
            description: "Ask agent about VIL engine status".to_string(),
            source: CommandSource::Passthrough,
            shortcut: None,
            wired: false,
            surface: CommandSurface::Hidden,
        },
        HelperCommand {
            command: "/swarm".to_string(),
            description: "Ask agent about swarm status".to_string(),
            source: CommandSource::Passthrough,
            shortcut: None,
            wired: false,
            surface: CommandSurface::Hidden,
        },
        HelperCommand {
            command: "/context".to_string(),
            description: "Pin files into the context composer".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        HelperCommand {
            command: "/rulebook".to_string(),
            description: "Ask agent to manage rulebooks".to_string(),
            source: CommandSource::Passthrough,
            shortcut: None,
            wired: false,
            surface: CommandSurface::Hidden,
        },
        HelperCommand {
            command: "/resume".to_string(),
            description: "Ask agent to resume from checkpoint".to_string(),
            source: CommandSource::Passthrough,
            shortcut: None,
            wired: false,
            surface: CommandSurface::Hidden,
        },
        HelperCommand {
            command: "/help".to_string(),
            description: "Ask agent for help with available commands".to_string(),
            source: CommandSource::Passthrough,
            shortcut: Some("?".to_string()),
            wired: false,
            surface: CommandSurface::Hidden,
        },
        // --- BuiltIn: wired to TUI handlers ---
        HelperCommand {
            command: "/runtime".to_string(),
            description: "Show runtime inspector".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        HelperCommand {
            command: "/agents".to_string(),
            description: "Show multi-agent queue".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        HelperCommand {
            command: "/shell".to_string(),
            description: "Run interactive shell command".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        HelperCommand {
            command: "/shell-focus".to_string(),
            description: "Refocus background shell".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        HelperCommand {
            command: "/shell-bg".to_string(),
            description: "Background active shell".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        HelperCommand {
            command: "/shell-kill".to_string(),
            description: "Terminate active shell".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        // --- Session commands ---
        HelperCommand {
            command: "/clear".to_string(),
            description: "Clear conversation".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        HelperCommand {
            command: "/new".to_string(),
            description: "Start new session".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        HelperCommand {
            command: "/sessions".to_string(),
            description: "List sessions".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        HelperCommand {
            command: "/export".to_string(),
            description: "Export bundle JSON (redacted): /export [path]".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        HelperCommand {
            command: "/import".to_string(),
            description: "Import bundle JSON: /import <path>".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        // --- Project / Task commands ---
        HelperCommand {
            command: "/review".to_string(),
            description: "Review current changes".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        HelperCommand {
            command: "/model".to_string(),
            description: "Switch active model".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        HelperCommand {
            command: "/files".to_string(),
            description: "Search files in workspace".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        HelperCommand {
            command: "/changes".to_string(),
            description: "Review current changeset".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        HelperCommand {
            command: "/file-changes".to_string(),
            description: "Searchable popup of changed files".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        HelperCommand {
            command: "/plan".to_string(),
            description: "Open or create the session plan".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        HelperCommand {
            command: "/plan-review".to_string(),
            description: "Open the plan review overlay".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        HelperCommand {
            command: "/plan-edit".to_string(),
            description: "Edit plan.md in $EDITOR".to_string(),
            source: CommandSource::BuiltIn,
            shortcut: None,
            wired: true,
            surface: CommandSurface::OperatorAction,
        },
        // --- BuiltInWithPrompt: sends canned prompt to agent ---
        HelperCommand {
            command: "/fix".to_string(),
            description: "Fix linter/build errors".to_string(),
            source: CommandSource::BuiltInWithPrompt {
                prompt_content: "Fix linter/build errors in this repo. If needed, run the appropriate checks and apply minimal safe changes.".to_string(),
            },
            shortcut: None,
            wired: true,
            surface: CommandSurface::Template,
        },
        HelperCommand {
            command: "/explain".to_string(),
            description: "Explain code or concepts".to_string(),
            source: CommandSource::BuiltInWithPrompt {
                prompt_content: "Explain the relevant code or concept. Ask for the specific file/path and what to explain if unclear.".to_string(),
            },
            shortcut: None,
            wired: true,
            surface: CommandSurface::Template,
        },

        // --- UX-unification patch C — arc feature discoverability.
        //
        // These surface hook/cron/subagent/fetch/monitor/signal as
        // first-class palette entries (Ctrl+P → /) so operators
        // don't have to read source or ask the LLM "what can you
        // do?". They dispatch canned prompts that ask the agent
        // to invoke the corresponding tool with operator-supplied
        // parameters. See docs/adr/ADR-001 for the hybrid boundary.
        HelperCommand {
            command: "/hook-create".to_string(),
            description: "Register a PreToolUse hook (sandboxed command) via the agent".to_string(),
            source: CommandSource::BuiltInWithPrompt {
                prompt_content: "Use the `hook_create` tool to register a new hook. Ask the operator for: (1) event — one of PreToolUse/PostToolUse/UserPromptSubmit/Stop/SubagentStop/Notification/SessionStart/SessionEnd/PreCompact; (2) matcher — regex against tool name, empty means match all; (3) argv — the shell command to run. Kind is always `command` in VAC v1 (prompt/agent/http are reserved; see HookSandbox docs). After creation, confirm the registration + sandbox policy.".to_string(),
            },
            shortcut: None,
            wired: true,
            surface: CommandSurface::Template,
        },
        HelperCommand {
            command: "/hook-list".to_string(),
            description: "List registered hooks in .vac/hooks.json".to_string(),
            source: CommandSource::BuiltInWithPrompt {
                prompt_content: "Call the `hook_list` tool and render each entry as: id, event, matcher, kind, description.".to_string(),
            },
            shortcut: None,
            wired: true,
            surface: CommandSurface::Template,
        },
        HelperCommand {
            command: "/cron-add".to_string(),
            description: "Schedule a recurring agent task (cron) via the agent".to_string(),
            source: CommandSource::BuiltInWithPrompt {
                prompt_content: "Use the `schedule_cron` tool to register a recurring task. Ask the operator for: (1) id — unique identifier; (2) cron expression — 5-field minute-hour-dom-month-dow; (3) task — prompt the agent runs when the schedule fires. After creation, remind the operator that the autopilot daemon must be running (`vac autopilot up --execute`) for schedules to actually fire.".to_string(),
            },
            shortcut: None,
            wired: true,
            surface: CommandSurface::Template,
        },
        HelperCommand {
            command: "/cron-list".to_string(),
            description: "List cron-scheduled tasks".to_string(),
            source: CommandSource::BuiltInWithPrompt {
                prompt_content: "Call the `cron_list` tool and render each entry: id, schedule, prompt, last_fire_unix, fire_count.".to_string(),
            },
            shortcut: None,
            wired: true,
            surface: CommandSurface::Template,
        },
        HelperCommand {
            command: "/subagent-run".to_string(),
            description: "Dispatch a first-level subagent (explore/plan/verify/general/statusline)".to_string(),
            source: CommandSource::BuiltInWithPrompt {
                prompt_content: "Use the `agent_run` tool to dispatch a subagent. Ask the operator which kind (explore / plan / verify / general-purpose / statusline-setup) and what the subagent should do. Compose a focused prompt. Note: VAC v1 supports first-level delegation only; the subagent itself cannot call agent_run (see ADR-002).".to_string(),
            },
            shortcut: None,
            wired: true,
            surface: CommandSurface::Template,
        },
        HelperCommand {
            command: "/fetch".to_string(),
            description: "Fetch a URL via the agent (Authorization opt-in)".to_string(),
            source: CommandSource::BuiltInWithPrompt {
                prompt_content: "Use the `web_fetch` tool to issue an HTTP request. Ask for URL, method (default GET), any body/headers needed. Authorization header is stripped by default; only forward it when the operator explicitly requests (`allow_authorization: true`) since that elevates the call to destructive.".to_string(),
            },
            shortcut: None,
            wired: true,
            surface: CommandSurface::Template,
        },
        HelperCommand {
            command: "/monitor".to_string(),
            description: "Run a command and filter stdout lines via the agent".to_string(),
            source: CommandSource::BuiltInWithPrompt {
                prompt_content: "Use the `monitor` tool. Ask the operator for: argv (program + args), match_regex, and optional max_lines / timeout_secs. Report the collected matching lines.".to_string(),
            },
            shortcut: None,
            wired: true,
            surface: CommandSurface::Template,
        },
        HelperCommand {
            command: "/signal".to_string(),
            description: "Distill scored key-lines from a session signal buffer".to_string(),
            source: CommandSource::BuiltInWithPrompt {
                prompt_content: "Use the `signal_distilled` tool to pull scored key lines + tail from the named stream (e.g. `shell`, `vil_dev`, `build-*`). If the operator did not name a stream, call `signal_list` first to discover available ids.".to_string(),
            },
            shortcut: None,
            wired: true,
            surface: CommandSurface::Template,
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
        shortcut: None,
        wired: false,
        surface: CommandSurface::Template,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// UX-unification patch C drift guard — the arc feature
    /// entries that make hook/cron/subagent/fetch/monitor/signal
    /// discoverable from the command palette must stay wired.
    /// If a future edit removes them, the palette goes back to
    /// "agent-only discovery" for these features, which is
    /// exactly what the reviewer flagged as the UX gap.
    #[test]
    fn arc_feature_palette_entries_are_present() {
        let cmds = vac_commands();
        let arc_aliases = [
            "/hook-create",
            "/hook-list",
            "/cron-add",
            "/cron-list",
            "/subagent-run",
            "/fetch",
            "/monitor",
            "/signal",
        ];
        for alias in arc_aliases {
            assert!(
                cmds.iter().any(|c| c.command == alias),
                "palette must expose {alias} (patch C). Full list: {:?}",
                cmds.iter().map(|c| &c.command).collect::<Vec<_>>(),
            );
        }
    }

    /// Arc entries are prompt templates — they send a canned
    /// prompt to the agent, not a one-shot action. Verify source
    /// kind so a future edit doesn't accidentally convert them
    /// to `Passthrough` (which would send the raw `/hook-create`
    /// text and lose the tool-invocation instructions).
    #[test]
    fn arc_feature_entries_are_builtin_with_prompt() {
        let cmds = vac_commands();
        for alias in ["/hook-create", "/cron-add", "/subagent-run", "/fetch", "/monitor", "/signal"] {
            let entry = cmds.iter().find(|c| c.command == alias).unwrap();
            match &entry.source {
                CommandSource::BuiltInWithPrompt { prompt_content } => {
                    assert!(
                        !prompt_content.is_empty(),
                        "{alias} must carry a non-empty prompt_content",
                    );
                }
                other => panic!(
                    "{alias} should be BuiltInWithPrompt so it dispatches a templated \
                     message to the agent; got {:?}",
                    std::mem::discriminant(other),
                ),
            }
        }
    }
}
