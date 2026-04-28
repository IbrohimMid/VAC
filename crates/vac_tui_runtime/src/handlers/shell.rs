//! Shell-popup input handler.
//!
//! Handles keyboard events when the interactive shell popup is visible or
//! when shell navigation keys are pressed. Returns `true` when the event
//! was consumed by the shell layer.

use crate::app::{AppState, InputEvent, OutputEvent};
use tokio::sync::mpsc::Sender;

/// Handle a key event routed to the shell popup.
/// Returns `true` if the event was consumed.
pub fn handle_shell_key(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    event: &InputEvent,
) -> bool {
    if !state.execution.shell.session_store.popup_visible {
        return false;
    }

    let has_active_command = state
        .execution
        .shell
        .session_store
        .active()
        .and_then(|session| session.command.as_ref())
        .is_some();
    if !has_active_command {
        return false;
    }

    match event {
        InputEvent::InputSubmitted => {
            let text = state.composer.input.get_content().to_string();
            let payload = if text.is_empty() {
                "\n".to_string()
            } else {
                format!("{text}\n")
            };

            let mut command_to_send: Option<vac_shell::ShellCommand> = None;
            if let Some(session) = state.execution.shell.session_store.active_mut() {
                if !text.is_empty() && session.history.last() != Some(&text) {
                    session.history.push(text.clone());
                }
                session.history_idx = None;
                session.waiting_for_input = false;
                command_to_send = session.command.clone();
            }

            if let Some(shell) = command_to_send {
                shell.send_input(payload);
                state.push_activity(crate::app::ActivityKind::Status, "Sent input to shell");
            }
            state.composer.input.clear();
            true
        }
        InputEvent::Up | InputEvent::ScrollUp => {
            if let Some(session) = state.execution.shell.session_store.active_mut()
                && !session.history.is_empty()
            {
                let max_idx = session.history.len() - 1;
                let next_idx = match session.history_idx {
                    Some(i) => i.saturating_sub(1),
                    None => max_idx,
                };
                session.history_idx = Some(next_idx);
                if let Some(cmd) = session.history.get(next_idx) {
                    state.composer.input.set_content(cmd);
                    state.layout.scroll.cursor_position = state.composer.input.get_content().len();
                }
                return true;
            }
            false
        }
        InputEvent::Down | InputEvent::ScrollDown => {
            if let Some(session) = state.execution.shell.session_store.active_mut()
                && let Some(idx) = session.history_idx
            {
                let next_idx = idx + 1;
                if next_idx >= session.history.len() {
                    session.history_idx = None;
                    state.composer.input.clear();
                    state.layout.scroll.cursor_position = 0;
                } else {
                    session.history_idx = Some(next_idx);
                    if let Some(cmd) = session.history.get(next_idx) {
                        state.composer.input.set_content(cmd);
                        state.layout.scroll.cursor_position =
                            state.composer.input.get_content().len();
                    }
                }
                return true;
            }
            false
        }
        _ => {
            let _ = output_tx;
            false
        }
    }
}

/// Background the active shell (keep running, hide popup).
pub fn background(state: &mut AppState) {
    if let Some(session) = state.execution.shell.session_store.active_mut()
        && session.command.is_some()
    {
        session.backgrounded = true;
        state.execution.shell.session_store.popup_visible = false;
    }
}

/// Bring the backgrounded shell back into the foreground.
pub fn foreground(state: &mut AppState) {
    if let Some(session) = state.execution.shell.session_store.active_mut()
        && session.command.is_some()
    {
        session.backgrounded = false;
        state.execution.shell.session_store.popup_visible = true;
    }
}

/// Kill the active shell.
pub fn kill(state: &mut AppState) {
    if let Some(shell) = state
        .execution
        .shell
        .session_store
        .active()
        .and_then(|session| -> Option<vac_shell::ShellCommand> { session.command.clone() })
    {
        let _ = shell.kill();
    }
}

/// Reset only the active shell session.
pub fn reset(state: &mut AppState) {
    if let Some(session) = state.execution.shell.session_store.active_mut() {
        session.command = None;
        session.output.clear();
        session.output_signal.clear();
        session.history.clear();
        session.history_idx = None;
        session.waiting_for_input = false;
        session.backgrounded = false;
        session.exit_code = None;
        session.last_error = None;
        session.prompt_ready = false;
        session.password_mode = false;
        session.lifecycle = vac_shell::ShellLifecycle::Running;
    }
    state.execution.shell.session_store.popup_visible = false;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::app::AppStateOptions;
    use std::path::PathBuf;

    fn make_state() -> AppState {
        AppState::new(AppStateOptions {
            model: None,
            session_id: None,
            checkpoint_path: None,
            project_root: PathBuf::from("."),
        })
    }

    fn dummy_command(label: &str) -> vac_shell::ShellCommand {
        let (stdin_tx, _stdin_rx) = tokio::sync::mpsc::channel(4);
        vac_shell::ShellCommand {
            id: format!("cmd-{label}"),
            command: label.to_string(),
            stdin_tx,
        }
    }

    #[test]
    fn active_session_switch_isolates_output() {
        let mut state = make_state();
        let first = state
            .execution
            .shell
            .session_store
            .push_new("shell-1".to_string());
        state.execution.shell.session_store.sessions[first].output = "first".to_string();
        let second = state
            .execution
            .shell
            .session_store
            .push_new("shell-2".to_string());
        state.execution.shell.session_store.sessions[second].output = "second".to_string();

        state.execution.shell.session_store.switch_to(first);
        assert_eq!(
            state
                .execution
                .shell
                .session_store
                .active()
                .map(|session| session.output.as_str()),
            Some("first")
        );

        state.execution.shell.session_store.switch_to(second);
        assert_eq!(
            state
                .execution
                .shell
                .session_store
                .active()
                .map(|session| session.output.as_str()),
            Some("second")
        );
    }

    #[test]
    fn history_isolated_per_session() {
        let mut state = make_state();
        let first = state
            .execution
            .shell
            .session_store
            .push_new("shell-1".to_string());
        state.execution.shell.session_store.sessions[first].history =
            vec!["cargo check".to_string()];
        let second = state
            .execution
            .shell
            .session_store
            .push_new("shell-2".to_string());
        state.execution.shell.session_store.sessions[second].history = vec!["npm test".to_string()];

        state.execution.shell.session_store.switch_to(first);
        assert_eq!(
            state
                .execution
                .shell
                .session_store
                .active()
                .map(|session| session.history.clone()),
            Some(vec!["cargo check".to_string()])
        );

        state.execution.shell.session_store.switch_to(second);
        assert_eq!(
            state
                .execution
                .shell
                .session_store
                .active()
                .map(|session| session.history.clone()),
            Some(vec!["npm test".to_string()])
        );
    }

    #[test]
    fn background_foreground_lifecycle() {
        let mut state = make_state();
        let idx = state
            .execution
            .shell
            .session_store
            .push_new("shell-1".to_string());
        state.execution.shell.session_store.sessions[idx].command = Some(dummy_command("shell-1"));
        state.execution.shell.session_store.popup_visible = true;

        background(&mut state);
        assert!(!state.execution.shell.session_store.popup_visible);
        assert!(state.execution.shell.session_store.sessions[idx].backgrounded);

        foreground(&mut state);
        assert!(state.execution.shell.session_store.popup_visible);
        assert!(!state.execution.shell.session_store.sessions[idx].backgrounded);
    }

    #[test]
    fn reset_session_does_not_affect_other_sessions() {
        let mut state = make_state();
        let first = state
            .execution
            .shell
            .session_store
            .push_new("shell-1".to_string());
        state.execution.shell.session_store.sessions[first].output = "keep me".to_string();
        state.execution.shell.session_store.sessions[first].history =
            vec!["cargo test".to_string()];

        let second = state
            .execution
            .shell
            .session_store
            .push_new("shell-2".to_string());
        state.execution.shell.session_store.sessions[second].command =
            Some(dummy_command("shell-2"));
        state.execution.shell.session_store.sessions[second].output = "clear me".to_string();
        state.execution.shell.session_store.sessions[second].history =
            vec!["npm run dev".to_string()];
        state.execution.shell.session_store.switch_to(second);
        state.execution.shell.session_store.popup_visible = true;

        reset(&mut state);

        assert_eq!(
            state.execution.shell.session_store.sessions[first].output,
            "keep me"
        );
        assert_eq!(
            state.execution.shell.session_store.sessions[first].history,
            vec!["cargo test".to_string()]
        );
        assert!(
            state.execution.shell.session_store.sessions[second]
                .output
                .is_empty()
        );
        assert!(
            state.execution.shell.session_store.sessions[second]
                .history
                .is_empty()
        );
        assert!(
            state.execution.shell.session_store.sessions[second]
                .command
                .is_none()
        );
        assert!(!state.execution.shell.session_store.popup_visible);
    }
}
