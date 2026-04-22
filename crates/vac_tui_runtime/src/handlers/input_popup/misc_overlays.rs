//! Miscellaneous overlay handlers: file changes, helper dropdown, command palette,
//! shortcuts, task tray, theme picker.

use crate::app::{AppState, InputEvent, OutputEvent};
use crate::handlers::HandlerContext;
use crate::handlers::review as review_handler;
use crate::overlay::OverlayId;
use tokio::sync::mpsc::Sender;

/// Handle file changes overlay (select modified files).
pub fn handle_file_changes(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    event: InputEvent,
) {
    let filtered = crate::services::file_changes_popup::filtered_paths(state);
    match event {
        InputEvent::HandleEsc => {
            crate::overlay::close_overlay(state, OverlayId::FileChanges);
        }
        InputEvent::Up | InputEvent::ScrollUp => {
            state.file_changes_selected = state.file_changes_selected.saturating_sub(1);
            if state.file_changes_selected < state.file_changes_scroll {
                state.file_changes_scroll = state.file_changes_selected;
            }
        }
        InputEvent::Down | InputEvent::ScrollDown => {
            if !filtered.is_empty() {
                let max_idx = filtered.len().saturating_sub(1);
                if state.file_changes_selected < max_idx {
                    state.file_changes_selected += 1;
                }
            }
        }
        InputEvent::InputChanged(c) => {
            state.file_changes_search.push(c);
            state.file_changes_selected = 0;
            state.file_changes_scroll = 0;
        }
        InputEvent::InputBackspace => {
            state.file_changes_search.pop();
            state.file_changes_selected = 0;
            state.file_changes_scroll = 0;
        }
        InputEvent::ReviewRevertSelected => {
            if let Some(path) = filtered.get(state.file_changes_selected).cloned() {
                let prior = state.review.selected_path.clone();
                state.review.selected_path = Some(path);
                let mut ctx = HandlerContext::new(state, output_tx);
                let _ = review_handler::revert_selected(&mut ctx);
                ctx.state.review.selected_path = prior;
            }
        }
        InputEvent::InputSubmitted => {
            if let Some(path) = filtered.get(state.file_changes_selected).cloned() {
                state.review.selected_path = Some(path);
                state.workbench_tab = crate::app::WorkbenchTab::Review;
                state.focus = crate::app::WorkspaceFocus::Workbench;
                crate::overlay::close_overlay(state, OverlayId::FileChanges);
            }
        }
        _ => {}
    }
}

/// Handle helper dropdown (inline helper command suggestions).
pub fn handle_helper_dropdown(state: &mut AppState, event: InputEvent) {
    if state.focus != crate::app::WorkspaceFocus::Input {
        return;
    }
    match event {
        InputEvent::Up => {
            if state.helper_selected > 0 {
                state.helper_selected -= 1;
                if state.helper_selected < state.helper_scroll {
                    state.helper_scroll = state.helper_selected;
                }
            }
        }
        InputEvent::Down => {
            if !state.filtered_helpers.is_empty() {
                let max_idx = state.filtered_helpers.len().saturating_sub(1);
                if state.helper_selected < max_idx {
                    state.helper_selected += 1;
                    if state.helper_selected >= state.helper_scroll + 5 {
                        state.helper_scroll = state.helper_selected.saturating_sub(4);
                    }
                }
            }
        }
        InputEvent::InputSubmitted => {
            if let Some(cmd) = state.filtered_helpers.get(state.helper_selected).cloned() {
                state.input.set_content(&cmd.command);
                state.input.move_cursor_end();
                state.input.input(' ');
            }
            crate::overlay::close_overlay(state, OverlayId::HelperDropdown);
        }
        InputEvent::HandleEsc => {
            crate::overlay::close_overlay(state, OverlayId::HelperDropdown);
        }
        _ => {}
    }
}

/// Handle command palette (quick command execution).
pub fn handle_command_palette(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    event: InputEvent,
) {
    use crate::handlers::input_commands::dispatch_builtin_command;

    match event {
        InputEvent::HandleEsc | InputEvent::HideCommandPalette => {
            crate::overlay::close_overlay(state, OverlayId::CommandPalette);
        }
        InputEvent::CommandPaletteInput(c) => {
            state.command_palette_input.push(c);
            state.command_palette_selected = 0;
        }
        InputEvent::CommandPaletteBackspace => {
            state.command_palette_input.pop();
            state.command_palette_selected = 0;
        }
        InputEvent::CommandPaletteUp => {
            let filtered = state.filtered_commands();
            if state.command_palette_selected > 0 {
                state.command_palette_selected -= 1;
            } else if !filtered.is_empty() {
                state.command_palette_selected = filtered.len() - 1;
            }
        }
        InputEvent::CommandPaletteDown => {
            let filtered = state.filtered_commands();
            if state.command_palette_selected < filtered.len().saturating_sub(1) {
                state.command_palette_selected += 1;
            } else {
                state.command_palette_selected = 0;
            }
        }
        InputEvent::CommandPaletteSelect => {
            let filtered = state.filtered_commands();
            if let Some(cmd) = filtered.get(state.command_palette_selected).cloned() {
                dispatch_builtin_command(state, output_tx, &cmd.command, None);
            }
            crate::overlay::close_overlay(state, OverlayId::CommandPalette);
        }
        _ => {}
    }
}

/// Handle shortcuts overlay (keyboard shortcuts help + session switcher).
pub fn handle_shortcuts(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    use crate::handlers::input_commands::execute_shortcuts_command;

    match event {
        InputEvent::HandleEsc | InputEvent::HideShortcuts => {
            crate::overlay::close_overlay(state, OverlayId::Shortcuts);
        }
        InputEvent::Tab => {
            state.shortcuts_mode = match state.shortcuts_mode {
                crate::app::ShortcutsPopupMode::Commands => {
                    crate::app::ShortcutsPopupMode::Shortcuts
                }
                crate::app::ShortcutsPopupMode::Shortcuts => {
                    crate::app::ShortcutsPopupMode::Sessions
                }
                crate::app::ShortcutsPopupMode::Sessions => {
                    crate::app::ShortcutsPopupMode::Commands
                }
            };
            state.shortcuts_scroll = 0;
        }
        InputEvent::Up => {
            state.shortcuts_scroll = state.shortcuts_scroll.saturating_sub(1);
        }
        InputEvent::Down => {
            let max = match state.shortcuts_mode {
                crate::app::ShortcutsPopupMode::Commands => {
                    crate::services::shortcuts_popup::filter_commands("", state).len()
                }
                crate::app::ShortcutsPopupMode::Shortcuts => {
                    crate::services::shortcuts_popup::get_shortcuts_count()
                }
                crate::app::ShortcutsPopupMode::Sessions => state.sessions.len(),
            }
            .saturating_sub(1);
            if state.shortcuts_scroll < max {
                state.shortcuts_scroll += 1;
            }
        }
        InputEvent::InputSubmitted => {
            if state.shortcuts_mode == crate::app::ShortcutsPopupMode::Sessions {
                if let Some(sel) = state.sessions.get(state.shortcuts_scroll).cloned() {
                    let _ = output_tx.try_send(OutputEvent::SwitchToSession(sel.id));
                    state.push_activity(crate::app::ActivityKind::Session, "Switch session");
                    crate::overlay::close_overlay(state, OverlayId::Shortcuts);
                }
            } else if state.shortcuts_mode == crate::app::ShortcutsPopupMode::Commands {
                let cmds = crate::services::shortcuts_popup::filter_commands("", state);
                if let Some(cmd) = cmds.get(state.shortcuts_scroll) {
                    let keep_open = matches!(
                        &cmd.action,
                        crate::services::commands::CommandAction::OpenSessions
                            | crate::services::commands::CommandAction::OpenShortcuts
                    );
                    let _ = execute_shortcuts_command(state, output_tx, cmd);
                    if !keep_open {
                        crate::overlay::close_overlay(state, OverlayId::Shortcuts);
                    }
                }
            }
        }
        _ => {}
    }
}

/// Handle task tray overlay (active job management).
pub fn handle_task_tray(state: &mut AppState, _output_tx: &Sender<OutputEvent>, event: InputEvent) {
    let job_count = state.runtime.jobs.len();
    match event {
        InputEvent::HandleEsc => {
            crate::overlay::close_overlay(state, OverlayId::TaskTray);
        }
        InputEvent::Up | InputEvent::ScrollUp => {
            state.task_tray_selected = state.task_tray_selected.saturating_sub(1);
        }
        InputEvent::Down | InputEvent::ScrollDown => {
            if job_count > 0 {
                state.task_tray_selected =
                    (state.task_tray_selected + 1).min(job_count.saturating_sub(1));
            }
        }
        InputEvent::InputChanged('f') | InputEvent::InputChanged('F') => {
            state.task_tray_filter_active_only = !state.task_tray_filter_active_only;
            state.task_tray_selected = 0;
        }
        InputEvent::InputChanged('x') | InputEvent::InputChanged('X') => {
            if let Some(job) = state.runtime.jobs.get(state.task_tray_selected) {
                let id = job.id;
                let _ = _output_tx.try_send(OutputEvent::CancelRuntimeJob(id));
            }
        }
        _ => {}
    }
}

/// Handle theme picker overlay.
pub fn handle_theme_picker(state: &mut AppState, event: InputEvent) {
    use crate::services::theme::ThemePreset;
    let count = ThemePreset::ALL.len();
    match event {
        InputEvent::HandleEsc => {
            crate::overlay::close_overlay(state, OverlayId::ThemePicker);
        }
        InputEvent::Up | InputEvent::ScrollUp => {
            state.theme_picker_selected = state.theme_picker_selected.saturating_sub(1);
        }
        InputEvent::Down | InputEvent::ScrollDown => {
            state.theme_picker_selected =
                (state.theme_picker_selected + 1).min(count.saturating_sub(1));
        }
        InputEvent::InputSubmitted => {
            if let Some(&preset) = ThemePreset::ALL.get(state.theme_picker_selected) {
                state.theme = crate::services::theme::Theme::new(preset);
            }
            crate::overlay::close_overlay(state, OverlayId::ThemePicker);
        }
        _ => {}
    }
}
