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
            state.workspace.file_index.changes_selected = state.workspace.file_index.changes_selected.saturating_sub(1);
            if state.workspace.file_index.changes_selected < state.workspace.file_index.changes_scroll {
                state.workspace.file_index.changes_scroll = state.workspace.file_index.changes_selected;
            }
        }
        InputEvent::Down | InputEvent::ScrollDown => {
            if !filtered.is_empty() {
                let max_idx = filtered.len().saturating_sub(1);
                if state.workspace.file_index.changes_selected < max_idx {
                    state.workspace.file_index.changes_selected += 1;
                }
            }
        }
        InputEvent::InputChanged(c) => {
            state.workspace.file_index.changes_search.push(c);
            state.workspace.file_index.changes_selected = 0;
            state.workspace.file_index.changes_scroll = 0;
        }
        InputEvent::InputBackspace => {
            state.workspace.file_index.changes_search.pop();
            state.workspace.file_index.changes_selected = 0;
            state.workspace.file_index.changes_scroll = 0;
        }
        InputEvent::ReviewRevertSelected => {
            if let Some(path) = filtered.get(state.workspace.file_index.changes_selected).cloned() {
                let prior = state.workspace.review.selected_path.clone();
                state.workspace.review.selected_path = Some(path);
                let mut ctx = HandlerContext::new(state, output_tx);
                let _ = review_handler::revert_selected(&mut ctx);
                ctx.state.workspace.review.selected_path = prior;
            }
        }
        InputEvent::InputSubmitted => {
            if let Some(path) = filtered.get(state.workspace.file_index.changes_selected).cloned() {
                state.workspace.review.selected_path = Some(path);
                state.layout.workbench_tab = crate::app::WorkbenchTab::Review;
                state.layout.focus = crate::app::WorkspaceFocus::Workbench;
                crate::overlay::close_overlay(state, OverlayId::FileChanges);
            }
        }
        _ => {}
    }
}

/// Handle helper dropdown (inline helper command suggestions).
pub fn handle_helper_dropdown(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    event: InputEvent,
) {
    use crate::handlers::input_commands::dispatch_builtin_command;

    if state.layout.focus != crate::app::WorkspaceFocus::Input {
        return;
    }
    match event {
        InputEvent::Up => {
            if state.layout.command_palette.helper_selected > 0 {
                state.layout.command_palette.helper_selected -= 1;
                if state.layout.command_palette.helper_selected < state.layout.command_palette.helper_scroll {
                    state.layout.command_palette.helper_scroll = state.layout.command_palette.helper_selected;
                }
            }
        }
        InputEvent::Down => {
            if !state.layout.command_palette.filtered_helpers.is_empty() {
                let max_idx = state.layout.command_palette.filtered_helpers.len().saturating_sub(1);
                if state.layout.command_palette.helper_selected < max_idx {
                    state.layout.command_palette.helper_selected += 1;
                    if state.layout.command_palette.helper_selected >= state.layout.command_palette.helper_scroll + 5 {
                        state.layout.command_palette.helper_scroll = state.layout.command_palette.helper_selected.saturating_sub(4);
                    }
                }
            }
        }
        // Dogfood F1 fix: pressing Enter on a highlighted inline
        // slash suggestion now **executes** the command, matching
        // the Ctrl+P palette behaviour (`handle_command_palette`).
        // Pre-fix, Enter only pasted `/model ` into the input and
        // required a second Enter to run — confusing, inconsistent
        // with the primary palette.
        InputEvent::InputSubmitted => {
            let cmd = state
                .layout
                .command_palette
                .filtered_helpers
                .get(state.layout.command_palette.helper_selected)
                .cloned();
            crate::overlay::close_overlay(state, OverlayId::HelperDropdown);
            if let Some(cmd) = cmd {
                // Clear the typed `/` prefix before dispatching so
                // the conversation log doesn't show the raw query.
                state.composer.input.set_content("");
                tracing::info!(
                    target: "vac_tui_runtime::helper_dropdown",
                    command = %cmd.command,
                    "dispatching selected helper command",
                );
                let handled = dispatch_builtin_command(
                    state,
                    output_tx,
                    &cmd.command,
                    None,
                );
                if !handled {
                    tracing::warn!(
                        target: "vac_tui_runtime::helper_dropdown",
                        command = %cmd.command,
                        "dispatch_builtin_command returned false — command \
                         not in state.layout.commands; palette filtered from \
                         a stale list?",
                    );
                }
            } else {
                tracing::warn!(
                    target: "vac_tui_runtime::helper_dropdown",
                    selected = state.layout.command_palette.helper_selected,
                    total = state.layout.command_palette.filtered_helpers.len(),
                    "no helper at selected index — dropdown closed without dispatch",
                );
            }
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
            state.layout.command_palette.input.push(c);
            state.layout.command_palette.selected = 0;
        }
        InputEvent::CommandPaletteBackspace => {
            state.layout.command_palette.input.pop();
            state.layout.command_palette.selected = 0;
        }
        InputEvent::CommandPaletteUp => {
            let filtered = state.filtered_commands();
            if state.layout.command_palette.selected > 0 {
                state.layout.command_palette.selected -= 1;
            } else if !filtered.is_empty() {
                state.layout.command_palette.selected = filtered.len() - 1;
            }
        }
        InputEvent::CommandPaletteDown => {
            let filtered = state.filtered_commands();
            if state.layout.command_palette.selected < filtered.len().saturating_sub(1) {
                state.layout.command_palette.selected += 1;
            } else {
                state.layout.command_palette.selected = 0;
            }
        }
        InputEvent::CommandPaletteSelect => {
            let filtered = state.filtered_commands();
            if let Some(cmd) = filtered.get(state.layout.command_palette.selected).cloned() {
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
            state.layout.command_palette.shortcuts_mode = match state.layout.command_palette.shortcuts_mode {
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
            state.layout.command_palette.shortcuts_scroll = 0;
        }
        InputEvent::Up => {
            state.layout.command_palette.shortcuts_scroll = state.layout.command_palette.shortcuts_scroll.saturating_sub(1);
        }
        InputEvent::Down => {
            let max = match state.layout.command_palette.shortcuts_mode {
                crate::app::ShortcutsPopupMode::Commands => {
                    crate::services::shortcuts_popup::filter_commands("", state).len()
                }
                crate::app::ShortcutsPopupMode::Shortcuts => {
                    crate::services::shortcuts_popup::get_shortcuts_count()
                }
                crate::app::ShortcutsPopupMode::Sessions => state.session.sessions.len(),
            }
            .saturating_sub(1);
            if state.layout.command_palette.shortcuts_scroll < max {
                state.layout.command_palette.shortcuts_scroll += 1;
            }
        }
        InputEvent::InputSubmitted => {
            if state.layout.command_palette.shortcuts_mode == crate::app::ShortcutsPopupMode::Sessions {
                if let Some(sel) = state.session.sessions.get(state.layout.command_palette.shortcuts_scroll).cloned() {
                    let _ = output_tx.try_send(OutputEvent::SwitchToSession(sel.id));
                    state.push_activity(crate::app::ActivityKind::Session, "Switch session");
                    crate::overlay::close_overlay(state, OverlayId::Shortcuts);
                }
            } else if state.layout.command_palette.shortcuts_mode == crate::app::ShortcutsPopupMode::Commands {
                let cmds = crate::services::shortcuts_popup::filter_commands("", state);
                if let Some(cmd) = cmds.get(state.layout.command_palette.shortcuts_scroll) {
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
    let job_count = state.execution.runtime.jobs.len();
    match event {
        InputEvent::HandleEsc => {
            crate::overlay::close_overlay(state, OverlayId::TaskTray);
        }
        InputEvent::Up | InputEvent::ScrollUp => {
            state.execution.task_tray.selected = state.execution.task_tray.selected.saturating_sub(1);
        }
        InputEvent::Down | InputEvent::ScrollDown => {
            if job_count > 0 {
                state.execution.task_tray.selected =
                    (state.execution.task_tray.selected + 1).min(job_count.saturating_sub(1));
            }
        }
        InputEvent::InputChanged('f') | InputEvent::InputChanged('F') => {
            state.execution.task_tray.filter_active_only = !state.execution.task_tray.filter_active_only;
            state.execution.task_tray.selected = 0;
        }
        InputEvent::InputChanged('x') | InputEvent::InputChanged('X') => {
            if let Some(job) = state.execution.runtime.jobs.get(state.execution.task_tray.selected) {
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
            state.operator_config.operator.theme_picker_selected = state.operator_config.operator.theme_picker_selected.saturating_sub(1);
        }
        InputEvent::Down | InputEvent::ScrollDown => {
            state.operator_config.operator.theme_picker_selected =
                (state.operator_config.operator.theme_picker_selected + 1).min(count.saturating_sub(1));
        }
        InputEvent::InputSubmitted => {
            if let Some(&preset) = ThemePreset::ALL.get(state.operator_config.operator.theme_picker_selected) {
                state.core.theme = crate::services::theme::Theme::new(preset);
            }
            crate::overlay::close_overlay(state, OverlayId::ThemePicker);
        }
        _ => {}
    }
}
