//! Shell-popup input handler.
//!
//! Handles keyboard events when the interactive shell popup is visible or
//! when shell navigation keys are pressed.  Returns `true` when the event
//! was consumed by the shell layer.

use crate::tui::app::{AppState, InputEvent, OutputEvent};
use tokio::sync::mpsc::Sender;

/// Handle a key event routed to the shell popup.
/// Returns `true` if the event was consumed.
pub fn handle_shell_key(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    event: &InputEvent,
) -> bool {
    // Shell popup only captures events when visible with an active command.
    if !state.shell.popup_visible || state.shell.active_command.is_none() {
        return false;
    }

    match event {
        InputEvent::InputSubmitted => {
            let text = state.input.get_content().to_string();
            if state.shell.history.last() != Some(&text) {
                state.shell.history.push(text.clone());
            }
            state.shell.history_idx = None;
            let payload = format!("{}\n", text);
            if let Some(shell) = state.shell.active_command.clone() {
                shell.send_input(payload);
                state.shell.waiting_for_input = false;
                state.push_activity(crate::tui::app::ActivityKind::Status, "Sent input to shell");
            }
            state.input.clear();
            return true;
        }
        InputEvent::Up | InputEvent::ScrollUp => {
            if !state.shell.history.is_empty() {
                let max_idx = state.shell.history.len() - 1;
                let next_idx = match state.shell.history_idx {
                    Some(i) => i.saturating_sub(1),
                    None => max_idx,
                };
                state.shell.history_idx = Some(next_idx);
                if let Some(cmd) = state.shell.history.get(next_idx) {
                    state.input.set_content(cmd);
                    state.cursor_position = state.input.get_content().len();
                }
                return true;
            }
        }
        InputEvent::Down | InputEvent::ScrollDown => {
            if let Some(idx) = state.shell.history_idx {
                let next_idx = idx + 1;
                if next_idx >= state.shell.history.len() {
                    state.shell.history_idx = None;
                    state.input.clear();
                    state.cursor_position = 0;
                } else {
                    state.shell.history_idx = Some(next_idx);
                    if let Some(cmd) = state.shell.history.get(next_idx) {
                        state.input.set_content(cmd);
                        state.cursor_position = state.input.get_content().len();
                    }
                }
                return true;
            }
        }
        _ => {}
    }

    // Unhandled — let it fall through
    let _ = output_tx; // suppress unused warning
    false
}

/// Background the active shell (keep running, hide popup).
pub fn background(state: &mut AppState) {
    if state.shell.active_command.is_some() {
        state.shell.popup_visible = false;
        state.shell.backgrounded = true;
    }
}

/// Bring the backgrounded shell back into the foreground.
pub fn foreground(state: &mut AppState) {
    if state.shell.active_command.is_some() {
        state.shell.popup_visible = true;
        state.shell.backgrounded = false;
    }
}

/// Kill the active shell.
pub fn kill(state: &mut AppState) {
    if let Some(shell) = state.shell.active_command.clone() {
        let _ = shell.kill();
    }
}

/// Reset all shell state (called when a shell session ends).
pub fn reset(state: &mut AppState) {
    state.shell.active_command = None;
    state.shell.popup_visible = false;
    state.shell.waiting_for_input = false;
    state.shell.backgrounded = false;
    state.shell.exit_code = None;
    state.shell.last_error = None;
    state.shell.output.clear();
}
